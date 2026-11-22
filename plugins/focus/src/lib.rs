//! Paint the block around the selection anchor while the editor has focus.
use gpdoc::{DecorationFrame, PaintLayer, ViewBehavior, ViewPlugin};
use gpui::{App, Bounds, Pixels, Window, fill};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::rc::Rc;

#[derive(Clone, Copy)]
pub enum FocusMode {
    Deepest,
    Shallowest,
    All,
}

pub struct Focus {
    mode: FocusMode,
}

pub fn focus(mode: FocusMode) -> Focus {
    Focus { mode }
}

/// Fill a focused block's measured bounds.
pub fn paint_focus(bounds: Bounds<Pixels>, window: &mut Window, _: &mut App) {
    window.paint_quad(fill(bounds, window.text_style().color.opacity(0.1)));
}

impl From<Focus> for gpdoc::EditorPlugin {
    fn from(plugin: Focus) -> Self {
        plugin.with_view(paint_focus).into()
    }
}

impl Focus {
    pub fn with_view<F: Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static>(
        self,
        paint: F,
    ) -> ViewPlugin {
        let paint = Rc::new(paint);
        ViewPlugin::new("focus", move || FocusView {
            mode: self.mode,
            paint: paint.clone(),
        })
    }
}

struct FocusView<F> {
    mode: FocusMode,
    paint: Rc<F>,
}

impl<F: Fn(Bounds<Pixels>, &mut Window, &mut App)> ViewBehavior for FocusView<F> {
    fn paint(
        &self,
        layer: PaintLayer,
        frame: &DecorationFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if layer != PaintLayer::Background || !frame.focused {
            return;
        }
        let mut blocks = frame
            .visible_blocks()
            .filter(|(before, node, _)| frame.contains_anchor(*before, node))
            .map(|(_, _, rect)| rect);
        let rect = match self.mode {
            FocusMode::Deepest => blocks.last(),
            FocusMode::Shallowest => blocks.next(),
            FocusMode::All => {
                for rect in blocks {
                    (self.paint)(rect, window, cx);
                }
                return;
            }
        };
        if let Some(rect) = rect {
            (self.paint)(rect, window, cx);
        }
    }
}
