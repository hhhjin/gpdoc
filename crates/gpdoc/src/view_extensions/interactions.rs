use crate::{EditorView, layout::TextLayout};
use gpdoc_core::{DocPos, Document, Node};
use gpui::{Bounds, Pixels, Point, Window};

/// A candidate insertion boundary and the horizontal line drawn for it.
#[derive(Clone, Copy, Debug)]
pub struct BlockDropTarget {
    /// Start of the measured block supplying this edge, or None for a viewport fallback.
    pub block_before: Option<DocPos>,
    pub at: DocPos,
    pub indicator: Bounds<Pixels>,
}

impl BlockDropTarget {
    fn edge(before: DocPos, node: &Node, rect: Bounds<Pixels>, y: Pixels) -> Self {
        let (at, y) = if y < rect.center().y {
            (before, rect.top())
        } else {
            (DocPos(before.0 + node.size()), rect.bottom())
        };
        Self {
            block_before: Some(before),
            at,
            indicator: Bounds::new(
                gpui::point(rect.left(), y),
                gpui::size(rect.size.width, gpui::px(2.)),
            ),
        }
    }
}

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
    /// Document origin in window coordinates at the last layout.
    pub fn document_origin(&self) -> Option<Point<Pixels>> {
        Some(self.layout.as_ref()?.bounds.origin)
    }

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
        match self.drop_indicator_kind {
            super::DropIndicator::Text => {
                let at = layout.index_for_point(mouse);
                let rect = layout.bounds_for_range(at..at)?;
                Some(Bounds::new(
                    rect.origin,
                    gpui::size(gpui::px(2.), rect.size.height),
                ))
            }
            super::DropIndicator::Block(marker) => marker,
        }
    }

    /// Edges under the pointer, from the innermost rendered block to the document.
    pub(super) fn block_drop_targets(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
    ) -> Vec<BlockDropTarget> {
        self.geometry(window);
        self.layout.as_ref().map_or_else(Vec::new, |layout| {
            self.block_drop_targets_in_layout(point, layout)
        })
    }

    pub(super) fn block_drop_targets_in_layout(
        &self,
        point: Point<Pixels>,
        layout: &TextLayout,
    ) -> Vec<BlockDropTarget> {
        if self.is_composing() || !self.scroll.bounds().contains(&point) {
            return Vec::new();
        }
        let hit = layout.child_bounds_at(point);
        let doc = self.editor().document();
        let mut targets = Vec::new();
        if let Some((before, rect)) = hit
            && let Ok(pos) = doc.resolve(before)
            && let Some(node) = doc.node_at(before)
        {
            if !node.is_leaf() && !node.inline_content() && node.child_count() == 0 {
                targets.push(BlockDropTarget {
                    block_before: Some(before),
                    at: DocPos(before.0 + 1),
                    indicator: Bounds::new(rect.origin, gpui::size(rect.size.width, gpui::px(2.))),
                });
            }
            targets.push(BlockDropTarget::edge(before, node, rect, point.y));
            for depth in (1..=pos.depth()).rev() {
                let before = DocPos(pos.before(depth).expect("ancestor has a boundary"));
                let node = pos.node(depth);
                if let Some(rect) = block_bounds(self, layout, before, node) {
                    targets.push(BlockDropTarget::edge(before, node, rect, point.y));
                }
            }
        } else if let Some((at, y)) = self.drop_boundary(layout, point) {
            targets.push(BlockDropTarget {
                block_before: None,
                at,
                indicator: Bounds::new(
                    gpui::point(layout.bounds.left(), y),
                    gpui::size(layout.bounds.size.width, gpui::px(2.)),
                ),
            });
        }
        targets
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
