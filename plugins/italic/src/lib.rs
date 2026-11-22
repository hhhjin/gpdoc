use gpdoc_plugin::marks::FormattingMark;
use gpdoc_plugin::prelude::{Plugin, unit_command};

unit_command!(pub Set, "setItalic");
unit_command!(pub Unset, "unsetItalic");
unit_command!(pub Toggle, "toggleItalic");

pub fn italic() -> Plugin {
    let plugin: Plugin = FormattingMark {
        name: "italic",
        set: Set,
        unset: Unset,
        toggle: Toggle,
        shortcut: Some("Mod-i"),
        delimiters: &["*", "_"],
    }
    .into();
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, italic_format);
    plugin
}

/// Apply the native text format for the `italic` mark.
#[cfg(feature = "gpui")]
pub fn italic_format() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("italicFormat").mark_format("italic", |_, style| {
        style.italic = true;
    })
}
