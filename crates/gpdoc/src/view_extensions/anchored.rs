//! Mount plugin elements against the current native document geometry.
use crate::{EditorView, layout::TextLayout};
use gpdoc_core::DocPos;
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Entity, IntoElement, Pixels, Point, Size, Window,
    point, size,
};
use std::ops::Range;

/// The current layout for a plugin rendering interactive elements.
pub struct ViewFrame<'a> {
    pub view: Entity<EditorView>,
    pub focused: bool,
    pub clip: Bounds<Pixels>,
    layout: &'a TextLayout,
    selection: (usize, usize),
}

impl ViewFrame<'_> {
    fn visible_bounds(&self, offsets: Range<usize>) -> Option<Bounds<Pixels>> {
        self.layout
            .bounds_for_range(offsets)
            .filter(|r| self.clip.intersects(r))
    }

    /// The innermost measured block under the pointer, respecting layout clips.
    pub fn block_at(&self, point: Point<Pixels>) -> Option<DocPos> {
        self.layout.child_bounds_at(point).map(|(before, _)| before)
    }

    /// Current allocation of a measured block.
    pub fn block_bounds(&self, before: DocPos) -> Option<Bounds<Pixels>> {
        self.layout.child_bounds(before)
    }

    /// Top of the block's first line, rendered object, or child layout inside its wrappers.
    pub fn block_content_top(&self, before: DocPos) -> Option<Pixels> {
        self.layout.child_content_top(before)
    }

    /// Insertion marker for a drag over the editor viewport.
    pub fn drop_indicator_at(&self, position: Point<Pixels>, cx: &App) -> Option<Bounds<Pixels>> {
        self.view.read(cx).drop_indicator(self.layout, position)
    }

    /// Bounds at the active end of the current selection, when visible.
    pub fn caret_bounds(&self) -> Option<Bounds<Pixels>> {
        let (_, head) = self.selection;
        self.visible_bounds(head..head)
    }

    /// Bounds on the first visual line of the current selection, when visible.
    pub fn selection_bounds(&self) -> Option<Bounds<Pixels>> {
        let (anchor, head) = self.selection;
        self.visible_bounds(anchor.min(head)..anchor.max(head))
    }

    /// Bounds on the first visual line of a range in the displayed document, including IME.
    pub fn bounds_for_range(&self, range: Range<DocPos>, cx: &App) -> Option<Bounds<Pixels>> {
        let offsets = self.view.read(cx).buffer.visible_range(range)?;
        self.visible_bounds(offsets)
    }
}

type Placement = Box<dyn FnOnce(Size<Pixels>, Bounds<Pixels>) -> Point<Pixels>>;

/// An interactive element placed after GPUI measures its requested size.
pub struct AnchoredElement {
    element: AnyElement,
    position: Placement,
    exclusive: bool,
}

impl AnchoredElement {
    /// Place an element using its measured size and the editor's visible bounds.
    pub fn new(
        element: impl IntoElement,
        position: impl FnOnce(Size<Pixels>, Bounds<Pixels>) -> Point<Pixels> + 'static,
    ) -> Self {
        Self {
            element: element.into_any_element(),
            position: Box::new(position),
            exclusive: false,
        }
    }

    /// Prefer the space above the anchor, flipping or shifting inside the viewport as needed.
    pub fn above(anchor: Bounds<Pixels>, element: impl IntoElement) -> Self {
        Self::new(element, move |dimensions, clip| {
            position_near(anchor, dimensions, clip, true)
        })
    }

    /// Prefer the space below the anchor.
    pub fn below(anchor: Bounds<Pixels>, element: impl IntoElement) -> Self {
        Self::new(element, move |dimensions, clip| {
            position_near(anchor, dimensions, clip, false)
        })
    }

    /// While visible, display only this element. The first exclusive element in installation order wins.
    pub fn exclusive(mut self) -> Self {
        self.exclusive = true;
        self
    }

    fn prepaint(mut self, clip: Bounds<Pixels>, window: &mut Window, cx: &mut App) -> AnyElement {
        let available = size(AvailableSpace::MaxContent, AvailableSpace::MaxContent);
        let dimensions = self.element.layout_as_root(available, window, cx);
        self.element
            .prepaint_at((self.position)(dimensions, clip), window, cx);
        self.element
    }
}

fn position_near(
    anchor: Bounds<Pixels>,
    dimensions: Size<Pixels>,
    clip: Bounds<Pixels>,
    prefer_above: bool,
) -> Point<Pixels> {
    let above = anchor.top() - clip.top();
    let below = clip.bottom() - anchor.bottom();
    let use_above = if above >= dimensions.height && below >= dimensions.height {
        prefer_above
    } else {
        above > below || (above == below && prefer_above)
    };
    let y = if use_above {
        anchor.top() - dimensions.height
    } else {
        anchor.bottom()
    };
    point(
        anchor.left().clamp(
            clip.left(),
            (clip.right() - dimensions.width).max(clip.left()),
        ),
        y.clamp(
            clip.top(),
            (clip.bottom() - dimensions.height).max(clip.top()),
        ),
    )
}

pub(crate) fn prepaint_elements(
    entity: &Entity<EditorView>,
    layout: &TextLayout,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let editor = entity.read(cx);
    let frame = ViewFrame {
        view: entity.clone(),
        selection: editor.buffer.selection(),
        focused: editor.focus.is_focused(window),
        clip: window.content_mask().bounds,
        layout,
    };
    let behaviors = editor.behaviors.clone();
    let mut elements = Vec::new();
    for behavior in behaviors {
        if let Some(element) = behavior.render(&frame, window, cx) {
            if element.exclusive {
                return vec![element.prepaint(frame.clip, window, cx)];
            }
            elements.push(element);
        }
    }
    elements
        .into_iter()
        .map(|e| e.prepaint(frame.clip, window, cx))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn placement_flips_at_the_edges_of_a_scrolled_viewport() {
        let clip = Bounds::new(point(px(20.), px(100.)), size(px(200.), px(150.)));
        let menu = size(px(80.), px(40.));
        let caret = |x, y| Bounds::new(point(px(x), px(y)), size(px(1.), px(20.)));
        let preferred = caret(80., 160.);
        assert_eq!(
            position_near(preferred, menu, clip, true),
            point(px(80.), px(120.))
        );
        assert_eq!(
            position_near(preferred, menu, clip, false),
            point(px(80.), px(180.))
        );
        assert_eq!(
            position_near(caret(210., 215.), menu, clip, false),
            point(px(140.), px(175.))
        );
        assert_eq!(
            position_near(caret(18., 105.), menu, clip, true),
            point(px(20.), px(125.))
        );
    }
}
