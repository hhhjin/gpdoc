use crate::TableMap;
use gpdoc_plugin::core::{Affinity, PositionMap, SelectionBehavior};
use gpdoc_plugin::prelude::{
    CommandContext, DocPos, Document, Edit, Error, Node, Selection, Transaction,
};
use std::ops::Range;

#[derive(Debug)]
struct CellSelection;

pub(crate) const CELL_SELECTION: &dyn SelectionBehavior = &CellSelection;

pub fn cell_selection(anchor: DocPos, head: DocPos) -> Selection {
    Selection::Custom {
        kind: CELL_SELECTION,
        anchor,
        head,
    }
}

pub fn selected_cell_endpoints(selection: Selection) -> Option<(DocPos, DocPos)> {
    match selection {
        Selection::Custom { kind, anchor, head } if kind == CELL_SELECTION => Some((anchor, head)),
        _ => None,
    }
}

fn cell_content(doc: &Document, before: DocPos) -> Option<Range<DocPos>> {
    let cell = doc.node_at(before)?;
    Some(DocPos(before.0 + 1)..DocPos(before.0 + cell.size() - 1))
}

impl SelectionBehavior for CellSelection {
    fn validate(&self, doc: &Document, anchor: DocPos, head: DocPos) -> Result<(), Error> {
        let table = |before: DocPos| {
            let pos = doc.resolve(before).ok()?;
            if !matches!(
                doc.node_at(before)?.kind.as_str(),
                "tableCell" | "tableHeader"
            ) || pos.depth() < 2
                || pos.parent().kind != "tableRow"
                || pos.node(pos.depth() - 1).kind != "table"
            {
                return None;
            }
            Some(pos.start(pos.depth() - 1))
        };
        if table(anchor).is_some_and(|start| Some(start) == table(head)) {
            Ok(())
        } else {
            Err(Error::InvalidEdit(
                "cell selection requires cells in the same table".into(),
            ))
        }
    }

    fn bounds(&self, doc: &Document, anchor: DocPos, head: DocPos) -> Range<DocPos> {
        let last = anchor.max(head);
        anchor.min(head)..DocPos(last.0 + doc.node_at(last).map_or(0, Node::size))
    }

    fn ranges(&self, doc: &Document, anchor: DocPos, head: DocPos) -> Vec<Range<DocPos>> {
        TableMap::selected_cells(doc, anchor, head)
            .into_iter()
            .flatten()
            .filter_map(|before| cell_content(doc, before))
            .collect()
    }

    fn primary_range(&self, doc: &Document, _anchor: DocPos, head: DocPos) -> Range<DocPos> {
        cell_content(doc, head).unwrap_or(head..head)
    }

    fn map(
        &self,
        doc: &Document,
        anchor: DocPos,
        head: DocPos,
        map: &PositionMap,
    ) -> Option<(DocPos, DocPos)> {
        let anchor = map.map(anchor, Affinity::After);
        let head = map.map(head, Affinity::After);
        if anchor.deleted || head.deleted {
            return None;
        }
        self.validate(doc, anchor.position, head.position).ok()?;
        Some((anchor.position, head.position))
    }

    fn prepare_input(
        &self,
        ctx: CommandContext<'_>,
        anchor: DocPos,
        head: DocPos,
    ) -> Result<Transaction, Error> {
        let doc = ctx.document();
        let positions = TableMap::selected_cells(doc, anchor, head)
            .ok_or_else(|| Error::InvalidEdit("invalid table selection".into()))?;
        let paragraph = Node::text(ctx.config().default_text_block(), "");
        let mut target = head.0 + 2;
        let mut tx = ctx.transaction();
        // Replacing in reverse order keeps all remaining ranges relative to this document.
        for before in positions.into_iter().rev() {
            let cell = doc.node_at(before).ok_or(Error::InvalidPosition(before))?;
            if cell.content.children() == std::slice::from_ref(&paragraph) {
                continue;
            }
            let start = DocPos(before.0 + 1);
            let end = DocPos(before.0 + cell.size() - 1);
            if before < head {
                target = target - (end.0 - start.0) + paragraph.size();
            }
            tx.push(Edit::ReplaceNodes {
                range: start..end,
                nodes: vec![paragraph.clone()],
            });
        }
        tx.select(Selection::caret(DocPos(target)));
        Ok(tx)
    }
}
