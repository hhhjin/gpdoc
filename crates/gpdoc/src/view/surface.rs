use super::*;
use gpui::{Div, Entity, StyleRefinement};

/// A scrollable input surface for one editor. Place one surface per editor entity.
/// Styles apply to the viewport. Use [Self::layout] to place its editable document.
#[derive(gpui::IntoElement)]
pub struct EditorSurface<E: IntoElement + 'static = Div> {
    editor: Entity<EditorView>,
    root: Div,
    document: E,
}

impl EditorSurface {
    /// Create a full-size surface with a full-width document.
    pub fn new(editor: &Entity<EditorView>) -> Self {
        Self {
            editor: editor.clone(),
            root: div().size_full(),
            document: div().w_full().child(TextElement {
                view: editor.clone(),
            }),
        }
    }

    /// Place the supplied document once in a GPUI layout inside the scroll viewport.
    /// The surrounding space stays inside the editor’s pointer and scroll region.
    pub fn layout<E: IntoElement + 'static>(
        self,
        place: impl FnOnce(Div) -> E,
    ) -> EditorSurface<E> {
        EditorSurface {
            editor: self.editor,
            root: self.root,
            document: place(self.document),
        }
    }
}

impl<E: IntoElement + 'static> Styled for EditorSurface<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.root.style()
    }
}

impl<E: IntoElement + 'static> RenderOnce for EditorSurface<E> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.editor.update(cx, |view, cx| {
            view.bind_surface(self.root, self.document, window, cx)
        })
    }
}

impl EditorView {
    fn bind_surface(
        &mut self,
        root: Div,
        document: impl IntoElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        for behavior in &self.behaviors {
            behavior.update(self);
        }
        if self.input_context_listeners.is_empty() {
            self.observe_input_context(window, cx);
        }
        let surface = root
            .id(("gpdoc-editor", cx.entity().entity_id()))
            .key_context("gpdocEditor")
            .track_focus(&self.focus)
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_action(cx.listener(|v, _: &Left, w, c| v.horizontal(false, false, w, c)))
            .on_action(cx.listener(|v, _: &Right, w, c| v.horizontal(true, false, w, c)))
            .on_action(cx.listener(|v, _: &Up, w, c| v.vertical(false, false, w, c)))
            .on_action(cx.listener(|v, _: &Down, w, c| v.vertical(true, false, w, c)))
            .on_action(cx.listener(|v, _: &ExtendLeft, w, c| v.horizontal(false, true, w, c)))
            .on_action(cx.listener(|v, _: &ExtendRight, w, c| v.horizontal(true, true, w, c)))
            .on_action(cx.listener(|v, _: &ExtendUp, w, c| v.vertical(false, true, w, c)))
            .on_action(cx.listener(|v, _: &ExtendDown, w, c| v.vertical(true, true, w, c)))
            .on_action(
                cx.listener(|v, _: &MoveToLineLeft, w, c| v.move_to_line_edge(false, false, w, c)),
            )
            .on_action(
                cx.listener(|v, _: &MoveToLineRight, w, c| v.move_to_line_edge(true, false, w, c)),
            )
            .on_action(cx.listener(|v, _: &MoveToDocumentStart, w, c| {
                v.move_to_document_edge(false, false, w, c)
            }))
            .on_action(cx.listener(|v, _: &MoveToDocumentEnd, w, c| {
                v.move_to_document_edge(true, false, w, c)
            }))
            .on_action(
                cx.listener(|v, _: &ExtendToLineLeft, w, c| v.move_to_line_edge(false, true, w, c)),
            )
            .on_action(
                cx.listener(|v, _: &ExtendToLineRight, w, c| v.move_to_line_edge(true, true, w, c)),
            )
            .on_action(cx.listener(|v, _: &ExtendToDocumentStart, w, c| {
                v.move_to_document_edge(false, true, w, c)
            }))
            .on_action(cx.listener(|v, _: &ExtendToDocumentEnd, w, c| {
                v.move_to_document_edge(true, true, w, c)
            }))
            .on_action(cx.listener(|v, _: &Backspace, w, c| v.delete(false, w, c)))
            .on_action(cx.listener(|v, _: &DeleteForward, w, c| v.delete(true, w, c)))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste_action))
            .on_action(cx.listener(|v, _: &Undo, w, c| v.apply_history(false, w, c)))
            .on_action(cx.listener(|v, _: &Redo, w, c| v.apply_history(true, w, c)))
            .child(document);
        self.extensions
            .surface_handlers
            .iter()
            .fold(surface, |surface, attach| attach(surface, cx))
    }
}
