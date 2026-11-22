use gpdoc_plugin::prelude::{DocPos, Error, Node, Plugin, Selection};
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_collaboration::{Collaboration, StateVector};
use gpdoc_plugin_paragraph::paragraph;

fn pair() -> (Collaboration, Collaboration) {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph(), bold()])
        .build()
        .unwrap();
    let alice = Collaboration::new(
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "A😀B")])
            .unwrap(),
        "default",
    )
    .unwrap();
    let bob = Collaboration::from_update(
        &config,
        "default",
        &alice.update_since(&StateVector::default()),
    )
    .unwrap();
    (alice, bob)
}

fn text(peer: &Collaboration) -> String {
    peer.editor().document().root().text_content()
}

#[test]
fn concurrent_edits_converge_and_local_undo_preserves_remote_text() {
    let (mut alice, mut bob) = pair();
    alice.select(Selection::caret(DocPos(2))).unwrap();
    bob.select(Selection::caret(DocPos(4))).unwrap();
    let (_, a) = alice.edit(|ed| ed.insert_text("α")).unwrap();
    let (_, b) = bob.edit(|ed| ed.insert_text("한")).unwrap();
    bob.apply_update(&a).unwrap();
    alice.apply_update(&b).unwrap();
    assert_eq!(text(&alice), "Aα😀한B");
    assert_eq!(text(&alice), text(&bob));
    assert_eq!(bob.editor().selection(), Selection::caret(DocPos(6)));
    let undo = alice.undo().unwrap();
    bob.apply_update(&undo).unwrap();
    assert_eq!(text(&alice), "A😀한B");
    assert_eq!(text(&alice), text(&bob));
    bob.apply_update(&alice.redo().unwrap()).unwrap();
    bob.apply_update(&a).unwrap();
    assert_eq!(text(&bob), "Aα😀한B");
}

#[test]
fn concurrent_mark_and_typing_keep_character_identity() {
    let (mut alice, mut bob) = pair();
    alice
        .select(Selection::Text {
            anchor: DocPos(1),
            head: DocPos(5),
        })
        .unwrap();
    bob.select(Selection::caret(DocPos(5))).unwrap();
    let (_, a) = alice.edit(|ed| ed.run(gpdoc_plugin_bold::Toggle)).unwrap();
    let (_, b) = bob.edit(|ed| ed.insert_text("!")).unwrap();
    alice.apply_update(&b).unwrap();
    bob.apply_update(&a).unwrap();
    assert_eq!(alice.editor().document(), bob.editor().document());
    assert_eq!(text(&bob), "A😀B!");
    assert!(
        bob.editor()
            .document()
            .text_runs_in(DocPos(1)..DocPos(5))
            .all(|(_, r)| r.marks.iter().any(|m| m.kind == "bold"))
    );
    assert_eq!(bob.editor().selection(), Selection::caret(DocPos(6)));
}

#[test]
fn invalid_remote_content_and_failed_local_edits_leave_both_states_intact() {
    use yrs::{Doc, ReadTxn, Transact, XmlElementPrelim, XmlFragment};
    let (mut alice, _) = pair();
    let bad = Doc::new();
    let xml = bad.get_or_insert_xml_fragment("default");
    xml.push_back(&mut bad.transact_mut(), XmlElementPrelim::empty("unknown"));
    let update = bad
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    let before = alice.state_vector();
    assert!(alice.apply_update(&update).is_err());
    assert!(
        alice
            .edit(|ed| {
                ed.insert_text("discard")?;
                ed.select(Selection::caret(DocPos(1000)))
            })
            .is_err()
    );
    assert_eq!(text(&alice), "A😀B");
    assert_eq!(alice.state_vector(), before);
}

#[test]
fn out_of_order_updates_integrate_when_their_dependencies_arrive() {
    let (mut alice, mut bob) = pair();
    let (_, first) = alice.edit(|ed| ed.insert_text("one")).unwrap();
    let (_, second) = alice.edit(|ed| ed.insert_text("two")).unwrap();
    bob.apply_update(&second).unwrap();
    bob.apply_update(&first).unwrap();
    assert_eq!(text(&bob), "onetwoA😀B");
}

#[test]
fn deleting_first_of_identical_siblings_keeps_the_concurrently_edited_last_block() {
    use gpdoc_plugin::core::Edit;
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph()])
        .build()
        .unwrap();
    let mut alice = Collaboration::new(
        gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "same"); 3],
        )
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
    let (_, delete) = alice
        .edit(|ed| {
            let mut tx = ed.transaction();
            tx.push(Edit::ReplaceNodes {
                range: DocPos(0)..DocPos(6),
                nodes: vec![],
            });
            ed.apply(tx)
        })
        .unwrap();
    bob.select(Selection::caret(DocPos(17))).unwrap();
    let (_, typing) = bob.edit(|ed| ed.insert_text("!")).unwrap();
    bob.apply_update(&delete).unwrap();
    alice.apply_update(&typing).unwrap();
    assert_eq!(
        alice.editor().document().blocks(),
        &[
            Node::text("paragraph", "same"),
            Node::text("paragraph", "same!")
        ]
    );
    assert_eq!(alice.editor().document(), bob.editor().document());
    bob.apply_update(&alice.undo().unwrap()).unwrap();
    assert_eq!(
        bob.editor().document().blocks(),
        &[
            Node::text("paragraph", "same"),
            Node::text("paragraph", "same"),
            Node::text("paragraph", "same!")
        ]
    );
}

