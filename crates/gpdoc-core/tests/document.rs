use gpdoc_core::model::declarations::*;
use gpdoc_core::model::{
    Attrs, Content, DocPos, Error, Mark, Node, PositionError, Schema, TextRun,
};
use serde_json::Value;

fn block(name: &str, content: ContentSpec) -> NodeSpec {
    NodeSpec {
        placement: Placement::Block,
        name: name.into(),
        groups: vec!["block".into()],
        attrs: AttrSpecs::new(),
        content,
    }
}
fn paragraph() -> NodeSpec {
    block("p", ContentSpec::text(AllowedMarks::All))
}
fn quote() -> NodeSpec {
    block("quote", ContentSpec::expression("block+"))
}
fn schema(nodes: Vec<NodeSpec>) -> Schema {
    Schema::from_declarations(nodes, vec![], "block+").unwrap()
}

#[test]
fn schema_compilation_checks_declarations() {
    let mut ungrouped = paragraph();
    ungrouped.groups.clear();
    for nodes in [
        vec![paragraph(), paragraph()],
        vec![ungrouped],
        vec![
            paragraph(),
            block(
                "custom",
                ContentSpec::text(AllowedMarks::Only(vec!["absent".into()])),
            ),
        ],
        vec![
            paragraph(),
            block("custom", ContentSpec::expression("absent*")),
        ],
        vec![paragraph(), block("custom", ContentSpec::expression("p|"))],
    ] {
        assert!(Schema::from_declarations(nodes, vec![], "block+").is_err());
    }
}

#[test]
fn child_rules_match_overlapping_groups_without_greedy_capture() {
    // An optional prefix can consume paragraphs, but the suffix needs its own paragraph.
    let schema = schema(vec![
        paragraph(),
        quote(),
        block("panel", ContentSpec::expression("block{0,2} p")),
    ]);
    let p = Node::text("p", "text");
    let q = Node::container("quote", vec![p.clone()]);
    for (kids, valid) in [
        (vec![], false),
        (vec![p.clone()], true),
        (vec![q.clone(), p.clone()], true),
        (vec![p.clone(), p.clone(), p.clone()], true),
        (vec![p.clone(), q], false),
        (vec![p.clone(), p.clone(), p.clone(), p], false),
    ] {
        assert_eq!(schema.allows_children("panel", &kids), valid);
        assert_eq!(
            schema
                .document(vec![Node::container("panel", kids)])
                .is_ok(),
            valid
        );
    }
}

fn rich_block(marks: Vec<Mark>, kind: &str, attrs: Attrs) -> Node {
    Node::new(
        kind,
        attrs,
        Content::Inline(vec![
            TextRun {
                text: "a".into(),
                marks,
            }
            .into(),
        ]),
    )
}
fn link(attrs: Attrs) -> Mark {
    Mark::new("link", attrs)
}

#[test]
fn document_validates_content_shapes_attributes_and_marks_in_descendants() {
    let mut inner = block(
        "inner",
        ContentSpec::text(AllowedMarks::Only(vec!["link".into()])),
    );
    inner.groups.clear();
    inner.attrs.insert(
        "level".into(),
        AttrSpec {
            kind: AttrType::Integer,
            required: true,
            default: None,
        },
    );
    let section = block("section", ContentSpec::expression("inner | p"));
    let plain = block("plain", ContentSpec::text(AllowedMarks::Only(vec![])));
    let link_spec = MarkSpec {
        groups: vec![],
        excludes: None,
        name: "link".into(),
        inclusive_end: false,
        attrs: [(
            "href".into(),
            AttrSpec {
                kind: AttrType::String,
                required: true,
                default: None,
            },
        )]
        .into(),
    };
    let schema = Schema::from_declarations(
        vec![paragraph(), inner, section, plain],
        vec![link_spec],
        "block+",
    )
    .unwrap();
    let attrs: Attrs = [("level".into(), Value::from(1))].into();
    let mark = link([("href".into(), Value::String("here".into()))].into());
    let valid = rich_block(vec![mark.clone()], "inner", attrs.clone());
    assert!(
        schema
            .document(vec![Node::container("section", vec![valid.clone()])])
            .is_ok()
    );
    assert!(schema.document(vec![valid]).is_err());
    assert!(schema.document(vec![]).is_err());
    assert!(schema.document(vec![Node::text("section", "x")]).is_err());
    assert!(
        schema
            .document(vec![rich_block(vec![mark.clone()], "plain", Attrs::new())])
            .is_err()
    );
    for invalid in [
        rich_block(vec![], "inner", Attrs::new()),
        rich_block(
            vec![],
            "inner",
            [("level".into(), Value::String("1".into()))].into(),
        ),
        rich_block(
            vec![],
            "inner",
            [
                ("level".into(), Value::from(1)),
                ("other".into(), Value::Bool(true)),
            ]
            .into(),
        ),
        rich_block(vec![mark.clone(), mark], "inner", attrs.clone()),
        rich_block(vec![link(Attrs::new())], "inner", attrs.clone()),
        rich_block(
            vec![link([("href".into(), Value::Bool(true))].into())],
            "inner",
            attrs.clone(),
        ),
        rich_block(vec![Mark::new("absent", Attrs::new())], "inner", attrs),
    ] {
        assert!(matches!(
            schema.document(vec![Node::container("section", vec![invalid])]),
            Err(Error(_))
        ));
    }
}

