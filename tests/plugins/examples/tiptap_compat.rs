//! Stdin/stdout driver for tests/tiptap. Uses the same public APIs as a host app.
#[path = "tiptap_compat/blocks.rs"]
mod blocks;
#[path = "tiptap_compat/commands.rs"]
mod commands;
#[path = "tiptap_compat/document.rs"]
mod document;
#[path = "tiptap_compat/media.rs"]
mod media;
#[path = "tiptap_compat/structure.rs"]
mod structure;
#[path = "tiptap_compat/styles.rs"]
mod styles;
use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::core::Shortcut;
use gpdoc_plugin::prelude::{
    CommandOutcome, DocPos, Editor, Error, Plugin, Selection, StoredCommand, unit_command,
};
use gpdoc_plugin_blockquote::blockquote;
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_bullet_list::bullet_list;
use gpdoc_plugin_code_block::code_block;
use gpdoc_plugin_hard_break::hard_break;
use gpdoc_plugin_heading::{Level, heading};
use gpdoc_plugin_highlight::highlight;
use gpdoc_plugin_horizontal_rule::horizontal_rule;
use gpdoc_plugin_inline_code::inline_code;
use gpdoc_plugin_italic::italic;
use gpdoc_plugin_link::link;
use gpdoc_plugin_list_item::list_item;
use gpdoc_plugin_ordered_list::ordered_list;
use gpdoc_plugin_paragraph::paragraph;
use gpdoc_plugin_strike::strike;
use gpdoc_plugin_subscript::subscript;
use gpdoc_plugin_superscript::superscript;
use gpdoc_plugin_underline::underline;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, Write};

unit_command!(SetSectionTitle, "setSectionTitle");
unit_command!(ApplyTitle, "applyTitle");

