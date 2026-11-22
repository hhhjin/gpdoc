use gpdoc_plugin::marks::FormattingMark;
use gpdoc_plugin::prelude::*;

unit_command!(pub Set, "setStrike");
unit_command!(pub Unset, "unsetStrike");
unit_command!(pub Toggle, "toggleStrike");

pub fn strike() -> Plugin {
    let plugin: Plugin = FormattingMark {
        name: "strike",
        set: Set,
        unset: Unset,
        toggle: Toggle,
        shortcut: Some("Mod-Shift-s"),
        delimiters: &["~~"],
    }
    .into();
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, strike_format);
    plugin
}

/// Apply the native text format for the `strike` mark.
#[cfg(feature = "gpui")]
pub fn strike_format() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("strikeFormat").mark_format("strike", |_, style| {
        style.strike = true;
    })
}
