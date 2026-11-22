use gpdoc_plugin::prelude::{AttrSpecs, Attrs, Plugin, unit_command};
use gpdoc_plugin_list_item::{list_container, list_input_rule, toggle_list, wrap_list};

unit_command!(pub Wrap, "wrapInBulletList");
unit_command!(pub Toggle, "toggleBulletList");

pub fn bullet_list() -> Plugin {
    Plugin::new("bulletList", |p| {
        list_container(p, "bulletList", AttrSpecs::new());
        p.command::<Wrap>(|ctx, _| wrap_list(ctx, "bulletList", "listItem", &Attrs::new()));
        p.input_rule(list_input_rule(
            "bulletList",
            "listItem",
            r"^\s*([-+*])\s$",
            |_| Some((Attrs::new(), Attrs::new())),
        ));
        p.command::<Toggle>(|ctx, _| toggle_list(ctx, "bulletList", "listItem", &Attrs::new()));
        p.shortcut("Mod-Shift-8", Toggle);
    })
}

/// Compose the editable content of each `bulletList` node.
#[cfg(feature = "gpui")]
pub fn bullet_list_body(render: impl gpdoc::BlockRenderer) -> gpdoc::ViewPlugin {
    gpdoc::editable_block("bulletList", render)
}
