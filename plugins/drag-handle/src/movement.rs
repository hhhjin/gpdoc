//! Plan an atomic move at a schema-compatible block boundary.
use gpdoc::BlockDropTarget;
use gpdoc_plugin::core::{
    DocPos, Document, Editor, ReplaceAroundStep, ReplaceStep, ResolvedPos, Selection, Slice, Step,
    Transaction,
};
use std::ops::Range;

pub(super) fn plan(
    editor: &Editor,
    selection: Selection,
    targets: &[BlockDropTarget],
) -> Option<(BlockDropTarget, Transaction)> {
    let doc = editor.document();
    let range = selection.bounds(doc);
    // Adjacent blocks share an insertion position. Only a hit on the dragged
    // content itself should rule out all ancestor edges.
    if targets
        .first()?
        .block_before
        .is_some_and(|before| range.contains(&before))
    {
        return None;
    }
    let source = doc.resolve(range.start).ok()?;
    let slice = doc.root().slice(range.start.0, range.end.0, false).ok()?;
    let width = slice.size();
    let (target, mut tx, new_at) = targets.iter().find_map(|target| {
        let at = target.at.0;
        if (range.start.0..=range.end.0).contains(&at) {
            return None;
        }
        let dest = doc.resolve(target.at).ok()?;
        let positions = range.start.0..range.end.0;
        let mut tx = editor.transaction();
        let new_at = if source.same_parent(&dest) {
            reorder_siblings(doc, positions, at, slice.clone(), &mut tx)?
        } else {
            move_between_parents(doc, &source, positions, at, slice.clone(), &mut tx)?
        };
        Some((*target, tx, new_at))
    })?;
    tx.select(match selection {
        Selection::NodeRange { anchor, head } if anchor > head => Selection::NodeRange {
            anchor: DocPos(new_at + width),
            head: DocPos(new_at),
        },
        Selection::NodeRange { .. } => Selection::NodeRange {
            anchor: DocPos(new_at),
            head: DocPos(new_at + width),
        },
        _ => Selection::Node {
            before: DocPos(new_at),
        },
    });
    Some((target, tx))
}

fn reorder_siblings(
    doc: &Document,
    range: Range<usize>,
    at: usize,
    slice: Slice,
    tx: &mut Transaction,
) -> Option<usize> {
    // Keep intervening siblings mapped through the gap, including their bookmarks.
    let width = slice.size();
    let (gap_from, gap_to, insert, new_at) = if at > range.end {
        (range.end, at, 0, at - width)
    } else {
        (at, range.start, width, at)
    };
    let step = ReplaceAroundStep {
        from: range.start.min(at),
        to: range.end.max(at),
        gap_from,
        gap_to,
        insert,
        slice,
        structure: false,
    };
    step.apply(doc.schema(), doc.root()).ok()?;
    tx.push(Step::ReplaceAround(step));
    Some(new_at)
}

fn move_between_parents(
    doc: &Document,
    source: &ResolvedPos,
    mut removed: Range<usize>,
    at: usize,
    slice: Slice,
    tx: &mut Transaction,
) -> Option<usize> {
    // Insert first so lifting the sole block of a quote can still satisfy doc's block+.
    let width = slice.size();
    let insert = ReplaceStep {
        from: at,
        to: at,
        slice,
        structure: false,
    };
    let inserted = insert.apply(doc.schema(), doc.root()).ok()?;
    for depth in (0..=source.depth()).rev() {
        let shift = if at <= removed.start { width } else { 0 };
        let delete = ReplaceStep {
            from: removed.start + shift,
            to: removed.end + shift,
            slice: Slice::empty(),
            structure: false,
        };
        if delete.apply(doc.schema(), &inserted).is_ok() {
            tx.push(Step::Replace(insert));
            tx.push(Step::Replace(delete));
            return Some(if at >= removed.end {
                at - removed.len()
            } else {
                at
            });
        }
        // A quote or list may require at least one child. Prune it only if it
        // would be empty. Structural slots such as table cells must survive.
        if removed != (source.start(depth)..source.end(depth))
            || !doc
                .schema()
                .node_type(&source.node(depth).kind)?
                .in_group("block")
        {
            return None;
        }
        removed = source.before(depth).ok()?..source.after(depth).ok()?;
        if removed.start < at && at < removed.end {
            return None;
        }
    }
    None
}
