use gpdoc_plugin::prelude::{AttrSpecs, Attrs, Plugin, unit_command};
use gpdoc_plugin_list_item::{list_container, list_input_rule, toggle_list, wrap_list};

unit_command!(pub Wrap, "wrapInBulletList");
unit_command!(pub Toggle, "toggleBulletList");

gpdoc_plugin::plugin_config! {
    pub Config {
        shortcuts {
            toggle: Toggle => ["Mod-Shift-8"],
        }
        input_rules InputRules { bullet_marker: input }
    }
}

pub fn bullet_list() -> Plugin<Config> {
    Plugin::<Config>::new("bulletList", |p| {
        list_container(p, "bulletList", AttrSpecs::new());
        p.command::<Wrap>(|ctx, _| wrap_list(ctx, "bulletList", "listItem", &Attrs::new()));
        p.input_rules().bullet_marker().set(list_input_rule(
            "bulletList",
            "listItem",
            r"^\s*([-+*])\s$",
            |_| Some((Attrs::new(), Attrs::new())),
        ));
        p.command::<Toggle>(|ctx, _| toggle_list(ctx, "bulletList", "listItem", &Attrs::new()));
    })
}

/// Compose the editable content of each `bulletList` node.
#[cfg(feature = "gpui")]
pub fn bullet_list_body(render: impl gpdoc::BlockRenderer) -> gpdoc::ViewPlugin {
    gpdoc::editable_block("bulletList", render)
}
