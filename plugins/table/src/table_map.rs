//! Locations of cells in a Tiptap table, including cells spanning multiple slots.
use gpdoc_plugin::prelude::{ContentRef, DocPos, Document, Node};

/// A slot stores an index into `cells`, in document order.
pub struct TableMap {
    pub slots: Vec<Vec<usize>>,
    pub cells: Vec<DocPos>,
}

impl TableMap {
    pub fn read(table: &Node, before: DocPos) -> Option<Self> {
        Self::build(table, before, false)
    }

    /// Clamp spans to the table and assign fresh indices to missing cells.
    /// The new indices follow the document cells listed in `cells`.
    pub fn for_repair(table: &Node, before: DocPos) -> Option<Self> {
        Self::build(table, before, true)
    }

    fn build(table: &Node, before: DocPos, repair: bool) -> Option<Self> {
        if table.kind != "table" {
            return None;
        }
        let ContentRef::Blocks(rows) = table.content() else {
            return None;
        };
        let mut slots = vec![Vec::new(); rows.len()];
        let mut cells = Vec::new();
        let mut width = 0;
        let mut pos = before.0 + 1;
        for (r, row) in rows.iter().enumerate() {
            let ContentRef::Blocks(children) = row.content() else {
                return None;
            };
            pos += 1;
            let mut col = 0;
            for cell in children {
                while slots[r].get(col).is_some_and(|id| *id != usize::MAX) {
                    col += 1;
                }
                let span = |name: &str| {
                    let Some(value) = cell.attrs.get(name) else {
                        return Some(1);
                    };
                    value
                        .as_i64()
                        .and_then(|n| usize::try_from(n).ok())
                        .filter(|&n| n > 0)
                        .or_else(|| repair.then_some(1))
                };
                let mut right = col.checked_add(span("colspan")?)?;
                let mut bottom = r.checked_add(span("rowspan")?)?;
                if repair {
                    bottom = bottom.min(rows.len());
                    for line in &slots[r..bottom] {
                        if let Some(collision) = line
                            .iter()
                            .enumerate()
                            .skip(col)
                            .take(right - col)
                            .find(|(_, id)| **id != usize::MAX)
                        {
                            right = collision.0;
                        }
                    }
                }
                width = width.max(right);
                // Bound expansion of potentially very large spans in imported documents.
                if width.checked_mul(rows.len())? > Self::MAX_SLOTS || bottom > rows.len() {
                    return None;
                }
                for line in &mut slots[r..bottom] {
                    if line.len() < right {
                        line.resize(right, usize::MAX);
                    }
                    for slot in &mut line[col..right] {
                        if *slot != usize::MAX {
                            return None;
                        }
                        *slot = cells.len();
                    }
                }
                cells.push(DocPos(pos));
                pos += cell.size();
                col = right;
            }
            pos += 1;
        }
        if repair {
            width = width.max(1);
            if width.checked_mul(rows.len())? > Self::MAX_SLOTS {
                return None;
            }
            let mut next = cells.len();
            for row in &mut slots {
                row.resize(width, usize::MAX);
                for slot in row {
                    if *slot == usize::MAX {
                        *slot = next;
                        next += 1;
                    }
                }
            }
        } else if width == 0
            || slots
                .iter()
                .any(|r| r.len() != width || r.contains(&usize::MAX))
        {
            return None;
        }
        Some(Self { slots, cells })
    }
    /// Document positions of all distinct cells in a rectangular selection.
    pub fn selected_cells(
        doc: &Document,
        anchor: DocPos,
        head: DocPos,
    ) -> Option<std::collections::BTreeSet<DocPos>> {
        let pos = doc.resolve(anchor).ok()?;
        let depth = (1..=pos.depth())
            .rev()
            .find(|&d| pos.node(d).kind == "table")?;
        let before = DocPos(pos.before(depth).ok()?);
        let map = Self::read(doc.node_at(before)?, before)?;
        let bounds = |p| Self::rect(&map.slots, map.cells.binary_search(&p).ok()?);
        let (top, left, bottom, right) = bounds(anchor)?;
        let (other_top, other_left, other_bottom, other_right) = bounds(head)?;
        Some(
            map.slots[top.min(other_top)..bottom.max(other_bottom)]
                .iter()
                .flat_map(|row| &row[left.min(other_left)..right.max(other_right)])
                .map(|&id| map.cells[id])
                .collect(),
        )
    }

    pub const MAX_SLOTS: usize = 100_000;

    /// Half-open top, left, bottom, right bounds in logical cell slots.
    pub fn rect(slots: &[Vec<usize>], id: usize) -> Option<(usize, usize, usize, usize)> {
        for (r, row) in slots.iter().enumerate() {
            if let Some(c) = row.iter().position(|slot| *slot == id) {
                let right = c + row[c..].iter().take_while(|s| **s == id).count();
                let bottom = r + slots[r..]
                    .iter()
                    .take_while(|line| line.get(c) == Some(&id))
                    .count();
                return Some((r, c, bottom, right));
            }
        }
        None
    }
}
