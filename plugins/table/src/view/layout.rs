//! Resolve table slots into editable GPUI rows and shared tracks for spanning cells.
mod large_band;

use super::{CellRenderer, TableCell};
use crate::{TableMap, selected_cell_endpoints};
use gpdoc::{EditableChild, NodeView};
use gpdoc_plugin::core::{ContentRef, DocPos, Node, Selection, Value};
use gpui::{AnyElement, App, Div, Pixels, Window, div, prelude::*, px};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::ops::Range;

pub(super) const PADDING: f32 = 4.;
pub(super) const MIN_WIDTH: f32 = 24.;

#[derive(Clone)]
struct CellArea {
    rows: Range<usize>,
    cols: Range<usize>,
}

impl CellArea {
    fn union(&self, other: &Self) -> Self {
        Self {
            rows: self.rows.start.min(other.rows.start)..self.rows.end.max(other.rows.end),
            cols: self.cols.start.min(other.cols.start)..self.cols.end.max(other.cols.end),
        }
    }

    fn intersects(&self, other: &Self) -> bool {
        self.rows.start < other.rows.end
            && other.rows.start < self.rows.end
            && self.cols.start < other.cols.end
            && other.cols.start < self.cols.end
    }
}

#[derive(Default, Clone, Copy)]
struct ColumnSize {
    fixed: f32,
    flexible: usize,
}

impl ColumnSize {
    fn element(self) -> Div {
        let mut el = div()
            .flex_basis(px(self.fixed))
            .min_w(px(self.fixed + MIN_WIDTH * self.flexible as f32))
            .flex_shrink_0();
        el.style().flex_grow = Some(self.flexible as f32);
        el
    }
}

pub(super) struct TableLayout {
    rows: Vec<DocPos>,
    positions: Vec<DocPos>,
    areas: Vec<CellArea>,
    // Prefix sums give a spanning cell's fixed and flexible widths in constant time.
    columns: Vec<ColumnSize>,
}

impl TableLayout {
    pub(super) fn read(table: &Node, before: DocPos) -> Option<Self> {
        let map = TableMap::read(table, before)?;
        let mut widths = vec![None; map.slots.first()?.len()];
        let mut areas = vec![
            CellArea {
                rows: 0..0,
                cols: 0..0
            };
            map.cells.len()
        ];
        for (row, slots) in map.slots.iter().enumerate() {
            for (col, &cell) in slots.iter().enumerate() {
                let area = &mut areas[cell];
                if area.rows.is_empty() {
                    area.rows.start = row;
                    area.cols.start = col;
                }
                area.rows.end = row + 1;
                area.cols.end = col + 1;
            }
        }
        let ContentRef::Blocks(rows) = table.content() else {
            return None;
        };
        let cells = rows
            .iter()
            .filter_map(|row| match row.content() {
                ContentRef::Blocks(cells) => Some(cells),
                _ => None,
            })
            .flatten();
        for (cell, area) in cells.zip(&areas) {
            if let Some(values) = cell.attrs.get("colwidth").and_then(Value::as_array) {
                for (width, value) in widths[area.cols.clone()].iter_mut().zip(values) {
                    if let Some(w) = value.as_i64().filter(|&w| w > 0) {
                        *width = Some((w as f32).max(MIN_WIDTH));
                    }
                }
            }
        }
        let mut columns = Vec::with_capacity(widths.len() + 1);
        let mut sum = ColumnSize::default();
        columns.push(sum);
        for width in widths {
            match width {
                Some(w) => sum.fixed += w,
                None => sum.flexible += 1,
            }
            columns.push(sum);
        }
        let rows = gpdoc_plugin::selection::children_at(rows, before.0 + 1)
            .map(|row| DocPos(row.before))
            .collect();
        Some(Self {
            rows,
            positions: map.cells,
            areas,
            columns,
        })
    }

