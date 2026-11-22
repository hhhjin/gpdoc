use gpdoc_plugin::marks::FormattingMark;
use gpdoc_plugin::prelude::*;

unit_command!(pub Set, "setUnderline");
unit_command!(pub Unset, "unsetUnderline");
unit_command!(pub Toggle, "toggleUnderline");

pub fn underline() -> Plugin {
    let plugin: Plugin = FormattingMark {
        name: "underline",
        set: Set,
        unset: Unset,
        toggle: Toggle,
        shortcut: Some("Mod-u"),
        delimiters: &[],
    }
    .into();
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, underline_format);
    plugin
}

/// Apply the native text format for the `underline` mark.
#[cfg(feature = "gpui")]
pub fn underline_format() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("underlineFormat").mark_format("underline", |_, style| {
        style.underline = true;
    })
}
