#[cfg(feature = "gpui")]
use gpdoc_plugin::core::Baseline;
use gpdoc_plugin::marks::FormattingMark;
use gpdoc_plugin::prelude::*;

unit_command!(pub Set, "setSubscript");
unit_command!(pub Unset, "unsetSubscript");
unit_command!(pub Toggle, "toggleSubscript");

pub fn subscript() -> Plugin {
    let plugin: Plugin = FormattingMark {
        name: "subscript",
        set: Set,
        unset: Unset,
        toggle: Toggle,
        shortcut: Some("Mod-,"),
        delimiters: &[],
    }
    .into();
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, subscript_format);
    plugin
}

/// Apply the native text format for the `subscript` mark.
#[cfg(feature = "gpui")]
pub fn subscript_format() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("subscriptFormat").mark_format("subscript", |_, style| {
        style.baseline = Baseline::Subscript;
    })
}
