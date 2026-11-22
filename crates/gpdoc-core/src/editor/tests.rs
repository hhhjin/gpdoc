use super::*;
use crate::{AllowedMarks, ContentSpec, MarkSpec, NodeSpec, plugin::Plugin};

fn fixture(extra_nodes: Vec<NodeSpec>, marks: Vec<MarkSpec>) -> EditorConfig {
    let plugin = Plugin::new("fixture", |spec| {
        spec.node(
            NodeSpec::block("p")
                .group("block")
                .content(ContentSpec::text(AllowedMarks::All)),
        );
        spec.node(NodeSpec::block("quote").group("block").content("block+"));
        spec.node(NodeSpec::block("gallery").group("block").content("quote*"));
        spec.default_text_block("p");
        for node in extra_nodes {
            spec.node(node);
        }
        for mark in marks {
            spec.mark(mark);
        }
        spec.input_rule(|ctx| {
            let (doc, selection, inserted) = (ctx.document, ctx.selection, ctx.inserted);
            assert_eq!(selection, Selection::caret(inserted.end));
            if doc.blocks() == [Node::text("p", "!")] {
                Ok(Some(vec![Edit::SetBlock {
                    before: DocPos(0),
                    kind: "unknown".into(),
                    attrs: Attrs::new(),
                }]))
            } else {
                Ok(None)
            }
        });
    });
    EditorConfig::builder().plugin(plugin).build().unwrap()
}

fn config() -> EditorConfig {
    fixture(vec![], vec![])
}
fn p(text: &str) -> Node {
    Node::text("p", text)
}
fn quote(text: &str) -> Node {
    Node::container("quote", vec![p(text)])
}

#[test]
fn transactions_bind_marks_at_the_receiving_schema() {
    let config = fixture(vec![], vec![MarkSpec::new("em")]);
    let mut editor = crate::Editor::from_document(&config, vec![p("")]).unwrap();
    let mark = Mark::new("em", Attrs::new());
    let mut tx = editor.transaction();
    tx.set_stored_marks(vec![mark.clone()]);
    editor.apply(tx).unwrap();
    editor.insert_text("a").unwrap();
    let text = editor.document().blocks()[0].child(0);
    assert_eq!(text.marks()[0].kind, "em");

    let mut tx = editor.transaction();
    tx.push(Edit::RemoveExactMark {
        range: DocPos(1)..DocPos(2),
        mark,
    });
    editor.apply(tx).unwrap();
    assert_eq!(editor.document().blocks(), &[p("a")]);
}

#[test]
fn foreign_and_stale_edits_leave_both_editors_untouched() {
    let config = config();
    let mut first = crate::Editor::from_document(&config, vec![p("a")]).unwrap();
    let mut second = crate::Editor::from_document(&config, vec![p("b")]).unwrap();
    let old = first.transaction();
    assert!(matches!(
        second.apply(old.clone()),
        Err(Error::ForeignTransaction)
    ));
    assert_eq!(second.revision(), 0);
    assert_eq!(second.document().blocks(), &[p("b")]);

    first.insert_text("A").unwrap();
    first.undo().unwrap();
    assert!(matches!(first.apply(old), Err(Error::StaleRevision)));
    assert_eq!(first.revision(), 2);
    first.redo().unwrap();
    assert_eq!(first.document().blocks(), &[p("Aa")]);
    assert_eq!(first.revision(), 3);
}

#[test]
fn failed_final_selection_keeps_document_revision_and_redo() {
    let mut editor = crate::Editor::from_document(&config(), vec![p("😀")]).unwrap();
    editor.insert_text("a").unwrap();
    editor.undo().unwrap();
    let mut edit = editor.transaction();
    edit.push(Edit::ReplaceText {
        range: DocPos(1)..DocPos(1),
        text: "b".into(),
    });
    edit.select(Selection::caret(DocPos(3))); // after the high surrogate
    assert!(matches!(
        editor.apply(edit),
        Err(Error::InvalidPosition(DocPos(3)))
    ));
    assert_eq!(editor.revision(), 2);
    assert_eq!(editor.document().blocks(), &[p("😀")]);
    assert_eq!(editor.selection(), Selection::caret(DocPos(1)));
    editor.redo().unwrap();
    assert_eq!(editor.document().blocks(), &[p("a😀")]);
}

