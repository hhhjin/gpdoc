use gpdoc_plugin::core::{Boundary, BoundaryEdit};
use gpdoc_plugin::prelude::{
    AttrSpecs, Attrs, CommandContext, Content, ContentRef, DocPos, Edit, Error, InputRuleContext,
    Node, NodeSpec, Plugin, PluginSpec, Selection, Transaction, Value, unit_command,
};
use gpdoc_plugin::selection::{Located, children_at, path_at, selected_blocks};
use std::ops::Range;

const LIST_ITEM: &str = "listItem";

unit_command!(pub Split, "splitListItem");
unit_command!(pub Sink, "sinkListItem");
unit_command!(pub Lift, "liftListItem");

pub fn list_item() -> Plugin {
    let plugin = Plugin::new("listItem", |p| {
        p.node(NodeSpec::block(LIST_ITEM).content("paragraph block*"));
        p.drag_handle(LIST_ITEM, ["paragraph"]);
        p.command::<Split>(|ctx, _| split_item(ctx, LIST_ITEM));
        p.on_break(LIST_ITEM, |ctx| ctx.plan(Split));
        p.command::<Sink>(|ctx, _| sink_item(ctx, LIST_ITEM));
        p.shortcut("Tab", Sink);
        p.command::<Lift>(|ctx, _| lift_item(ctx, LIST_ITEM));
        p.shortcut("Shift-Tab", Lift);
    });
    #[cfg(feature = "gpui")]
    let plugin = {
        use gpdoc::PluginViewExt;
        plugin.gpui_default_view(view::default_items)
    };
    plugin
}

/// Register a list container for list items.
pub fn list_container(p: &mut PluginSpec, kind: &'static str, attrs: AttrSpecs) {
    p.node(
        NodeSpec::block(kind)
            .group("block")
            .group("list")
            .content("listItem+")
            .attrs(attrs),
    );
}

fn open(kind: &str, attrs: &Attrs) -> Boundary {
    Boundary::Open {
        kind: kind.to_owned(),
        attrs: attrs.clone(),
    }
}
fn edit(range: Range<usize>, insert: Vec<Boundary>) -> BoundaryEdit {
    BoundaryEdit {
        range: DocPos(range.start)..DocPos(range.end),
        insert,
    }
}

pub fn wrap_list(
    editor: CommandContext<'_>,
    kind: &str,
    item_kind: &str,
    args: &Attrs,
) -> Result<Option<Transaction>, Error> {
    let blocks = selected_blocks(editor);
    if blocks.is_empty() || blocks.iter().any(|b| b.node.kind != "paragraph") {
        return Ok(None);
    }
    let pos = editor.document().root().resolve(blocks[0].before)?;
    let count = blocks.len();
    if !pos.parent().can_replace_with(
        editor.schema(),
        pos.index(pos.depth()),
        pos.index(pos.depth()) + count,
        kind,
        &[],
    ) {
        return Ok(None);
    }
    let item = || open(item_kind, &Attrs::new());
    let first = blocks[0].before;
    let previous = pos.node_before()?.is_some_and(|n| n.kind == kind);
    let mut edits = vec![if previous {
        edit(first - 1..first, vec![item()])
    } else {
        edit(first..first, vec![open(kind, args), item()])
    }];
    for block in &blocks[1..] {
        edits.push(edit(
            block.before..block.before,
            vec![Boundary::Close, item()],
        ));
    }
    let last = blocks[blocks.len() - 1].end();
    let next = editor
        .document()
        .root()
        .resolve(last)?
        .node_after()?
        .is_some_and(|n| n.kind == kind);
    edits.push(if next {
        edit(last..last + 1, vec![Boundary::Close])
    } else {
        edit(last..last, vec![Boundary::Close, Boundary::Close])
    });
    Ok(Some(editor.one_step(Edit::Boundaries(edits))))
}

pub fn sink_item(
    editor: CommandContext<'_>,
    item_kind: &str,
) -> Result<Option<Transaction>, Error> {
    let Some((list, item)) = selected_item(editor, item_kind) else {
        return Ok(None);
    };
    let ContentRef::Blocks(items) = list.node.content() else {
        return Ok(None);
    };
    let Some(previous) =
        children_at(items, list.before + 1).find(|sibling| sibling.end() == item.before)
    else {
        return Ok(None);
    };
    let ContentRef::Blocks(content) = previous.node.content() else {
        return Ok(None);
    };
    // Extend the previous item's trailing list when possible. Both cases keep
    // every original text and list-item boundary in document order.
    let first = if content
        .last()
        .is_some_and(|last| last.kind == list.node.kind)
    {
        edit(item.before - 2..item.before, vec![])
    } else {
        edit(
            item.before - 1..item.before,
            vec![open(&list.node.kind, &list.node.attrs)],
        )
    };
    let last = edit(
        item.end()..item.end(),
        vec![Boundary::Close, Boundary::Close],
    );
    Ok(Some(editor.one_step(Edit::Boundaries(vec![first, last]))))
}

