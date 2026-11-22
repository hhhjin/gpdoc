use super::interactions::block_bounds;
use crate::{
    EditorView,
    layout::{TextLayout, TextSection},
};
use gpdoc_core::{DocPos, Document, Node, Selection};
use gpui::{App, Bounds, Entity, Hsla, Pixels, SharedString, Window};
use std::ops::Range;

/// The document geometry available while GPUI paints the current text section.
pub struct DecorationFrame<'a> {
    pub view: &'a EditorView,
    pub clip: Bounds<Pixels>,
    pub focused: bool,
    layout: &'a TextLayout,
    section: &'a TextSection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintLayer {
    /// Before character backgrounds, selection, and document text.
    Background,
    /// Above character backgrounds and below the native selection and text.
    TextBackground,
    /// Above the document text and native selection.
    Foreground,
}

impl<'a> DecorationFrame<'a> {
    pub(crate) fn new(
        view: &'a EditorView,
        layout: &'a TextLayout,
        section: &'a TextSection,
        window: &Window,
    ) -> Self {
        Self {
            view,
            layout,
            section,
            focused: view.focus.is_focused(window),
            clip: window.content_mask().bounds,
        }
    }

    pub fn bounds(&self) -> Bounds<Pixels> {
        self.layout.bounds
    }

    pub fn drop_indicator_at(&self, position: gpui::Point<Pixels>) -> Option<Bounds<Pixels>> {
        self.view.drop_indicator(self.layout, position)
    }

    /// The document currently displayed, including any IME preview.
    pub fn document(&self) -> &Document {
        self.view.buffer.state().0
    }

    pub fn selection(&self) -> Selection {
        self.view.buffer.state().1
    }

    pub fn contains_anchor(&self, before: DocPos, block: &Node) -> bool {
        let anchor = match self.selection() {
            Selection::Text { anchor, .. }
            | Selection::NodeRange { anchor, .. }
            | Selection::Custom { anchor, .. } => anchor,
            Selection::Node { before } => before,
            Selection::Gap { at } => at,
        };
        before <= anchor && anchor.0 - before.0 < block.size()
    }

    /// The measured bounds of the block starting at this document position.
    pub fn block_bounds(&self, before: DocPos) -> Option<Bounds<Pixels>> {
        let node = self.document().node_at(before)?;
        if node.is_inline() {
            return None;
        }
        block_bounds(self.view, self.layout, before, node)
    }

    pub fn visible_blocks(&self) -> impl Iterator<Item = (DocPos, &Node, Bounds<Pixels>)> {
        self.document().iter_blocks().filter_map(|(before, block)| {
            let rect = block_bounds(self.view, self.layout, before, block)?;
            rect.intersects(&self.clip).then_some((before, block, rect))
        })
    }

    /// Bounds on the first visual line of the range. An empty range gives the caret bounds.
    pub fn bounds_for_range(&self, range: Range<DocPos>) -> Option<Bounds<Pixels>> {
        self.layout
            .bounds_for_range(self.view.buffer.visible_range(range)?)
    }

    /// Visual rectangles covered by a document range, including separate spans of bidirectional text.
    pub fn rects_for_range(
        &self,
        range: Range<DocPos>,
    ) -> impl Iterator<Item = Bounds<Pixels>> + '_ {
        self.view
            .buffer
            .visible_range(range)
            .into_iter()
            .flat_map(|span| self.layout.rects(span))
    }

    /// Map a committed document range through IME composition and folds, then return its
    /// visual rectangles in the section being painted. Wrapped and bidirectional text
    /// can produce multiple rectangles.
    pub fn rects_for_committed_range(
        &self,
        range: Range<DocPos>,
    ) -> impl Iterator<Item = Bounds<Pixels>> + '_ {
        self.view
            .buffer
            .decoration_range(range)
            .into_iter()
            .flat_map(|span| self.section.rects(span))
    }

    /// Fill the visible part of a committed document range in this section.
    /// Use `PaintLayer::TextBackground` to draw above mark backgrounds and below selection.
    pub fn paint_range_background(&self, range: Range<DocPos>, color: Hsla, window: &mut Window) {
        for rect in self.rects_for_committed_range(range) {
            window.paint_quad(gpui::fill(rect, color));
        }
    }

    /// Draw a decoration using the view's current font and line height.
    pub fn paint_text(
        &self,
        label: SharedString,
        rect: Bounds<Pixels>,
        color: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) {
        if rect.bottom() < self.clip.top() || rect.top() > self.clip.bottom() {
            return;
        }
        let style = window.text_style();
        let mut run = style.to_run(label.len());
        run.color = color;
        let line = window.text_system().shape_line(
            label,
            style.font_size.to_pixels(window.rem_size()),
            &[run],
            None,
        );
        crate::gpui_api::paint_label(&line, rect.origin, rect.size.height, window, cx);
    }
}

pub(crate) fn paint_decorations(
    entity: &Entity<EditorView>,
    layout: &TextLayout,
    section: &TextSection,
    layer: PaintLayer,
    window: &mut Window,
    cx: &mut App,
) {
    entity.update(cx, |view, cx| {
        let frame = DecorationFrame::new(view, layout, section, window);
        if layer == PaintLayer::Background
            && let selection @ Selection::NodeRange { .. } = frame.selection()
        {
            let doc = frame.document();
            let range = selection.bounds(doc);
            if let Ok(start) = doc.resolve(range.start) {
                for (before, _, rect) in frame.visible_blocks() {
                    if range.contains(&before)
                        && doc
                            .resolve(before)
                            .is_ok_and(|pos| pos.depth() == start.depth())
                    {
                        window.paint_quad(gpui::fill(rect, gpui::rgba(0x659ff355)));
                    }
                }
            }
        }
        for behavior in &frame.view.behaviors {
            behavior.paint(layer, &frame, window, cx);
        }
    });
}
