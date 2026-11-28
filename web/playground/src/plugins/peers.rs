use super::*;
use gpdoc_plugin_collaboration::{Collaboration, CollaborationView, StateVector};
use gpdoc_plugin_collaboration_caret::{CollaborationCaret, User, collaboration_carets};
use gpui::{App, Focusable};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Clone, Default)]
pub(super) struct Awareness(Rc<RefCell<HashMap<u64, CollaborationCaret>>>);

impl Awareness {
    pub fn configure(
        &self,
        config: gpdoc::EditorConfigBuilder,
        slug: &str,
    ) -> gpdoc::EditorConfigBuilder {
        if slug != "collaboration-caret" {
            return config;
        }
        let peers = self.clone();
        config.plugin(collaboration_carets(move |session| {
            peers
                .0
                .borrow()
                .get(&session.awareness().local_id())
                .map(|caret| caret.peers(session))
                .unwrap_or_default()
        }))
    }
}

pub(super) fn connect(
    alice: &Entity<EditorView>,
    bob: &Entity<EditorView>,
    awareness: Awareness,
    slug: &str,
    window: &mut Window,
    cx: &mut App,
) -> Result<()> {
    let first = Collaboration::new(alice.read(cx).editor().fork(), "default")?;
    let second = Collaboration::from_update(
        first.editor().config(),
        "default",
        &first.update_since(&StateVector::default())?,
    )?;
    let a = first.awareness().local_id();
    let b = second.awareness().local_id();
    for (view, peer, session, id, other, name) in [
        (alice, bob, first, a, b, "Alice"),
        (bob, alice, second, b, a, "Bob"),
    ] {
        let queue = Rc::new(RefCell::new(Vec::new()));
        let outgoing = queue.clone();
        awareness
            .0
            .borrow_mut()
            .insert(id, CollaborationCaret::new(&session));
        view.update(cx, |v, cx| {
            v.bind_collaboration(
                session,
                move |update| outgoing.borrow_mut().push(update),
                window,
                cx,
            )
        })?;
        let peer = peer.clone();
        let awareness = awareness.clone();
        let show_caret = slug == "collaboration-caret";
        let mut last = None;
        window
            .observe(view, cx, move |view, window, cx| {
                for update in queue.take() {
                    peer.update(cx, |v, cx| {
                        v.receive_collaboration_update(update, window, cx)
                    })
                    .expect("receive local peer update");
                }
                if !show_caret {
                    return;
                }
                let view = view.read(cx);
                let session = view.collaboration().expect("bound session");
                let state = (
                    session.editor().revision(),
                    session.editor().selection(),
                    view.focus_handle(cx).is_focused(window),
                );
                if Some(state) == last {
                    return;
                }
                last = Some(state);
                let mut awareness = awareness.0.borrow_mut();
                let user = User::from_iter([("name".into(), name.into())]);
                let update = awareness
                    .get_mut(&id)
                    .unwrap()
                    .publish(session, user, state.2)
                    .expect("publish local caret");
                awareness
                    .get_mut(&other)
                    .unwrap()
                    .receive(&update)
                    .expect("receive local caret");
                window.refresh();
            })
            .detach();
    }
    Ok(())
}
