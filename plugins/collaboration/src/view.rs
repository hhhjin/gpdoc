//! GPUI binding for a Yjs document session.
use crate::Collaboration;
use gpdoc::{EditSession, EditorView, SessionUpdate};
use gpdoc_plugin::core::{Editor as CoreEditor, Error};
use gpui::{Context, Window};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::collections::VecDeque;

/// Bind a native gpdoc view to a collaboration session and exchange updates
/// over an application-owned transport.
pub trait CollaborationView {
    fn bind_collaboration(
        &mut self,
        session: Collaboration,
        send: impl FnMut(Vec<u8>) + 'static,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> Result<(), Error>;

    fn collaboration(&self) -> Option<&Collaboration>;

    fn receive_collaboration_update(
        &mut self,
        update: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> Result<(), Error>;
}

impl CollaborationView for EditorView {
    fn bind_collaboration(
        &mut self,
        session: Collaboration,
        send: impl FnMut(Vec<u8>) + 'static,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> Result<(), Error> {
        self.bind_session(
            Binding {
                session,
                send: Box::new(send),
                pending: VecDeque::new(),
            },
            window,
            cx,
        )
    }

    fn collaboration(&self) -> Option<&Collaboration> {
        Some(&self.session::<Binding>()?.session)
    }

    fn receive_collaboration_update(
        &mut self,
        update: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> Result<(), Error> {
        let binding = self
            .session_mut::<Binding>()
            .ok_or_else(|| Error::InvalidEdit("view has no collaboration session".into()))?;
        binding.pending.push_back(update);
        self.refresh_session(window, cx)
    }
}

struct Binding {
    session: Collaboration,
    send: Box<dyn FnMut(Vec<u8>)>,
    pending: VecDeque<Vec<u8>>,
}

fn editor_error(error: crate::Error) -> Error {
    match error {
        crate::Error::Editor(e) => e,
        other => Error::InvalidEdit(other.to_string()),
    }
}

impl EditSession for Binding {
    fn editor(&self) -> &CoreEditor {
        self.session.editor()
    }

    fn synchronize(&mut self, committed: &CoreEditor) -> Result<SessionUpdate, Error> {
        if let Some(update) = self.session.record(committed).map_err(editor_error)? {
            (self.send)(update);
        }
        let revision = self.session.editor().revision();
        let mut rejected = None;
        while let Some(update) = self.pending.pop_front() {
            if let Err(err) = self.session.apply_update(&update) {
                rejected.get_or_insert(err);
            }
        }
        if let Some(err) = rejected {
            return Err(editor_error(err));
        }
        Ok(if self.session.editor().revision() == revision {
            SessionUpdate::Keep
        } else {
            SessionUpdate::Reload
        })
    }

    fn undo(&mut self) -> Result<(), Error> {
        let update = self.session.undo().map_err(editor_error)?;
        (self.send)(update);
        Ok(())
    }

    fn redo(&mut self) -> Result<(), Error> {
        let update = self.session.redo().map_err(editor_error)?;
        (self.send)(update);
        Ok(())
    }
}
