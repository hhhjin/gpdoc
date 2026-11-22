//! Search host-provided commands at the start of a top-level paragraph.
#[cfg(feature = "gpui")]
use gpdoc::PluginViewExt;
use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::core::StoredCommand;
use gpdoc_plugin::prelude::*;
use gpdoc_plugin_suggestion::{SuggestionMatch, Trigger};
use std::sync::Arc;

unit_command!(Clear, "clearSlashQuery");
#[derive(Clone, Debug)]
struct Accept(usize);
impl Command for Accept {
    const NAME: &'static str = "acceptSlashCommand";
}

/// A displayed label, search aliases, and the installed command to execute.
#[derive(Clone)]
pub struct SlashCommand {
    pub id: &'static str,
    pub label: &'static str,
    pub aliases: &'static [&'static str],
    pub command: StoredCommand,
}

/// A catalog can also be contributed by another plugin with `PluginSpec::metadata`.
#[derive(Clone, Default)]
pub struct SlashCatalog {
    entries: Arc<[SlashCommand]>,
}

impl SlashCatalog {
    pub fn new(entries: impl Into<Arc<[SlashCommand]>>) -> Self {
        Self {
            entries: entries.into(),
        }
    }
}

/// Install slash commands, including entries contributed by other installed plugins.
pub fn slash_commands(commands: impl Into<Arc<[SlashCommand]>>) -> Plugin {
    let plugin = Plugin::new("slashCommand", |p| {
        p.metadata(SlashCatalog::new(commands));
        p.command::<Clear>(|ctx, _| {
            let Some(found) = find(ctx.config(), ctx.document(), ctx.selection()) else {
                return Ok(None);
            };
            Ok(Some(ctx.one_step(Edit::ReplaceText {
                range: found.range,
                text: String::new(),
            })))
        });
        p.command::<Accept>(|ctx, Accept(index)| {
            let Some(entry) = entries(ctx.config()).nth(index) else {
                return Ok(None);
            };
            ctx.plan_chain([StoredCommand::new(Clear), entry.command.clone()])
        });
    });
    #[cfg(feature = "gpui")]
    let plugin = plugin.gpui_default_view(slash_menu);
    plugin
}

/// The default, keyboard-accessible text list of matching commands.
#[cfg(feature = "gpui")]
pub fn slash_menu() -> gpdoc::ViewPlugin {
    slash_picker(|menu: SlashMenu, _, _| menu.labeled_list(|entry| entry.label.to_owned()))
}

/// Create a picker for the slash commands in the current editor's config.
#[cfg(feature = "gpui")]
pub fn slash_picker(
    render: impl gpdoc_plugin_suggestion::MenuRenderer<SlashCommand>,
) -> gpdoc::ViewPlugin {
    use gpdoc_plugin_suggestion::{SuggestionItem, suggestion_picker};
    suggestion_picker(
        "slashMenu",
        |editor| find(editor.config(), editor.document(), editor.selection()),
        |editor, matched| {
            let query = matched.query.to_lowercase();
            entries(editor.config())
                .enumerate()
                .filter(|(_, entry)| {
                    std::iter::once(entry.label)
                        .chain(entry.aliases.iter().copied())
                        .any(|term| term.to_lowercase().starts_with(&query))
                })
                .map(|(index, entry)| SuggestionItem::new(entry.clone(), Accept(index)))
                .collect()
        },
        render,
    )
    .for_plugin("slashCommand")
}

#[cfg(feature = "gpui")]
pub type SlashMenu = gpdoc_plugin_suggestion::SuggestionMenu<SlashCommand>;

fn entries(config: &EditorConfig) -> impl Iterator<Item = &SlashCommand> {
    config
        .plugin_data::<SlashCatalog>()
        .flat_map(|(_, catalog)| catalog.entries.iter())
}

fn find(
    config: &EditorConfig,
    document: &Document,
    selection: Selection,
) -> Option<SuggestionMatch> {
    let mut trigger = Trigger::new('/');
    trigger.start_of_line = true;
    let matched = trigger.find(document, selection)?;
    let start = document.resolve(matched.range.start).ok()?;
    (start.depth() == 1
        && start.parent_offset == 0
        && start.parent().kind == config.default_text_block())
    .then_some(matched)
}
