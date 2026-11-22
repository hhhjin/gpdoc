//! Paint the insertion point while a drag is over the editor.
use gpdoc::{AnchoredElement, ViewBehavior, ViewFrame, ViewPlugin};
use gpui::{App, Bounds, ExternalPaths, Pixels, Window, canvas, fill, prelude::*};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::rc::Rc;

pub struct DropCursor;

pub fn drop_cursor() -> DropCursor {
    DropCursor
}

/// Draw the insertion marker at the editor's current drop target.
pub fn paint_drop_cursor(bounds: Bounds<Pixels>, window: &mut Window, _: &mut App) {
    window.paint_quad(fill(bounds, window.text_style().color));
}

impl From<DropCursor> for gpdoc::EditorPlugin {
    fn from(plugin: DropCursor) -> Self {
        plugin.with_view(paint_drop_cursor).into()
    }
}

impl DropCursor {
    pub fn with_view<F: Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static>(
        self,
        paint: F,
    ) -> ViewPlugin {
        let paint = Rc::new(paint);
        ViewPlugin::new("dropCursor", move || DropIndicator(paint.clone()))
            .with_text_drag::<ExternalPaths>()
    }
}

struct DropIndicator<F>(Rc<F>);
impl<F: Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static> ViewBehavior for DropIndicator<F> {
    fn render(
        self: Rc<Self>,
        frame: &ViewFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnchoredElement> {
        if !cx.has_active_drag() {
            return None;
        }
        let rect = frame.drop_indicator_at(window.mouse_position(), cx)?;
        let marker = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                (self.0)(bounds, window, cx);
            },
        )
        .w(rect.size.width)
        .h(rect.size.height);
        Some(AnchoredElement::new(marker, move |_, _| rect.origin))
    }
}
