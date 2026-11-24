use gpdoc_plugin::marks::{delimited_mark_input, delimited_mark_paste, register_formatting_mark};
use gpdoc_plugin::prelude::*;

unit_command!(pub Set, "setStrike");
unit_command!(pub Unset, "unsetStrike");
unit_command!(pub Toggle, "toggleStrike");

gpdoc_plugin::plugin_config! {
    pub Config {
        shortcuts {
            toggle: Toggle => ["Mod-Shift-s"],
        }
        input_rules InputRules { double_tilde: mark("strike") }
        paste_rules PasteRules { double_tilde: paste }
    }
}

pub fn strike() -> Plugin<Config> {
    let plugin = Plugin::<Config>::new("strike", |p| {
        register_formatting_mark::<Set, Unset, Toggle>(p, "strike");
        p.input_rules()
            .double_tilde()
            .set(delimited_mark_input("strike", "~~"));
        p.paste_rules()
            .double_tilde()
            .set(delimited_mark_paste("strike", "~~"));
    });
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
