use gpdoc_plugin::marks::{
    delimited_mark_input, delimited_mark_paste, set_mark, toggle_mark, unset_mark,
};
use gpdoc_plugin::prelude::{Attrs, MarkSpec, Plugin, unit_command};

unit_command!(pub Set, "setCode");
unit_command!(pub Unset, "unsetCode");
unit_command!(pub Toggle, "toggleCode");

gpdoc_plugin::plugin_config! {
    pub Config {
        shortcuts {
            toggle: Toggle => ["Mod-e"],
        }
        input_rules InputRules { backtick: mark("code") }
        paste_rules PasteRules { backtick: paste }
    }
}

pub fn inline_code() -> Plugin<Config> {
    let plugin = Plugin::<Config>::new("code", |p| {
        p.mark(MarkSpec::new("code").excludes("_"));
        p.input_rules()
            .backtick()
            .set(delimited_mark_input("code", "`"));
        p.paste_rules()
            .backtick()
            .set(delimited_mark_paste("code", "`"));
        p.command::<Set>(|ctx, _| set_mark("code")(ctx, Attrs::new()));
        p.command::<Unset>(|ctx, _| unset_mark("code")(ctx, Attrs::new()));
        p.command::<Toggle>(|ctx, _| toggle_mark("code")(ctx, Attrs::new()));
    });
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, inline_code_format);
    plugin
}

/// Apply the native text format for the `code` mark.
#[cfg(feature = "gpui")]
pub fn inline_code_format() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("codeFormat").mark_format("code", |_, style| {
        style.font_family = Some("monospace".into())
    })
}
