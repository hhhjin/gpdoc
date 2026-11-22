//! Pointer selection, column resizing, and editable layout for native tables.
mod layout;
use crate::{cell_selection, selected_cell_endpoints};
use gpdoc::{DecorationFrame, EditorView, PaintLayer, ViewBehavior, ViewPlugin};
use gpdoc_plugin::core::{DocPos, Document, Node};
use gpui::{
    App, Bounds, Context, Div, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Window,
    div, fill, point, prelude::*, px, rgb, rgba, size,
};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;

use layout::{MIN_WIDTH, TableLayout};
use std::{cell::Cell, ops::Range, rc::Rc};

const RESIZE_TOLERANCE: f32 = 3.;

/// A measured table cell. Layers sit behind its editable document content.
pub struct TableCell {
    pub node: Node,
    pub before: DocPos,
    pub rows: Range<usize>,
    pub columns: Range<usize>,
    pub selected: bool,
    track: Div,
    content: Div,
}

impl TableCell {
    pub fn is_header(&self) -> bool {
        self.node.kind == "tableHeader"
    }

    /// Add a full-cell decoration behind the content, in insertion order.
    pub fn layer(mut self, decoration: Div) -> Self {
        self.track = self.track.child(decoration.absolute().inset_0());
        self
    }

    /// Place the editable subtree inside this container.
    pub fn content(mut self, container: Div) -> Self {
        self.content = container;
        self
    }
}

/// Standard header, cell border, selection fill, and content padding.
pub fn default_cell(mut cell: TableCell, _: &mut Window, _: &mut App) -> TableCell {
    let decoration = div()
        .border_1()
        .border_color(rgb(0xb7bec8))
        .when(cell.is_header(), |header| header.bg(rgb(0xe9edf4)));
    cell = cell.layer(decoration);
    if cell.selected {
        cell = cell.layer(div().bg(rgba(0x659ff355)));
    }
    cell.content(div().p(px(layout::PADDING)))
}

/// Paint the guide at the current column-resize position.
pub fn default_resize_guide(bounds: Bounds<Pixels>, window: &mut Window, _: &mut App) {
    window.paint_quad(fill(bounds, rgb(0x659ff3)));
}

type CellRenderer = dyn Fn(TableCell, &mut Window, &mut App) -> TableCell;
type GuidePainter = dyn Fn(Bounds<Pixels>, &mut Window, &mut App);

/// Compose the editable grid and its resize guide in an application view function.
pub fn table_grid(
    render_cell: impl Fn(TableCell, &mut Window, &mut App) -> TableCell + 'static,
    paint_guide: impl Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static,
) -> ViewPlugin {
    let paint_guide: Rc<GuidePainter> = Rc::new(paint_guide);
    ViewPlugin::new("table", move || TablePointer {
        drag: Cell::default(),
        paint_guide: paint_guide.clone(),
    })
    .child_layout("table", move |node, children, window, cx| {
        layout::render(node, children, &render_cell, window, cx)
    })
    .for_plugin("table")
}

/// Editable grid with selection and resize feedback in the surrounding text color.
pub(super) fn plain_grid() -> ViewPlugin {
    table_grid(
        |cell, window, _| {
            if cell.selected {
                cell.layer(div().bg(window.text_style().color.opacity(0.18)))
            } else {
                cell
            }
        },
        |bounds, window, _| window.paint_quad(fill(bounds, window.text_style().color)),
    )
}

struct CellHit {
    cell: DocPos,
    table: DocPos,
    bounds: Bounds<Pixels>,
}

fn cell_at(view: &mut EditorView, pt: Point<Pixels>, window: &mut Window) -> Option<CellHit> {
    let (before, bounds) = view.child_bounds_at(pt, window)?;
    let doc = view.editor().document();
    // A nested layout may contribute its own hit regions within an editable cell.
    let pos = doc.resolve(before).ok()?;
    if matches!(
        doc.node_at(before)?.kind.as_str(),
        "tableCell" | "tableHeader"
    ) && pos.depth() >= 2
        && pos.parent().kind == "tableRow"
        && pos.node(pos.depth() - 1).kind == "table"
    {
        return Some(CellHit {
            cell: before,
            table: DocPos(pos.before(pos.depth() - 1).ok()?),
            bounds,
        });
    }
    let d = (3..=pos.depth()).rev().find(|&d| {
        matches!(pos.node(d).kind.as_str(), "tableCell" | "tableHeader")
            && pos.node(d - 2).kind == "table"
    })?;
    let cell = DocPos(pos.before(d).ok()?);
    let table = DocPos(pos.before(d - 2).ok()?);
    let bounds = view.child_bounds(cell, window)?;
    Some(CellHit {
        cell,
        table,
        bounds,
    })
}

