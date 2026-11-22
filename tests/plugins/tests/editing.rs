use gpdoc_plugin::prelude::{ContentRef, DocPos, Node, Selection};
use gpdoc_plugin_blockquote::blockquote;
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_bullet_list::bullet_list;
use gpdoc_plugin_heading::heading;
use gpdoc_plugin_inline_code::inline_code;
use gpdoc_plugin_italic::italic;
use gpdoc_plugin_link::link;
use gpdoc_plugin_list_item::list_item;
use gpdoc_plugin_ordered_list::ordered_list;
use gpdoc_plugin_paragraph::paragraph as paragraph_plugin;

#[test]
fn replace_across_paragraphs_and_undo() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph_plugin()])
        .build()
        .unwrap();
    let original = vec![Node::text("paragraph", "ab"), Node::text("paragraph", "cd")];
    let mut editor = gpdoc_plugin::core::Editor::from_document(&config, original.clone()).unwrap();
    let selection = Selection::Text {
        anchor: DocPos(2),
        head: DocPos(6),
    };
    editor.select(selection).unwrap();
    editor.insert_text("X").unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[Node::text("paragraph", "aXd")]
    );
    assert_eq!(editor.selection(), Selection::caret(DocPos(3)));
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), original);
    assert_eq!(editor.selection(), selection);
}

use gpdoc_plugin::core::Affinity;
use gpdoc_plugin::prelude::{
    Attrs, Command, CommandOutcome, Content, Edit, Error, Mark, StoredCommand, TextRun, Value,
};

fn editor_with(blocks: Vec<Node>) -> gpdoc_plugin::core::Editor {
    gpdoc_plugin::core::Editor::from_document(
        &gpdoc_plugin::core::EditorConfig::builder()
            .plugins([
                paragraph_plugin(),
                blockquote(),
                heading(),
                bullet_list(),
                list_item(),
                ordered_list(),
                link(Default::default()),
                inline_code(),
                bold(),
                italic(),
            ])
            .build()
            .unwrap(),
        blocks,
    )
    .unwrap()
}

fn paragraph(text: &str) -> Node {
    Node::text("paragraph", text)
}

#[test]
fn a_failed_second_step_preserves_document_selection_revision_and_redo() {
    let mut editor = editor_with(vec![paragraph("A😀B")]);
    let backwards = Selection::Text {
        anchor: DocPos(4),
        head: DocPos(2),
    };
    editor.select(backwards).unwrap();
    editor.insert_text("x").unwrap();
    editor.undo().unwrap();
    let revision = editor.revision();
    assert_eq!(editor.selection(), backwards);
    let fresh = editor.transaction();
    let mut bad = fresh.clone();
    bad.push(Edit::ReplaceText {
        range: DocPos(1)..DocPos(1),
        text: "x".into(),
    });
    // In the provisional document the emoji spans 3..5.
    bad.push(Edit::ReplaceText {
        range: DocPos(3)..DocPos(4),
        text: "y".into(),
    });
    assert!(editor.apply(bad).is_err());
    assert_eq!(editor.document().blocks(), &[paragraph("A😀B")]);
    assert_eq!(editor.selection(), backwards);
    assert_eq!(editor.revision(), revision);
    editor.redo().unwrap();
    assert_eq!(editor.document().blocks(), &[paragraph("AxB")]);
    assert!(matches!(editor.apply(fresh), Err(Error::StaleRevision)));
}

#[test]
fn mapped_selections_keep_direction_and_history_emits_inverse_maps() {
    let mut editor = editor_with(vec![paragraph("abc")]);
    let backwards = Selection::Text {
        anchor: DocPos(4),
        head: DocPos(2),
    };
    editor.select(backwards).unwrap();
    let mut edit = editor.transaction();
    edit.push(Edit::ReplaceText {
        range: DocPos(1)..DocPos(1),
        text: "😀".into(),
    });
    let change = editor.apply(edit).unwrap();
    assert_eq!(
        change.selection,
        Selection::Text {
            anchor: DocPos(6),
            head: DocPos(4)
        }
    );
    assert_eq!(
        change.map.map(DocPos(1), Affinity::Before).position,
        DocPos(1)
    );
    assert_eq!(
        change.map.map(DocPos(1), Affinity::After).position,
        DocPos(3)
    );
    let undo = editor.undo().unwrap();
    assert_eq!(undo.map.map(DocPos(5), Affinity::After).position, DocPos(3));
    assert_eq!(undo.selection, backwards);
    assert_eq!(
        editor
            .redo()
            .unwrap()
            .map
            .map(DocPos(3), Affinity::After)
            .position,
        DocPos(5)
    );
}

