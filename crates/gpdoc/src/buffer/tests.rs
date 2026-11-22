use super::*;
use crate::{EditorConfig, EditorConfigBuilder, ViewPlugin};
use gpdoc_core::{
    AllowedMarks, Attrs, ContentSpec, Edit, Node, NodeSpec,
    plugin::{Plugin, PluginSpec},
};

fn fixture(declare: impl FnOnce(&mut PluginSpec)) -> EditorConfigBuilder {
    EditorConfig::builder().plugin(Plugin::new("fixture", |spec| {
        spec.node(
            NodeSpec::block("p")
                .group("block")
                .content(ContentSpec::text(AllowedMarks::All)),
        );
        spec.node(NodeSpec::block("quote").group("block").content("block+"));
        spec.default_text_block("p");
        spec.input_rule(|ctx| {
            Ok((ctx.document.blocks() == [p("!")]).then(|| {
                vec![Edit::ReplaceText {
                    range: ctx.inserted,
                    text: "expanded".into(),
                }]
            }))
        });
        declare(spec);
    }))
}
fn p(text: &str) -> Node {
    Node::text("p", text)
}
fn buffer(nodes: Vec<Node>) -> InputBuffer {
    InputBuffer::open(
        gpdoc_core::Editor::from_document(&fixture(|_| {}).build().unwrap(), nodes).unwrap(),
    )
    .unwrap()
}

#[test]
fn nested_node_and_mark_formats_follow_multibyte_text_and_preedit() {
    use gpdoc_core::{Attrs, Content, Mark, MarkSpec, TextFormat, TextRun};

    let formats = ViewPlugin::named("formats")
        .node_format("quote", |_, style| style.italic = true)
        .mark_format("note", |_, style| {
            style.background = Some([255, 220, 160, 255])
        });
    let config = fixture(|p| p.mark(MarkSpec::new("note")))
        .plugin(formats)
        .build()
        .unwrap();
    let run = TextRun {
        text: "한😀".into(),
        marks: vec![Mark::new("note", Attrs::new())],
    };
    let block = Node::new("p", Attrs::new(), Content::Inline(vec![run.into()]));
    let mut buffer = InputBuffer::open(
        gpdoc_core::Editor::from_document(
            &config,
            vec![Node::container("quote", vec![block]), p("x")],
        )
        .unwrap(),
    )
    .unwrap();
    let spans = |b: &InputBuffer| {
        b.formats()
            .iter()
            .map(|s| (s.byte_end, s.utf16_end, s.format.clone()))
            .collect::<Vec<_>>()
    };
    let note = TextFormat {
        italic: true,
        background: Some([255, 220, 160, 255]),
        ..Default::default()
    };
    let original = spans(&buffer);
    assert_eq!(
        original,
        [(7, 3, note.clone()), (9, 5, TextFormat::default())]
    );
    buffer.select(3, 3).unwrap();
    buffer.mark(None, "字", None).unwrap();
    assert_eq!(
        spans(&buffer),
        [(10, 4, note), (12, 6, TextFormat::default())]
    );
    buffer.cancel_mark();
    assert_eq!(spans(&buffer), original);
}

#[test]
fn marked_hangul_stays_out_of_history_until_one_commit() {
    let mut b = buffer(vec![p("A😀B")]);
    b.select(3, 1).unwrap();
    for stage in ["ㅎ", "하", "한"] {
        b.mark(None, stage, Some(1..1)).unwrap();
        assert_eq!(b.marked_range(), Some(1..2));
        assert_eq!(b.text(), format!("A{stage}B"));
        assert_eq!(b.editor().revision(), 0);
        assert_eq!(b.editor().document().blocks(), &[p("A😀B")]);
    }
    b.replace(None, "한").unwrap();
    b.unmark().unwrap();
    assert_eq!(b.editor().revision(), 1);
    assert_eq!(b.selection(), (2, 2));
    b.undo().unwrap();
    assert_eq!(b.selection(), (3, 1));
    assert_eq!(b.text(), "A😀B");
    b.redo().unwrap();
    assert_eq!(b.text(), "A한B");
}