#[test]
fn history_maps_ranges_and_preserves_selection_direction() {
    let mut editor = crate::Editor::from_document(&config(), vec![p("abcd")]).unwrap();
    let original = Selection::Text {
        anchor: DocPos(4),
        head: DocPos(2),
    };
    editor.select(original).unwrap();
    let mut edit = editor.transaction();
    edit.push(Edit::ReplaceText {
        range: DocPos(2)..DocPos(2),
        text: "😀".into(),
    });
    let applied = editor.apply(edit).unwrap();
    let reversed = Selection::Text {
        anchor: DocPos(6),
        head: DocPos(4),
    };
    assert_eq!(applied.selection, reversed);
    assert_eq!(
        applied.map.map(DocPos(4), Affinity::After).position,
        DocPos(6)
    );
    let undone = editor.undo().unwrap();
    assert_eq!(undone.selection, original);
    assert_eq!(
        undone.map.map(DocPos(6), Affinity::After).position,
        DocPos(4)
    );
    let redone = editor.redo().unwrap();
    assert_eq!(redone.selection, reversed);
    assert_eq!(
        redone.map.map(DocPos(4), Affinity::After).position,
        DocPos(6)
    );
}

#[test]
fn selected_node_tracks_its_opening_and_deletion_prefers_later_text_on_tie() {
    let mut editor =
        crate::Editor::from_document(&config(), vec![p("a"), quote("b"), p("c")]).unwrap();
    let selection = Selection::Node { before: DocPos(3) };
    editor.select(selection).unwrap();
    let mut insert = editor.transaction();
    insert.push(Edit::ReplaceNodes {
        range: DocPos(3)..DocPos(3),
        nodes: vec![p("x")],
    });
    editor.apply(insert).unwrap();
    assert_eq!(editor.selection(), Selection::Node { before: DocPos(6) });
    editor.delete_selection().unwrap();
    assert_eq!(editor.document().blocks(), &[p("a"), p("x"), p("c")]);
    assert_eq!(editor.selection(), Selection::caret(DocPos(7)));
    editor.undo().unwrap();
    assert_eq!(editor.selection(), Selection::Node { before: DocPos(6) });
    assert_eq!(
        editor.document().blocks(),
        &[p("a"), p("x"), quote("b"), p("c")]
    );
}

#[test]
fn clearing_last_block_creates_default_and_undo_restores_selected_tree() {
    let mut editor = crate::Editor::from_document(&config(), vec![quote("a")]).unwrap();
    let whole = Selection::Node { before: DocPos(0) };
    editor.select(whole).unwrap();
    editor.delete_selection().unwrap();
    assert_eq!(editor.document().blocks(), &[p("")]);
    assert_eq!(editor.selection(), Selection::caret(DocPos(1)));
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), &[quote("a")]);
    assert_eq!(editor.selection(), whole);
    editor.insert_text("x").unwrap();
    assert_eq!(editor.document().blocks(), &[p("x")]);
    assert_eq!(editor.selection(), Selection::caret(DocPos(2)));
    assert!(matches!(editor.redo(), Err(Error::NoHistory)));
    editor.undo().unwrap();
    assert_eq!(editor.selection(), whole);
}

#[test]
fn input_rule_failure_keeps_prior_redo_entry_and_caret() {
    let mut editor = crate::Editor::from_document(&config(), vec![p("")]).unwrap();
    editor.insert_text("a").unwrap();
    editor.undo().unwrap();
    assert!(editor.insert_text("!").is_err());
    assert_eq!(editor.document().blocks(), &[p("")]);
    assert_eq!(editor.revision(), 2);
    assert_eq!(editor.selection(), Selection::caret(DocPos(1)));
    editor.redo().unwrap();
    assert_eq!(editor.document().blocks(), &[p("a")]);
}