#[test]
fn foreign_and_stale_transactions_cannot_be_replayed() {
    let mut first = editor_with(vec![paragraph("")]);
    let mut other = editor_with(vec![paragraph("")]);
    let mut tx = first.transaction();
    tx.push(Edit::ReplaceText {
        range: DocPos(1)..DocPos(1),
        text: "x".into(),
    });
    assert!(matches!(
        other.apply(tx.clone()),
        Err(Error::ForeignTransaction)
    ));
    let stale = tx.clone();
    first.apply(tx).unwrap();
    assert!(matches!(first.apply(stale), Err(Error::StaleRevision)));
    first.undo().unwrap();
    first.insert_text("y").unwrap();
    assert!(matches!(first.redo(), Err(Error::NoHistory)));
    assert_eq!(other.document().blocks(), &[paragraph("")]);
}

#[test]
fn deleted_node_favors_following_caret_and_restores_whole_node_on_undo() {
    let quote = Node::container("blockquote", vec![paragraph("Remove this quote")]);
    let original = vec![paragraph("A"), quote.clone(), paragraph("B")];
    let mut editor = editor_with(original.clone());
    let node = Selection::Node { before: DocPos(3) };
    editor.select(node).unwrap();
    editor.delete_selection().unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[paragraph("A"), paragraph("B")]
    );
    assert_eq!(editor.selection(), Selection::caret(DocPos(4)));
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), original);
    assert_eq!(editor.selection(), node);

    let mut last = editor_with(vec![quote]);
    last.select(Selection::Node { before: DocPos(0) }).unwrap();
    last.delete_selection().unwrap();
    assert_eq!(last.document().blocks(), &[paragraph("")]);
    assert_eq!(last.selection(), Selection::caret(DocPos(1)));
}

fn apply_command<C: Command>(editor: &mut gpdoc_plugin::core::Editor, command: C) {
    assert!(
        matches!(editor.run(command).unwrap(), CommandOutcome::Applied(_)),
        "{}",
        C::NAME
    );
}

#[test]
fn nested_list_commands_preserve_each_item_and_track_the_caret() {
    fn item(text: &str) -> Node {
        Node::container("listItem", vec![paragraph(text)])
    }
    let original = Node::container("bulletList", vec![item("one"), item("two"), item("three")]);
    let mut editor = editor_with(vec![original.clone()]);
    let before = editor
        .document()
        .nodes()
        .into_iter()
        .find(|(_, node)| **node == paragraph("two"))
        .unwrap()
        .0;
    let end = editor
        .document()
        .text_pos_at_byte(before, "two".len())
        .unwrap();
    editor.select(Selection::caret(end)).unwrap();
    apply_command(&mut editor, gpdoc_plugin_list_item::Sink);
    let nested = Node::container(
        "listItem",
        vec![
            paragraph("one"),
            Node::container("bulletList", vec![item("two")]),
        ],
    );
    assert_eq!(
        editor.document().blocks(),
        &[Node::container("bulletList", vec![nested, item("three")])]
    );
    let Selection::Text { head, .. } = editor.selection() else {
        panic!("expected a caret")
    };
    assert_eq!(
        *editor.document().text_at(head).unwrap().1,
        paragraph("two")
    );
    apply_command(&mut editor, gpdoc_plugin_list_item::Lift);
    assert_eq!(editor.document().blocks(), &[original]);
    assert_eq!(editor.selection(), Selection::caret(end));
}

