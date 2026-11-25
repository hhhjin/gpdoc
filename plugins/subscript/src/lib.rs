#[cfg(feature = "gpui")]
use gpdoc_plugin::core::Baseline;
use gpdoc_plugin::marks::register_formatting_mark;
use gpdoc_plugin::prelude::*;

unit_command!(pub Set, "setSubscript");
unit_command!(pub Unset, "unsetSubscript");
unit_command!(pub Toggle, "toggleSubscript");

gpdoc_plugin::plugin_shortcuts! {
    pub Shortcuts {
        toggle: Toggle => ["Mod-,"],
    }
}

pub fn subscript() -> Plugin<Shortcuts> {
    let plugin = Plugin::<Shortcuts>::new("subscript", |p| {
        register_formatting_mark::<Set, Unset, Toggle>(p, "subscript");
    });
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