    fn size(&self, cols: Range<usize>) -> ColumnSize {
        let (left, right) = (self.columns[cols.start], self.columns[cols.end]);
        ColumnSize {
            fixed: right.fixed - left.fixed,
            flexible: right.flexible - left.flexible,
        }
    }

    // Rows without a vertical span use one flex row. Only connected spanning rows
    // share measurements, so unrelated rows can be measured independently.
    fn bands(&self) -> Vec<Range<usize>> {
        let mut ends: Vec<_> = (1..=self.rows.len()).collect();
        for area in &self.areas {
            ends[area.rows.start] = ends[area.rows.start].max(area.rows.end);
        }
        let mut bands = Vec::new();
        let (mut first, mut end) = (0, 0);
        for (i, row_end) in ends.into_iter().enumerate() {
            end = end.max(row_end);
            if i + 1 == end {
                bands.push(first..end);
                first = end;
            }
        }
        bands
    }

    fn cell(
        body: Div,
        child: EditableChild,
        area: &CellArea,
        selection: Option<&CellArea>,
        render_cell: &CellRenderer,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let cell = render_cell(
            TableCell {
                node: child.node().clone(),
                before: child.before(),
                rows: area.rows.clone(),
                columns: area.cols.clone(),
                selected: selection.is_some_and(|sel| area.intersects(sel)),
                track: body.relative(),
                content: div(),
            },
            window,
            cx,
        );
        child.render_with(window, cx, |editable| {
            cell.track.child(cell.content.child(editable))
        })
    }

    fn selection(&self, sel: Selection) -> Option<CellArea> {
        let (anchor, head) = selected_cell_endpoints(sel)?;
        let index = |pos| self.positions.binary_search(&pos).ok();
        Some(self.areas[index(anchor)?].union(&self.areas[index(head)?]))
    }

    pub(super) fn resize_track(&self, cell: DocPos, measured: Pixels) -> Option<(usize, Pixels)> {
        let range = &self.areas[self.positions.binary_search(&cell).ok()?].cols;
        let last = range.end - 1;
        let segment = self.size(last..last + 1);
        let width = if segment.flexible == 0 {
            px(segment.fixed)
        } else {
            let size = self.size(range.clone());
            (measured - px(size.fixed)) / size.flexible as f32
        };
        Some((last, width.max(px(MIN_WIDTH))))
    }
}

// Taffy's browser backend clamps implicit tracks to 10,000 in each direction.
// Larger connected spans are positioned from measured row bounds instead.
const MAX_GRID_ROWS: usize = 10_000;

fn grid_item(rows: Range<usize>) -> Div {
    let line = |index: usize| i16::try_from(index + 1).expect("bounded native grid");
    div()
        .col_start(1)
        .col_end(2)
        .row_start(line(rows.start))
        .row_end(line(rows.end))
}

