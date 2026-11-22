//! Attach externally managed document state and history to an editor view.
use crate::EditorView;
use gpdoc_core::{Editor as CoreEditor, Error};
use gpui::{Context, Window};
use std::any::Any;

/// The result of reconciling the view's committed edits with a session.
pub enum SessionUpdate {
    /// Keep the current view, including its input-method composition.
    Keep,
    /// Open the session's current editor in the view.
    Reload,
}

/// A document session owns the authoritative editor and its undo and redo history.
/// The view reconciles committed content after the input method has finished
/// composing. Sessions can queue incoming changes until that point.
pub trait EditSession: Any {
    fn editor(&self) -> &CoreEditor;

    /// If synchronization fails, the view reloads the last accepted editor.
    fn synchronize(&mut self, committed: &CoreEditor) -> Result<SessionUpdate, Error>;

    fn undo(&mut self) -> Result<(), Error>;
    fn redo(&mut self) -> Result<(), Error>;
}

impl EditorView {
    /// Adopt the session's document after checking that it belongs to the view's editor config.
    /// Rebinding ends any preview from the old document. A rejected binding
    /// leaves both the existing session and its input composition intact.
    pub fn bind_session(
        &mut self,
        session: impl EditSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Error> {
        self.replace_session_editor(session.editor().fork())?;
        self.session = Some(Box::new(session));
        self.notify_change(window, cx);
        Ok(())
    }

    pub fn session<S: EditSession>(&self) -> Option<&S> {
        (self.session.as_deref()? as &dyn Any).downcast_ref()
    }

    pub fn session_mut<S: EditSession>(&mut self) -> Option<&mut S> {
        (self.session.as_deref_mut()? as &mut dyn Any).downcast_mut()
    }

    /// Reconcile updates queued by the session and refresh document geometry.
    pub fn refresh_session(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Error> {
        if self.is_composing() {
            return Ok(());
        }
        let result = self.synchronize_session();
        self.notify_change(window, cx);
        result
    }

    pub(crate) fn synchronize_session(&mut self) -> Result<(), Error> {
        let Some(session) = &mut self.session else {
            return Ok(());
        };
        if self.buffer.has_preedit() {
            return Ok(());
        }
        let result = session.synchronize(self.buffer.editor());
        if !matches!(result, Ok(SessionUpdate::Keep)) {
            let next = session.editor().fork();
            self.replace_session_editor(next)?;
        }
        result.map(|_| ())
    }

    pub(crate) fn view_history(&mut self, redo: bool) -> Result<(), Error> {
        if self.session.is_none() {
            return if redo {
                self.buffer.redo()
            } else {
                self.buffer.undo()
            };
        }
        if !redo && self.buffer.has_preedit() {
            self.buffer.cancel_mark();
            return self.synchronize_session();
        }
        self.buffer.unmark()?;
        self.synchronize_session()?;
        let session = self.session.as_deref_mut().expect("bound editing session");
        if redo {
            session.redo()?;
        } else {
            session.undo()?;
        }
        let next = session.editor().fork();
        self.replace_session_editor(next)
    }
}