pub fn lift_item(
    editor: CommandContext<'_>,
    item_kind: &str,
) -> Result<Option<Transaction>, Error> {
    let Some((list, item)) = selected_item(editor, item_kind) else {
        return Ok(None);
    };
    let first_item = item.before == list.before + 1;
    let last_item = item.end() + 1 == list.end();

    let list_path = path_at(editor.document().blocks(), DocPos(list.before));
    let nested = list_path
        .iter()
        .rev()
        .nth(1)
        .is_some_and(|parent| parent.node.kind == item_kind);
    let edits = if nested {
        // Close the old parent before the lifted item. Its former following
        // siblings, if any, become a sublist of the lifted item.
        let start = if first_item {
            edit(list.before..item.before, vec![Boundary::Close])
        } else {
            edit(
                item.before..item.before,
                vec![Boundary::Close, Boundary::Close],
            )
        };
        let end = if last_item {
            edit(item.end() - 1..list.end(), vec![])
        } else {
            edit(
                item.end() - 1..item.end(),
                vec![open(&list.node.kind, &list.node.attrs)],
            )
        };
        vec![start, end]
    } else {
        // Lift the item's blocks out of a top-level (or quoted) list. Keep
        // list shells only for the siblings that remain on either side.
        let start = if first_item {
            edit(list.before..item.before + 1, vec![])
        } else {
            edit(item.before..item.before + 1, vec![Boundary::Close])
        };
        let end = if last_item {
            edit(item.end() - 1..list.end(), vec![])
        } else {
            edit(
                item.end() - 1..item.end(),
                vec![open(&list.node.kind, &list.node.attrs)],
            )
        };
        vec![start, end]
    };
    Ok(Some(editor.one_step(Edit::Boundaries(edits))))
}

pub fn selected_item<'a>(
    editor: CommandContext<'a>,
    item_kind: &str,
) -> Option<(Located<'a>, Located<'a>)> {
    let range = editor.selection().bounds(editor.document());
    let (from, to) = if matches!(editor.selection(), Selection::Node { .. }) {
        (range.start, range.start)
    } else {
        (range.start, range.end)
    };
    let innermost = |position| {
        let path = path_at(editor.document().blocks(), position);
        path.windows(2)
            .rev()
            .find(|pair| {
                editor
                    .schema()
                    .node_type(&pair[0].node.kind)
                    .is_some_and(|ty| ty.in_group("list"))
                    && pair[1].node.kind == item_kind
            })
            .map(|pair| (pair[0], pair[1]))
    };
    let (list, item) = innermost(from)?;
    (innermost(to)?.1.before == item.before).then_some((list, item))
}

/// Wrap selected paragraphs or lift the current item from this list.
pub fn toggle_list(
    editor: CommandContext<'_>,
    kind: &str,
    item: &str,
    attrs: &Attrs,
) -> Result<Option<Transaction>, Error> {
    let doc = editor.document();
    let range = editor.selection().bounds(doc);
    let nearest = |position| {
        let path = path_at(doc.blocks(), position);
        path.windows(2)
            .rev()
            .find(|pair| {
                editor
                    .schema()
                    .node_type(&pair[0].node.kind)
                    .is_some_and(|ty| ty.in_group("list"))
            })
            .map(|p| (p[0], p[1]))
    };
    if let (Some((list, first)), Some((last_list, last))) =
        (nearest(range.start), nearest(range.end))
        && list.before == last_list.before
    {
        let ContentRef::Blocks(items) = list.node.content() else {
            return Ok(None);
        };
        if list.node.kind != kind {
            let nodes: Vec<_> = items
                .iter()
                .map(|old| {
                    if old.kind == item {
                        old.clone()
                    } else {
                        Node::new(item, Attrs::new(), old.content().to_owned())
                    }
                })
                .collect();
            let target = Node::new(kind, attrs.clone(), Content::Blocks(nodes));
            if editor.schema().bind_node(&target).is_err() {
                return Ok(None);
            }
            let mut tx = editor.transaction();
            tx.push(Edit::ReplaceNodes {
                range: DocPos(list.before)..DocPos(list.end()),
                nodes: vec![target],
            });
            tx.select(editor.selection());
            return Ok(Some(tx));
        }
        if first.before == last.before {
            return lift_item(editor, item);
        }
        // Unwrap consecutive siblings in one edit, retaining a list shell for
        // each unselected group on either side.
        let mut edits = vec![if first.before == list.before + 1 {
            edit(list.before..first.before + 1, vec![])
        } else {
            edit(first.before..first.before + 1, vec![Boundary::Close])
        }];
        for current in children_at(items, list.before + 1)
            .filter(|n| first.before < n.before && n.before <= last.before)
        {
            edits.push(edit(current.before - 1..current.before + 1, vec![]));
        }
        edits.push(if last.end() + 1 == list.end() {
            edit(last.end() - 1..list.end(), vec![])
        } else {
            edit(
                last.end() - 1..last.end(),
                vec![open(kind, &list.node.attrs)],
            )
        });
        return Ok(Some(editor.one_step(Edit::Boundaries(edits))));
    }
    wrap_list(editor, kind, item, attrs)
}