#[test]
fn deleting_last_text_keeps_textless_siblings_and_puts_a_caret_in_the_same_slot() {
    let gallery = Node::container("gallery", vec![]);
    let mut editor = crate::Editor::from_document(
        &config(),
        vec![gallery.clone(), quote("a"), gallery.clone()],
    )
    .unwrap();
    let selected = Selection::Node { before: DocPos(2) };
    editor.select(selected).unwrap();
    editor.delete_selection().unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[gallery.clone(), p(""), gallery.clone()]
    );
    assert_eq!(editor.selection(), Selection::caret(DocPos(3)));
    editor.insert_text("x").unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[gallery.clone(), p("x"), gallery]
    );
}

#[test]
fn deleting_last_text_in_a_restricted_parent_falls_back_to_nearest_root_boundary() {
    let original = Node::container("gallery", vec![quote("a")]);
    let mut editor = crate::Editor::from_document(&config(), vec![original.clone()]).unwrap();
    let selected = Selection::Node { before: DocPos(1) };
    editor.select(selected).unwrap();
    editor.delete_selection().unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[Node::container("gallery", vec![]), p("")]
    );
    assert_eq!(editor.selection(), Selection::caret(DocPos(3)));
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), &[original]);
    assert_eq!(editor.selection(), selected);
}

#[test]
fn explicit_replacement_of_the_selection_collapses_at_the_inserted_end() {
    let mut editor = crate::Editor::from_document(&config(), vec![p("ab"), p("cd")]).unwrap();
    editor
        .select(Selection::Text {
            anchor: DocPos(6),
            head: DocPos(2),
        })
        .unwrap();
    let mut tx = editor.transaction();
    tx.push(Edit::ReplaceText {
        range: DocPos(2)..DocPos(6),
        text: "😀".into(),
    });
    let change = editor.apply(tx).unwrap();
    assert_eq!(editor.document().blocks(), &[p("a😀d")]);
    assert_eq!(change.selection, Selection::caret(DocPos(4)));
}

#[test]
fn pending_native_input_cannot_replace_a_newer_revision_or_another_editor() {
    let config = config();
    let original = vec![p("😀"), p("z")];
    let mut editor = crate::Editor::from_document(&config, original.clone()).unwrap();
    let mut other = crate::Editor::from_document(&config, original.clone()).unwrap();
    let mut pending = editor.begin_text_input();
    let selection = Selection::Text {
        anchor: DocPos(6),
        head: DocPos(1),
    };
    pending.replace(selection, &["a", "b"]).unwrap();
    assert_eq!(pending.document().blocks(), &[p("a"), p("b")]);
    assert_eq!(editor.document().blocks(), original);
    assert!(matches!(
        other.commit_text_input(pending.clone()),
        Err(Error::ForeignTransaction)
    ));
    editor.insert_text("x").unwrap();
    assert!(matches!(
        editor.commit_text_input(pending),
        Err(Error::StaleRevision)
    ));
    assert_eq!(editor.document().blocks(), &[p("x😀"), p("z")]);
    assert_eq!(other.document().blocks(), original);
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), original);
}

#[test]
fn paste_open_paragraph_slice_maps_the_caret_and_undoes_as_one_edit() {
    let paragraphs = vec![p("ab"), p("cd")];
    let mut editor = crate::Editor::from_document(&config(), paragraphs.clone()).unwrap();
    editor.select(Selection::caret(DocPos(2))).unwrap();
    let slice = editor.document().root().slice(2, 6, false).unwrap();
    assert_eq!((slice.open_start, slice.open_end), (1, 1));

    let mut tx = editor.transaction();
    let encoded = serde_json::json!({
        "stepType": "replace",
        "from": 2,
        "to": 2,
        "slice": slice.to_json(),
    });
    tx.push(crate::transform::Step::from_json(editor.schema(), &encoded).unwrap());
    editor.apply(tx).unwrap();
    assert_eq!(editor.document().blocks(), [p("ab"), p("cb"), p("cd")]);
    assert_eq!(editor.selection(), Selection::caret(DocPos(6)));
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), paragraphs);
    assert_eq!(editor.selection(), Selection::caret(DocPos(2)));
}