#[test]
fn visible_ranges_cover_partial_and_disjoint_composition_edits() {
    let mut b = buffer(vec![Node::container("quote", vec![p("x")]), p("y")]);
    b.mark(Some(1..1), "abc", Some(0..2)).unwrap();
    assert_eq!(b.selection(), (1, 3));
    b.mark(Some(2..3), "Z", None).unwrap();
    assert_eq!(b.text(), "xaZc\ny");
    assert_eq!(b.marked_range(), Some(2..3));
    b.replace(Some(5..6), "z").unwrap();
    assert_eq!(
        b.editor.document().blocks(),
        &[Node::container("quote", vec![p("xaZc")]), p("z")]
    );
    b.undo().unwrap();
    assert_eq!(b.text(), "x\ny");
}

#[test]
fn paste_across_nested_blocks_creates_real_paragraphs_and_preserves_undo_selection() {
    let mut b = buffer(vec![Node::container("quote", vec![p("ab"), p("cd")])]);
    b.select(4, 1).unwrap();
    b.replace(None, "A\r\n\r\nB\r").unwrap();
    assert_eq!(b.text(), "aA\n\nB\nd");
    assert_eq!(
        b.editor.document().blocks(),
        &[Node::container(
            "quote",
            vec![p("aA"), p(""), p("B"), p("d")]
        )]
    );
    assert_eq!(b.selection(), (6, 6));
    b.undo().unwrap();
    assert_eq!(b.selection(), (4, 1));
    assert_eq!(b.text(), "ab\ncd");
}

#[test]
fn substring_adjusts_native_queries_but_invalid_edits_keep_the_preview() {
    let mut b = buffer(vec![p("👩‍💻"), p("A")]);
    assert_eq!(b.text_for_range(1..4), Some(("👩‍💻".into(), 0..5)));
    assert_eq!(b.text_for_range(5..usize::MAX), Some(("\nA".into(), 5..7)));
    assert_eq!(b.text_for_range(8..8), None);
    b.mark(Some(0..0), "😀", None).unwrap();
    assert!(b.replace(Some(1..2), "X").is_err());
    assert!(b.mark(None, "😀", Some(1..1)).is_err());
    assert_eq!(b.text(), "😀👩‍💻\nA");
    b.cancel_mark();
    assert_eq!(b.text(), "👩‍💻\nA");
    assert_eq!(b.editor().revision(), 0);
}

#[test]
fn unmark_runs_rules_on_commit_and_exact_replacement_restores_original_caret() {
    let mut b = buffer(vec![p("a")]);
    b.select(1, 1).unwrap();
    b.mark(Some(0..1), "!", None).unwrap();
    assert_eq!(b.text(), "!");
    b.unmark().unwrap();
    assert_eq!(b.text(), "expanded");
    b.undo().unwrap();
    assert_eq!(b.text(), "a");
    assert_eq!(b.selection(), (1, 1));
}

#[test]
fn native_two_set_replacements_commit_one_syllable_and_redo_it() {
    let mut b = buffer(vec![p("A😀B")]);
    b.select(3, 1).unwrap();
    b.native_replace(None, "ㅎ").unwrap();
    for next in ["하", "한", "한"] {
        b.native_replace(Some(1..2), next).unwrap();
        assert!(b.has_preedit());
        assert_eq!(b.marked_range(), None);
        assert_eq!(b.editor().revision(), 0);
    }
    assert_eq!(b.text(), "A한B");
    b.undo().unwrap();
    assert_eq!(b.selection(), (3, 1));
    assert_eq!(b.text(), "A😀B");
    b.redo().unwrap();
    assert_eq!(b.text(), "A한B");
    assert!(!b.has_preedit());
}