#[test]
fn link_editing_restores_href_and_extent_without_moving_positions() {
    let mut editor = editor_with(vec![paragraph("Go!")]);
    let range = Selection::Text {
        anchor: DocPos(1),
        head: DocPos(3),
    };
    editor.select(range).unwrap();
    let old = "https://example.org/old";
    let new = "https://example.org/new";
    apply_command(&mut editor, gpdoc_plugin_link::Set::new(old));
    let snapshot = editor.document().clone();
    apply_command(&mut editor, gpdoc_plugin_link::Set::new(new));
    let expected = Content::Inline(vec![
        TextRun {
            text: "Go".into(),
            marks: vec![Mark::new(
                "link",
                [
                    ("href".into(), Value::from(new)),
                    ("class".into(), Value::Null),
                    ("title".into(), Value::Null),
                    ("target".into(), Value::String("_blank".into())),
                    (
                        "rel".into(),
                        Value::String("noopener noreferrer nofollow".into()),
                    ),
                ]
                .into(),
            )],
        }
        .into(),
        TextRun {
            text: "!".into(),
            marks: vec![],
        }
        .into(),
    ]);
    assert_eq!(
        editor.document().blocks()[0],
        Node::new("paragraph", Attrs::new(), expected)
    );
    let change = editor.undo().unwrap();
    assert_eq!(editor.document(), &snapshot);
    assert_eq!(change.selection, range);
    assert_eq!(
        change.map.map(DocPos(2), Affinity::After).position,
        DocPos(2)
    );
    apply_command(&mut editor, gpdoc_plugin_link::Unset);
    assert_eq!(editor.document().blocks(), &[paragraph("Go!")]);
}

#[test]
fn heading_input_and_undo_form_one_history_entry() {
    let mut editor = editor_with(vec![paragraph("#")]);
    editor.select(Selection::caret(DocPos(2))).unwrap();
    editor.insert_text(" ").unwrap();
    let heading = &editor.document().blocks()[0];
    assert_eq!(heading.kind, "heading");
    assert_eq!(heading.attrs.get("level"), Some(&Value::from(1)));
    assert_eq!(editor.selection(), Selection::caret(DocPos(1)));
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), &[paragraph("#")]);
    assert_eq!(editor.selection(), Selection::caret(DocPos(2)));
}

#[test]
fn crossing_a_heading_and_paragraph_keeps_the_first_blocks_attributes() {
    let title = Node::new(
        "heading",
        Attrs::from([("level".into(), Value::from(2))]),
        Node::text("paragraph", "First draft").content().to_owned(),
    );
    let mut editor = editor_with(vec![title.clone(), paragraph("Body text")]);
    editor
        .select(Selection::Text {
            anchor: DocPos(19),
            head: DocPos(6),
        })
        .unwrap();
    editor.insert_text(" ").unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[Node::new(
            "heading",
            title.attrs.clone(),
            paragraph("First text").content().to_owned()
        )]
    );
    assert_eq!(editor.selection(), Selection::caret(DocPos(7)));
}

#[test]
fn text_inserted_at_a_mark_end_follows_its_declared_policy() {
    for (command, kind, inclusive) in [
        (
            StoredCommand::new(gpdoc_plugin_link::Set::new("https://example.org")),
            "link",
            true,
        ),
        (
            StoredCommand::new(gpdoc_plugin_inline_code::Set),
            "code",
            true,
        ),
        (StoredCommand::new(gpdoc_plugin_bold::Set), "bold", true),
        (StoredCommand::new(gpdoc_plugin_italic::Set), "italic", true),
    ] {
        let mut editor = editor_with(vec![paragraph("abc")]);
        editor
            .select(Selection::Text {
                anchor: DocPos(1),
                head: DocPos(3),
            })
            .unwrap();
        assert!(matches!(
            editor.run_stored(&command).unwrap(),
            CommandOutcome::Applied(_)
        ));
        editor.select(Selection::caret(DocPos(2))).unwrap();
        editor.insert_text("X").unwrap();
        editor.select(Selection::caret(DocPos(4))).unwrap();
        editor.insert_text("Y").unwrap();

        let ContentRef::Inline(runs) = editor.document().blocks()[0].content() else {
            panic!("expected text")
        };
        let marked: String = runs
            .iter()
            .filter(|run| run.marks().iter().any(|mark| mark.kind == kind))
            .map(|run| run.projected_text())
            .collect();
        assert_eq!(marked, if inclusive { "aXbY" } else { "aXb" }, "{kind}");
    }
}