#[test]
fn utf16_positions_roundtrip_across_text_runs_and_nested_blocks() {
    let make_run = |text: &str| TextRun {
        text: text.into(),
        marks: vec![],
    };
    let mark = Mark::new("em", Default::default());
    let mut marked = make_run("😀€");
    marked.marks.push(mark);
    let p = Node::new(
        "p",
        Attrs::new(),
        Content::Inline(vec![make_run("A").into(), marked.into()]),
    );
    let quoted = Node::container("quote", vec![p.clone()]);
    let marks = vec![MarkSpec {
        groups: vec![],
        excludes: None,
        name: "em".into(),
        inclusive_end: true,
        attrs: Default::default(),
    }];
    let doc = Schema::from_declarations(vec![paragraph(), quote()], marks, "block+")
        .unwrap()
        .document(vec![quoted.clone(), Node::text("p", "")])
        .unwrap();
    assert_eq!(p.size(), 6);
    assert_eq!(quoted.size(), 8);
    assert_eq!(
        doc.nodes()
            .iter()
            .map(|(pos, n)| (pos.0, n.kind.as_str()))
            .collect::<Vec<_>>(),
        [(0, "quote"), (1, "p"), (2, "text"), (3, "text"), (8, "p")]
    );
    assert_eq!(doc.node_at(DocPos(1)), Some(&p));
    for (doc_pos, bytes) in [(2, 0), (3, 1), (5, "A😀".len()), (6, "A😀€".len())] {
        assert_eq!(doc.text_pos_at_byte(DocPos(1), bytes), Ok(DocPos(doc_pos)));
        assert_eq!(doc.text_at(DocPos(doc_pos)), Ok((DocPos(1), &p)));
        assert_eq!(doc.text_byte_offset(DocPos(doc_pos)), Ok(bytes));
    }
    assert_eq!(
        doc.text_at(DocPos(4)),
        Err(PositionError::InvalidPosition(DocPos(4)))
    );
    for invalid in [0, 1, 7, 8, 10, usize::MAX] {
        assert!(doc.text_at(DocPos(invalid)).is_err());
    }
    assert!(doc.text_pos_at_byte(DocPos(1), 2).is_err());
    assert!(doc.text_pos_at_byte(DocPos(1), 100).is_err());
    assert!(doc.text_pos_at_byte(DocPos(0), 0).is_err());
    assert_eq!(doc.text_pos_at_byte(DocPos(8), 0), Ok(DocPos(9)));
}

#[test]
fn nearest_text_breaks_ties_forward_and_handles_textless_nodes() {
    let schema = schema(vec![paragraph(), block("divider", ContentSpec::Leaf)]);
    let divider = Node::leaf("divider", Attrs::new());
    let doc = schema
        .document(vec![
            Node::text("p", "😀"),
            divider.clone(),
            divider.clone(),
            Node::text("p", ""),
        ])
        .unwrap();
    // Text cursors are 1, 3, and 7. Position 5 is between the two dividers.
    assert_eq!(doc.nearest_text(DocPos(2)), Some(DocPos(3)));
    assert_eq!(doc.nearest_text(DocPos(4)), Some(DocPos(3)));
    assert_eq!(doc.nearest_text(DocPos(5)), Some(DocPos(7)));
    assert_eq!(doc.nearest_text(DocPos(usize::MAX)), Some(DocPos(7)));
    assert_eq!(
        schema
            .document(vec![divider])
            .unwrap()
            .nearest_text(DocPos(0)),
        None
    );
}

#[test]
fn snapshots_share_subtrees_and_reject_unaddressable_size() {
    let schema = schema(vec![paragraph(), quote()]);
    let first = schema.document(vec![Node::text("p", "x")]).unwrap();
    let mut shared = first.blocks()[0].clone();
    let second = schema
        .document(vec![Node::container("quote", vec![shared.clone()])])
        .unwrap();
    assert!(shared.ptr_eq(second.node_at(DocPos(1)).unwrap()));
    assert!(shared.ptr_eq(second.root().child(0).child(0)));
    for _ in 0..usize::BITS {
        shared = Node::container("quote", vec![shared.clone(), shared]);
    }
    assert!(schema.document(vec![shared]).is_err());
}

