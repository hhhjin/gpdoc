use gpdoc_plugin::marks::register_formatting_mark;
use gpdoc_plugin::prelude::*;

unit_command!(pub Set, "setUnderline");
unit_command!(pub Unset, "unsetUnderline");
unit_command!(pub Toggle, "toggleUnderline");

gpdoc_plugin::plugin_shortcuts! {
    pub Shortcuts {
        toggle: Toggle => ["Mod-u"],
    }
}

pub fn underline() -> Plugin<Shortcuts> {
    let plugin = Plugin::<Shortcuts>::new("underline", |p| {
        register_formatting_mark::<Set, Unset, Toggle>(p, "underline");
    });
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
