use gpdoc_plugin::core::{Boundary, BoundaryEdit};
use gpdoc_plugin::prelude::{
    CommandContext, ContentRef, DocPos, Edit, Error, Plugin, Selection, Transaction, unit_command,
};
use gpdoc_plugin::selection::children_at;
use gpdoc_plugin_list_item::{lift_item, selected_item};

unit_command!(pub Backspace, "listBackspace");
unit_command!(pub Delete, "listDelete");
unit_command!(pub Tab, "listTab");

pub fn list_keymap() -> Plugin {
    Plugin::new("listKeymap", |p| {
        p.command::<Backspace>(|ctx, _| join_items(ctx, false));
        p.shortcut("Backspace", Backspace);
        p.shortcut("Mod-Backspace", Backspace);
        p.command::<Delete>(|ctx, _| join_items(ctx, true));
        p.shortcut("Delete", Delete);
        p.shortcut("Mod-Delete", Delete);
        p.command::<Tab>(|ctx, _| tab_after_list(ctx));
        p.shortcut("Tab", Tab);
    })
}

fn join_items(ctx: CommandContext<'_>, forward: bool) -> Result<Option<Transaction>, Error> {
    let Selection::Text { anchor, head } = ctx.selection() else {
        return Ok(None);
    };
    if anchor != head {
        return Ok(None);
    }
    let Some((list, item)) = ["listItem", "taskItem"]
        .iter()
        .find_map(|kind| selected_item(ctx, kind))
    else {
        return Ok(None);
    };
    let ContentRef::Blocks(items) = list.node.content() else {
        return Ok(None);
    };
    let ContentRef::Blocks(blocks) = item.node.content() else {
        return Ok(None);
    };
    let Some(first) = blocks.first() else {
        return Ok(None);
    };
    let pos = ctx.document().resolve(head)?;
    if pos.parent().kind != "paragraph" {
        return Ok(None);
    }
    if !forward && head.0 == item.before + 2 && item.before == list.before + 1 {
        return lift_item(ctx, &item.node.kind);
    }
    let boundary = if forward && head.0 + 2 == item.end() {
        item.end()
    } else if !forward && head.0 == item.before + 2 {
        item.before
    } else {
        return Ok(None);
    };
    let mut adjacent = children_at(items, list.before + 1);
    let Some(left) = adjacent.find(|n| n.end() == boundary) else {
        return Ok(None);
    };
    let Some(right) = adjacent.next() else {
        return Ok(None);
    };
    let (ContentRef::Blocks(left_blocks), ContentRef::Blocks(right_blocks)) =
        (left.node.content(), right.node.content())
    else {
        return Ok(None);
    };
    if first.kind != "paragraph"
        || !left_blocks.last().is_some_and(|b| b.kind == "paragraph")
        || !right_blocks.first().is_some_and(|b| b.kind == "paragraph")
    {
        return Ok(None);
    }
    let mut tx = ctx.transaction();
    tx.push(Edit::Boundaries(vec![BoundaryEdit {
        range: DocPos(boundary - 2)..DocPos(boundary + 2),
        insert: vec![],
    }]));
    tx.select(Selection::caret(DocPos(boundary - 2)));
    Ok(Some(tx))
}

fn tab_after_list(ctx: CommandContext<'_>) -> Result<Option<Transaction>, Error> {
    let Selection::Text { anchor, head } = ctx.selection() else {
        return Ok(None);
    };
    if anchor != head {
        return Ok(None);
    }
    let schema = ctx.schema();
    let pos = ctx.document().resolve(head)?;
    if pos.depth() == 0 || pos.parent_offset != 0 {
        return Ok(None);
    }
    if ["listItem", "taskItem"]
        .iter()
        .any(|k| selected_item(ctx, k).is_some())
    {
        return Ok(None);
    }
    let start = pos.before(pos.depth())?;
    let end = pos.after(pos.depth())?;
    let Some(list) = ctx.document().root().resolve(start)?.node_before()? else {
        return Ok(None);
    };
    if !["orderedList", "bulletList", "taskList"].contains(&list.kind.as_str()) {
        return Ok(None);
    }
    let Some(item) = list.content.children().last() else {
        return Ok(None);
    };
    if !item.can_replace_with(
        schema,
        item.child_count(),
        item.child_count(),
        &pos.parent().kind,
        &[],
    ) {
        return Ok(None);
    }
    let mut tx = ctx.transaction();
    tx.push(Edit::Boundaries(vec![
        BoundaryEdit {
            range: DocPos(start - 2)..DocPos(start),
            insert: vec![],
        },
        BoundaryEdit {
            range: DocPos(end)..DocPos(end),
            insert: vec![Boundary::Close, Boundary::Close],
        },
    ]));
    tx.select(Selection::caret(DocPos(start - 1)));
    Ok(Some(tx))
}
