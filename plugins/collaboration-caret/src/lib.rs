//! Transient remote selections using y-octo and the Yjs awareness protocol.
#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{
    CaretLayout, CaretPart, CollaborationCarets, collaboration_carets, paint_remote_caret,
};

use gpdoc_plugin::core::Selection;
use gpdoc_plugin_collaboration::{Collaboration, RelativeSelection};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use y_octo::Awareness;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Awareness(#[from] y_octo::JwstCodecError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Tiptap's `user` object. Typical keys include `name` and `color`.
pub type User = Map<String, Value>;

#[derive(Deserialize, Serialize)]
struct State {
    #[serde(default)]
    user: User,
    cursor: Option<RelativeSelection>,
}

pub struct RemoteCaret {
    pub client: u64,
    pub user: User,
    pub selection: Option<Selection>,
}

pub struct CollaborationCaret {
    awareness: Awareness,
}

impl CollaborationCaret {
    pub fn new(session: &Collaboration) -> Self {
        Self {
            awareness: session.awareness(),
        }
    }

    /// Publish the local user's current selection. Use `focused = false` to hide their caret.
    pub fn publish(
        &mut self,
        session: &Collaboration,
        user: User,
        focused: bool,
    ) -> Result<Vec<u8>, Error> {
        let cursor = focused.then(|| session.relative_selection()).flatten();
        self.awareness
            .set_local_state(serde_json::to_string(&State { user, cursor })?);
        self.local_update()
    }

    /// Send the returned update before disconnecting to remove this peer immediately.
    pub fn leave(&mut self) -> Result<Vec<u8>, Error> {
        self.awareness.clear_local_state();
        self.local_update()
    }

    fn local_update(&self) -> Result<Vec<u8>, Error> {
        Ok(self
            .awareness
            .encode_update(Some(&[self.awareness.local_id()]))?)
    }

    pub fn receive(&mut self, update: &[u8]) -> Result<(), Error> {
        self.awareness.apply_update_v1(update)?;
        Ok(())
    }

    /// Resolve received positions against the current document each time the view paints.
    pub fn peers(&self, session: &Collaboration) -> Vec<RemoteCaret> {
        let mut peers: Vec<_> = self
            .awareness
            .get_states()
            .iter()
            .filter_map(|(id, client)| {
                if *id == self.awareness.local_id() {
                    return None;
                }
                let state: State = serde_json::from_str(client.content()).ok()?;
                let selection = state
                    .cursor
                    .and_then(|pin| session.resolve_selection(&pin))
                    .filter(|s| s.validate(session.editor().document()).is_ok());
                Some(RemoteCaret {
                    client: *id,
                    user: state.user,
                    selection,
                })
            })
            .collect();
        peers.sort_by_key(|peer| peer.client);
        peers
    }
}
