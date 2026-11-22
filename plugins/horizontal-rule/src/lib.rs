use gpdoc_plugin::core::Affinity;
use gpdoc_plugin::prelude::{
    Attrs, DocPos, Edit, Fragment, Node, NodeSpec, Plugin, Selection, Slice, Step, unit_command,
};
use regex::Regex;

unit_command!(pub Set, "setHorizontalRule");

/// Draw a full-width separator at the rule’s document position.
#[cfg(feature = "gpui")]
pub fn rule_line() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("ruleLine")
        .for_plugin("horizontalRule")
        .node_format("horizontalRule", |_, format| format.horizontal_rule = true)
}

pub fn horizontal_rule() -> Plugin {
    let marker = Regex::new(r"^(?:---|—-|___\s|\*\*\*\s)\z").expect("valid rule marker");
    let plugin = Plugin::new("horizontalRule", |p| {
        p.node(NodeSpec::block("horizontalRule").group("block"));
        p.input_rule(move |ctx| {
            let Selection::Text { anchor, head } = ctx.selection else {
                return Ok(None);
            };
            if anchor != head || ctx.inserted.end != head {
                return Ok(None);
            }
            let doc = ctx.document.root();
            let schema = ctx.schema;
            let pos = doc.resolve(head.0)?;
            if pos.depth() == 0 || pos.parent().kind == "codeBlock" || pos.parent_offset > 4 {
                return Ok(None);
            }
            let prefix = pos
                .parent()
                .text_between(0, pos.parent_offset, "", "\u{fffc}")?;
            if !marker.is_match(&prefix) {
                return Ok(None);
            }
            let input = doc.resolve(ctx.inserted.start.0)?;
            if input
                .node_before()?
                .or(input.node_after()?)
                .is_some_and(|n| n.marks.iter().any(|m| m.kind == "code"))
            {
                return Ok(None);
            }
            let before = pos.before(pos.depth())?;
            let rule = ctx
                .schema
                .bind_node(&Node::leaf("horizontalRule", Attrs::new()))?;
            let slice = Slice::new(Fragment::from(vec![rule]), 0, 0);
            let Some(step) = Step::fit_replace(schema, doc, before, before, &slice)? else {
                return Ok(None);
            };
            let (_, map) = step.preview(ctx.document)?;
            let range = map.map(DocPos(before + 1), Affinity::After).position
                ..map.map(head, Affinity::After).position;
            Ok(Some(vec![
                step.into(),
                Edit::ReplaceText {
                    range,
                    text: String::new(),
                },
            ]))
        });
        p.command::<Set>(|ctx, _| {
            let doc = ctx.document().root();
            let schema = ctx.schema();
            let selection = ctx.selection();
            let start = selection.primary_range(ctx.document()).start.0;
            let pos = doc.resolve(start)?;
            let can_insert = if matches!(selection, Selection::Node { .. }) {
                let index = pos.index(pos.depth());
                pos.parent()
                    .can_replace_with(schema, index, index + 1, "horizontalRule", &[])
            } else {
                (0..=pos.depth()).rev().any(|d| {
                    pos.node(d)
                        .content_match_at(schema, pos.index(d))
                        .is_ok_and(|m| m.match_type("horizontalRule").is_some())
                })
            };
            if !can_insert {
                return Ok(None);
            }
            let (from, to) = match selection {
                Selection::Gap { at } => (at.0, at.0),
                Selection::Custom { .. } => {
                    let range = selection.primary_range(ctx.document());
                    (range.start.0, range.end.0)
                }
                Selection::NodeRange { anchor, head } => (anchor.min(head).0, anchor.max(head).0),
                Selection::Node { before } => {
                    let after = before.0
                        + ctx
                            .document()
                            .node_at(before)
                            .expect("selected node")
                            .size();
                    (after, after)
                }
                Selection::Text { anchor, head } => {
                    let left = doc.resolve(anchor.min(head).0)?;
                    let right = doc.resolve(anchor.max(head).0)?;
                    let start = if left.parent_offset == 0 {
                        left.before(left.depth())?
                    } else {
                        left.pos
                    };
                    let end = if right.parent_offset == right.parent().content.size() {
                        right.after(right.depth())?
                    } else {
                        right.pos
                    };
                    // At an edge, insert alongside the block. Replace an empty block.
                    if anchor == head && left.parent().content.size() > 0 {
                        let at = if left.parent_offset == 0 { start } else { end };
                        (at, at)
                    } else {
                        (start, end)
                    }
                }
            };
            let rule = ctx
                .schema()
                .bind_node(&Node::leaf("horizontalRule", Attrs::new()))?;
            let slice = Slice::new(Fragment::from(vec![rule]), 0, 0);
            let Some(step) = Step::fit_replace(schema, doc, from, to, &slice)? else {
                return Ok(None);
            };
            let (next, map) = step.preview(ctx.document())?;
            let mut after = None;
            for (_, changed) in map.changes() {
                next.root()
                    .nodes_between(changed.start.0, changed.end.0, |node, pos, _, _| {
                        if node.kind == "horizontalRule" {
                            after = Some(pos + 1);
                        }
                        !node.is_leaf()
                    })?;
            }
            let Some(after) = after else { return Ok(None) };
            let mut tx = ctx.transaction();
            tx.push(step);
            let pos = next.resolve(DocPos(after))?;
            match pos.node_after()? {
                Some(node)
                    if schema
                        .node_type(&node.kind)
                        .is_some_and(|ty| ty.is_textblock()) =>
                {
                    tx.select(Selection::caret(DocPos(after + 1)))
                }
                Some(_) => tx.select(Selection::Node {
                    before: DocPos(after),
                }),
                None => {
                    let kind = ctx.config().default_text_block();
                    if !pos.parent().can_replace_with(
                        schema,
                        pos.index(pos.depth()),
                        pos.index(pos.depth()),
                        kind,
                        &[],
                    ) {
                        tx.select(Selection::Node {
                            before: DocPos(after - 1),
                        });
                    } else {
                        let at = DocPos(after);
                        tx.push(Edit::ReplaceNodes {
                            range: at..at,
                            nodes: vec![Node::text(kind, "")],
                        });
                        tx.select(Selection::caret(DocPos(after + 1)));
                    }
                }
            }
            Ok(Some(tx))
        });
    });
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, rule_line);
    plugin
}
