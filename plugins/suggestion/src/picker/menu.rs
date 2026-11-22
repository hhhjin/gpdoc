use super::{Picker, State, SuggestionItem};
use gpdoc::{AnchoredElement, EditorView, ViewBehavior};
use gpui::{Bounds, Div, Entity, IntoElement, MouseButton, Pixels, Stateful, div, prelude::*};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::rc::Rc;

/// An open completion menu for one editor and one set of search results.
pub struct SuggestionMenu<T> {
    pub(super) anchor: Bounds<Pixels>,
    pub(super) query: String,
    pub(super) selected: usize,
    pub(super) items: Rc<[SuggestionItem<T>]>,
    pub(super) picker: Rc<Picker<T>>,
    pub(super) editor: Entity<EditorView>,
    pub(super) revision: u64,
}

impl<T: 'static> SuggestionMenu<T> {
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Place application content at the caret and keep it inside the editor.
    pub fn popover(&self, content: impl IntoElement) -> AnchoredElement {
        AnchoredElement::below(self.anchor, content)
    }

    /// A vertical list that keeps the active row in a window of at most `limit` rows.
    pub fn list<E: IntoElement>(
        &self,
        limit: usize,
        render: impl FnMut(SuggestionRow<'_, T>) -> E,
    ) -> Div {
        let limit = limit.max(1);
        div().flex().flex_col().children(
            self.rows()
                .skip(self.selected.saturating_sub(limit - 1))
                .take(limit)
                .map(render),
        )
    }

    /// A caret-anchored text list with a marker for the current keyboard selection.
    pub fn labeled_list(&self, label: impl Fn(&T) -> String) -> AnchoredElement {
        self.popover(self.list(8, |row| {
            let marker = if row.is_active() { "› " } else { "  " };
            row.element().child(format!("{marker}{}", label(row.value)))
        }))
    }

    /// All results, for applications that implement their own list layout.
    pub fn rows(&self) -> impl ExactSizeIterator<Item = SuggestionRow<'_, T>> {
        self.items
            .iter()
            .enumerate()
            .map(|(index, item)| SuggestionRow {
                menu: self,
                index,
                value: &item.value,
            })
    }
}

pub struct SuggestionRow<'a, T> {
    menu: &'a SuggestionMenu<T>,
    index: usize,
    pub value: &'a T,
}

impl<T: 'static> SuggestionRow<'_, T> {
    pub fn is_active(&self) -> bool {
        self.menu.selected == self.index
    }

    /// Compose children inside this hit target to retain editor focus and click-to-accept.
    pub fn element(&self) -> Stateful<Div> {
        let index = self.index;
        let editor = self.menu.editor.clone();
        let picker = self.menu.picker.clone();
        let expected = self.menu.items.clone();
        let revision = self.menu.revision;
        div().id(("suggestion", index))
            .debug_selector(move || format!("gpdoc-suggestion-{index}"))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.prevent_default();
                editor.update(cx, |view, cx| {
                    picker.update(view);
                    let current = matches!(&*picker.state.borrow(), State::Open { items, .. } if Rc::ptr_eq(items, &expected));
                    if current && view.editor().revision() == revision {
                        picker.accept(index, view, window, cx);
                    }
                });
                cx.stop_propagation();
            })
    }
}
