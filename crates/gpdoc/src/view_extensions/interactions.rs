use crate::{EditorView, layout::TextLayout};
use gpdoc_core::{DocPos, Document, Node};
use gpui::{Bounds, Pixels, Point, Window};

pub(super) fn blocks(doc: &Document) -> impl Iterator<Item = (DocPos, &Node)> {
    doc.blocks().iter().scan(0, |at, node| {
        let before = DocPos(*at);
        *at += node.size();
        Some((before, node))
    })
}

pub(super) fn block_bounds(
    view: &EditorView,
    layout: &TextLayout,
    before: DocPos,
    node: &Node,
) -> Option<Bounds<Pixels>> {
    if let Some(bounds) = layout.child_bounds(before) {
        return Some(bounds);
    }
    let span = view
        .buffer
        .block_offsets(before..DocPos(before.0 + node.size()))?;
    let top = layout.bounds_for_range(span.start..span.start)?;
    let bottom = layout.bounds_for_range(span.end..span.end)?;
    let bounds = layout
        .slot_bounds(span.start)?
        .union(&layout.slot_bounds(span.end)?);
    Some(Bounds::from_corners(
        gpui::point(bounds.left(), top.top()),
        gpui::point(bounds.right(), bottom.bottom()),
    ))
}

impl EditorView {
    /// Resolve a screen point to its document position in the current layout.
    pub fn position_at(&mut self, point: Point<Pixels>, window: &mut Window) -> Option<DocPos> {
        let offset = self.geometry(window)?.index_for_point(point);
        self.buffer.position(offset).ok()
    }

    /// Find the top-level block whose allocated bounds contain a window point.
    pub fn block_at_point(&mut self, point: Point<Pixels>, window: &mut Window) -> Option<DocPos> {
        self.geometry(window)?;
        let layout = self.layout.as_ref()?;
        blocks(self.editor().document()).find_map(|(before, node)| {
            block_bounds(self, layout, before, node)?
                .contains(&point)
                .then_some(before)
        })
    }

    /// Document boundaries of the top-level blocks intersecting a window rectangle.
    pub fn blocks_in_rect(
        &mut self,
        rect: Bounds<Pixels>,
        window: &mut Window,
    ) -> Option<std::ops::Range<DocPos>> {
        self.geometry(window)?;
        let layout = self.layout.as_ref()?;
        let mut hits = blocks(self.editor().document()).filter_map(|(before, node)| {
            block_bounds(self, layout, before, node)?
                .intersects(&rect)
                .then(|| before..DocPos(before.0 + node.size()))
        });
        let first = hits.next()?;
        Some(first.start..hits.last().unwrap_or(first).end)
    }

    /// Find the nearest top-level block boundary and its screen y coordinate.
    pub fn block_boundary_at(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
    ) -> Option<(DocPos, Pixels)> {
        self.geometry(window)?;
        self.drop_boundary(self.layout.as_ref()?, point)
    }

    pub(crate) fn drop_indicator(
        &self,
        layout: &TextLayout,
        mouse: Point<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        if !self.scroll.bounds().contains(&mouse) {
            return None;
        }
        if self.inline_drop_target {
            let at = layout.index_for_point(mouse);
            let rect = layout.bounds_for_range(at..at)?;
            Some(Bounds::new(
                rect.origin,
                gpui::size(gpui::px(2.), rect.size.height),
            ))
        } else {
            let (_, y) = self.drop_boundary(layout, mouse)?;
            Some(Bounds::new(
                gpui::point(layout.bounds.left(), y),
                gpui::size(layout.bounds.size.width, gpui::px(2.)),
            ))
        }
    }

    pub(crate) fn drop_boundary(
        &self,
        layout: &TextLayout,
        position: Point<Pixels>,
    ) -> Option<(DocPos, Pixels)> {
        let doc = self.editor().document();
        let mut end = None;
        for (before, node) in blocks(doc) {
            let Some(rect) = block_bounds(self, layout, before, node) else {
                continue;
            };
            if position.y < rect.center().y {
                return Some((before, rect.top()));
            }
            end = Some((DocPos(before.0 + node.size()), rect.bottom()));
        }
        end
    }
}
