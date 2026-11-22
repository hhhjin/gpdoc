//! Input and keyboard editing of extended grapheme clusters.
use super::InputBuffer;
use crate::{EditorConfigBuilder, ViewPlugin};
use gpdoc_core::{
    EditorConfig, Node, NodeSpec,
    plugin::{Plugin, PluginSpec},
};

fn config(declare: impl FnOnce(&mut PluginSpec)) -> EditorConfigBuilder {
    crate::EditorConfig::builder().plugin(Plugin::new("fixture", |spec| {
        spec.node(
            NodeSpec::block("paragraph")
                .group("block")
                .content("inline*"),
        );
        spec.default_text_block("paragraph");
        declare(spec);
    }))
}

fn buffer(text: &str) -> InputBuffer {
    let config = EditorConfig::default();
    InputBuffer::open(
        gpdoc_core::Editor::from_document(&config, vec![Node::text("paragraph", text)]).unwrap(),
    )
    .unwrap()
}

#[test]
fn french_dead_key_commits_either_accent_encoding_without_normalizing_it() {
    for accented_e in ["é", "e\u{301}"] {
        let mut input = buffer("caf!");
        input.select(3, 3).unwrap();

        // A dead-key preview followed by the native committed string.
        input.mark(None, "´", Some(1..1)).unwrap();
        assert_eq!(input.marked_range(), Some(3..4));
        assert_eq!(input.editor().revision(), 0);
        input.native_replace(None, accented_e).unwrap();

        let end = 3 + accented_e.encode_utf16().count();
        assert_eq!(input.text(), format!("caf{accented_e}!"));
        assert_eq!(input.marked_range(), None);
        assert_eq!(input.selection(), (end, end));
        input.move_by_grapheme(false, false).unwrap();
        assert_eq!(input.selection(), (3, 3));
        input.delete_grapheme_or_selection(true).unwrap();
        assert_eq!(input.text(), "caf!");
        input.undo().unwrap();
        assert_eq!(input.text(), format!("caf{accented_e}!"));
        assert_eq!(input.selection(), (3, 3));
        input.undo().unwrap();
        assert_eq!(input.text(), "caf!");
        assert_eq!(input.selection(), (3, 3));
    }
}

#[test]
fn navigation_selection_and_deletion_keep_complex_graphemes_intact() {
    for (language, cluster, width) in [
        ("Vietnamese", "a\u{302}\u{301}", 3), // Circumflex and acute on the same vowel.
        ("Hindi", "क्षि", 4),                  // Consonant, virama, consonant, vowel sign.
        ("Thai", "กิ้", 3),                     // Base consonant, above vowel, tone mark.
        ("Arabic", "نُ", 2),                   // Nun with damma.
        ("Hebrew", "שָׁ", 3),                   // Shin with qamats and shin dot.
    ] {
        // Keep an astral character before the cluster so byte offsets cannot pass as UTF-16.
        let original = format!("😀{cluster}!");
        let start = 2;
        let end = start + width;
        let mut input = buffer(&original);
        input.select(start, start).unwrap();

        input.move_by_grapheme(true, false).unwrap();
        assert_eq!(input.selection(), (end, end), "{language}");
        input.delete_grapheme_or_selection(false).unwrap();
        assert_eq!(input.text(), "😀!", "{language}");
        assert_eq!(input.selection(), (start, start), "{language}");
        input.undo().unwrap();
        assert_eq!(input.text(), original, "{language}");
        assert_eq!(input.selection(), (end, end), "{language}");

        input.move_by_grapheme(false, true).unwrap();
        assert_eq!(input.selection(), (end, start), "{language}");
        input.delete_grapheme_or_selection(true).unwrap();
        assert_eq!(input.text(), "😀!", "{language}");
        input.undo().unwrap();
        assert_eq!(input.selection(), (end, start), "{language}");

        // A native input source can leave a caret between a base and an attached mark.
        input.select(start + 1, start + 1).unwrap();
        input.delete_grapheme_or_selection(true).unwrap();
        assert_eq!(input.text(), "😀!", "{language}");
        input.undo().unwrap();
        assert_eq!(input.text(), original, "{language}");
        assert_eq!(input.selection(), (start + 1, start + 1), "{language}");
    }
}

