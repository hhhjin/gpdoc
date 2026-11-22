use crate::link_at;
use gpdoc::{AnchoredElement, EditorView, ViewBehavior, ViewFrame, ViewPlugin};
use gpdoc_plugin::core::{Attrs, DocPos, Selection};
use gpui::{App, Entity, IntoElement, MouseButton, Window, div, prelude::*};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{ops::Range, rc::Rc};

/// A single linked run under the caret, including adjacent text with the same link.
pub struct LinkAtCaret {
    pub editor: Entity<EditorView>,
    pub range: Range<DocPos>,
    pub attrs: Attrs,
}

/// Render application controls above a link while the caret is inside it.
pub fn link_popover<E: IntoElement>(
    render: impl Fn(LinkAtCaret, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    let render = Rc::new(render);
    ViewPlugin::new("linkPopover", move || Popover(render.clone()))
}

struct Popover<F>(Rc<F>);
impl<F, E> ViewBehavior for Popover<F>
where
    E: IntoElement,
    F: Fn(LinkAtCaret, &mut Window, &mut App) -> E,
{
    fn render(
        self: Rc<Self>,
        frame: &ViewFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnchoredElement> {
        let editor = frame.view.read(cx);
        if !frame.focused || editor.is_composing() {
            return None;
        }
        let Selection::Text { anchor, head } = editor.editor().selection() else {
            return None;
        };
        if anchor != head {
            return None;
        }
        let anchor = frame.caret_bounds()?;
        let (range, mark) = link_at(editor.editor().document(), head).ok()??;
        let attrs = mark.attrs.clone();
        let content = (self.0)(
            LinkAtCaret {
                editor: frame.view.clone(),
                range,
                attrs,
            },
            window,
            cx,
        );
        let panel = div()
            .id("gpdoc-link-popover")
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .child(content);
        Some(AnchoredElement::above(anchor, panel))
    }
}
