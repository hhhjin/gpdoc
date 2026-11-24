#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{ImageNode, image_content, image_controls};

#[cfg(feature = "gpui")]
use gpdoc::PluginViewExt;
use gpdoc_plugin::atom::{atom, captured_atom_input_rule, insert_atom};
use gpdoc_plugin::prelude::{
    AttrSpec, AttrSpecs, AttrType, Attrs, Command, Node, Placement, Plugin, Value,
};
use regex::Regex;
use std::num::NonZeroU32;

#[derive(Clone, Debug, Default)]
pub struct Set {
    pub src: String,
    pub alt: Option<String>,
    pub title: Option<String>,
    pub width: Option<NonZeroU32>,
    pub height: Option<NonZeroU32>,
}

impl Command for Set {
    const NAME: &'static str = "setImage";
}

impl Set {
    fn into_attrs(self) -> Attrs {
        let mut attrs = Attrs::from([("src".into(), self.src.into())]);
        if let Some(value) = self.alt {
            attrs.insert("alt".into(), value.into());
        }
        if let Some(value) = self.title {
            attrs.insert("title".into(), value.into());
        }
        if let Some(value) = self.width {
            attrs.insert("width".into(), value.get().into());
        }
        if let Some(value) = self.height {
            attrs.insert("height".into(), value.get().into());
        }
        attrs
    }
}

fn image_label(attrs: &Attrs) -> String {
    ["alt", "title"]
        .into_iter()
        .find_map(|key| attrs.get(key)?.as_str().filter(|s| !s.is_empty()))
        .unwrap_or("image")
        .into()
}

gpdoc_plugin::plugin_config! {
    pub Config {
        input_rules InputRules { markdown_image: input }
    }
}

pub fn image() -> Plugin<Config> {
    image_with_placement(Placement::Block)
}

pub fn image_with_placement(placement: Placement) -> Plugin<Config> {
    let attrs = ["src", "alt", "title", "width", "height"].map(|key| {
        let kind = if matches!(key, "width" | "height") {
            AttrType::Json
        } else {
            AttrType::NullableString
        };
        (key.into(), AttrSpec::with_default(kind, Value::Null))
    });
    let markdown = Regex::new(r#"(?:^|\s)(!\[(.*)\]\((\S+?)(?:\s+["'](\S+)["'])?\))$"#).unwrap();
    let plugin = Plugin::<Config>::new("image", |p| {
        p.node(atom("image", placement, AttrSpecs::from(attrs)));
        p.atom_label("image", image_label);
        p.input_rules()
            .markdown_image()
            .set(captured_atom_input_rule("image", move |text| {
                let cap = markdown.captures(text)?;
                let attrs = [
                    ("alt".into(), Value::String(cap[2].into())),
                    ("src".into(), Value::String(cap[3].into())),
                    (
                        "title".into(),
                        cap.get(4)
                            .map_or(Value::Null, |m| Value::String(m.as_str().into())),
                    ),
                ]
                .into();
                Some((cap.get(1)?.start(), attrs))
            }));

        p.command::<Set>(|ctx, args| {
            insert_atom(
                ctx,
                Node::leaf("image", args.into_attrs()),
                ctx.selection().bounds(ctx.document()),
            )
        });
    });
    #[cfg(feature = "gpui")]
    let plugin = plugin.gpui_default_view(image_content);
    plugin
}