/// Split the paragraph and its containing item in the same transaction.
pub fn split_item(
    editor: CommandContext<'_>,
    item_kind: &str,
) -> Result<Option<Transaction>, Error> {
    let Some((_list, item)) = selected_item(editor, item_kind) else {
        return Ok(None);
    };
    let Selection::Text { anchor, head } = editor.selection() else {
        return Ok(None);
    };
    let from = anchor.min(head);
    let to = anchor.max(head);
    let doc = editor.document().root();
    let pos = doc.resolve(from.0)?;
    if !pos.same_parent(&doc.resolve(to.0)?)
        || pos.parent().kind != "paragraph"
        || pos.depth() < 2
        || pos.node(pos.depth() - 1).kind != item_kind
    {
        return Ok(None);
    }
    if pos.parent().content.size() == 0 && item.node.size() == 4 {
        return lift_item(editor, item_kind);
    }
    let mut tx = editor.transaction();
    if from != to {
        tx.push(Edit::ReplaceText {
            range: from..to,
            text: String::new(),
        });
    }
    tx.push(Edit::Boundaries(vec![edit(
        from.0..from.0,
        vec![
            Boundary::Close,
            Boundary::Close,
            open(item_kind, &Attrs::new()),
            open("paragraph", &Attrs::new()),
        ],
    )]));
    tx.select(Selection::caret(DocPos(from.0 + 4)));
    Ok(Some(tx))
}

/// Match a list prefix at the start of a paragraph and keep the remaining
/// inline content and its marks. Numbered lists continue a matching counter.
pub fn list_input_rule(
    list: &'static str,
    item: &'static str,
    pattern: &str,
    attrs: impl Fn(&str) -> Option<(Attrs, Attrs)> + Send + Sync + 'static,
) -> impl Fn(InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error> {
    let regex = regex::Regex::new(pattern).expect("valid list marker pattern");
    move |ctx| {
        let Selection::Text { anchor, head } = ctx.selection else {
            return Ok(None);
        };
        if anchor != head || head != ctx.inserted.end {
            return Ok(None);
        }
        let pos = ctx.document.resolve(head)?;
        if pos.parent().kind != "paragraph" {
            return Ok(None);
        }
        let schema = ctx.schema;
        let text = pos
            .parent()
            .text_between(0, pos.parent_offset, "", "\u{fffc}")?;
        let Some(caps) = regex.captures(&text) else {
            return Ok(None);
        };
        let Some((list_attrs, item_attrs)) = attrs(caps.get(1).map_or("", |m| m.as_str())) else {
            return Ok(None);
        };
        let depth = pos.depth() - 1;
        let index = pos.index(depth);
        if !pos
            .node(depth)
            .can_replace_with(schema, index, index + 1, list, &[])
        {
            return Ok(None);
        }
        let before = pos.before(pos.depth())?;
        let after = pos.after(pos.depth())? - pos.parent_offset;
        let previous = index
            .checked_sub(1)
            .and_then(|i| pos.node(depth).content.maybe_child(i));
        let join = previous.is_some_and(|node| {
            if node.kind != list || list == "taskList" {
                return false;
            }
            let Some(start) = list_attrs.get("start").and_then(Value::as_i64) else {
                return true;
            };
            node.attrs
                .get("type")
                .is_none_or(|v| v.is_null() || v.as_str() == Some("1"))
                && node
                    .attrs
                    .get("start")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1)
                    .checked_add(node.child_count() as i64)
                    == Some(start)
        });
        let (start, tokens) = if join {
            (before - 1, vec![open(item, &item_attrs)])
        } else {
            (
                before,
                vec![open(list, &list_attrs), open(item, &item_attrs)],
            )
        };
        Ok(Some(vec![
            Edit::ReplaceText {
                range: DocPos(before + 1)..head,
                text: String::new(),
            },
            Edit::Boundaries(vec![
                edit(start..before, tokens),
                edit(after..after, vec![Boundary::Close, Boundary::Close]),
            ]),
        ]))
    }
}

#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{ListItemRow, list_item_body};
