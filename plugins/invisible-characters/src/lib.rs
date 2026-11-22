//! Mark spaces, line breaks, and paragraph ends without adding document content.
#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{CharacterMarker, MarkerKind, character_markers, paint_marker};

use gpdoc_plugin::core::{CommandContext, Editor, EditorConfig};
use gpdoc_plugin::{Command, CommandResult, Plugin, unit_command};

#[derive(Clone, Debug, Default, PartialEq)]
struct Visibility(bool);

/// Return the current visibility, falling back to the configured initial value.
pub fn is_visible(editor: &Editor) -> bool {
    visibility(editor.plugin_state(), editor.config())
}

fn visibility(state: Option<&Visibility>, config: &EditorConfig) -> bool {
    state.map_or_else(
        || {
            config
                .plugin_data::<Visibility>()
                .next()
                .is_some_and(|(_, initial)| initial.0)
        },
        |state| state.0,
    )
}

/// Calling the command without a setting shows whitespace markers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Show {
    pub visible: Option<bool>,
}

impl Command for Show {
    const NAME: &'static str = "showInvisibleCharacters";
}
unit_command!(pub Hide, "hideInvisibleCharacters");
unit_command!(pub Toggle, "toggleInvisibleCharacters");

pub fn invisible_characters(visible: bool) -> Plugin {
    let plugin = Plugin::new("invisibleCharacters", |p| {
        p.metadata(Visibility(visible));
        p.command::<Show>(|ctx, args| set_visibility(ctx, args.visible.unwrap_or(true)));
        p.command::<Hide>(|ctx, _| set_visibility(ctx, false));
        p.command::<Toggle>(|ctx, _| {
            let current = visibility(ctx.plugin_state(), ctx.config());
            set_visibility(ctx, !current)
        });
    });
    #[cfg(feature = "gpui")]
    let plugin =
        gpdoc::PluginViewExt::gpui_default_view(plugin, || character_markers(paint_marker));
    plugin
}

fn set_visibility(ctx: CommandContext<'_>, visible: bool) -> CommandResult {
    let mut tx = ctx.transaction();
    tx.update_plugin_state(|state: &mut Visibility| state.0 = visible);
    Ok(Some(tx))
}