#[test]
fn inline_leaf_moves_deletes_and_reconverts_as_one_unit_next_to_emoji() {
    use gpdoc_core::{Attrs, Content, DocPos, Selection, TextRun};
    let text = |s: &str| {
        TextRun {
            text: s.into(),
            marks: vec![],
        }
        .into()
    };
    let p = Node::new(
        "paragraph",
        Attrs::new(),
        Content::Inline(vec![
            text("😀"),
            Node::leaf("mention", Attrs::new()),
            text("\u{301}!"),
        ]),
    );
    let mut editor = gpdoc_core::Editor::from_document(
        &config(|p| p.node(NodeSpec::inline("mention")))
            .build()
            .unwrap(),
        vec![p],
    )
    .unwrap();
    let snapshot = editor.document().clone();
    editor
        .select(Selection::Node { before: DocPos(3) })
        .unwrap();
    let mut input = InputBuffer::open(editor).unwrap();
    assert_eq!(input.selection(), (2, 3));
    assert_eq!(input.text(), "😀\u{fffc}\u{301}!");
    assert_eq!(input.display_text(), "😀◇\u{301}!");
    input.move_by_grapheme(false, false).unwrap();
    assert_eq!(input.selection(), (2, 2));
    input.move_by_grapheme(true, false).unwrap();
    assert_eq!(input.selection(), (3, 3));
    input.delete_grapheme_or_selection(false).unwrap();
    assert_eq!(input.text(), "😀\u{301}!");
    input.undo().unwrap();
    assert_eq!(input.editor().document(), &snapshot);
    input.mark(Some(2..3), "ㅎ", Some(1..1)).unwrap();
    assert_eq!(input.text(), "😀ㅎ\u{301}!");
    input.native_replace(None, "한").unwrap();
    assert_eq!(input.text(), "😀한\u{301}!");
    input.undo().unwrap();
    assert_eq!(input.editor().document(), &snapshot);
}

#[test]
fn ime_replacement_keeps_explicit_marks_between_plain_characters() {
    use gpdoc_core::{Attrs, DocPos, Mark, MarkSpec};
    let editor = gpdoc_core::Editor::from_document(
        &config(|p| p.mark(MarkSpec::new("emphasis")))
            .build()
            .unwrap(),
        vec![Node::text("paragraph", "ab")],
    )
    .unwrap();
    let mut input = InputBuffer::open(editor).unwrap();
    input.select(1, 1).unwrap();
    let mut tx = input.editor().transaction();
    tx.set_stored_marks(vec![Mark::new("emphasis", Attrs::new())]);
    input.apply_node_view(tx).unwrap();
    input.mark(None, "ㅎ", Some(1..1)).unwrap();
    input.mark(None, "하", Some(1..1)).unwrap();
    input.native_replace(None, "한").unwrap();
    assert_eq!(input.text(), "a한b");
    let runs: Vec<_> = input
        .editor()
        .document()
        .text_runs_in(DocPos(1)..DocPos(4))
        .map(|(_, run)| {
            (
                run.text,
                run.marks.iter().any(|mark| mark.kind == "emphasis"),
            )
        })
        .collect();
    assert_eq!(runs, [("a", false), ("한", true), ("b", false)]);
    input.undo().unwrap();
    assert_eq!(input.text(), "ab");
}

#[test]
fn break_and_divider_have_separate_native_rows_and_document_positions() {
    use gpdoc_core::{Attrs, ContentSpec, DocPos, Edit, Selection};
    let line_break = NodeSpec::inline("break")
        .group("inline")
        .content(ContentSpec::LineBreak);
    let divider = NodeSpec::block("divider").group("block");
    let config = config(|p| {
        p.node(line_break);
        p.node(divider);
    })
    .plugin(
        ViewPlugin::named("divider-style")
            .node_format("divider", |_, style| style.horizontal_rule = true),
    )
    .build()
    .unwrap();
    let editor = gpdoc_core::Editor::from_document(
        &config,
        vec![
            Node::text("paragraph", "A😀"),
            Node::leaf("divider", Attrs::new()),
            Node::text("paragraph", "B"),
        ],
    )
    .unwrap();
    let mut input = InputBuffer::open(editor).unwrap();
    input.select(1, 1).unwrap();
    let mut tx = input.editor().transaction();
    tx.push(Edit::ReplaceInline {
        range: DocPos(2)..DocPos(2),
        nodes: vec![Node::leaf("break", Attrs::new())],
    });
    input.apply_node_view(tx).unwrap();
    assert_eq!(input.display_text(), "A\n😀\n\nB");
    input.select(5, 5).unwrap();
    assert_eq!(
        input.editor().selection(),
        Selection::Node { before: DocPos(6) }
    );
    input.delete_grapheme_or_selection(true).unwrap();
    assert_eq!(input.text(), "A\n😀\nB");
    input.undo().unwrap();
    assert_eq!(input.dividers(), &[(DocPos(6), 5)]);
    input.select(2, 2).unwrap();
    input.delete_grapheme_or_selection(false).unwrap();
    assert_eq!(input.text(), "A😀\n\nB");
}