pub(super) fn render(
    slot: NodeView<'_>,
    rows: Vec<EditableChild>,
    render_cell: &CellRenderer,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let Some(layout) = TableLayout::read(slot.node, slot.before) else {
        return div()
            .flex()
            .flex_col()
            .children(rows.into_iter().map(|row| row.render(window, cx)))
            .into_any_element();
    };
    let selection = layout.selection(slot.view.read(cx).editor().selection());
    let mut table = div().w_full().flex().flex_col();
    let bands = layout.bands();
    let visible_positions: Vec<_> = if bands.iter().any(|band| band.len() > MAX_GRID_ROWS) {
        rows.iter().map(EditableChild::before).collect()
    } else {
        Vec::new()
    };
    let minimum = window.line_height() + px(2. * PADDING);
    let mut visible_rows = rows.into_iter().peekable();
    let mut areas = layout.positions.iter().zip(&layout.areas).peekable();
    for band in bands {
        let mut band_rows = Vec::with_capacity(band.len());
        let oversized = band.len() > MAX_GRID_ROWS;
        let mut spans = large_band::Spans::default();
        let mut grid_spans = Vec::new();
        for i in band.clone() {
            let Some(row) = visible_rows.next_if(|row| row.before() == layout.rows[i]) else {
                if band.len() > 1 {
                    band_rows.push(div().into_any_element());
                }
                continue;
            };
            let (wrapper, cells) = match row.split() {
                Ok(parts) => parts,
                Err(row) => {
                    let body = row.render(window, cx);
                    band_rows.push(if band.len() == 1 {
                        body
                    } else {
                        div().min_h(minimum).child(body).into_any_element()
                    });
                    continue;
                }
            };
            if cells.is_empty() && !wrapper.has_wrapper() && band.len() > 1 {
                let empty = div().min_h(minimum);
                band_rows.push(wrapper.wrap(empty.into_any_element(), window, cx));
                continue;
            }
            let mut ordinary = div().w_full().flex();
            let mut measure = div().w_full().flex();
            let mut next_cell = 0;
            let mut next_span = 0;
            for child in cells {
                // Folded slots still occupy logical table positions.
                while areas.peek().is_some_and(|(pos, _)| **pos < child.before()) {
                    areas.next();
                }
                let (_, area) = areas
                    .next()
                    .expect("table projection covers every visible cell");
                let track = layout.size(area.cols.clone()).element();
                if area.rows.len() > 1 {
                    let covered = area.rows.start - band.start..area.rows.end - band.start;
                    if oversized {
                        let body = TableLayout::cell(
                            div().size_full(),
                            child,
                            area,
                            selection.as_ref(),
                            render_cell,
                            window,
                            cx,
                        );
                        let first = visible_positions
                            .partition_point(|&before| before <= layout.rows[area.rows.start]);
                        let end = visible_positions
                            .partition_point(|&before| before <= layout.rows[area.rows.end - 1]);
                        let intrinsic =
                            spans.measure(covered, body, minimum * (end - first) as f32);
                        measure = measure
                            .child(layout.size(next_span..area.cols.start).element())
                            .child(track.child(intrinsic));
                        next_span = area.cols.end;
                    } else {
                        let body = TableLayout::cell(
                            track,
                            child,
                            area,
                            selection.as_ref(),
                            render_cell,
                            window,
                            cx,
                        );
                        grid_spans.push(
                            grid_item(covered)
                                .flex()
                                .child(layout.size(0..area.cols.start).element())
                                .child(body)
                                .child(
                                    layout
                                        .size(area.cols.end..layout.columns.len() - 1)
                                        .element(),
                                ),
                        );
                    }
                } else {
                    if next_cell < area.cols.start {
                        ordinary =
                            ordinary.child(layout.size(next_cell..area.cols.start).element());
                    }
                    ordinary = ordinary.child(TableLayout::cell(
                        track,
                        child,
                        area,
                        selection.as_ref(),
                        render_cell,
                        window,
                        cx,
                    ));
                    next_cell = area.cols.end;
                }
            }
            ordinary = ordinary.child(layout.size(next_cell..layout.columns.len() - 1).element());
            let body = wrapper.wrap(ordinary.into_any_element(), window, cx);
            if band.len() == 1 {
                band_rows.push(body);
            } else if next_span > 0 {
                measure = measure.child(layout.size(next_span..layout.columns.len() - 1).element());
                band_rows.push(
                    div()
                        .w_full()
                        .min_h(minimum)
                        .grid()
                        .grid_cols(1)
                        .child(grid_item(0..1).child(body))
                        .child(grid_item(0..1).child(measure))
                        .into_any_element(),
                );
            } else {
                band_rows.push(div().w_full().min_h(minimum).child(body).into_any_element());
            }
        }
        if band.len() == 1 {
            table = table.children(band_rows);
        } else if oversized {
            table = table.child(spans.overlay(band_rows));
        } else {
            let grid = div().w_full().grid().grid_cols(1);
            let rows = band_rows
                .into_iter()
                .enumerate()
                .map(|(i, body)| grid_item(i..i + 1).child(body));
            table = table.child(grid.children(rows).children(grid_spans));
        }
    }
    table.into_any_element()
}
