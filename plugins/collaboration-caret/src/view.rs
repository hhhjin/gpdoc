//! Paint names, cursors and remote selections in a GPUI editor.
use crate::RemoteCaret;
use gpdoc::{DecorationFrame, PaintLayer, ViewBehavior, ViewPlugin};
use gpdoc_plugin::core::Selection;
use gpdoc_plugin_collaboration::{Collaboration, CollaborationView};
use gpui::{App, Bounds, Hsla, Pixels, Rgba, SharedString, Window, fill, point, px, size};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{cell::RefCell, rc::Rc};

type Peers = dyn Fn(&Collaboration) -> Vec<RemoteCaret>;

type CaretPainter = dyn Fn(CaretPart, &CaretLayout, &mut Window, &mut App);

/// Resolves transient remote selections against the collaboration session bound to the editor.
pub struct CollaborationCarets {
    resolve: Rc<Peers>,
}

pub fn collaboration_carets(
    resolve: impl Fn(&Collaboration) -> Vec<RemoteCaret> + 'static,
) -> CollaborationCarets {
    CollaborationCarets {
        resolve: Rc::new(resolve),
    }
}

impl From<CollaborationCarets> for gpdoc::EditorPlugin {
    fn from(plugin: CollaborationCarets) -> Self {
        plugin.with_view(paint_remote_caret).into()
    }
}

impl CollaborationCarets {
    /// Paint the selection behind text, then the cursor and label above it.
    pub fn with_view(
        self,
        paint: impl Fn(CaretPart, &CaretLayout, &mut Window, &mut App) + 'static,
    ) -> ViewPlugin {
        let paint: Rc<CaretPainter> = Rc::new(paint);
        ViewPlugin::new("collaborationCaret", move || Carets {
            resolve: self.resolve.clone(),
            paint: paint.clone(),
            foreground: RefCell::default(),
        })
    }
}

struct Carets {
    resolve: Rc<Peers>,
    paint: Rc<CaretPainter>,
    foreground: RefCell<Vec<CaretLayout>>,
}

impl ViewBehavior for Carets {
    fn paint(
        &self,
        layer: PaintLayer,
        frame: &DecorationFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) {
        match layer {
            PaintLayer::Background => {
                let carets = carets(frame, self.resolve.as_ref(), window.text_style().color);
                for caret in &carets {
                    (self.paint)(CaretPart::Selection, caret, window, cx);
                }
                self.foreground.replace(carets);
            }
            PaintLayer::TextBackground => {}
            PaintLayer::Foreground => {
                for caret in self.foreground.take() {
                    (self.paint)(CaretPart::Cursor, &caret, window, cx);
                    (self.paint)(CaretPart::Label, &caret, window, cx);
                }
            }
        }
    }
}

/// The part being painted in its editor paint layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaretPart {
    Selection,
    Cursor,
    Label,
}

/// A remote peer and the measured geometry shared by all of its painted parts.
pub struct CaretLayout {
    pub peer: RemoteCaret,
    pub selection: Vec<Bounds<Pixels>>,
    pub head: Bounds<Pixels>,
    pub color: Hsla,
}

fn carets(frame: &DecorationFrame<'_>, resolve: &Peers, fallback_color: Hsla) -> Vec<CaretLayout> {
    let view = frame.view;
    let Some(session) = view.collaboration() else {
        return Vec::new();
    };
    if view.is_composing() {
        return vec![];
    }
    resolve(session)
        .into_iter()
        .filter_map(|peer| {
            let selection = peer.selection?;
            selection.validate(view.editor().document()).ok()?;
            let range = selection.bounds(view.editor().document());
            let head = match selection {
                Selection::Text { head, .. }
                | Selection::NodeRange { head, .. }
                | Selection::Custom { head, .. } => head,
                _ => range.end,
            };
            let head = frame.bounds_for_range(head..head)?;
            let selection = frame.rects_for_range(range).collect();
            let color = peer
                .user
                .get("color")
                .and_then(|s| s.as_str())
                .and_then(|s| Rgba::try_from(s).ok())
                .map(Hsla::from)
                .unwrap_or(fallback_color);
            Some(CaretLayout {
                peer,
                selection,
                head,
                color,
            })
        })
        .collect()
}

/// Paint the default peer-colored selection, cursor or name label.
pub fn paint_remote_caret(part: CaretPart, caret: &CaretLayout, window: &mut Window, cx: &mut App) {
    let rect = caret.head;
    match part {
        CaretPart::Selection => {
            for bounds in &caret.selection {
                window.paint_quad(fill(*bounds, caret.color.opacity(0.22)));
            }
        }
        CaretPart::Cursor => window.paint_quad(fill(
            Bounds::new(rect.origin, size(px(2.), rect.size.height)),
            caret.color,
        )),
        CaretPart::Label => {
            let Some(name) = caret
                .peer
                .user
                .get("name")
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty())
            else {
                return;
            };
            let style = window.text_style();
            let font_size = style.font_size.to_pixels(window.rem_size());
            let mut run = style.to_run(name.len());
            run.color = label_text_color(caret.color);
            let line = window.text_system().shape_line(
                SharedString::from(name.to_owned()),
                font_size,
                &[run],
                None,
            );
            let height = rect.size.height;
            let clip = window.content_mask().bounds;
            let width = line.width + px(4.);
            let x = rect
                .left()
                .clamp(clip.left(), (clip.right() - width).max(clip.left()));
            let origin = point(x, (rect.top() - height).max(clip.top()));
            window.paint_quad(fill(Bounds::new(origin, size(width, height)), caret.color));
            gpdoc::paint_label(
                &line,
                point(origin.x + px(2.), origin.y),
                height,
                window,
                cx,
            );
        }
    }
}

/// Choose black or white for the strongest contrast against the peer's badge.
fn label_text_color(background: Hsla) -> Hsla {
    let color = Rgba::from(background);
    let linear = |value: f32| {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b);
    if luminance > 0.179 {
        Hsla::black()
    } else {
        Hsla::white()
    }
}
