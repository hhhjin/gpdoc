//! Editable summary and body blocks for Tiptap details documents.
use gpdoc_plugin::prelude::{
    AttrSpec, AttrSpecs, AttrType, ContentRef, DocPos, Edit, Node, NodeSpec, Plugin, Selection,
    Value, unit_command,
};
use gpdoc_plugin::selection::{path_at, selected_blocks};

unit_command!(pub Set, "setDetails");
unit_command!(pub Unset, "unsetDetails");

pub fn details() -> Plugin {
    details_with_persistence(false)
}

/// Persist the open state in the document when requested by the host.
pub fn details_with_persistence(persist: bool) -> Plugin {
    let attrs = if persist {
        AttrSpecs::from([(
            "open".into(),
            AttrSpec::with_default(AttrType::Bool, Value::Bool(false)),
        )])
    } else {
        AttrSpecs::new()
    };
    let plugin = Plugin::new("details", |p| {
        p.node(NodeSpec::block("detailsSummary").content("text*"));
        p.node(NodeSpec::block("detailsContent").content("block+"));
        p.node(
            NodeSpec::block("details")
                .group("block")
                .content("detailsSummary detailsContent")
                .attrs(attrs),
        );
        p.command::<Set>(|ctx, _| {
            let blocks = selected_blocks(ctx);
            let (Some(first), Some(last)) = (blocks.first(), blocks.last()) else {
                return Ok(None);
            };
            let children: Vec<_> = blocks.iter().map(|block| block.node.clone()).collect();
            if !ctx.schema().allows_children("detailsContent", &children) {
                return Ok(None);
            }
            let doc = ctx.document().root();
            let start = doc.resolve(first.before)?;
            let end = doc.resolve(last.end())?;
            if !start.same_parent(&end)
                || !start.parent().can_replace_with(
                    ctx.schema(),
                    start.index(start.depth()),
                    end.index(end.depth()),
                    "details",
                    &[],
                )
            {
                return Ok(None);
            }
            let mut tx = ctx.transaction();
            tx.push(Edit::ReplaceNodes {
                range: DocPos(first.before)..DocPos(last.end()),
                nodes: vec![Node::container(
                    "details",
                    vec![
                        Node::text("detailsSummary", ""),
                        Node::container("detailsContent", children),
                    ],
                )],
            });
            tx.select(Selection::caret(DocPos(first.before + 2)));
            Ok(Some(tx))
        });
        p.command::<Unset>(|ctx, _| {
            let Selection::Text { anchor, head } = ctx.selection() else {
                return Ok(None);
            };
            let Some(details) = path_at(ctx.document().blocks(), anchor.min(head))
                .into_iter()
                .rev()
                .find(|b| b.node.kind == "details" && anchor.max(head).0 < b.end())
            else {
                return Ok(None);
            };
            let ContentRef::Blocks(parts) = details.node.content() else {
                return Ok(None);
            };
            let [summary, body] = parts else {
                return Ok(None);
            };
            let ContentRef::Blocks(children) = body.content() else {
                return Ok(None);
            };
            let start = ctx.document().root().resolve(details.before)?;
            let schema = ctx.schema();
            let parent = schema
                .node_type(&start.parent().kind)
                .expect("registered parent type");
            let Some(default) = parent.content_match.default_type(schema) else {
                return Ok(None);
            };
            let mut nodes = vec![Node::new(
                default,
                Default::default(),
                summary.content().to_owned(),
            )];
            nodes.extend_from_slice(children);
            let mut tx = ctx.transaction();
            tx.push(Edit::ReplaceNodes {
                range: DocPos(details.before)..DocPos(details.end()),
                nodes,
            });
            tx.select(Selection::caret(DocPos(details.before + 1)));
            Ok(Some(tx))
        });
    });
    #[cfg(feature = "gpui")]
    let plugin = {
        use gpdoc::PluginViewExt;
        plugin.gpui_default_view(view::default_disclosure)
    };
    plugin
}

#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{DetailsSummary, disclosure};
