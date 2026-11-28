use std::ops::Range;

use gpdoc_core::{
    Command, CommandOutcome, Editor as CoreEditor, Error, Selection, Shortcut, StoredCommand,
};
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler, FocusHandle, Focusable,
    Global, KeyBinding, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollHandle, Subscription, UTF16Selection, Window, actions, div,
    point, prelude::*, px,
};

use super::{
    buffer::InputBuffer,
    gpui_api,
    layout::{Caret, CaretEdge, TextElement, TextLayout},
};

mod input;
mod surface;
pub use surface::EditorSurface;

actions!(
    gpdoc,
    [
        Left,
        Right,
        Up,
        Down,
        ExtendLeft,
        ExtendRight,
        ExtendUp,
        ExtendDown,
        MoveToLineLeft,
        MoveToLineRight,
        MoveToDocumentStart,
        MoveToDocumentEnd,
        ExtendToLineLeft,
        ExtendToLineRight,
        ExtendToDocumentStart,
        ExtendToDocumentEnd,
        Backspace,
        DeleteForward,
        Newline,
        Cancel,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
    ]
);

struct EditorKeys;

impl Global for EditorKeys {}

fn ensure_editor_keys(cx: &mut App) {
    if cx.has_global::<EditorKeys>() {
        return;
    }
    cx.set_global(EditorKeys);
    let context = Some("gpdocEditor");
    let is_mac = gpui_api::is_macos(cx);
    let [select_all, copy, cut, undo, redo] = if is_mac {
        ["cmd-a", "cmd-c", "cmd-x", "cmd-z", "cmd-shift-z"]
    } else {
        ["ctrl-a", "ctrl-c", "ctrl-x", "ctrl-z", "ctrl-shift-z"]
    };
    cx.bind_keys([
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("shift-left", ExtendLeft, context),
        KeyBinding::new("shift-right", ExtendRight, context),
        KeyBinding::new("shift-up", ExtendUp, context),
        KeyBinding::new("shift-down", ExtendDown, context),
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("delete", DeleteForward, context),
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("escape", Cancel, context),
        KeyBinding::new(select_all, SelectAll, context),
        KeyBinding::new(copy, Copy, context),
        KeyBinding::new(cut, Cut, context),
        KeyBinding::new(undo, Undo, context),
        KeyBinding::new(redo, Redo, context),
    ]);
    // The browser delivers keyboard and menu pastes through EntityInputHandler::paste.
    #[cfg(not(target_family = "wasm"))]
    {
        let paste = if is_mac { "cmd-v" } else { "ctrl-v" };
        cx.bind_keys([KeyBinding::new(paste, Paste, context)]);
    }
    if is_mac {
        cx.bind_keys([
            KeyBinding::new("cmd-left", MoveToLineLeft, context),
            KeyBinding::new("cmd-right", MoveToLineRight, context),
            KeyBinding::new("cmd-up", MoveToDocumentStart, context),
            KeyBinding::new("cmd-down", MoveToDocumentEnd, context),
            KeyBinding::new("cmd-shift-left", ExtendToLineLeft, context),
            KeyBinding::new("cmd-shift-right", ExtendToLineRight, context),
            KeyBinding::new("cmd-shift-up", ExtendToDocumentStart, context),
            KeyBinding::new("cmd-shift-down", ExtendToDocumentEnd, context),
        ]);
    } else {
        cx.bind_keys([
            KeyBinding::new("home", MoveToLineLeft, context),
            KeyBinding::new("end", MoveToLineRight, context),
            KeyBinding::new("ctrl-home", MoveToDocumentStart, context),
            KeyBinding::new("ctrl-end", MoveToDocumentEnd, context),
            KeyBinding::new("ctrl-y", Redo, context),
        ]);
    }
}

/// Check the platform's primary shortcut modifier (Command on macOS, Control elsewhere).
pub fn primary_pressed(modifiers: Modifiers, cx: &App) -> bool {
    if gpui_api::is_macos(cx) {
        modifiers.platform
    } else {
        modifiers.control
    }
}