#[test]
fn changing_one_paragraph_shares_its_untouched_sibling() {
    let mut editor = editor_with(vec![paragraph("one"), paragraph("two")]);
    let before = editor.document().clone();
    editor.insert_text("x").unwrap();
    assert!(std::ptr::eq(
        &*before.blocks()[1],
        &*editor.document().blocks()[1]
    ));
}

#[test]
fn replace_any_text_range_across_nested_quotes_preserves_surviving_text() {
    fn quote(children: Vec<Node>) -> Node {
        Node::container("blockquote", children)
    }
    let original = vec![
        paragraph("€😀"),
        quote(vec![
            paragraph(""),
            quote(vec![paragraph("x"), paragraph("B")]),
        ]),
        quote(vec![paragraph("C"), paragraph("👩‍💻")]),
        paragraph("Z"),
    ];
    let mut editor = editor_with(original.clone());
    let mut flat = String::new();
    let mut positions = Vec::new();
    for (before, node) in editor.document().iter_nodes() {
        if let ContentRef::Inline(runs) = node.content() {
            let text: String = runs.iter().map(|r| r.projected_text()).collect();
            positions.push((DocPos(before.0 + 1), flat.len()));
            let mut pos = before.0 + 1;
            for ch in text.chars() {
                flat.push(ch);
                pos += ch.len_utf16();
                positions.push((DocPos(pos), flat.len()));
            }
        }
    }

    for (index, &(from, byte_from)) in positions.iter().enumerate() {
        for &(to, byte_to) in &positions[index..] {
            editor
                .select(Selection::Text {
                    anchor: to,
                    head: from,
                })
                .unwrap();
            let changed = editor
                .insert_text("X")
                .unwrap_or_else(|e| panic!("{from:?}..{to:?}: {e}"));
            let actual: String = editor
                .document()
                .iter_nodes()
                .filter_map(|(_, node)| match node.content() {
                    ContentRef::Inline(runs) => Some(runs),
                    _ => None,
                })
                .flatten()
                .map(|run| run.projected_text())
                .collect();
            assert_eq!(
                actual,
                format!("{}X{}", &flat[..byte_from], &flat[byte_to..])
            );
            assert_eq!(changed.selection, Selection::caret(DocPos(from.0 + 1)));
            editor.undo().unwrap();
            assert_eq!(editor.document().blocks(), original);
        }
    }
}

#[test]
fn empty_edits_and_selection_only_transactions_preserve_redo() {
    let mut editor = editor_with(vec![paragraph("x")]);
    editor.insert_text("y").unwrap();
    editor.undo().unwrap();
    let old_revision = editor.revision();
    editor.delete_selection().unwrap();
    let mut move_caret = editor.transaction();
    move_caret.select(Selection::caret(DocPos(2)));
    let moved = editor.apply(move_caret).unwrap();
    assert_eq!(moved.selection, Selection::caret(DocPos(2)));
    assert_eq!(moved.revision, old_revision);
    editor.redo().unwrap();
    assert_eq!(editor.document().blocks(), &[paragraph("yx")]);
}

#[test]
fn deleting_the_only_list_item_prunes_its_empty_list_in_the_same_undo() {
    let list = Node::container(
        "bulletList",
        vec![Node::container("listItem", vec![paragraph("item")])],
    );
    let original = vec![paragraph("A"), list, paragraph("B")];
    let mut editor = editor_with(original.clone());
    let selection = Selection::Node { before: DocPos(4) };
    editor.select(selection).unwrap();
    editor.delete_selection().unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[paragraph("A"), paragraph("B")]
    );
    assert_eq!(editor.selection(), Selection::caret(DocPos(4)));
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), original);
    assert_eq!(editor.selection(), selection);
}