fn section_title(level: Level, article: bool) -> Plugin {
    Plugin::new("sectionTitle", |p| {
        p.command::<SetSectionTitle>(move |ctx, _| {
            ctx.plan_chain([
                StoredCommand::new(gpdoc_plugin_heading::Set(level)),
                StoredCommand::new(gpdoc_plugin_bold::Set),
            ])
        });
        if article {
            p.command::<ApplyTitle>(|ctx, _| ctx.plan(SetSectionTitle));
            p.shortcut("F7", ApplyTitle);
        } else {
            p.shortcut("Mod-Shift-2", SetSectionTitle);
        }
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    plugins: Vec<String>,
    doc: Value,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
enum Step {
    Select {
        from: usize,
        to: usize,
    },
    Paste {
        text: String,
    },
    Type {
        text: String,
    },
    Insert {
        text: String,
    },
    Key {
        key: String,
        #[serde(default)]
        primary: bool,
        #[serde(default)]
        shift: bool,
    },
    SetSectionTitle,
    ToggleBold,
    Command {
        name: String,
        #[serde(default)]
        args: std::collections::BTreeMap<String, Value>,
        // Reference commands may use a different positional signature.
        #[serde(rename = "params", default)]
        _params: Vec<Value>,
    },
    SetHighlight,
    Undo,
    Redo,
}

fn apply(editor: &mut Editor, step: Step) -> Result<bool, Error> {
    match step {
        Step::Select { from, to } => editor.select(Selection::Text {
            anchor: DocPos(from),
            head: DocPos(to),
        })?,
        Step::Paste { text } => {
            editor.paste_text(&text)?;
        }
        Step::Insert { text } | Step::Type { text } => {
            editor.insert_text(&text)?;
        }
        Step::Command { name, args, .. } => {
            // Tiptap exposes a boolean for rejected command arguments. Rust also
            // reports a diagnostic. In either case the oracle compares the full
            // document and selection after the attempted command.
            return match commands::decode(&name, args)
                .and_then(|command| editor.run_stored(&command))
            {
                Ok(CommandOutcome::Applied(_)) => Ok(true),
                Ok(CommandOutcome::NotApplicable) | Err(Error::InvalidCommandArguments { .. }) => {
                    Ok(false)
                }
                Err(error) => Err(error),
            };
        }
        Step::Key {
            key,
            primary,
            shift,
        } => {
            let config = editor.config().clone();
            let mut key = Shortcut::new(&key);
            key.primary = primary;
            key.shift = shift;
            for command in config.shortcuts(&key) {
                if matches!(editor.run_stored(command)?, CommandOutcome::Applied(_)) {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        Step::SetSectionTitle => {
            return Ok(matches!(
                editor.run(SetSectionTitle)?,
                CommandOutcome::Applied(_)
            ));
        }
        Step::ToggleBold => {
            let result = editor.run(gpdoc_plugin_bold::Toggle)?;
            return Ok(matches!(result, CommandOutcome::Applied(_)));
        }
        Step::SetHighlight => match editor.run(gpdoc_plugin_highlight::Set(None)) {
            Ok(CommandOutcome::Applied(_)) => return Ok(true),
            Ok(CommandOutcome::NotApplicable) | Err(Error::UnknownCommand(_)) => return Ok(false),
            Err(err) => return Err(err),
        },
        Step::Undo => {
            editor.undo()?;
        }
        Step::Redo => {
            editor.redo()?;
        }
    }
    Ok(true)
}

fn snapshot(editor: &Editor, applied: bool) -> Result<Value, gpdoc_plugin::core::JsonError> {
    let selection = match editor.selection() {
        Selection::Text { anchor, head } => {
            json!({"type":"text", "anchor":anchor.0, "head":head.0})
        }
        Selection::Custom { kind, anchor, head } => {
            let name = match editor.config().selection_plugin(kind) {
                Some("table") => "cell",
                plugin => plugin.expect("registered selection"),
            };
            json!({"type":name,"anchor":anchor.0,"head":head.0})
        }
        Selection::Gap { at } => json!({"type":"gapcursor","pos":at.0}),
        Selection::NodeRange { anchor, head } => {
            json!({"type":"nodeRange","anchor":anchor.0,"head":head.0})
        }
        Selection::Node { before } => json!({"type":"node", "anchor":before.0}),
    };
    let doc: Value = serde_json::from_str(&editor.document().to_json()?)?;
    Ok(json!({"doc":doc, "selection":selection, "applied":applied}))
}

fn run(config: &EditorConfig, case: Case) -> Vec<Value> {
    let mut editor = match gpdoc_plugin::core::Editor::from_json(&config, &case.doc.to_string()) {
        Ok(editor) => editor,
        Err(err) => return vec![json!({"openError":err.to_string()})],
    };
    let mut states = Vec::new();
    for step in std::iter::once(None).chain(case.steps.into_iter().map(Some)) {
        let applied = match step.map_or(Ok(true), |step| apply(&mut editor, step)) {
            Ok(applied) => applied,
            Err(err) => {
                states.push(json!({"editError":err.to_string()}));
                break;
            }
        };
        match snapshot(&editor, applied) {
            Ok(state) => states.push(state),
            Err(err) => {
                states.push(json!({"saveError":err.to_string()}));
                break;
            }
        }
    }
    states
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<Case> = serde_json::from_reader(io::stdin().lock())?;

    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut results = Vec::new();
    for case in cases {
        let mut plugins = vec![paragraph()];
        for plugin in &case.plugins {
            let entry = match plugin.as_str() {
                "ListKit" => {
                    plugins.extend(gpdoc_plugin_list_kit::list_kit());
                    continue;
                }
                "TextStyleKit" => {
                    plugins.extend(gpdoc_plugin_text_style::text_style_kit());
                    continue;
                }
                // The corresponding document plugin declares these structural nodes together.
                "TableCell" | "TableHeader" | "TableRow" | "DetailsContent" | "DetailsSummary" => {
                    continue;
                }
                "SectionTitle" => section_title(Level::H2, false),
                "ArticleTitle" => section_title(Level::H3, true),
                "Subscript" => subscript(),
                "Superscript" => superscript(),
                "Strike" => strike(),
                "Underline" => underline(),
                "HardBreak" => hard_break(),
                "HorizontalRule" => horizontal_rule(),
                "Heading" => heading(),
                "Blockquote" => blockquote(),
                "CodeBlock" => code_block(),
                "ListItem" => list_item(),
                "BulletList" => bullet_list(),
                "OrderedList" => ordered_list(),
                "Bold" => bold(),
                "Italic" => italic(),
                "Link" => link(Default::default()),
                "InlineCode" => inline_code(),
                "Highlight" => highlight(),
                name => blocks::plugin(name)
                    .or_else(|| document::plugin(name))
                    .or_else(|| structure::plugin(name))
                    .or_else(|| styles::plugin(name))
                    .or_else(|| media::plugin(name))
                    .ok_or_else(|| format!("unknown gpdoc test plugin: {name}"))?,
            };
            plugins.push(entry);
        }
        results.push(run(
            &gpdoc_plugin::core::EditorConfig::builder()
                .plugins(plugins)
                .build()?,
            case,
        ));
    }
    serde_json::to_writer(&mut out, &results)?;
    out.flush()?;
    Ok(())
}
