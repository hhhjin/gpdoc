//! Show a writing hint inside empty text blocks.
use gpdoc::{DecorationFrame, PaintLayer, ViewBehavior, ViewPlugin};
use gpdoc_plugin::core::{ContentRef, DocPos};
use gpui::{App, Bounds, Pixels, Window};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::rc::Rc;

#[derive(Clone)]
pub struct Placeholder {
    pub text: String,
    /// Show the hint only in the block containing the selection anchor.
    pub current_only: bool,
    pub include_children: bool,
}

/// Configure where this editor hint appears.
pub fn placeholder(text: impl Into<String>) -> Placeholder {
    Placeholder {
        text: text.into(),
        current_only: true,
        include_children: false,
    }
}

/// A visible empty block's hint and the measured start of its first line.
pub struct PlaceholderHint<'a> {
    pub text: &'a str,
    pub block: DocPos,
    pub bounds: Bounds<Pixels>,
}

/// Paint the hint with the editor's font and line height.
pub fn paint_placeholder(
    hint: PlaceholderHint<'_>,
    frame: &DecorationFrame<'_>,
    window: &mut Window,
    cx: &mut App,
) {
    frame.paint_text(
        hint.text.to_owned().into(),
        hint.bounds,
        window.text_style().color.opacity(0.55),
        window,
        cx,
    );
}

impl From<Placeholder> for gpdoc::EditorPlugin {
    fn from(plugin: Placeholder) -> Self {
        plugin.with_view(paint_placeholder).into()
    }
}

impl Placeholder {
    pub fn with_view<F>(self, paint: F) -> ViewPlugin
    where
        F: Fn(PlaceholderHint<'_>, &DecorationFrame<'_>, &mut Window, &mut App) + 'static,
    {
        let paint = Rc::new(paint);
        ViewPlugin::new("placeholder", move || HintView {
            options: self.clone(),
            paint: paint.clone(),
        })
    }
}

struct HintView<F> {
    options: Placeholder,
    paint: Rc<F>,
}

impl<F: Fn(PlaceholderHint<'_>, &DecorationFrame<'_>, &mut Window, &mut App)> ViewBehavior
    for HintView<F>
{
    fn paint(
        &self,
        layer: PaintLayer,
        frame: &DecorationFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if layer != PaintLayer::Foreground {
            return;
        }
        let Some(line) = self.options.text.lines().next() else {
            return;
        };
        for (before, block, _) in frame.visible_blocks() {
            if matches!(block.content(), ContentRef::Inline(children) if children.is_empty())
                && (!self.options.current_only || frame.contains_anchor(before, block))
                && (self.options.include_children
                    || frame
                        .document()
                        .resolve(before)
                        .is_ok_and(|pos| pos.depth() == 0))
            {
                let start = DocPos(before.0 + 1);
                if let Some(rect) = frame.bounds_for_range(start..start) {
                    (self.paint)(
                        PlaceholderHint {
                            text: line,
                            block: before,
                            bounds: rect,
                        },
                        frame,
                        window,
                        cx,
                    );
                }
            }
        }
    }
}
