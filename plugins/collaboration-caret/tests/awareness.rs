use gpdoc_plugin::prelude::{DocPos, Node, Selection};
use gpdoc_plugin_collaboration::{Collaboration, StateVector};
use gpdoc_plugin_collaboration_caret::{CollaborationCaret, User};
use gpdoc_plugin_paragraph::paragraph;

#[test]
fn remote_caret_tracks_text_and_disappears_on_disconnect() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph()])
        .build()
        .unwrap();
    let mut alice = Collaboration::new(
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "A😀B")])
            .unwrap(),
        "default",
    )
    .unwrap();
    let mut bob = Collaboration::from_update(
        &config,
        "default",
        &alice.update_since(&StateVector::default()),
    )
    .unwrap();
    let mut local = CollaborationCaret::new(&alice);
    let mut remote = CollaborationCaret::new(&bob);
    alice
        .select(Selection::Text {
            anchor: DocPos(4),
            head: DocPos(2),
        })
        .unwrap();
    let user = User::from_iter([("name".into(), "Alice".into())]);
    remote
        .receive(&local.publish(&alice, user.clone(), true).unwrap())
        .unwrap();
    bob.edit(|ed| ed.insert_text("한")).unwrap();
    let peers = remote.peers(&bob);
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].user, user);
    assert_eq!(
        peers[0].selection,
        Some(Selection::Text {
            anchor: DocPos(5),
            head: DocPos(3)
        })
    );
    remote.receive(&local.leave().unwrap()).unwrap();
    assert!(remote.peers(&bob).is_empty());
}
