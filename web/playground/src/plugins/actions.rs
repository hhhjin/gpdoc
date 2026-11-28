//! Actions exposed by the documentation toolbar.
use super::*;
use gpdoc::{Command, StoredCommand};
use gpdoc_plugin_heading::Level;
use serde::Deserialize;
use std::num::{NonZeroU32, NonZeroUsize};

#[derive(Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum DemoAction {
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrike,
    ToggleCode,
    ToggleSubscript,
    ToggleSuperscript,
    ToggleBlockquote,
    ToggleBulletList,
    ToggleTaskList,
    UnsetLink,
    UnsetRubyText,
    SetParagraph,
    SplitListItem,
    LiftListItem,
    SinkListItem,
    ListBackspace,
    SetHorizontalRule,
    SetHardBreak,
    UnsetDetails,
    AddRowAfter,
    AddColumnAfter,
    GoToNextResult,
    ToggleHighlight,
    SetHeading {
        level: u8,
    },
    ToggleHeading {
        level: u8,
    },
    ToggleOrderedList {
        start: Option<i64>,
    },
    SetLink {
        href: String,
    },
    SetRubyText {
        rt: String,
    },
    SetTextAlign {
        text_align: String,
    },
    SetCodeBlock {
        language: String,
    },
    InsertTable {
        rows: NonZeroUsize,
        cols: NonZeroUsize,
        with_header_row: bool,
    },
    SetImage {
        src: String,
        alt: Option<String>,
        width: Option<NonZeroU32>,
    },
    SetAudio {
        src: String,
        #[serde(rename = "loop")]
        repeat: bool,
    },
    InsertInlineMath {
        latex: String,
    },
    StyleSelection(StyleSelection),
    WrapDetails {
        summary: String,
    },
    AcceptMention {
        label: String,
    },
    Find {
        term: String,
    },
    ReplaceMatches {
        term: String,
        #[serde(default)]
        replacement: String,
    },
    SetMarkdown {
        markdown: String,
    },
    ExportMarkdown,
    FindSuggestion {
        trigger: char,
    },
    Duplicate,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StyleSelection {
    color: String,
    font_size: String,
}

impl Command for StyleSelection {
    const NAME: &'static str = "styleSelection";
}

pub(super) fn style_commands() -> gpdoc_core::plugin::Plugin {
    gpdoc_core::plugin::Plugin::new("exampleTextStyle", |p| {
        p.command::<StyleSelection>(|ctx, style| {
            ctx.plan_chain([
                StoredCommand::new(gpdoc_plugin_text_style::SetColor(Some(style.color))),
                StoredCommand::new(gpdoc_plugin_text_style::SetFontSize(Some(style.font_size))),
            ])
        });
    })
}

impl ActionContext<'_, '_> {
    fn run_command(&mut self, command: impl Command) -> Result<()> {
        match self.view.run(command, self.window, self.cx)? {
            CommandOutcome::Applied(_) => Ok(()),
            CommandOutcome::NotApplicable => {
                Err("This command cannot run at the current selection.".into())
            }
        }
    }