#[test]
fn resyllabification_new_input_and_paste_end_each_implicit_group() {
    let mut b = buffer(vec![p("")]);
    b.native_replace(None, "ㅎ").unwrap();
    for next in ["하", "한", "하"] {
        b.native_replace(Some(0..1), next).unwrap();
    }
    b.native_replace(None, "나").unwrap();
    assert_eq!(b.editor().document().blocks(), &[p("하")]);
    assert_eq!(b.text(), "하나");
    b.native_replace(Some(1..2), "낙").unwrap();
    b.native_replace(Some(1..2), "나").unwrap();
    b.replace(None, "paste\n").unwrap();
    assert_eq!(b.text(), "하나paste\n");
    b.undo().unwrap();
    assert_eq!(b.text(), "하나");
    b.undo().unwrap();
    assert_eq!(b.text(), "하");
    b.undo().unwrap();
    assert_eq!(b.text(), "");
}

#[test]
fn moving_away_commits_standalone_jamo_and_later_range_is_an_independent_edit() {
    let mut b = buffer(vec![p("")]);
    b.native_replace(None, "ㅎ").unwrap();
    b.select(0, 0).unwrap();
    assert_eq!(b.editor().document().blocks(), &[p("ㅎ")]);
    b.native_replace(Some(0..1), "하").unwrap();
    assert!(!b.has_preedit());
    b.undo().unwrap();
    assert_eq!(b.text(), "ㅎ");
    b.undo().unwrap();
    assert_eq!(b.text(), "");
}

#[test]
fn japanese_kana_to_kanji_preserves_clause_selection_and_commits_once() {
    let mut b = buffer(vec![p("A😀old!")]);
    b.select(6, 3).unwrap();

    b.mark(None, "にほんご", Some(4..4)).unwrap();
    assert_eq!(b.marked_range(), Some(3..7));
    b.mark(None, "日本語", Some(0..2)).unwrap();
    assert_eq!(b.text(), "A😀日本語!");
    assert_eq!(b.marked_range(), Some(3..6));
    assert_eq!(b.selection(), (3, 5));
    assert_eq!(b.text_for_range(3..5), Some(("日本".into(), 3..5)));
    assert_eq!(b.editor().document().blocks(), &[p("A😀old!")]);

    b.native_replace(None, "日本語").unwrap();
    b.unmark().unwrap();
    assert_eq!(b.marked_range(), None);
    assert_eq!(b.selection(), (6, 6));
    assert_eq!(b.editor().revision(), 1);
    assert_eq!(b.editor().document().blocks(), &[p("A😀日本語!")]);
    b.undo().unwrap();
    assert_eq!(b.text(), "A😀old!");
    assert_eq!(b.selection(), (6, 3));
    b.redo().unwrap();
    assert_eq!(b.text(), "A😀日本語!");
    assert_eq!(b.selection(), (6, 6));
}

#[test]
fn cancelling_japanese_reconversion_preserves_the_prior_selection_and_history() {
    let mut b = buffer(vec![p("A😀橋")]);
    b.select(4, 4).unwrap();
    b.native_replace(None, "!").unwrap();
    b.select(4, 3).unwrap();

    // Switch candidates for the existing word "hashi", then cancel reconversion.
    b.mark(Some(3..4), "はし", Some(0..2)).unwrap();
    assert_eq!(b.marked_range(), Some(3..5));
    b.mark(None, "箸", Some(0..1)).unwrap();
    assert_eq!(b.text(), "A😀箸!");
    b.cancel_mark();
    assert_eq!(b.text(), "A😀橋!");
    assert_eq!(b.selection(), (4, 3));
    assert!(!b.has_preedit());
    assert_eq!(b.editor().revision(), 1);
    b.undo().unwrap();
    assert_eq!(b.text(), "A😀橋");
    assert_eq!(b.selection(), (4, 4));
}

