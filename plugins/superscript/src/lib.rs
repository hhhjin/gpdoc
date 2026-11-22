#[cfg(feature = "gpui")]
use gpdoc_plugin::core::Baseline;
use gpdoc_plugin::marks::FormattingMark;
use gpdoc_plugin::prelude::*;

unit_command!(pub Set, "setSuperscript");
unit_command!(pub Unset, "unsetSuperscript");
unit_command!(pub Toggle, "toggleSuperscript");

pub fn superscript() -> Plugin {
    let plugin: Plugin = FormattingMark {
        name: "superscript",
        set: Set,
        unset: Unset,
        toggle: Toggle,
        shortcut: Some("Mod-."),
        delimiters: &[],
    }
    .into();
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, superscript_format);
    plugin
}

/// Apply the native text format for the `superscript` mark.
#[cfg(feature = "gpui")]
pub fn superscript_format() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("superscriptFormat").mark_format("superscript", |_, style| {
        style.baseline = Baseline::Superscript;
    })
}
