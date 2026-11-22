//! Per-editor completion state and input dispatch for composable menus.
use crate::SuggestionMatch;
use gpdoc::{AnchoredElement, EditorView, ViewBehavior, ViewFrame, ViewPlugin};
use gpdoc_plugin::core::{Command, CommandOutcome, DocPos, Editor as CoreEditor, StoredCommand};
use gpui::{App, Context, Focusable, Keystroke, Window};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{cell::RefCell, rc::Rc};

mod menu;
pub use menu::{SuggestionMenu, SuggestionRow};

/// The value available to the row renderer and the command used to accept it.
pub struct SuggestionItem<T> {
    pub value: T,
    pub command: StoredCommand,
}

impl<T> SuggestionItem<T> {
    pub fn new(value: T, command: impl Command) -> Self {
        Self {
            value,
            command: StoredCommand::new(command),
        }
    }
}

/// A named view function or a closure capturing application state.
pub trait MenuRenderer<T>:
    Fn(SuggestionMenu<T>, &mut Window, &mut App) -> AnchoredElement + 'static
{
}
impl<T, F> MenuRenderer<T> for F where
    F: Fn(SuggestionMenu<T>, &mut Window, &mut App) -> AnchoredElement + 'static
{
}

type Provider<T> = dyn Fn(&CoreEditor, &SuggestionMatch) -> Vec<SuggestionItem<T>>;

/// Create a view for a completion source. Each editor gets its own picker state.
pub fn suggestion_picker<T: 'static>(
    name: impl Into<String>,
    matcher: impl Fn(&CoreEditor) -> Option<SuggestionMatch> + 'static,
    items: impl Fn(&CoreEditor, &SuggestionMatch) -> Vec<SuggestionItem<T>> + 'static,
    render: impl MenuRenderer<T>,
) -> ViewPlugin {
    let matcher = Rc::new(matcher);
    let items = Rc::new(items);
    let render_menu = Rc::new(render);
    ViewPlugin::new(name, move || Picker {
        matcher: matcher.clone(),
        items: items.clone(),
        render_menu: render_menu.clone(),
        state: RefCell::new(State::Idle),
    })
}

enum State<T> {
    Idle,
    Dismissed(DocPos),
    Open {
        matched: SuggestionMatch,
        revision: u64,
        items: Rc<[SuggestionItem<T>]>,
        selected: usize,
    },
}

type Matcher = dyn Fn(&gpdoc_plugin::core::Editor) -> Option<SuggestionMatch>;

struct Picker<T> {
    matcher: Rc<Matcher>,
    items: Rc<Provider<T>>,
    render_menu: Rc<dyn MenuRenderer<T>>,
    state: RefCell<State<T>>,
}

impl<T: 'static> Picker<T> {
    fn accept(
        &self,
        index: usize,
        view: &mut EditorView,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        let (start, command) = {
            let state = self.state.borrow();
            let State::Open { matched, items, .. } = &*state else {
                return false;
            };
            let Some(item) = items.get(index) else {
                return false;
            };
            (matched.range.start, item.command.clone())
        };
        match view.run_stored(&command, window, cx) {
            Ok(CommandOutcome::Applied(_)) => {
                self.state.replace(State::Dismissed(start));
                cx.notify();
                true
            }
            Ok(CommandOutcome::NotApplicable) => false,
            Err(error) => {
                eprintln!("Could not accept suggestion: {error}");
                true
            }
        }
    }
}

impl<T: 'static> ViewBehavior for Picker<T> {
    fn update(&self, view: &EditorView) {
        let editor = view.editor();
        let matched = (!view.is_composing())
            .then(|| (self.matcher)(editor))
            .flatten();
        let Some(matched) = matched else {
            self.state.replace(State::Idle);
            return;
        };
        match &*self.state.borrow() {
            State::Dismissed(at) if *at == matched.range.start => return,
            State::Open {
                matched: previous,
                revision,
                ..
            } if *previous == matched && *revision == editor.revision() => return,
            _ => {}
        }
        let items = (self.items)(editor, &matched).into();
        self.state.replace(State::Open {
            matched,
            revision: editor.revision(),
            items,
            selected: 0,
        });
    }

    fn key_down(
        &self,
        view: &mut EditorView,
        stroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        if stroke.modifiers != Default::default()
            || view.is_composing()
            || !view.focus_handle(cx).is_focused(window)
        {
            return false;
        }
        let mut state = self.state.borrow_mut();
        let State::Open {
            matched,
            items,
            selected,
            ..
        } = &mut *state
        else {
            return false;
        };
        let n = items.len();
        if n == 0 {
            return false;
        }
        match stroke.key.as_str() {
            "up" => *selected = (*selected + n - 1) % n,
            "down" => *selected = (*selected + 1) % n,
            "escape" => *state = State::Dismissed(matched.range.start),
            "enter" => {
                let index = *selected;
                drop(state);
                return self.accept(index, view, window, cx);
            }
            _ => return false,
        }
        cx.notify();
        true
    }

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
        let anchor = frame.caret_bounds()?;
        let state = self.state.borrow();
        let State::Open {
            matched,
            items,
            selected,
            ..
        } = &*state
        else {
            return None;
        };
        if items.is_empty() {
            return None;
        }
        let menu = SuggestionMenu {
            anchor,
            query: matched.query.clone(),
            selected: *selected,
            items: items.clone(),
            picker: self.clone(),
            editor: frame.view.clone(),
            revision: editor.editor().revision(),
        };
        drop(state);
        Some((self.render_menu)(menu, window, cx).exclusive())
    }
}