    pub(super) fn perform(&mut self, action: DemoAction) -> Result<()> {
        use DemoAction::*;
        match action {
            ToggleBold => self.run_command(gpdoc_plugin_bold::Toggle),
            ToggleItalic => self.run_command(gpdoc_plugin_italic::Toggle),
            ToggleUnderline => self.run_command(gpdoc_plugin_underline::Toggle),
            ToggleStrike => self.run_command(gpdoc_plugin_strike::Toggle),
            ToggleCode => self.run_command(gpdoc_plugin_inline_code::Toggle),
            ToggleSubscript => self.run_command(gpdoc_plugin_subscript::Toggle),
            ToggleSuperscript => self.run_command(gpdoc_plugin_superscript::Toggle),
            ToggleBlockquote => self.run_command(gpdoc_plugin_blockquote::Toggle),
            ToggleBulletList => self.run_command(gpdoc_plugin_bullet_list::Toggle),
            ToggleTaskList => self.run_command(gpdoc_plugin_task_list::Toggle),
            UnsetLink => self.run_command(gpdoc_plugin_link::Unset),
            UnsetRubyText => self.run_command(gpdoc_plugin_ruby_text::Unset),
            SetParagraph => self.run_command(gpdoc_plugin_paragraph::Set),
            SplitListItem => self.run_command(gpdoc_plugin_list_item::Split),
            LiftListItem => self.run_command(gpdoc_plugin_list_item::Lift),
            SinkListItem => self.run_command(gpdoc_plugin_list_item::Sink),
            ListBackspace => self.run_command(gpdoc_plugin_list_keymap::Backspace),
            SetHorizontalRule => self.run_command(gpdoc_plugin_horizontal_rule::Set),
            SetHardBreak => self.run_command(gpdoc_plugin_hard_break::Set),
            UnsetDetails => self.run_command(gpdoc_plugin_details::Unset),
            AddRowAfter => self.run_command(gpdoc_plugin_table::AddRowAfter),
            AddColumnAfter => self.run_command(gpdoc_plugin_table::AddColumnAfter),
            GoToNextResult => self.run_command(gpdoc_plugin_find_and_replace::GoToNextResult),
            ToggleHighlight => self.run_command(gpdoc_plugin_highlight::Toggle(None)),
            SetHeading { level } => {
                self.run_command(gpdoc_plugin_heading::Set(Level::try_from(level)?))
            }
            ToggleHeading { level } => {
                self.run_command(gpdoc_plugin_heading::Toggle(Level::try_from(level)?))
            }
            ToggleOrderedList { start } => self.run_command(gpdoc_plugin_ordered_list::Toggle {
                start,
                ..Default::default()
            }),
            SetLink { href } => self.run_command(gpdoc_plugin_link::Set::new(href)),
            SetRubyText { rt } => self.run_command(gpdoc_plugin_ruby_text::Set { rt: Some(rt) }),
            SetTextAlign { text_align } => {
                self.run_command(gpdoc_plugin_text_align::Set(text_align))
            }
            SetCodeBlock { language } => self.run_command(gpdoc_plugin_code_block::Set(
                gpdoc_plugin_code_block::Language::Set(language),
            )),
            InsertTable {
                rows,
                cols,
                with_header_row,
            } => self.run_command(gpdoc_plugin_table::Insert {
                rows,
                columns: cols,
                with_header_row,
            }),
            SetImage { src, alt, width } => self.run_command(gpdoc_plugin_image::Set {
                src: self
                    .view
                    .resolve_media_url(&src)
                    .ok_or("Enter an HTTP or HTTPS image URL.")?
                    .into(),
                alt,
                width,
                ..Default::default()
            }),
            SetAudio { src, repeat } => self.run_command(gpdoc_plugin_audio::Set {
                src,
                repeat: Some(repeat),
                ..Default::default()
            }),
            InsertInlineMath { latex } => {
                self.run_command(gpdoc_plugin_mathematics::InsertInline { latex, pos: None })
            }
            StyleSelection(style) => self.run_command(style),
            WrapDetails { summary } => {
                self.run_command(gpdoc_plugin_details::Set)?;
                let editor = self.view.editor();
                let mut tx = editor.transaction();
                tx.push(Edit::ReplaceText {
                    range: editor.selection().bounds(editor.document()),
                    text: summary,
                });
                self.view.apply_node_view(tx, self.window, self.cx)?;
                Ok(())
            }
            AcceptMention { label } => {
                let mention = gpdoc_plugin_mention::Mention::new(label.to_lowercase(), &label);
                self.run_command(gpdoc_plugin_mention::Accept(mention))
            }
            Find { term } => self.run_command(gpdoc_plugin_find_and_replace::SetSearchTerm(term)),
            ReplaceMatches { term, replacement } => {
                use gpdoc_plugin_find_and_replace::{ReplaceAll, SetReplaceTerm, SetSearchTerm};
                self.run_command(SetSearchTerm(term))?;
                self.run_command(SetReplaceTerm(replacement))?;
                self.run_command(ReplaceAll)?;
                Ok(())
            }
            SetMarkdown { markdown } => {
                let imported = gpdoc_plugin_markdown::open(self.view.editor().config(), &markdown)?;
                self.run_command(gpdoc_plugin_markdown::Set { markdown })?;
                *self.facts = imported
                    .warnings
                    .into_iter()
                    .map(|warning| ("Warning".into(), warning.message))
                    .collect();
                Ok(())
            }
            ExportMarkdown => {
                let exported = gpdoc_plugin_markdown::to_markdown(self.view.editor())?;
                *self.facts = vec![("Markdown".into(), exported.value)];
                self.facts.extend(
                    exported
                        .warnings
                        .into_iter()
                        .map(|warning| ("Warning".into(), warning.message)),
                );
                Ok(())
            }
            FindSuggestion { trigger } => {
                let found = gpdoc_plugin_suggestion::Trigger::new(trigger).find(
                    self.view.editor().document(),
                    self.view.editor().selection(),
                );
                *self.facts = match found {
                    Some(found) => vec![
                        ("Query".into(), found.query),
                        (
                            "Document range".into(),
                            format!("{}–{}", found.range.start.0, found.range.end.0),
                        ),
                    ],
                    None => vec![("Query".into(), "No match at the caret".into())],
                };
                Ok(())
            }
            Duplicate => {
                let editor = self.view.editor();
                let start = editor.selection().bounds(editor.document()).start;
                let mut end = DocPos(0);
                for node in editor.document().blocks() {
                    end = DocPos(end.0 + node.size());
                    if start < end {
                        let mut tx = editor.transaction();
                        tx.push(Edit::ReplaceNodes {
                            range: end..end,
                            nodes: vec![node.clone()],
                        });
                        self.view.apply_node_view(tx, self.window, self.cx)?;
                        return Ok(());
                    }
                }
                Err("Place the caret in a block first.".into())
            }
        }
    }
}
