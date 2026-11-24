use gpdoc_plugin::prelude::{
    Attrs, ContentSpec, Edit, Node, NodeSpec, Plugin, Selection, unit_command,
};

unit_command!(pub Set, "setHardBreak");

gpdoc_plugin::plugin_shortcuts! {
    pub Shortcuts {
        insert: Set => ["Shift-Enter", "Mod-Enter"],
    }
}

pub fn hard_break() -> Plugin<Shortcuts> {
    Plugin::new("hardBreak", |p| {
        p.node(
            NodeSpec::inline("hardBreak")
                .group("inline")
                .content(ContentSpec::LineBreak),
        );
        p.command::<Set>(|ctx, _| {
            let Selection::Text { anchor, head } = ctx.selection() else {
                return Ok(None);
            };
            let schema = ctx.schema();
            let from = ctx.document().resolve(anchor.min(head))?;
            let to = ctx.document().resolve(anchor.max(head))?;
            if from.same_parent(&to)
                && !from.parent().can_replace_with(
                    schema,
                    from.index(from.depth()),
                    to.index(to.depth()),
                    "hardBreak",
                    &[],
                )
            {
                return Ok(None);
            }
            let marks = ctx.cursor_marks()?.unwrap_or_else(|| from.marks(schema));
            let mut tx = ctx.transaction();
            tx.push(Edit::ReplaceInline {
                range: anchor.min(head)..anchor.max(head),
                nodes: vec![Node::leaf("hardBreak", Attrs::new())],
            });
            tx.set_stored_marks(marks);
            Ok(Some(tx))
        });
    })
}