/// Live editing state shared by document layout, input, and plugins.
pub struct EditorView {
    pub(crate) session: Option<Box<dyn super::EditSession>>,
    pub(super) buffer: InputBuffer,
    pub(super) focus: FocusHandle,
    pub(super) layout: Option<TextLayout>,
    pub(crate) block_views: std::rc::Rc<super::node_view::BlockViews>,
    pub(crate) node_views: std::collections::BTreeMap<String, super::node_view::Renderer>,
    pub(crate) block_drag: Option<gpui::WeakEntity<Option<crate::view_extensions::BlockHover>>>,
    pub(crate) drop_indicator_kind: crate::view_extensions::DropIndicator,
    pub(crate) behaviors: Vec<std::rc::Rc<dyn super::ViewBehavior>>,
    pub(crate) extensions: crate::view_extensions::ViewExtensions,
    media_base_url: Option<url::Url>,
    geometry_dirty: bool,
    dragging: bool,
    pointer_owner: Option<std::rc::Rc<dyn super::ViewBehavior>>,
    preferred_x: Option<Pixels>,
    caret_edge: CaretEdge,
    pub(crate) scroll: ScrollHandle,
    reveal_head: bool,
    input_context_listeners: Vec<Subscription>,
}

impl EditorView {
    pub(super) fn from_buffer(
        buffer: InputBuffer,
        views: &crate::view_extensions::ViewConfig,
        cx: &mut Context<Self>,
    ) -> Self {
        ensure_editor_keys(cx);
        let behaviors = views
            .extensions
            .behaviors
            .iter()
            .map(|create| create())
            .collect();
        Self {
            session: None,
            buffer,
            focus: cx.focus_handle(),
            layout: None,
            node_views: views.node_views.clone(),
            block_views: views.block_views.clone(),
            extensions: views.extensions.clone(),
            behaviors,
            block_drag: None,
            drop_indicator_kind: crate::view_extensions::DropIndicator::Block(None),
            media_base_url: None,
            geometry_dirty: false,
            dragging: false,
            pointer_owner: None,
            preferred_x: None,
            caret_edge: CaretEdge::Leading,
            scroll: ScrollHandle::new(),
            reveal_head: false,
            input_context_listeners: Vec::new(),
        }
    }

    /// Focus the text input after a plugin control handles a click.
    pub fn focus_in(&self, window: &mut Window, cx: &mut Context<Self>) {
        gpui_api::focus(window, &self.focus_handle(cx), cx);
    }

    /// Set the document URL used to open relative media sources on double-click.
    pub fn set_media_base_url(&mut self, url: Option<url::Url>) {
        self.media_base_url = url;
    }

    /// Resolve a media control's source using the same rules as native document activation.
    pub fn resolve_media_url(&self, source: &str) -> Option<url::Url> {
        crate::layout::activation_url(source, self.media_base_url.as_ref(), false)
    }

    /// Whether the input method currently has an uncommitted composition.
    pub fn is_composing(&self) -> bool {
        self.buffer.has_preedit()
    }

    /// The editor with its committed document and history.
    pub fn editor(&self) -> &CoreEditor {
        self.buffer.editor()
    }

