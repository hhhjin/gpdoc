use gpdoc_plugin::marks::{delimited_mark_rules, set_mark, toggle_mark, unset_mark};
use gpdoc_plugin::prelude::{
    AttrSpec, AttrType, Attrs, Command, MarkSpec, Plugin, Value, unit_command,
};

fn attrs(color: Option<String>) -> Attrs {
    color
        .into_iter()
        .map(|value| ("color".into(), value.into()))
        .collect()
}

#[derive(Clone, Debug)]

pub struct Set(pub Option<String>);
impl Command for Set {
    const NAME: &'static str = "setHighlight";
}
#[derive(Clone, Debug)]

pub struct Toggle(pub Option<String>);
impl Command for Toggle {
    const NAME: &'static str = "toggleHighlight";
}
unit_command!(pub Unset, "unsetHighlight");

pub fn highlight() -> Plugin {
    highlight_with(false)
}

pub fn multicolor_highlight() -> Plugin {
    highlight_with(true)
}

fn highlight_with(multicolor: bool) -> Plugin {
    let mut spec = MarkSpec::new("highlight");
    if multicolor {
        spec.attrs.insert(
            "color".into(),
            AttrSpec::with_default(AttrType::NullableString, Value::Null),
        );
    }
    let plugin = Plugin::new("highlight", |p| {
        p.mark(spec);
        delimited_mark_rules(p, "highlight", &["=="]);
        p.command::<Set>(|ctx, command| set_mark("highlight")(ctx, attrs(command.0)));
        p.command::<Toggle>(|ctx, command| toggle_mark("highlight")(ctx, attrs(command.0)));
        p.shortcut("Mod-Shift-h", Toggle(None));
        p.command::<Unset>(|ctx, _| unset_mark("highlight")(ctx, Attrs::new()));
    });
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, || highlight_colors(None));
    plugin
}

/// Paint each mark's saved color, falling back to the supplied RGB value.
#[cfg(feature = "gpui")]
pub fn highlight_format(color: [u8; 3]) -> gpdoc::ViewPlugin {
    highlight_colors(Some([color[0], color[1], color[2], 255]))
}

#[cfg(feature = "gpui")]
fn highlight_colors(fallback: Option<[u8; 4]>) -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("highlightFormat").mark_format("highlight", move |attrs, style| {
        let rgba = match attrs.get("color") {
            Some(Value::String(css)) => css
                .parse::<svgtypes::Color>()
                .ok()
                .map(|c| [c.red, c.green, c.blue, c.alpha]),
            _ => None,
        };
        if let Some(color) = rgba.or(fallback) {
            style.background = Some(color);
        }
    })
}