#[test]
fn chinese_pinyin_candidate_replaces_the_reading_and_unmarks_at_its_new_end() {
    let mut b = buffer(vec![p("😀"), p("X!")]);
    b.select(4, 4).unwrap();
    b.mark(None, "ni", Some(2..2)).unwrap();
    b.mark(None, "nihao", Some(5..5)).unwrap();
    assert_eq!(b.marked_range(), Some(4..9));
    assert_eq!(b.selection(), (9, 9));

    b.mark(Some(4..9), "你好", Some(2..2)).unwrap();
    assert_eq!(b.marked_range(), Some(4..6));
    assert_eq!(b.text_for_range(4..7), Some(("你好!".into(), 4..7)));
    assert_eq!(b.editor().revision(), 0);
    b.unmark().unwrap();
    assert_eq!(b.marked_range(), None);
    assert_eq!(b.editor().document().blocks(), &[p("😀"), p("X你好!")]);
    assert_eq!(b.selection(), (6, 6));
    b.undo().unwrap();
    assert_eq!(b.text(), "😀\nX!");
    assert_eq!(b.selection(), (4, 4));
    b.redo().unwrap();
    assert_eq!(b.text(), "😀\nX你好!");
}

#[test]
fn projected_labels_edit_and_select_as_whole_atoms() {
    use gpdoc_core::Content;
    let atoms = Plugin::new("atoms", |p| {
        p.node(NodeSpec::block("p").group("block").content("inline*"));
        p.node(NodeSpec::inline("mention").group("inline"));
        p.node(NodeSpec::block("media").group("block"));
        p.default_text_block("p");
    });
    let mut labels = ViewPlugin::named("labels");
    for (name, label) in [("mention", "@한글"), ("media", "cover")] {
        labels = labels.node_format(name, move |_, style| style.atom_label = Some(label.into()));
    }
    let config = EditorConfig::builder()
        .plugin(atoms)
        .plugin(labels)
        .build()
        .unwrap();
    let text = |s: &str| {
        gpdoc_core::TextRun {
            text: s.into(),
            marks: vec![],
        }
        .into()
    };
    let first = Node::new(
        "p",
        Attrs::new(),
        Content::Inline(vec![
            text("A😀"),
            Node::leaf("mention", Attrs::new()),
            text("B"),
        ]),
    );
    let mut buffer = InputBuffer::open(
        gpdoc_core::Editor::from_document(
            &config,
            vec![first.clone(), Node::leaf("media", Attrs::new()), p("C")],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(buffer.text(), "A😀@한글B\ncover\nC");
    assert_eq!(buffer.offset(DocPos(4)), 3);
    assert_eq!(buffer.offset(DocPos(5)), 6);
    assert_eq!(buffer.position(6).unwrap(), DocPos(5));
    buffer.replace(Some(3..6), "!").unwrap();
    assert_eq!(buffer.editor().document().blocks()[0], p("A😀!B"));
    buffer.undo().unwrap();
    buffer.select(10, 10).unwrap();
    assert_eq!(
        buffer.editor().selection(),
        Selection::Node {
            before: DocPos(first.size())
        }
    );
    assert_eq!(buffer.selection(), (8, 13));
    buffer.replace(None, "replacement").unwrap();
    assert!(!buffer.text().contains("cover"));
}

fn fold_fixture(persist: bool) -> (EditorConfig, std::rc::Rc<crate::fold::FoldSpecs>) {
    use crate::{FoldRegion, FoldSpec};
    use gpdoc_core::{AttrSpec, AttrType};

    let mut container = NodeSpec::block("section")
        .group("block")
        .content("title body");
    if persist {
        container.attrs.insert(
            "expanded".into(),
            AttrSpec::with_default(AttrType::Bool, false.into()),
        );
    }
    let spec: FoldSpec = |node, before| {
        let title = node.child(0);
        let fallback = DocPos(before.0 + title.size());
        Some(FoldRegion {
            fallback,
            body: DocPos(fallback.0 + 1)..DocPos(before.0 + node.size() - 1),
            open_attribute: node.attrs.contains_key("expanded").then_some("expanded"),
        })
    };
    let config = fixture(|p| {
        p.node(container);
        p.node(NodeSpec::block("title").content("text*"));
        p.node(NodeSpec::block("body").content("block+"));
    })
    .build()
    .unwrap();
    let mut fold_specs = crate::fold::FoldSpecs::default();
    fold_specs.insert("section".into(), spec);
    (config, std::rc::Rc::new(fold_specs))
}

#[test]
fn fold_projects_only_visible_blocks_and_maps_view_state_through_edits() {
    let (config, fold_specs) = fold_fixture(false);
    let section = Node::container(
        "section",
        vec![
            Node::text("title", "Summary"),
            Node::container("body", vec![p("Hidden")]),
        ],
    );
    let mut buffer = InputBuffer::with_folds(
        gpdoc_core::Editor::from_document(&config, vec![p("A"), section, p("B")]).unwrap(),
        fold_specs,
    )
    .unwrap();
    assert_eq!(buffer.text(), "A\nSummary\nB");
    let before = buffer.folds()[0].before;
    let hidden_start = buffer.folds()[0].region.body.start.0 + 2;
    let hidden_match = DocPos(hidden_start)..DocPos(hidden_start + "Hidden".len());
    assert_eq!(buffer.decoration_range(hidden_match.clone()), None);
    let last_visible = buffer.position(11).unwrap();
    assert_eq!(
        buffer.decoration_range(hidden_match.start..last_visible),
        Some(10..11)
    );
    let revision = buffer.editor().revision();
    buffer.toggle_fold(before, false, revision).unwrap();
    assert_eq!(buffer.text(), "A\nSummary\nHidden\nB");
    assert_eq!(buffer.decoration_range(hidden_match), Some(10..16));
    assert_eq!(buffer.editor().revision(), revision);
    buffer.mark(Some(0..0), "한", None).unwrap();
    assert_eq!(buffer.text(), "한A\nSummary\nHidden\nB");
    buffer.unmark().unwrap();
    let moved = buffer.folds()[0].before;
    assert_eq!(moved, gpdoc_core::DocPos(before.0 + 1));
    buffer.select(13, 13).unwrap();
    buffer
        .toggle_fold(moved, true, buffer.editor().revision())
        .unwrap();
    assert_eq!(buffer.text(), "한A\nSummary\nB");
    assert_eq!(
        buffer.editor().selection(),
        Selection::caret(buffer.folds()[0].region.fallback)
    );
    assert!(
        buffer
            .editor()
            .document()
            .node_at(moved)
            .unwrap()
            .attrs
            .is_empty()
    );
}

#[test]
fn persistent_fold_toggles_expansion_attribute_and_undo_restores_it() {
    use gpdoc_core::Value;
    let (config, fold_specs) = fold_fixture(true);
    let mut buffer = InputBuffer::with_folds(
        gpdoc_core::Editor::from_document(
            &config,
            vec![Node::container(
                "section",
                vec![
                    Node::text("title", "Outer"),
                    Node::container(
                        "body",
                        vec![Node::container(
                            "section",
                            vec![
                                Node::text("title", "Nested"),
                                Node::container("body", vec![p("Body")]),
                            ],
                        )],
                    ),
                ],
            )],
        )
        .unwrap(),
        fold_specs,
    )
    .unwrap();
    assert_eq!(buffer.text(), "Outer");
    buffer
        .toggle_fold(DocPos(0), false, buffer.editor().revision())
        .unwrap();
    assert_eq!(buffer.text(), "Outer\nNested");
    assert_eq!(
        buffer.editor().document().blocks()[0].attrs["expanded"],
        Value::Bool(true)
    );
    let inner = buffer.folds()[1].before;
    buffer
        .toggle_fold(inner, false, buffer.editor().revision())
        .unwrap();
    assert_eq!(buffer.text(), "Outer\nNested\nBody");
    buffer.undo().unwrap();
    assert_eq!(buffer.text(), "Outer\nNested");
    buffer.undo().unwrap();
    assert_eq!(buffer.text(), "Outer");
    assert_eq!(buffer.folds().len(), 1);
}