#[test]
fn typing_at_a_closed_gap_creates_one_undoable_paragraph() {
    let config = fixture(vec![NodeSpec::block("atom").group("block")], vec![]);
    let leaf = Node::leaf("atom", Attrs::new());
    let mut editor =
        crate::Editor::from_document(&config, vec![leaf.clone(), leaf.clone()]).unwrap();
    let gap = Selection::Gap { at: DocPos(1) };
    editor.select(gap).unwrap();
    editor.insert_text("中").unwrap();
    assert_eq!(editor.document().blocks(), &[leaf.clone(), p("中"), leaf]);
    assert!(editor.select(Selection::Gap { at: DocPos(1) }).is_err());
    editor.undo().unwrap();
    assert_eq!(editor.selection(), gap);
}

#[test]
fn reversed_node_range_replaces_siblings_and_restores_them_on_undo() {
    let mut editor =
        crate::Editor::from_document(&config(), vec![p("A"), quote("B"), p("C"), p("D")]).unwrap();
    let selection = Selection::NodeRange {
        anchor: DocPos(11),
        head: DocPos(3),
    };
    editor.select(selection).unwrap();
    editor.insert_text("😀").unwrap();
    assert_eq!(editor.document().blocks(), &[p("A"), p("😀"), p("D")]);
    editor.undo().unwrap();
    assert_eq!(editor.selection(), selection);
    editor.delete_selection().unwrap();
    assert_eq!(editor.document().blocks(), &[p("A"), p("D")]);
}

#[test]
fn split_uses_current_draft_coordinates_and_failure_discards_the_whole_transaction() {
    let mut editor = crate::Editor::from_document(&config(), vec![p("a")]).unwrap();
    let mut tx = editor.transaction();
    tx.push(Edit::ReplaceText {
        range: DocPos(2)..DocPos(2),
        text: "bc".into(),
    });
    tx.push(Edit::Split { at: DocPos(3) });
    assert_eq!(tx.document().unwrap().blocks(), &[p("ab"), p("c")]);
    assert_eq!(editor.document().blocks(), &[p("a")]);

    let mut invalid = tx.clone();
    invalid.push(Edit::Split { at: DocPos(99) });
    assert!(invalid.document().is_err());
    assert!(editor.apply(invalid).is_err());
    assert_eq!(editor.revision(), 0);
    assert_eq!(editor.document().blocks(), &[p("a")]);

    editor.apply(tx).unwrap();
    assert_eq!(editor.document().blocks(), &[p("ab"), p("c")]);
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), &[p("a")]);
}

#[test]
fn pending_draft_does_not_restore_a_cursor_that_has_moved() {
    let mut editor = crate::Editor::from_document(&config(), vec![p("ab")]).unwrap();
    let tx = editor.transaction();
    let after_a = Selection::caret(DocPos(2));
    editor.select(after_a).unwrap();
    assert!(matches!(editor.apply(tx), Err(Error::StaleRevision)));
    assert_eq!(editor.selection(), after_a);
}

#[test]
fn break_handlers_follow_container_depth_and_can_decline() {
    use crate::plugin::Plugin;
    let types = Plugin::new("blocks", |p| {
        p.node(NodeSpec::block("p").group("block").content("text*"));
        p.node(NodeSpec::block("inner").group("block").content("p+"));
        p.node(NodeSpec::block("outer").group("block").content("inner+"));
        p.default_text_block("p");
        p.on_break("p", |_| panic!("a container handled the break"));
        p.on_break("outer", |ctx| {
            let range = ctx.selection().bounds(ctx.document());
            Ok(Some(ctx.one_step(Edit::ReplaceText {
                range,
                text: "outer".into(),
            })))
        });
        p.on_break("inner", |ctx| {
            let range = ctx.selection().bounds(ctx.document());
            Ok((range.start == DocPos(3)).then(|| {
                ctx.one_step(Edit::ReplaceText {
                    range,
                    text: "inner".into(),
                })
            }))
        });
    });
    let config = EditorConfig::builder().plugin(types).build().unwrap();
    for (offset, expected) in [(3, "innerx"), (4, "xouter")] {
        let original = Node::container("outer", vec![Node::container("inner", vec![p("x")])]);
        let mut editor = Editor::from_document(&config, vec![original]).unwrap();
        editor.select(Selection::caret(DocPos(offset))).unwrap();
        editor.paragraph_break().unwrap();
        assert_eq!(
            editor.document().blocks()[0]
                .child(0)
                .child(0)
                .text_content(),
            expected
        );
    }
}