#[test]
fn repeated_characters_keep_their_identity_in_a_concurrent_deletion() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph(), bold()])
        .build()
        .unwrap();
    let mut alice = Collaboration::new(
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "xxx")])
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
    alice
        .select(Selection::Text {
            anchor: DocPos(1),
            head: DocPos(2),
        })
        .unwrap();
    let (_, delete) = alice.edit(|ed| ed.delete_selection()).unwrap();
    bob.select(Selection::Text {
        anchor: DocPos(3),
        head: DocPos(4),
    })
    .unwrap();
    let (_, format) = bob.edit(|ed| ed.run(gpdoc_plugin_bold::Set)).unwrap();
    alice.apply_update(&format).unwrap();
    bob.apply_update(&delete).unwrap();
    assert_eq!(alice.editor().document(), bob.editor().document());
    let runs = alice
        .editor()
        .document()
        .text_runs_in(DocPos(1)..DocPos(3))
        .map(|(_, r)| (r.text, r.marks.len()))
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].1, 0);
    assert_eq!(runs[1].1, 1);
}

#[test]
fn text_from_two_concurrent_insertions_into_empty_blocks_keeps_both_remote_carets() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph(), bold()])
        .build()
        .unwrap();
    let mut alice = Collaboration::new(
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "")])
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
    let (_, a) = alice.edit(|ed| ed.insert_text("A")).unwrap();
    let (_, b) = bob.edit(|ed| ed.insert_text("B")).unwrap();
    bob.apply_update(&a).unwrap();
    alice.apply_update(&b).unwrap();
    let caret = bob.relative_selection().unwrap();
    let selected = bob.editor().selection();
    alice
        .select(Selection::Text {
            anchor: DocPos(1),
            head: DocPos(3),
        })
        .unwrap();
    let (_, style) = alice.edit(|ed| ed.run(gpdoc_plugin_bold::Set)).unwrap();
    bob.apply_update(&style).unwrap();
    assert_eq!(alice.editor().document(), bob.editor().document());
    assert_eq!(bob.resolve_selection(&caret), Some(selected));
}

#[test]
fn a_remote_transaction_rejected_by_a_local_doc_filter_leaves_the_yrs_state_intact() {
    let (mut sender, _) = pair();
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([
            paragraph(),
            bold(),
            Plugin::new("lengthLimit", |p| {
                p.filter_transaction(|ctx| {
                    Ok(ctx.document.root().text_content().chars().count() <= 3)
                });
            }),
        ])
        .build()
        .unwrap();
    let mut receiver = Collaboration::from_update(
        &config,
        "default",
        &sender.update_since(&StateVector::default()),
    )
    .unwrap();
    let before = receiver.state_vector();
    let (_, edit) = sender.edit(|ed| ed.insert_text("x")).unwrap();
    assert!(receiver.apply_update(&edit).is_err());
    assert_eq!(text(&receiver), "A😀B");
    assert_eq!(before, receiver.state_vector());
}

#[test]
fn stale_view_cannot_replace_remote_edits() {
    let (mut alice, mut bob) = pair();
    let mut stale = bob.editor().fork();
    let (_, a) = alice.edit(|ed| ed.insert_text("new")).unwrap();
    bob.apply_update(&a).unwrap();
    stale.insert_text("old").unwrap();
    assert!(matches!(
        bob.record(&stale),
        Err(gpdoc_plugin_collaboration::Error::Editor(
            Error::StaleRevision
        ))
    ));
    assert_eq!(text(&bob), "newA😀B");
}

#[test]
fn rectangular_selection_round_trips_between_kits_and_clears_each_cell_once() {
    use gpdoc_plugin_table::{cell_selection, table};
    let config = || {
        gpdoc_plugin::core::EditorConfig::builder()
            .plugins([paragraph(), table()])
            .build()
            .unwrap()
    };
    let cell = |text| Node::container("tableCell", vec![Node::text("paragraph", text)]);
    let row = Node::container("tableRow", vec![cell("one"), cell("two")]);
    let editor = gpdoc_plugin::core::Editor::from_document(
        &config(),
        vec![Node::container("table", vec![row])],
    )
    .unwrap();
    let cells: Vec<_> = editor
        .document()
        .iter_nodes()
        .filter_map(|(before, node)| (node.kind == "tableCell").then_some(before))
        .collect();
    let selection = cell_selection(cells[1], cells[0]);
    let mut alice = Collaboration::new(editor, "default").unwrap();
    alice.select(selection).unwrap();
    let bookmark = serde_json::to_vec(&alice.relative_selection().unwrap()).unwrap();
    let mut bob = Collaboration::from_update(
        &config(),
        "default",
        &alice.update_since(&StateVector::default()),
    )
    .unwrap();
    let selection = bob
        .resolve_selection(&serde_json::from_slice(&bookmark).unwrap())
        .unwrap();
    assert_eq!(selection, alice.editor().selection());

    let mut unknown: serde_json::Value = serde_json::from_slice(&bookmark).unwrap();
    unknown["kind"]["Plugin"] = "uninstalled".into();
    assert_eq!(
        bob.resolve_selection(&serde_json::from_value(unknown).unwrap()),
        None
    );
    let mut plain = gpdoc_plugin::core::Editor::from_document(
        &gpdoc_plugin::core::EditorConfig::builder()
            .plugins([paragraph()])
            .build()
            .unwrap(),
        vec![Node::text("paragraph", "")],
    )
    .unwrap();
    assert_eq!(
        plain.select(selection),
        Err(Error::InvalidEdit(
            "selection requires an installed plugin".into(),
        ))
    );

    bob.select(selection).unwrap();
    bob.edit(|editor| editor.insert_text("X")).unwrap();
    let contents: Vec<_> = bob
        .editor()
        .document()
        .iter_nodes()
        .filter(|(_, node)| node.kind == "tableCell")
        .map(|(_, node)| node.text_content())
        .collect();
    assert_eq!(contents, ["X", ""]);
}