#[test]
fn deleting_a_required_paragraph_preserves_its_nested_list() {
    let nested = Node::container(
        "bulletList",
        vec![Node::container("listItem", vec![paragraph("child")])],
    );
    let list = |text: &str| {
        Node::new(
            "orderedList",
            Attrs::from([
                ("start".into(), Value::from(1)),
                ("type".into(), Value::Null),
            ]),
            Content::Blocks(vec![Node::container(
                "listItem",
                vec![paragraph(text), nested.clone()],
            )]),
        )
    };
    let mut editor = editor_with(vec![list("parent")]);
    editor
        .select(Selection::Node { before: DocPos(2) })
        .unwrap();
    editor.delete_selection().unwrap();
    assert_eq!(editor.document().blocks(), &[list("")]);
    assert_eq!(editor.selection(), Selection::caret(DocPos(3)));
}

#[test]
fn inline_code_excludes_formatting_marks_on_the_same_text() {
    let mut editor = editor_with(vec![paragraph("sample")]);
    editor
        .select(Selection::Text {
            anchor: DocPos(1),
            head: DocPos(7),
        })
        .unwrap();
    editor.run(gpdoc_plugin_bold::Set).unwrap();
    editor.run(gpdoc_plugin_italic::Set).unwrap();
    editor.run(gpdoc_plugin_inline_code::Set).unwrap();
    editor.run(gpdoc_plugin_bold::Set).unwrap();
    let ContentRef::Inline(runs) = editor.document().blocks()[0].content() else {
        panic!("paragraph")
    };
    let code = editor.schema().mark("code", Attrs::new()).unwrap();
    assert_eq!(runs[0].marks(), &[code]);
}

#[test]
fn paragraph_break_at_heading_boundaries_and_inside_text() {
    let h2 =
        |text| Node::text("heading", text).with_attrs(Attrs::from([("level".into(), 2.into())]));
    for (original, anchor, head, expected, caret) in [
        (h2("ab"), 1, 1, vec![paragraph(""), h2("ab")], 3),
        (h2("ab"), 2, 2, vec![h2("a"), h2("b")], 4),
        (h2("ab"), 3, 3, vec![h2("ab"), paragraph("")], 5),
        (h2(""), 1, 1, vec![paragraph("")], 1),
        (h2("a😀"), 4, 2, vec![h2("a"), paragraph("")], 4),
        (paragraph(""), 1, 1, vec![paragraph(""), paragraph("")], 3),
    ] {
        let mut editor = editor_with(vec![original.clone()]);
        let selected = Selection::Text {
            anchor: DocPos(anchor),
            head: DocPos(head),
        };
        editor.select(selected).unwrap();
        editor.paragraph_break().unwrap();
        assert_eq!(editor.document().blocks(), expected);
        assert_eq!(editor.selection(), Selection::caret(DocPos(caret)));
        editor.undo().unwrap();
        assert_eq!(editor.document().blocks(), &[original]);
        assert_eq!(editor.selection(), selected);
    }
}

#[test]
fn paragraph_break_uses_list_handler_and_paste_keeps_heading_type() {
    let item = |text| Node::container("listItem", vec![paragraph(text)]);
    let mut editor = editor_with(vec![Node::container("bulletList", vec![item("ab")])]);
    editor.select(Selection::caret(DocPos(4))).unwrap();
    editor.paragraph_break().unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[Node::container("bulletList", vec![item("a"), item("b")])]
    );

    let mut editor = editor_with(vec![Node::text("heading", "a")]);
    editor.select(Selection::caret(DocPos(2))).unwrap();
    editor.paste_text("\nb").unwrap();
    assert_eq!(editor.document().blocks()[1].kind, "heading");
}

#[test]
fn paragraph_break_in_code_replaces_selection_with_literal_newline() {
    let config = gpdoc_plugin::EditorConfig::builder()
        .plugins([paragraph_plugin(), gpdoc_plugin_code_block::code_block()])
        .build()
        .unwrap();
    let code = |text| {
        Node::text("codeBlock", text).with_attrs(Attrs::from([("language".into(), "rust".into())]))
    };
    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![code("a😀b")]).unwrap();
    editor
        .select(Selection::Text {
            anchor: DocPos(4),
            head: DocPos(2),
        })
        .unwrap();
    editor.paragraph_break().unwrap();
    assert_eq!(editor.document().blocks(), &[code("a\nb")]);
    assert_eq!(editor.selection(), Selection::caret(DocPos(3)));
}
