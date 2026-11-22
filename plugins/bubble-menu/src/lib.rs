//! Menus anchored to the current editor selection.
use gpdoc::{AnchoredElement, EditorView, ViewBehavior, ViewFrame, ViewPlugin};
use gpdoc_plugin::core::Selection;
use gpui::{AnyElement, App, Entity, MouseButton, SharedString, Window, div, prelude::*};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

/// Configure a menu above a nonempty document selection.
pub fn bubble_menu() -> SelectionMenu {
    SelectionMenu {
        name: "bubbleMenu".into(),
        drag_delay: Duration::ZERO,
        should_show: Rc::new(|view| match view.editor().selection() {
            Selection::Text { anchor, head } => anchor != head,
            Selection::Gap { .. } => false,
            _ => true,
        }),
    }
}

/// Placement and visibility for an application-rendered selection menu.
#[must_use]
pub struct SelectionMenu {
    name: String,
    drag_delay: Duration,
    should_show: Rc<dyn Fn(&EditorView) -> bool>,
}

impl From<SelectionMenu> for gpdoc::EditorPlugin {
    fn from(menu: SelectionMenu) -> Self {
        ViewPlugin::named(menu.name).into()
    }
}

impl SelectionMenu {
    /// Give the menu its own registration name when installing multiple menus.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Hide the menu during text selection by mouse and wait this long after release.
    /// A zero delay keeps the default behavior of showing the menu during selection.
    pub fn drag_delay(mut self, delay: Duration) -> Self {
        self.drag_delay = delay;
        self
    }

    /// Replace the selection predicate. The editor must still be focused and outside composition.
    pub fn show_when(mut self, should_show: impl Fn(&EditorView) -> bool + 'static) -> Self {
        self.should_show = Rc::new(should_show);
        self
    }

    /// Compose the menu's GPUI contents. The plugin anchors them and preserves editor focus.
    pub fn with_view<E: IntoElement>(
        self,
        render: impl Fn(Entity<EditorView>, &mut Window, &mut App) -> E + 'static,
    ) -> ViewPlugin {
        let id = SharedString::from(self.name.clone());
        let render: Renderer =
            Rc::new(move |view, window, cx| render(view, window, cx).into_any_element());
        ViewPlugin::new(self.name, move || MenuView {
            id: id.clone(),
            drag_delay: self.drag_delay,
            pending: RefCell::default(),
            should_show: self.should_show.clone(),
            render: render.clone(),
        })
    }
}

type Renderer = Rc<dyn Fn(Entity<EditorView>, &mut Window, &mut App) -> AnyElement>;

struct PendingMenu {
    selection: Selection,
    ready: Rc<Cell<bool>>,
    _timer: gpui::Task<()>,
}

struct MenuView {
    id: SharedString,
    drag_delay: Duration,
    pending: RefCell<Option<PendingMenu>>,
    should_show: Rc<dyn Fn(&EditorView) -> bool>,
    render: Renderer,
}

impl ViewBehavior for MenuView {
    fn mouse_up(
        &self,
        view: &mut EditorView,
        _: &gpui::MouseUpEvent,
        _: &mut Window,
        cx: &mut gpui::Context<EditorView>,
    ) -> bool {
        let selection = view.editor().selection();
        if self.drag_delay.is_zero()
            || !view.is_dragging_selection()
            || !matches!(selection, Selection::Text { anchor, head } if anchor != head)
            || !(self.should_show)(view)
        {
            return false;
        }

        let ready = Rc::new(Cell::new(false));
        let elapsed = ready.clone();
        let timer = cx.background_executor().timer(self.drag_delay);
        let task = cx.spawn(async move |view, cx| {
            timer.await;
            elapsed.set(true);
            let _ = view.update(cx, |_, cx| cx.notify());
        });
        self.pending.replace(Some(PendingMenu {
            selection,
            ready,
            _timer: task,
        }));
        false
    }

    fn render(
        self: Rc<Self>,
        frame: &ViewFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnchoredElement> {
        let editor = frame.view.read(cx);
        if !frame.focused
            || editor.is_composing()
            || !(self.should_show)(editor)
            || (!self.drag_delay.is_zero() && editor.is_dragging_selection())
        {
            self.pending.take();
            return None;
        }
        if self
            .pending
            .borrow()
            .as_ref()
            .is_some_and(|p| p.selection == editor.editor().selection() && !p.ready.get())
        {
            return None;
        }
        self.pending.take();
        let anchor = frame.selection_bounds()?;
        let menu = div()
            .id(self.id.clone())
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .child((self.render)(frame.view.clone(), window, cx));
        Some(AnchoredElement::above(anchor, menu))
    }
}
