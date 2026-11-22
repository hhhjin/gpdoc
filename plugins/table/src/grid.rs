//! A logical table slot contains the index of its owning cell. Spanning cells
//! occupy several slots but are emitted into the document only at their origin.
use super::{empty_cell, selection_range};
use crate::{TableMap, selected_cell_endpoints};
use gpdoc_plugin::prelude::{
    Attrs, CommandContext, Content, ContentRef, DocPos, Edit, Error, Node, Selection, Step,
    Transaction, Value,
};
use gpdoc_plugin::selection::Located;
use gpdoc_plugin::selection::path_at;

#[derive(Clone, Copy)]
pub(super) enum Operation<'a> {
    AddColumn(bool),
    AddRow(bool),
    DeleteColumn,
    DeleteRow,
    HeaderRow,
    HeaderColumn,
    HeaderCell,
    SetAttribute(&'a str, &'a Value),
    Move(bool),
    Split,
    Merge,
    MergeOrSplit,
}

#[derive(Clone, Debug)]
pub struct ResizeColumn {
    pub cell: usize,
    pub column: usize,
    pub width: i64,
}

#[derive(Clone)]
struct Cell {
    node: Node,
    before: Option<usize>,
}

pub(super) struct Grid {
    before: usize,
    table: Node,
    cells: Vec<Cell>,
    slots: Vec<Vec<usize>>,
    row_attrs: Vec<Attrs>,
    widths: Vec<i64>,
}

