use super::is_visible;
use gpdoc::{DecorationFrame, PaintLayer, ViewBehavior, ViewPlugin};
use gpdoc_plugin::core::{ContentRef, DocPos};
use gpui::{App, Bounds, Pixels, Window};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::rc::Rc;

/// The document character represented by a marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    Space,
    LineBreak,
    ParagraphEnd,
}

pub struct CharacterMarker {
    pub kind: MarkerKind,
    pub position: DocPos,
    pub bounds: Bounds<Pixels>,
}

type MarkerPainter = dyn Fn(CharacterMarker, &DecorationFrame<'_>, &mut Window, &mut App);

struct InvisibleCharacters(Rc<MarkerPainter>);

/// Paint each marker at its text position while invisible characters are enabled.
pub fn character_markers(
    paint: impl Fn(CharacterMarker, &DecorationFrame<'_>, &mut Window, &mut App) + 'static,
) -> ViewPlugin {
    let paint: Rc<MarkerPainter> = Rc::new(paint);
    ViewPlugin::new("invisibleCharacters", move || {
        InvisibleCharacters(paint.clone())
    })
    .for_plugin("invisibleCharacters")
}

/// Draw a space dot, line-break arrow or paragraph sign using the editor font.
pub fn paint_marker(
    marker: CharacterMarker,
    frame: &DecorationFrame<'_>,
    window: &mut Window,
    cx: &mut App,
) {
    let glyph = match marker.kind {
        MarkerKind::Space => "·",
        MarkerKind::LineBreak => "↵",
        MarkerKind::ParagraphEnd => "¶",
    };
    frame.paint_text(
        glyph.into(),
        marker.bounds,
        window.text_style().color.opacity(0.55),
        window,
        cx,
    );
}

impl ViewBehavior for InvisibleCharacters {
    fn paint(
        &self,
        layer: PaintLayer,
        frame: &DecorationFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if layer != PaintLayer::Foreground || !is_visible(frame.view.editor()) {
            return;
        }
        for (before, block, _) in frame.visible_blocks() {
            let ContentRef::Inline(children) = block.content() else {
                continue;
            };
            let mut pos = before.0 + 1;
            for child in children {
                let mut at = pos;
                pos += child.size();
                for ch in frame.document().inline_text(child).chars() {
                    let next = at + ch.len_utf16();
                    let kind = match ch {
                        ' ' => Some(MarkerKind::Space),
                        '\n' => Some(MarkerKind::LineBreak),
                        _ => None,
                    };
                    if let Some(kind) = kind
                        && let Some(bounds) = frame.bounds_for_range(DocPos(at)..DocPos(next))
                    {
                        (self.0)(
                            CharacterMarker {
                                kind,
                                position: DocPos(at),
                                bounds,
                            },
                            frame,
                            window,
                            cx,
                        );
                    }
                    at = next;
                }
            }
            if block.kind == "paragraph"
                && let Some(bounds) = frame.bounds_for_range(DocPos(pos)..DocPos(pos))
            {
                (self.0)(
                    CharacterMarker {
                        kind: MarkerKind::ParagraphEnd,
                        position: DocPos(pos),
                        bounds,
                    },
                    frame,
                    window,
                    cx,
                );
            }
        }
    }
}
