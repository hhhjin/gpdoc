//! Drag a rectangle from outside the blocks to select top-level blocks.
use gpdoc::{AnchoredElement, EditorView, ViewBehavior, ViewFrame, ViewPlugin};
use gpdoc_plugin::core::{DocPos, Selection};
use gpui::{
    AnyElement, App, Bounds, Context, Div, Modifiers, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, Window, div, point, prelude::*, px,
};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{cell::Cell, rc::Rc};

pub struct BoxSelection;

pub fn box_selection() -> BoxSelection {
    BoxSelection
}

/// The translucent rectangle displayed while dragging across blocks.
pub fn selection_box(_: Bounds<Pixels>, window: &mut Window, _: &mut App) -> Div {
    let color = window.text_style().color;
    div()
        .size_full()
        .bg(color.opacity(0.08))
        .border_1()
        .border_color(color.opacity(0.55))
}

impl From<BoxSelection> for gpdoc::EditorPlugin {
    fn from(plugin: BoxSelection) -> Self {
        plugin.with_view(selection_box).into()
    }
}

impl BoxSelection {
    pub fn with_view<E: IntoElement>(
        self,
        render: impl Fn(Bounds<Pixels>, &mut Window, &mut App) -> E + 'static,
    ) -> ViewPlugin {
        let render: Rc<RenderBox> =
            Rc::new(move |bounds, window, cx| render(bounds, window, cx).into_any_element());
        ViewPlugin::new("boxSelection", move || BoxSelector {
            drag: Cell::new(None),
            render: render.clone(),
        })
    }
}

type RenderBox = dyn Fn(Bounds<Pixels>, &mut Window, &mut App) -> AnyElement;

#[derive(Clone, Copy, PartialEq)]
struct BoxDrag {
    document_anchor: Point<Pixels>,
    pointer: Point<Pixels>,
    caret: DocPos,
    last_rect: Option<Bounds<Pixels>>,
}

impl BoxDrag {
    fn bounds(&self, document_origin: Point<Pixels>) -> Bounds<Pixels> {
        let anchor = document_origin + self.document_anchor;
        let at = self.pointer;
        Bounds::from_corners(
            point(anchor.x.min(at.x), anchor.y.min(at.y)),
            point(anchor.x.max(at.x), anchor.y.max(at.y)),
        )
    }

    fn select(
        &self,
        rect: Bounds<Pixels>,
        view: &mut EditorView,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) {
        let selection = match view.blocks_in_rect(rect, window) {
            Some(range) if self.pointer.y < rect.bottom() => Selection::NodeRange {
                anchor: range.end,
                head: range.start,
            },
            Some(range) => Selection::NodeRange {
                anchor: range.start,
                head: range.end,
            },
            None => Selection::caret(self.caret),
        };
        if selection != view.editor().selection() {
            let _ = view.select_document_in_place(selection, window, cx);
        }
    }
}

struct BoxSelector {
    drag: Cell<Option<BoxDrag>>,
    render: Rc<RenderBox>,
}

impl BoxSelector {
    fn drag_to(
        &self,
        view: &mut EditorView,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        let Some(mut drag) = self.drag.get() else {
            return false;
        };
        // Keep the original window-space press until the pointer crosses the threshold.
        let delta = at - drag.pointer;
        if drag.last_rect.is_none() && delta.x.abs().max(delta.y.abs()) < px(4.) {
            return true;
        }
        let Some(origin) = view.document_origin() else {
            return true;
        };
        drag.pointer = at;
        let rect = drag.bounds(origin);
        drag.last_rect = Some(rect);
        self.drag.set(Some(drag));
        drag.select(rect, view, window, cx);
        cx.notify();
        true
    }
}

impl ViewBehavior for BoxSelector {
    fn mouse_down(
        &self,
        view: &mut EditorView,
        event: &MouseDownEvent,
        window: &mut Window,
        _: &mut Context<EditorView>,
    ) -> bool {
        if event.click_count == 1
            && event.modifiers == Modifiers::default()
            && !view.is_composing()
            && view.block_at_point(event.position, window).is_none()
            && let Some(caret) = view.position_at(event.position, window)
            && let Some(origin) = view.document_origin()
        {
            self.drag.set(Some(BoxDrag {
                document_anchor: event.position - origin,
                pointer: event.position,
                caret,
                last_rect: None,
            }));
        }
        // Let the native press place the caret. Claim the gesture on movement.
        false
    }

    fn mouse_move(
        &self,
        view: &mut EditorView,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        event.dragging() && self.drag_to(view, event.position, window, cx)
    }

    fn pointer_cancel(&self) {
        self.drag.set(None);
    }

    fn render(
        self: Rc<Self>,
        frame: &ViewFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnchoredElement> {
        let mut drag = self.drag.get()?;
        let last_rect = drag.last_rect?;
        let rect = drag.bounds(frame.document_origin());
        if last_rect != rect {
            drag.last_rect = Some(rect);
            self.drag.set(Some(drag));
            let view = frame.view.clone();
            let selector = self.clone();
            // Update after paint so GPUI delivers the selection change to observers.
            window.defer(cx, move |window, cx| {
                if selector.drag.get() == Some(drag) {
                    view.update(cx, |view, cx| drag.select(rect, view, window, cx));
                }
            });
        }
        let overlay = div()
            .debug_selector(|| "gpdoc-box-selection".into())
            .w(rect.size.width)
            .h(rect.size.height)
            .child((self.render)(rect, window, cx));
        Some(AnchoredElement::new(overlay, move |_, _| rect.origin))
    }

    fn mouse_up(
        &self,
        view: &mut EditorView,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        if self.drag.get().is_some() {
            self.drag_to(view, event.position, window, cx);
            self.pointer_cancel();
            cx.notify();
            return true;
        }
        false
    }
}