fn blocks(node: &Node) -> Option<&[Node]> {
    match node.content() {
        ContentRef::Blocks(nodes) => Some(nodes),
        _ => None,
    }
}
impl Grid {
    fn read(table: Located<'_>) -> Option<Self> {
        Self::from_map(table, TableMap::read(table.node, DocPos(table.before))?)
    }
    fn from_map(table: Located<'_>, map: TableMap) -> Option<Self> {
        let rows = blocks(table.node)?;
        let mut cells: Vec<_> = rows
            .iter()
            .flat_map(|r| blocks(r).into_iter().flatten())
            .zip(map.cells)
            .map(|(node, pos)| Cell {
                node: node.clone(),
                before: Some(pos.0),
            })
            .collect();
        if let Some(&max) = map.slots.iter().flatten().max() {
            cells.resize_with(max + 1, || Cell {
                node: empty_cell(false),
                before: None,
            });
        }
        let mut widths = vec![0; map.slots[0].len()];
        let mut votes = vec![0; widths.len()];
        for row in &map.slots {
            let mut start = 0;
            for (col, &id) in row.iter().enumerate() {
                if col == 0 || row[col - 1] != id {
                    start = col;
                }
                let Some(values) = cells[id].node.attrs.get("colwidth") else {
                    continue;
                };
                let Some(width) = values
                    .get(col - start)
                    .and_then(|w| w.as_i64())
                    .filter(|w| *w > 0)
                else {
                    continue;
                };
                if widths[col] == width {
                    votes[col] += 1;
                } else if votes[col] <= 1 {
                    widths[col] = width;
                    votes[col] = 1;
                }
            }
        }
        Some(Self {
            widths,
            before: table.before,
            table: table.node.clone(),
            cells,
            slots: map.slots,
            row_attrs: rows.iter().map(|r| r.attrs.clone()).collect(),
        })
    }
    pub(super) fn repairs(doc: &gpdoc_plugin::prelude::Document) -> Vec<Edit> {
        let mut steps = Vec::new();
        let mut enclosing_end = 0;
        for (before, node) in doc.iter_blocks().filter(|(_, n)| n.kind == "table") {
            if before.0 < enclosing_end || TableMap::read(node, before).is_some() {
                continue;
            }
            let Some(map) = TableMap::for_repair(node, before) else {
                continue;
            };
            let Some(grid) = Self::from_map(
                Located {
                    before: before.0,
                    node,
                },
                map,
            ) else {
                continue;
            };
            let (rows, _) = grid.document_rows();
            enclosing_end = before.0 + node.size();
            for (row, updated) in
                gpdoc_plugin::selection::children_at(blocks(node).unwrap(), before.0 + 1).zip(rows)
            {
                let old = blocks(row.node).unwrap();
                let new = blocks(&updated).unwrap();
                let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
                let suffix = old[prefix..]
                    .iter()
                    .rev()
                    .zip(new[prefix..].iter().rev())
                    .take_while(|(a, b)| a == b)
                    .count();
                if prefix + suffix != old.len() || prefix + suffix != new.len() {
                    let start =
                        row.before + 1 + old[..prefix].iter().map(Node::size).sum::<usize>();
                    let end = start
                        + old[prefix..old.len() - suffix]
                            .iter()
                            .map(Node::size)
                            .sum::<usize>();
                    steps.push(Edit::ReplaceNodes {
                        range: DocPos(start)..DocPos(end),
                        nodes: new[prefix..new.len() - suffix].to_vec(),
                    });
                }
            }
        }
        steps.reverse();
        steps
    }
    fn cell_at(&self, pos: usize) -> Option<usize> {
        self.cells.iter().rposition(|cell| {
            cell.before
                .is_some_and(|before| before <= pos && pos < before + cell.node.size())
        })
    }
    fn rect(&self, id: usize) -> Option<(usize, usize, usize, usize)> {
        TableMap::rect(&self.slots, id)
    }
    fn header(&self, id: usize) -> bool {
        self.cells[id].node.kind == "tableHeader"
    }
    fn allocate(&mut self, node: Node) -> usize {
        let id = self.cells.len();
        self.cells.push(Cell { node, before: None });
        id
    }
    fn add_axis(&mut self, row_axis: bool, at: usize) -> bool {
        // Transposing lets rows and columns share the same span and header rules.
        if !row_axis {
            self.transpose();
        }
        let width = self.slots[0].len();
        let height = self.slots.len();
        if width.saturating_mul(height + 1) > TableMap::MAX_SLOTS {
            return false;
        }
        let preferred = at.saturating_sub(1);
        let reference = if self.slots[preferred].iter().all(|id| self.header(*id)) {
            (at > 0 && at < height).then_some(at)
        } else {
            Some(preferred)
        };
        let mut new = Vec::with_capacity(width);
        for i in 0..width {
            if at > 0 && at < height && self.slots[at - 1][i] == self.slots[at][i] {
                new.push(self.slots[at][i]);
            } else {
                let header = reference.is_some_and(|row| self.header(self.slots[row][i]));
                new.push(self.allocate(empty_cell(header)));
            }
        }
        self.slots.insert(at, new);
        if row_axis {
            self.row_attrs.insert(at, Attrs::new());
        } else {
            self.widths.insert(at, 0);
            self.transpose();
        }
        true
    }
    fn transpose(&mut self) {
        self.slots = (0..self.slots[0].len())
            .map(|c| self.slots.iter().map(|r| r[c]).collect())
            .collect();
    }
    fn set_header(&mut self, id: usize, header: bool) {
        let node = &self.cells[id].node;
        self.cells[id].node = Node::new(
            if header { "tableHeader" } else { "tableCell" },
            node.attrs.clone(),
            node.content().to_owned(),
        );
    }
    fn toggle_headers(&mut self, row_axis: bool) {
        if !row_axis {
            self.transpose();
        }
        let target = !self.slots[0].iter().all(|id| self.header(*id));
        let skip = usize::from(self.slots.iter().all(|r| self.header(r[0])));
        let ids = self.slots[0][skip..].to_vec();
        for id in ids {
            self.set_header(id, target);
        }
        if !row_axis {
            self.transpose();
        }
    }
    fn document_rows(&self) -> (Vec<Node>, Vec<Option<usize>>) {
        let mut rows = Vec::new();
        let mut positions = vec![None; self.cells.len()];
        let mut pos = self.before + 1;
        for (r, row) in self.slots.iter().enumerate() {
            let mut nodes = Vec::new();
            pos += 1;
            for (c, &id) in row.iter().enumerate() {
                if positions[id].is_some() {
                    continue;
                }
                let cell = &self.cells[id].node;
                let mut attrs = cell.attrs.clone();
                let cols = row[c..].iter().take_while(|s| **s == id).count();
                let rows = self.slots[r..]
                    .iter()
                    .take_while(|line| line[c] == id)
                    .count();
                let widths = &self.widths[c..c + cols];
                attrs.insert(
                    "colwidth".into(),
                    if widths.iter().any(|w| *w > 0) {
                        widths.to_vec().into()
                    } else {
                        Value::Null
                    },
                );
                attrs.insert("colspan".into(), Value::from(cols as i64));
                attrs.insert("rowspan".into(), Value::from(rows as i64));
                let node = Node::new(&cell.kind, attrs, cell.content().to_owned());
                positions[id] = Some(pos);
                pos += node.size();
                nodes.push(node);
            }
            pos += 1;
            rows.push(Node::new(
                "tableRow",
                self.row_attrs[r].clone(),
                Content::Blocks(nodes),
            ));
        }
        (rows, positions)
    }
    fn move_cell(&self, ctx: CommandContext<'_>, id: usize, next: bool) -> Option<Transaction> {
        let other = if next {
            id.checked_add(1)?
        } else {
            id.checked_sub(1)?
        };
        let cell = self.cells.get(other)?;
        let before = cell.before?;
        let mut inside = ctx.document().iter_blocks().filter(|(pos, n)| {
            pos.0 > before && pos.0 < before + cell.node.size() && n.inline_content()
        });
        let (first, node) = inside.next()?;
        let last = inside.last().unwrap_or((first, node));
        let mut tx = ctx.transaction();
        tx.select(Selection::Text {
            anchor: DocPos(first.0 + 1),
            head: DocPos(last.0.0 + last.1.size() - 1),
        });
        Some(tx)
    }
    pub(super) fn resize_column(
        ctx: CommandContext<'_>,
        args: ResizeColumn,
    ) -> Result<Option<Transaction>, Error> {
        let ResizeColumn {
            cell,
            column: col,
            width,
        } = args;
        if width <= 0 {
            return Err(Error::InvalidCommandArguments {
                command: "setTableColumnWidth".into(),
                reason: "width must be positive".into(),
            });
        }
        let path = path_at(ctx.document().blocks(), DocPos(cell));
        let Some(table) = path.iter().rev().find(|n| n.node.kind == "table") else {
            return Ok(None);
        };
        let Some(grid) = Self::read(*table) else {
            return Ok(None);
        };
        if col >= grid.slots[0].len() {
            return Ok(None);
        }
        let mut tx = ctx.transaction();
        let mut previous = None;
        for row in &grid.slots {
            let id = row[col];
            if previous == Some(id) {
                continue;
            }
            previous = Some(id);
            let first = row[..col].iter().rev().take_while(|v| **v == id).count();
            let count = first + row[col..].iter().take_while(|v| **v == id).count();
            let cell = &grid.cells[id];
            let mut widths = match cell.node.attrs.get("colwidth") {
                Some(json) => json
                    .as_array()
                    .map(|a| a.iter().map(|v| v.as_i64().unwrap_or(0)).collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            widths.resize(count, 0);
            widths[first] = width;
            tx.push(Step::set_node_attribute(
                DocPos(cell.before.expect("original cell")),
                "colwidth",
                widths.into(),
            ));
        }
        Ok(Some(tx))
    }
    pub(super) fn command(
        ctx: CommandContext<'_>,
        op: Operation<'_>,
    ) -> Result<Option<Transaction>, Error> {
        let (from, to) = selection_range(ctx);
        let path = path_at(ctx.document().blocks(), DocPos(from));
        let Some(table) = path.iter().rev().find(|n| n.node.kind == "table") else {
            return Ok(None);
        };
        let Some(mut grid) = Self::read(*table) else {
            return Ok(None);
        };
        let Some(id) = grid.cell_at(from) else {
            return Ok(None);
        };
        let (mut r, mut c, mut bottom, mut right) =
            grid.rect(id).expect("selected cell has a slot");
        let mut cell_selection = match selected_cell_endpoints(ctx.selection()) {
            Some((anchor, head)) => {
                let (Some(a), Some(h)) = (grid.cell_at(anchor.0), grid.cell_at(head.0)) else {
                    return Ok(None);
                };
                let (t, l, b, end) = grid
                    .rect(if id == a { h } else { a })
                    .expect("selected cell has a slot");
                r = r.min(t);
                c = c.min(l);
                bottom = bottom.max(b);
                right = right.max(end);
                Some((a, h))
            }
            _ => None,
        };
        let ids: std::collections::BTreeSet<_> = grid.slots[r..bottom]
            .iter()
            .flat_map(|row| row[c..right].iter().copied())
            .collect();
        let multi = cell_selection.is_some_and(|(a, h)| a != h);
        use Operation::*;
        match op {
            Move(next) => return Ok(grid.move_cell(ctx, id, next)),
            Merge | MergeOrSplit if multi => {
                // Refuse a rectangle that cuts through a cell extending outside it.
                let crossing = grid.slots.iter().enumerate().any(|(row, line)| {
                    line.iter().enumerate().any(|(col, id)| {
                        ids.contains(id)
                            && (!(r..bottom).contains(&row) || !(c..right).contains(&col))
                    })
                });
                if crossing {
                    return Ok(None);
                }
                let first = grid.slots[r][c];
                let cell = &grid.cells[first].node;
                let mut content: Vec<Node> = ids
                    .iter()
                    .filter_map(|&i| blocks(&grid.cells[i].node))
                    .filter(
                        |nodes| !matches!(nodes,[p] if p.inline_content() && p.child_count() == 0),
                    )
                    .flatten()
                    .cloned()
                    .collect();
                if content.is_empty() {
                    content = blocks(cell).expect("cell blocks").to_vec();
                }
                grid.cells[first].node =
                    Node::new(&cell.kind, cell.attrs.clone(), Content::Blocks(content));
                for row in &mut grid.slots[r..bottom] {
                    row[c..right].fill(first);
                }
                cell_selection = Some((first, first));
            }
            Merge => return Ok(None),
            AddRow(after) => {
                if !grid.add_axis(true, if after { bottom } else { r }) {
                    return Ok(None);
                }
            }
            AddColumn(after) => {
                if !grid.add_axis(false, if after { right } else { c }) {
                    return Ok(None);
                }
            }
            DeleteRow => {
                if bottom - r == grid.slots.len() {
                    return Ok(None);
                }
                grid.slots.drain(r..bottom);
                grid.row_attrs.drain(r..bottom);
            }
            DeleteColumn => {
                if right - c == grid.slots[0].len() {
                    return Ok(None);
                }
                grid.widths.drain(c..right);
                for row in &mut grid.slots {
                    row.drain(c..right);
                }
            }
            HeaderRow => grid.toggle_headers(true),
            HeaderColumn => grid.toggle_headers(false),
            HeaderCell => {
                let target = !ids.iter().any(|&i| grid.header(i));
                for i in ids {
                    grid.set_header(i, target);
                }
            }
            SetAttribute(name, value) => {
                if !ctx
                    .schema()
                    .node_spec(&grid.cells[id].node.kind)
                    .is_some_and(|spec| spec.attrs.contains_key(name))
                {
                    return Ok(None);
                }
                if ids
                    .iter()
                    .all(|&i| grid.cells[i].node.attrs.get(name) == Some(value))
                {
                    return Ok(None);
                }
                let mut tx = ctx.transaction();
                for i in ids {
                    let cell = &grid.cells[i];
                    if cell.node.attrs.get(name) != Some(value) {
                        tx.push(Step::set_node_attribute(
                            DocPos(cell.before.expect("original cell")),
                            name,
                            value.clone(),
                        ));
                    }
                }
                return Ok(Some(tx));
            }
            Split | MergeOrSplit => {
                if multi || bottom - r == 1 && right - c == 1 {
                    return Ok(None);
                }
                let original = grid.cells[id].node.clone();
                for row in r..bottom {
                    for col in c..right {
                        let mut attrs = original.attrs.clone();
                        if let Some(widths) = attrs.get("colwidth").cloned() {
                            let width = widths.get(col - c).and_then(|w| w.as_i64()).unwrap_or(0);
                            attrs.insert(
                                "colwidth".into(),
                                if width > 0 {
                                    vec![width].into()
                                } else {
                                    Value::Null
                                },
                            );
                        }
                        if (row, col) == (r, c) {
                            grid.cells[id].node =
                                Node::new(&original.kind, attrs, original.content().to_owned());
                        } else {
                            let node = Node::new(
                                &original.kind,
                                attrs,
                                Content::Blocks(vec![Node::text("paragraph", "")]),
                            );
                            grid.slots[row][col] = grid.allocate(node);
                        }
                    }
                }
                if cell_selection.is_some() {
                    cell_selection = Some((grid.slots[r][c], grid.slots[bottom - 1][right - 1]));
                }
            }
        }
        let (rows, positions) = grid.document_rows();
        let original = blocks(&grid.table).expect("table rows");
        let prefix = original
            .iter()
            .zip(&rows)
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = original[prefix..]
            .iter()
            .rev()
            .zip(rows[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let before = grid.before + 1 + original[..prefix].iter().map(Node::size).sum::<usize>();
        let end = before
            + original[prefix..original.len() - suffix]
                .iter()
                .map(Node::size)
                .sum::<usize>();
        let mut tx = ctx.transaction();
        if prefix + suffix != rows.len() || prefix + suffix != original.len() {
            tx.push(Edit::ReplaceNodes {
                range: DocPos(before)..DocPos(end),
                nodes: rows[prefix..rows.len() - suffix].to_vec(),
            });
        }
        let fallback = grid.slots[r.min(grid.slots.len() - 1)][c.min(grid.slots[0].len() - 1)];
        let map = |pos: usize| {
            grid.cell_at(pos)
                .and_then(|id| {
                    positions[id].map(|before| before + pos - grid.cells[id].before.unwrap())
                })
                .unwrap_or_else(|| positions[fallback].unwrap() + 2)
        };
        if let Some((a, h)) = cell_selection {
            match (positions[a], positions[h]) {
                (Some(anchor), Some(head)) => {
                    tx.select(crate::cell_selection(DocPos(anchor), DocPos(head)))
                }
                _ => tx.select(Selection::caret(DocPos(positions[fallback].unwrap() + 2))),
            }
        } else if let Selection::Text { anchor, head } = ctx.selection()
            && to < table.end()
        {
            tx.select(Selection::Text {
                anchor: DocPos(map(anchor.0)),
                head: DocPos(map(head.0)),
            });
        }
        Ok(Some(tx))
    }
}
