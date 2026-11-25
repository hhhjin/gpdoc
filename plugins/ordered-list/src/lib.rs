use gpdoc_plugin::prelude::*;
use gpdoc_plugin_list_item::{list_container, list_input_rule, toggle_list, wrap_list};

#[derive(Clone, Debug, Default)]
pub struct Toggle {
    pub start: Option<i64>,
    pub list_type: Option<String>,
}
impl Command for Toggle {
    const NAME: &'static str = "toggleOrderedList";
}
#[derive(Clone, Debug, Default)]
pub struct Wrap {
    pub start: Option<i64>,
    pub list_type: Option<String>,
}
impl Command for Wrap {
    const NAME: &'static str = "wrapInOrderedList";
}

fn list_attrs(start: Option<i64>, list_type: Option<String>) -> Attrs {
    let mut attrs = Attrs::new();
    if let Some(start) = start {
        attrs.insert("start".into(), start.into());
    }
    if let Some(list_type) = list_type {
        attrs.insert("type".into(), list_type.into());
    }
    attrs
}

gpdoc_plugin::plugin_config! {
    pub Config {
        shortcuts {
            toggle: Toggle::default() => ["Mod-Shift-7"],
        }
        input_rules InputRules { ordered_marker: input }
    }
}

pub fn ordered_list() -> Plugin<Config> {
    let mut attrs = AttrSpecs::from([(
        "start".into(),
        AttrSpec::with_default(AttrType::Integer, Value::from(1)),
    )]);
    attrs.insert(
        "type".into(),
        AttrSpec::with_default(AttrType::NullableString, Value::Null),
    );
    Plugin::<Config>::new("orderedList", |p| {
        list_container(p, "orderedList", attrs);
        p.command::<Wrap>(|ctx, args| {
            wrap_list(
                ctx,
                "orderedList",
                "listItem",
                &list_attrs(args.start, args.list_type),
            )
        });
        p.input_rules().ordered_marker().set(list_input_rule(
            "orderedList",
            "listItem",
            r"^(\d+)\.\s$",
            |start| {
                Some((
                    Attrs::from([("start".into(), Value::from(start.parse::<i64>().ok()?))]),
                    Attrs::new(),
                ))
            },
        ));
        p.command::<Toggle>(|ctx, args| {
            toggle_list(
                ctx,
                "orderedList",
                "listItem",
                &list_attrs(args.start, args.list_type),
            )
        });
    })
}

/// Compose the editable content of each `orderedList` node.
#[cfg(feature = "gpui")]
pub fn ordered_list_body(render: impl gpdoc::BlockRenderer) -> gpdoc::ViewPlugin {
    gpdoc::editable_block("orderedList", render)
}