#[test]
fn schema_enforces_placement_and_ordered_inline_rules() {
    let mention = NodeSpec {
        placement: Placement::Inline,
        name: "mention".into(),
        groups: vec!["reference".into()],
        attrs: Default::default(),
        content: ContentSpec::Leaf,
    };
    let reference = block("reference_line", ContentSpec::expression("reference text*"));
    let schema = schema(vec![paragraph(), quote(), mention.clone(), reference]);
    let reference = |nodes| Node::new("reference_line", Attrs::new(), Content::Inline(nodes));
    let leaf = Node::leaf("mention", Attrs::new());
    let text = Node::from(TextRun {
        text: "abc".into(),
        marks: vec![],
    });
    assert!(
        schema
            .document(vec![reference(vec![leaf.clone(), text.clone()])])
            .is_ok()
    );
    assert!(
        schema
            .document(vec![reference(vec![text, leaf.clone()])])
            .is_err()
    );
    assert!(schema.document(vec![leaf.clone()]).is_err());
    assert!(
        schema
            .document(vec![Node::container("quote", vec![leaf])])
            .is_err()
    );
    let mixed = block("mixed", ContentSpec::expression("(mention | p)*"));
    assert!(
        Schema::from_declarations(vec![paragraph(), mixed, mention], vec![], "block+").is_err()
    );
}

#[test]
fn opening_a_document_merges_adjacent_text_runs_before_validation() {
    let schema = schema(vec![
        paragraph(),
        block("caption", ContentSpec::expression("text?")),
    ]);
    let runs = ["a", "b"]
        .into_iter()
        .map(|text| {
            Node::from(TextRun {
                text: text.to_string(),
                marks: vec![],
            })
        })
        .collect();
    let doc = schema
        .document(vec![Node::new(
            "caption",
            Attrs::new(),
            Content::Inline(runs),
        )])
        .unwrap();
    assert_eq!(doc.blocks(), &[Node::text("caption", "ab")]);
    assert_eq!(doc.root().child(0).child_count(), 1);
}

#[test]
fn document_json_preserves_custom_root_attributes_and_nested_inline_content() {
    use serde_json::json;
    let schema = Schema::from_json(&json!({
        "topNode": "line",
        "nodes": {
            "line": { "content": "inline*", "attrs": { "id": { "validate": "string" } } },
            "annotation": { "inline": true, "group": "inline", "content": "inline*" },
            "text": { "group": "inline" }
        }
    }))
    .unwrap();
    let source = json!({
        "type": "line",
        "attrs": { "id": "a" },
        "content": [{
            "type": "annotation",
            "content": [{ "type": "text", "text": "문서" }]
        }]
    });
    let doc = schema.document_from_json(&source.to_string()).unwrap();
    assert_eq!(doc.root().kind, "line");
    assert_eq!(doc.root().child(0).child(0).text.as_deref(), Some("문서"));
    assert_eq!(
        serde_json::from_str::<Value>(&doc.to_json().unwrap()).unwrap(),
        source
    );
    assert!(
        schema
            .document_from_json(r#"{"type":"annotation"}"#)
            .is_err()
    );
}

#[test]
fn blank_editor_prefers_the_default_text_block_and_fills_required_siblings() {
    use gpdoc_core::{EditorConfig, NodeSpec, Schema};
    let heading = NodeSpec::block("heading").group("block").content("text*");
    let paragraph = NodeSpec::block("paragraph").group("block").content("text*");
    let schema =
        Schema::from_declarations(vec![heading, paragraph], vec![], "block heading").unwrap();
    let editor =
        gpdoc_core::Editor::from_config(&EditorConfig::new(schema, "paragraph").unwrap()).unwrap();
    let kinds: Vec<_> = editor
        .document()
        .blocks()
        .iter()
        .map(|node| node.kind.as_str())
        .collect();
    assert_eq!(kinds, ["paragraph", "heading"]);
}

#[test]
fn blank_editor_opens_inside_an_optional_root_wrapper() {
    use gpdoc_core::{EditorConfig, NodeSpec, Schema};
    let wrapper = NodeSpec::block("wrapper").content("paragraph+");
    let paragraph = NodeSpec::block("paragraph").content("text*");
    let schema = Schema::from_declarations(vec![wrapper, paragraph], vec![], "wrapper*").unwrap();
    let mut editor =
        gpdoc_core::Editor::from_config(&EditorConfig::new(schema, "paragraph").unwrap()).unwrap();
    editor.insert_text("Hello").unwrap();
    assert_eq!(
        editor.document().blocks(),
        [gpdoc_core::Node::container(
            "wrapper",
            vec![gpdoc_core::Node::text("paragraph", "Hello")]
        )]
    );
}
