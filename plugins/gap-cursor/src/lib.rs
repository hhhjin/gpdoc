//! Navigate and select insertion points between closed blocks.
use gpdoc::{DecorationFrame, EditorView, PaintLayer, ViewBehavior, ViewPlugin};
use gpdoc_plugin::core::{DocPos, Selection};
use gpui::{
    App, Bounds, Context, Keystroke, MouseDownEvent, Pixels, Window, fill, point, px, size,
};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;

use std::rc::Rc;

pub struct GapCursor;

pub fn gap_cursor() -> GapCursor {
    GapCursor
}

/// Paint the marker at the selected block boundary.
pub fn paint_gap_cursor(bounds: Bounds<Pixels>, window: &mut Window, _: &mut App) {
    window.paint_quad(fill(bounds, window.text_style().color));
}

impl From<GapCursor> for gpdoc::EditorPlugin {
    fn from(plugin: GapCursor) -> Self {
        plugin.with_view(paint_gap_cursor).into()
    }
}

impl GapCursor {
    pub fn with_view<F: Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static>(
        self,
        paint: F,
    ) -> ViewPlugin {
        let paint = Rc::new(paint);
        ViewPlugin::new("gapCursor", move || GapNavigator(paint.clone()))
    }
}

struct GapNavigator<F>(Rc<F>);

impl<F: Fn(Bounds<Pixels>, &mut Window, &mut App)> ViewBehavior for GapNavigator<F> {
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
        let Selection::Gap { at } = frame.selection() else {
            return;
        };
        let origin = if let Some(next) = frame.block_bounds(at) {
            next.origin
        } else {
            let Some(previous) = frame
                .document()
                .resolve(at)
                .ok()
                .and_then(|pos| pos.node_before().ok().flatten())
            else {
                return;
            };
            let Some(rect) = frame.block_bounds(DocPos(at.0 - previous.size())) else {
                return;
            };
            point(rect.left(), rect.bottom())
        };
        (self.0)(Bounds::new(origin, size(px(22.), px(2.))), window, cx);
    }

    fn key_down(
        &self,
        view: &mut EditorView,
        stroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        if stroke.modifiers != Default::default() {
            return false;
        }
        let forward = match stroke.key.as_str() {
            "right" | "down" => true,
            "left" | "up" => false,
            _ => return false,
        };
        let doc = view.editor().document();
        let selection = view.editor().selection();
        let next = match selection {
            Selection::Node { before } => Selection::Gap {
                at: if forward {
                    DocPos(before.0 + doc.node_at(before).expect("selected node").size())
                } else {
                    before
                },
            },
            Selection::Gap { at } => {
                let pos = doc.resolve(at).expect("selected gap");
                let node = if forward {
                    pos.node_after()
                } else {
                    pos.node_before()
                };
                let Ok(Some(node)) = node else { return true };
                Selection::Node {
                    before: if forward {
                        at
                    } else {
                        DocPos(at.0 - node.size())
                    },
                }
            }
            _ => return false,
        };
        view.select_document(next, window, cx).is_ok()
    }
    fn mouse_down(
        &self,
        view: &mut EditorView,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        if view.is_composing() {
            return false;
        }
        let Some((at, y)) = view.block_boundary_at(event.position, window) else {
            return false;
        };
        (event.position.y - y).abs() <= px(6.)
            && view
                .select_document(Selection::Gap { at }, window, cx)
                .is_ok()
    }
}
