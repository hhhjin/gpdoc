use gpdoc_plugin::prelude::*;
use gpdoc_plugin::textblock::set_node;

unit_command!(pub Set, "setParagraph");

pub fn paragraph() -> Plugin {
    Plugin::new("paragraph", |p| {
        p.node(
            NodeSpec::block("paragraph")
                .group("block")
                .content("inline*"),
        );
        p.default_text_block("paragraph");
        p.command::<Set>(|ctx, _| set_node("paragraph", Attrs::new())(ctx, Attrs::new()));
        p.shortcut("Mod-Alt-0", Set);
    })
    .priority(1000)
}

/// Compose the editable content of each `paragraph` node.
#[cfg(feature = "gpui")]
pub fn paragraph_block(render: impl gpdoc::BlockRenderer) -> gpdoc::ViewPlugin {
    gpdoc::editable_block("paragraph", render)
}
