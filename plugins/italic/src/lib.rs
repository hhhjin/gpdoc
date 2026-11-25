use gpdoc_plugin::marks::{delimited_mark_input, delimited_mark_paste, register_formatting_mark};
use gpdoc_plugin::prelude::{Plugin, unit_command};

unit_command!(pub Set, "setItalic");
unit_command!(pub Unset, "unsetItalic");
unit_command!(pub Toggle, "toggleItalic");

gpdoc_plugin::plugin_config! {
    pub Config {
        shortcuts {
            toggle: Toggle => ["Mod-i"],
        }
        input_rules InputRules { asterisk: mark("italic"), underscore: mark("italic") }
        paste_rules PasteRules { asterisk: paste, underscore: paste }
    }
}

pub fn italic() -> Plugin<Config> {
    let plugin = Plugin::<Config>::new("italic", |p| {
        register_formatting_mark::<Set, Unset, Toggle>(p, "italic");
        p.input_rules()
            .asterisk()
            .set(delimited_mark_input("italic", "*"));
        p.paste_rules()
            .asterisk()
            .set(delimited_mark_paste("italic", "*"));
        p.input_rules()
            .underscore()
            .set(delimited_mark_input("italic", "_"));
        p.paste_rules()
            .underscore()
            .set(delimited_mark_paste("italic", "_"));
    });
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