fn resize_at(
    view: &mut EditorView,
    at: Point<Pixels>,
    window: &mut Window,
) -> Option<(DocPos, usize, Pixels)> {
    let sample = point(at.x - px(RESIZE_TOLERANCE), at.y);
    let hit = cell_at(view, sample, window)?;
    if (hit.bounds.right() - at.x).abs() > px(RESIZE_TOLERANCE) {
        return None;
    }
    let table = view.editor().document().node_at(hit.table)?;
    let layout = TableLayout::read(table, hit.table)?;
    let (col, width) = layout.resize_track(hit.cell, hit.bounds.size.width)?;
    Some((hit.cell, col, width))
}
struct TablePointer {
    drag: Cell<Option<Drag>>,
    paint_guide: Rc<GuidePainter>,
}

#[derive(Clone, Copy)]
enum Drag {
    Select {
        cell: DocPos,
        table: DocPos,
    },
    Resize {
        cell: DocPos,
        column: usize,
        width: Pixels,
        start: Pixels,
        revision: u64,
    },
}

impl ViewBehavior for TablePointer {
    fn vertical_block_boundary(
        &self,
        doc: &Document,
        head: DocPos,
        down: bool,
        neighbor: &dyn Fn(DocPos) -> Option<DocPos>,
    ) -> Option<DocPos> {
        let pos = doc.resolve(head).ok()?;
        let mut boundary = head;
        for depth in (3..=pos.depth()).rev() {
            if !matches!(pos.node(depth).kind.as_str(), "tableCell" | "tableHeader")
                || pos.node(depth - 2).kind != "table"
            {
                continue;
            }
            let cell = DocPos(pos.before(depth).ok()?);
            let end = DocPos(pos.after(depth).ok()?);
            if neighbor(boundary).is_some_and(|next| (cell..end).contains(&next)) {
                break;
            }

            let table = pos.node(depth - 2);
            let map = crate::TableMap::read(table, DocPos(pos.before(depth - 2).ok()?))?;
            let id = map.cells.binary_search(&cell).ok()?;
            let (top, col, bottom, _) = crate::TableMap::rect(&map.slots, id)?;
            let row = if down {
                map.slots.get(bottom)
            } else {
                top.checked_sub(1).and_then(|r| map.slots.get(r))
            };
            if let Some(row) = row {
                let target = map.cells[row[col]];
                return Some(if down {
                    target
                } else {
                    DocPos(target.0 + doc.node_at(target)?.size())
                });
            }

            // Leaving a nested table may also cross the edge of the enclosing cell.
            boundary = DocPos(if down {
                pos.end(depth - 2)
            } else {
                pos.start(depth - 2)
            });
        }
        (boundary != head).then_some(boundary)
    }

    fn pointer_cancel(&self) {
        self.drag.set(None);
    }

    fn mouse_down(
        &self,
        view: &mut EditorView,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        self.drag.set(None);
        if gpdoc::primary_pressed(event.modifiers, cx) || view.is_composing() {
            return false;
        }
        if let Some((cell, column, width)) = resize_at(view, event.position, window) {
            self.drag.set(Some(Drag::Resize {
                cell,
                column,
                width,
                start: event.position.x,
                revision: view.editor().revision(),
            }));
            return true;
        }
        if let Some(hit) = cell_at(view, event.position, window) {
            self.drag.set(Some(Drag::Select {
                cell: hit.cell,
                table: hit.table,
            }));
        }
        false
    }

    fn mouse_move(
        &self,
        view: &mut EditorView,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        if !event.dragging() {
            return false;
        }
        match self.drag.get() {
            Some(Drag::Resize { .. }) => {
                cx.notify();
                true
            }
            Some(Drag::Select { cell, table }) => {
                let Some(hit) = cell_at(view, event.position, window) else {
                    return false;
                };
                if hit.table != table
                    || hit.cell == cell
                        && selected_cell_endpoints(view.editor().selection()).is_none()
                {
                    return false;
                }
                view.select_document(cell_selection(cell, hit.cell), window, cx)
                    .is_ok()
            }
            None => false,
        }
    }

    fn mouse_up(
        &self,
        view: &mut EditorView,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        let Some(Drag::Resize {
            cell,
            column,
            width,
            start,
            revision,
        }) = self.drag.take()
        else {
            return false;
        };
        if view.editor().revision() == revision && (start - event.position.x).abs() > px(1.) {
            let width =
                f32::from((width + event.position.x - start).max(px(MIN_WIDTH))).round() as i64;
            let command = crate::ResizeColumn {
                cell: cell.0,
                column,
                width,
            };
            if let Err(error) = view.run(command, window, cx) {
                eprintln!("Failed to resize table column: {error}");
            }
        }
        cx.notify();
        true
    }

    fn paint(
        &self,
        layer: PaintLayer,
        frame: &DecorationFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if matches!(layer, PaintLayer::Foreground)
            && matches!(self.drag.get(), Some(Drag::Resize { .. }))
        {
            let bounds = frame.bounds();
            (self.paint_guide)(
                Bounds::new(
                    point(window.mouse_position().x, bounds.top()),
                    size(px(2.), bounds.size.height),
                ),
                window,
                cx,
            );
        }
    }
}