    /// Run a plugin command from a menu or toolbar. Confirm any preedit before planning the edit.
    pub fn run(
        &mut self,
        command: impl Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, Error> {
        self.execute(|editor| editor.run(command), window, cx)
    }

    /// Run a stored command through the view's input and edit lifecycle.
    pub fn run_stored(
        &mut self,
        command: &StoredCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, Error> {
        self.execute(|editor| editor.run_stored(command), window, cx)
    }

    fn execute(
        &mut self,
        command: impl FnOnce(&mut CoreEditor) -> Result<CommandOutcome, Error>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, Error> {
        let had_preedit = self.buffer.has_preedit();
        let result = self.buffer.execute(command);
        if matches!(result, Ok(CommandOutcome::Applied(_)))
            || (had_preedit && !self.buffer.has_preedit())
        {
            self.edited(Ok(()), window, cx);
        }
        result
    }

    pub fn apply_node_view(
        &mut self,
        tx: gpdoc_core::Transaction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Error> {
        self.apply_node_view_if_accepted(tx, window, cx).map(|_| ())
    }

    pub(crate) fn apply_node_view_if_accepted(
        &mut self,
        tx: gpdoc_core::Transaction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, Error> {
        let accepted = self.buffer.apply_node_view(tx)?;
        if accepted {
            self.edited(Ok(()), window, cx);
        }
        Ok(accepted)
    }

    fn plugin_key(&mut self, stroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.has_preedit() {
            return;
        }
        for behavior in self.behaviors.clone() {
            behavior.update(self);
            if behavior.key_down(self, stroke, window, cx) {
                cx.stop_propagation();
                return;
            }
        }
        let mods = stroke.modifiers;
        let other_modifier = if gpui_api::is_macos(cx) {
            mods.control
        } else {
            mods.platform
        };
        if other_modifier || mods.function {
            return;
        }
        let primary = primary_pressed(mods, cx);
        let mut key = Shortcut::new(&stroke.key);
        key.primary = primary;
        key.shift = mods.shift;
        key.alt = mods.alt;
        let config = self.editor().config().clone();
        for command in config.shortcuts(&key) {
            match self.run_stored(command, window, cx) {
                Ok(CommandOutcome::Applied(_)) => {
                    cx.stop_propagation();
                    break;
                }
                Ok(CommandOutcome::NotApplicable) => {}
                Err(error) => {
                    eprintln!("gpdoc could not run '{}': {error}", command.name());
                    cx.stop_propagation();
                    break;
                }
            }
        }
    }

    pub(super) fn set_layout(
        &mut self,
        layout: TextLayout,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let (_, head) = self.buffer.selection();
        let head = self.caret_at(head);
        let caret = layout.caret_bounds(head);
        if self.buffer.marked_range().is_some()
            && self.layout.as_ref().and_then(|old| old.caret_bounds(head)) != caret
        {
            // The window may have resized or scrolled while the candidate panel was open.
            window.invalidate_character_coordinates();
        }
        if self.reveal_head {
            self.reveal_head = false;
            if let Some(caret) = caret {
                let viewport = self.scroll.bounds();
                let dy = if caret.top() < viewport.top() {
                    viewport.top() - caret.top()
                } else if caret.bottom() > viewport.bottom() {
                    viewport.bottom() - caret.bottom()
                } else {
                    px(0.)
                };
                let old = self.scroll.offset();
                #[cfg(target_family = "wasm")]
                let max_y = self.scroll.max_offset().y;
                #[cfg(not(target_family = "wasm"))]
                let max_y = self.scroll.max_offset().height;
                let y = (old.y + dy).clamp(-max_y, px(0.));
                if old.y != y {
                    self.scroll.set_offset(point(old.x, y));
                    cx.notify();
                }
            }
        }
        self.layout = Some(layout);
        self.geometry_dirty = false;
    }

    // Cocoa may query candidate-window coordinates between a marked-text update and the next paint.
    pub(crate) fn geometry(&mut self, window: &mut Window) -> Option<&TextLayout> {
        if self.geometry_dirty {
            if !self
                .layout
                .as_mut()?
                .refresh(&self.buffer, &self.block_views, window)
            {
                self.layout = None;
            }
            self.geometry_dirty = false;
        }
        self.layout.as_ref()
    }

    pub(crate) fn apply_history(
        &mut self,
        redo: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self.view_history(redo);
        self.edited(result, window, cx);
    }

    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.synchronize_session() {
            eprintln!("gpdoc could not synchronize the editing session: {error}");
        }
        self.notify_change(window, cx);
    }

    pub(crate) fn replace_session_editor(&mut self, editor: CoreEditor) -> Result<(), Error> {
        if !self.editor().config().same_instance(editor.config()) {
            return Err(Error::InvalidSchema(
                "session belongs to a different editor config".into(),
            ));
        }
        self.buffer = InputBuffer::with_folds(editor, self.buffer.fold_specs.clone())?;
        self.geometry_dirty = true;
        self.preferred_x = None;
        self.caret_edge = CaretEdge::Leading;
        Ok(())
    }

    pub(crate) fn notify_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_head = true;
        window.invalidate_character_coordinates();
        cx.notify();
    }

    pub(crate) fn edited(
        &mut self,
        result: Result<(), Error>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => {
                self.preferred_x = None;
                self.caret_edge = CaretEdge::Leading;
                self.geometry_dirty = true;
                self.changed(window, cx);
            }
            Err(Error::NoHistory) => {}
            Err(error) => eprintln!("gpdoc could not apply the edit: {error}"),
        }
    }

    /// Set a validated document selection and refresh the focused editor.
    pub fn select_document(
        &mut self,
        selection: Selection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Error> {
        self.geometry_dirty |= self.buffer.has_preedit();
        self.buffer.select_doc(selection)?;
        self.preferred_x = None;
        self.caret_edge = CaretEdge::Leading;
        self.changed(window, cx);
        Ok(())
    }

    /// Set a document selection while preserving the viewport during a pointer gesture.
    pub fn select_document_in_place(
        &mut self,
        selection: Selection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Error> {
        self.select_document(selection, window, cx)?;
        self.reveal_head = false;
        Ok(())
    }

    /// Whether a primary-button gesture is extending the native text selection.
    pub fn is_dragging_selection(&self) -> bool {
        self.dragging
    }

    pub(super) fn caret_at(&self, offset: usize) -> Caret {
        Caret {
            offset,
            edge: self.caret_edge,
        }
    }

    fn select(&mut self, anchor: usize, head: Caret, window: &mut Window, cx: &mut Context<Self>) {
        let had_preedit = self.buffer.has_preedit();
        self.geometry_dirty |= had_preedit;
        match self.buffer.select(anchor, head.offset) {
            Ok(()) => {
                self.caret_edge = if !had_preedit && self.buffer.selection().1 == head.offset {
                    head.edge
                } else {
                    CaretEdge::Leading
                };
                self.changed(window, cx);
            }
            Err(error) => eprintln!("gpdoc could not select the requested text: {error}"),
        }
    }

    fn horizontal(
        &mut self,
        forward: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferred_x = None;
        self.geometry_dirty |= self.buffer.has_preedit();
        match self.buffer.move_by_grapheme(forward, extend) {
            Ok(()) => {
                self.caret_edge = CaretEdge::Leading;
                self.changed(window, cx);
            }
            Err(error) => eprintln!("gpdoc could not move the selection: {error}"),
        }
    }

    fn vertical(&mut self, down: bool, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (anchor, head) = self.buffer.selection();
        let caret = self.caret_at(head);
        let preferred = self.preferred_x;
        self.geometry(window);
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let redirect = |default| {
            let Ok(from) = self.buffer.position(head) else {
                return default;
            };
            let beyond = |boundary| layout.section_beyond(boundary, down, &self.buffer);
            let neighbor = |boundary| beyond(boundary).map(|(_, pos)| pos);
            let doc = self.buffer.state().0;
            match self
                .behaviors
                .iter()
                .find_map(|b| b.vertical_block_boundary(doc, from, down, &neighbor))
            {
                Some(boundary) => beyond(boundary).map(|(section, _)| section),
                None => default,
            }
        };
        let Some((dest, x)) = layout.vertical_destination(caret, preferred, down, redirect) else {
            return;
        };
        self.preferred_x = Some(x);
        self.select(if extend { anchor } else { dest.offset }, dest, window, cx);
    }

    fn move_to_line_edge(
        &mut self,
        right: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_preedit(window, cx);
        let (anchor, head) = self.buffer.selection();
        let head = self.caret_at(head);
        if let Some(dest) = self
            .geometry(window)
            .and_then(|layout| layout.line_edge(head, right))
        {
            self.preferred_x = None;
            self.select(if extend { anchor } else { dest.offset }, dest, window, cx);
        }
    }

    fn move_to_document_edge(
        &mut self,
        end: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_preedit(window, cx);
        if let Some(offset) = self.buffer.projection.document_edge(end) {
            let anchor = if extend {
                self.buffer.selection().0
            } else {
                offset
            };
            self.preferred_x = None;
            self.select(anchor, Caret::leading(offset), window, cx);
        }
    }

    fn delete(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.buffer.delete_grapheme_or_selection(forward);
        self.edited(result, window, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        // GPUI offers composing keystrokes to the macOS input context before dispatching actions.
        let result = if self.buffer.marked_range().is_some() {
            self.buffer.unmark()
        } else {
            self.buffer.paragraph_break()
        };
        self.edited(result, window, cx);
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.marked_range().is_some() {
            self.buffer.cancel_mark();
            self.edited(Ok(()), window, cx);
        } else {
            cx.propagate();
        }
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        let end = self.buffer.text().encode_utf16().count();
        self.select(0, Caret::leading(end), window, cx);
    }

    fn selected_text(&self) -> Option<String> {
        let (anchor, head) = self.buffer.selection();
        if anchor == head {
            return None;
        }
        self.buffer
            .text_for_range(anchor.min(head)..anchor.max(head))
            .map(|(text, _)| text)
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn clipboard_replacement(&self) -> Option<Range<usize>> {
        // A clipboard action targets the visible selection. Native typing with no explicit range
        // instead replaces the whole preedit. Without a composition, preserve engine node selections.
        self.buffer.marked_range().map(|_| {
            let (anchor, head) = self.buffer.selection();
            anchor.min(head)..anchor.max(head)
        })
    }

    fn paste_action(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = cx.read_from_clipboard() {
            self.paste_item(item, window, cx);
        }
    }

    fn paste_item(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut Context<Self>) {
        for behavior in self.behaviors.clone() {
            if behavior.paste(self, &item, window, cx) {
                return;
            }
        }
        if let Some(text) = item.text() {
            let result = self.buffer.paste(self.clipboard_replacement(), &text);
            self.edited(result, window, cx);
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            let result = self.buffer.replace(self.clipboard_replacement(), "");
            if result.is_ok() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            self.edited(result, window, cx);
        }
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pointer();
        gpui_api::focus(window, &self.focus, cx);
        #[cfg(target_family = "wasm")]
        window.request_virtual_keyboard();
        if (event.click_count == 2 || primary_pressed(event.modifiers, cx))
            && !self.buffer.has_preedit()
        {
            self.geometry(window);
            if let Some(url) = self.layout.as_ref().and_then(|layout| {
                layout.url_at(
                    event.position,
                    self.buffer.formats(),
                    self.media_base_url.as_ref(),
                )
            }) {
                cx.open_url(url.as_str());
                return;
            }
        }
        for behavior in self.behaviors.clone() {
            if behavior.mouse_down(self, event, window, cx) {
                self.pointer_owner = Some(behavior);
                return;
            }
        }
        let Some(layout) = self.geometry(window) else {
            return;
        };
        let pos = layout.index_for_point(event.position);
        let anchor = if event.modifiers.shift {
            self.buffer.selection().0
        } else {
            pos
        };
        self.dragging = true;
        self.preferred_x = None;
        self.select(anchor, Caret::leading(pos), window, cx);
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        for behavior in self.pointer_handlers() {
            if behavior.mouse_move(self, event, window, cx) {
                if event.dragging() {
                    self.pointer_owner = Some(behavior);
                }
                return;
            }
        }
        if self.dragging && event.dragging() {
            let Some(layout) = self.geometry(window) else {
                return;
            };
            let pos = layout.index_for_point(event.position);
            let (anchor, head) = self.buffer.selection();
            if head != pos {
                self.select(anchor, Caret::leading(pos), window, cx);
            }
        }
    }

    fn cancel_pointer(&mut self) {
        self.dragging = false;
        self.pointer_owner = None;
        for behavior in &self.behaviors {
            behavior.pointer_cancel();
        }
    }

    fn pointer_handlers(&self) -> Vec<std::rc::Rc<dyn super::ViewBehavior>> {
        let mut handlers = self.behaviors.clone();
        if let Some(owner) = &self.pointer_owner
            && let Some(i) = handlers
                .iter()
                .position(|other| std::rc::Rc::ptr_eq(owner, other))
        {
            handlers[..=i].rotate_right(1);
        }
        handlers
    }

    /// Find the measured editable child at a window position.
    pub fn child_bounds_at(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
    ) -> Option<(gpdoc_core::DocPos, Bounds<Pixels>)> {
        self.geometry(window)?.child_bounds_at(position)
    }

    /// Last measured bounds of a laid-out child, including its allocated padding.
    pub fn child_bounds(
        &mut self,
        before: gpdoc_core::DocPos,
        window: &mut Window,
    ) -> Option<Bounds<Pixels>> {
        self.geometry(window)?.child_bounds(before)
    }

    fn flush_preedit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.has_preedit() {
            let result = self.buffer.unmark();
            self.edited(result, window, cx);
        }
    }

    fn observe_input_context(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let blur = cx.on_blur(&self.focus, window, |view, window, cx| {
            view.cancel_pointer();
            view.flush_preedit(window, cx);
        });
        let activation = cx.observe_window_activation(window, |view, window, cx| {
            if !window.is_window_active() {
                view.cancel_pointer();
                view.flush_preedit(window, cx);
            }
        });
        let view = cx.weak_entity();
        let handle = window.window_handle();
        let input_source = cx.on_keyboard_layout_change(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = view.update(cx, |view, cx| view.flush_preedit(window, cx));
            });
        });
        // GPUI resolves actions before element key-down listeners. Offer the key to the
        // focused config first so shortcuts can consume a key before its default action.
        let focus = self.focus.clone();
        let view = cx.weak_entity();
        let shortcuts = cx.intercept_keystrokes(move |event, window, cx| {
            if focus.is_focused(window) {
                let _ = view.update(cx, |view, cx| view.plugin_key(&event.keystroke, window, cx));
            }
        });
        let appearance = cx.observe_window_appearance(window, |_, _, cx| cx.notify());
        self.input_context_listeners = vec![blur, activation, input_source, shortcuts, appearance];
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        for behavior in self.pointer_handlers() {
            if behavior.mouse_up(self, event, window, cx) {
                break;
            }
        }
        self.cancel_pointer();
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for EditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        EditorSurface::new(&cx.entity())
    }
}

impl InputBuffer {
    fn move_by_grapheme(&mut self, forward: bool, extend: bool) -> Result<(), Error> {
        let (anchor, head) = self.selection();
        let block_selected = !self.has_preedit()
            && matches!(self.editor().selection(), Selection::Node {before}
                if self.editor().document().node_at(before)
                        .filter(|node| node.is_leaf())
                        .and_then(|node| self.editor().schema().node_spec(&node.kind))
                        .is_some_and(|spec| !spec.inline));
        let dest = if !extend && block_selected {
            if forward {
                head.max(anchor)
                    .saturating_add(1)
                    .min(self.text().encode_utf16().count())
            } else {
                head.min(anchor).saturating_sub(1)
            }
        } else if !extend && anchor != head {
            if forward {
                head.max(anchor)
            } else {
                head.min(anchor)
            }
        } else {
            let neighbors = grapheme_neighbors(self.text(), self.object_bytes(), head);
            if forward {
                neighbors.next
            } else {
                neighbors.previous
            }
        };
        self.select(if extend { anchor } else { dest }, dest)
    }

    fn delete_grapheme_or_selection(&mut self, forward: bool) -> Result<(), Error> {
        let (anchor, head) = self.selection();
        let node_selected = !self.has_preedit()
            && matches!(
                self.editor().selection(),
                Selection::Node { .. } | Selection::Custom { .. } | Selection::NodeRange { .. }
            );
        let range = if node_selected {
            None
        } else if anchor == head {
            let neighbors = grapheme_neighbors(self.text(), self.object_bytes(), head);
            // A native input method can put the caret between scalar values in one grapheme.
            Some(if !neighbors.on_boundary {
                neighbors.previous..neighbors.next
            } else if forward {
                head..neighbors.next
            } else {
                neighbors.previous..head
            })
        } else {
            Some(anchor.min(head)..anchor.max(head))
        };
        self.replace(range, "")
    }
}

struct Neighbors {
    previous: usize,
    next: usize,
    on_boundary: bool,
}

fn grapheme_neighbors(text: &str, objects: &[std::ops::Range<usize>], caret: usize) -> Neighbors {
    let mut start = 0;
    let mut previous = 0;
    for (_, grapheme) in super::buffer::editor_graphemes(text, objects) {
        let end = start + grapheme.encode_utf16().count();
        if caret < end {
            return Neighbors {
                previous: if caret == start { previous } else { start },
                next: end,
                on_boundary: caret == start,
            };
        }
        previous = start;
        start = end;
    }
    Neighbors {
        previous,
        next: start,
        on_boundary: caret == start,
    }
}

#[cfg(test)]
mod language_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn movement_uses_graphemes_across_emoji_combining_marks_and_paragraphs() {
        let text = "A👩‍💻e\u{301}\nB";
        let boundaries = [0, 1, 6, 8, 9, 10];
        for (i, caret) in boundaries.into_iter().enumerate() {
            let neighbors = grapheme_neighbors(text, &[], caret);
            assert!(neighbors.on_boundary);
            assert_eq!(neighbors.previous, boundaries[i.saturating_sub(1)]);
            assert_eq!(
                neighbors.next,
                boundaries[(i + 1).min(boundaries.len() - 1)]
            );
        }
        let inside_emoji = grapheme_neighbors(text, &[], 3);
        assert!(!inside_emoji.on_boundary);
        assert_eq!((inside_emoji.previous, inside_emoji.next), (1, 6));
    }
}
