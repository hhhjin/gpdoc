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

#[derive(Clone, Copy)]
struct BoxDrag {
    origin: Point<Pixels>,
    caret: DocPos,
    rect: Option<Bounds<Pixels>>,
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
        let origin = drag.origin;
        if drag.rect.is_none() && (at.x - origin.x).abs().max((at.y - origin.y).abs()) < px(4.) {
            return true;
        }
        let rect = Bounds::from_corners(
            point(origin.x.min(at.x), origin.y.min(at.y)),
            point(origin.x.max(at.x), origin.y.max(at.y)),
        );
        drag.rect = Some(rect);
        self.drag.set(Some(drag));
        let selection = match view.blocks_in_rect(rect, window) {
            Some(range) if at.y < origin.y => Selection::NodeRange {
                anchor: range.end,
                head: range.start,
            },
            Some(range) => Selection::NodeRange {
                anchor: range.start,
                head: range.end,
            },
            None => Selection::caret(drag.caret),
        };
        if selection != view.editor().selection() {
            let _ = view.select_document(selection, window, cx);
        }
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
        {
            self.drag.set(Some(BoxDrag {
                origin: event.position,
                caret,
                rect: None,
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
        _: &ViewFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnchoredElement> {
        let rect = self.drag.get()?.rect?;
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
