//! Transient remote selections using Yrs and the Yjs awareness protocol.
#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{
    CaretLayout, CaretPart, CollaborationCarets, collaboration_carets, paint_remote_caret,
};

use gpdoc_plugin::core::Selection;
use gpdoc_plugin_collaboration::{Collaboration, Decode, Encode, RelativeSelection};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use yrs::{
    ClientID,
    sync::{Awareness, awareness::AwarenessUpdate},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Awareness(#[from] yrs::sync::awareness::Error),
    #[error(transparent)]
    Decode(#[from] yrs::encoding::read::Error),
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
    pub client: ClientID,
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
        self.awareness.set_local_state(State { user, cursor })?;
        self.local_update()
    }

    /// Send the returned update before disconnecting to remove this peer immediately.
    pub fn leave(&mut self) -> Result<Vec<u8>, Error> {
        self.awareness.remove_state(self.awareness.client_id());
        self.local_update()
    }

    fn local_update(&self) -> Result<Vec<u8>, Error> {
        Ok(self
            .awareness
            .update_with_clients([self.awareness.client_id()])?
            .encode_v1())
    }

    pub fn receive(&mut self, update: &[u8]) -> Result<(), Error> {
        Ok(self
            .awareness
            .apply_update(AwarenessUpdate::decode_v1(update)?)?)
    }

    /// Resolve received positions against the current document each time the view paints.
    pub fn peers(&self, session: &Collaboration) -> Vec<RemoteCaret> {
        let mut peers: Vec<_> = self
            .awareness
            .iter()
            .filter_map(|(id, client)| {
                if id == self.awareness.client_id() {
                    return None;
                }
                let state: State = serde_json::from_str(client.data.as_deref()?).ok()?;
                let selection = state
                    .cursor
                    .and_then(|pin| session.resolve_selection(&pin))
                    .filter(|s| s.validate(session.editor().document()).is_ok());
                Some(RemoteCaret {
                    client: id,
                    user: state.user,
                    selection,
                })
            })
            .collect();
        peers.sort_by_key(|peer| peer.client);
        peers
    }
}
