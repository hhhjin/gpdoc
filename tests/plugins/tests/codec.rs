use gpdoc_plugin::core::JsonError;
use gpdoc_plugin::prelude::{
    AllowedMarks, AttrSpec, AttrType, Content, ContentRef, ContentSpec, Mark, Node, NodeSpec,
    Plugin, TextRun,
};
use gpdoc_plugin_blockquote::blockquote;
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_bullet_list::bullet_list;
use gpdoc_plugin_code_block::code_block;
use gpdoc_plugin_heading::heading;
use gpdoc_plugin_italic::italic;
use gpdoc_plugin_link::link;
use gpdoc_plugin_list_item::list_item;
use gpdoc_plugin_ordered_list::ordered_list;
use gpdoc_plugin_paragraph::paragraph;
use serde_json::{Value, json};

fn task() -> Plugin {
    Plugin::new("task", |p| {
        p.node(
            NodeSpec::block("task")
                .group("block")
                .content(ContentSpec::text(AllowedMarks::All))
                .attrs([("done".into(), AttrSpec::required(AttrType::Bool))].into()),
        )
    })
}

#[test]
fn opens_prosemirror_json_and_preserves_structure_across_an_edit() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([
            paragraph(),
            blockquote(),
            heading(),
            bold(),
            link(Default::default()),
            task(),
        ])
        .build()
        .unwrap();
    let source = json!({"type":"doc","content":[
        {"type":"paragraph"},
        {"type":"blockquote","content":[
            {"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"제목"}]},
            {"type":"task","attrs":{"done":false},"content":[
                {"type":"text","text":"안녕 👋","marks":[{"type":"link","attrs":{"href":"https://example.com","title":"사이트","target":"_blank","rel":"noopener noreferrer nofollow","class":null}},{"type":"bold"}]},
                {"type":"text","text":"!"}
            ]}
        ]}
    ]});
    let mut editor = gpdoc_plugin::core::Editor::from_json(&config, &source.to_string()).unwrap();
    let saved: Value = serde_json::from_str(&editor.document().to_json().unwrap()).unwrap();
    assert_eq!(saved, source);

    editor.insert_text("첫 문단").unwrap();
    let mut reopened =
        gpdoc_plugin::core::Editor::from_json(&config, &editor.document().to_json().unwrap())
            .unwrap();
    assert_eq!(reopened.document(), editor.document());
    assert_eq!(reopened.revision(), 0);
    assert_eq!(
        reopened.undo().unwrap_err(),
        gpdoc_plugin::core::Error::NoHistory
    );

    let standard = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([
            paragraph(),
            blockquote(),
            heading(),
            bold(),
            link(Default::default()),
        ])
        .build()
        .unwrap();
    assert!(matches!(
        gpdoc_plugin::core::Editor::from_json(&standard, &source.to_string()),
        Err(gpdoc_plugin::core::OpenJsonError::Document(
            JsonError::Document(_)
        ))
    ));
}

#[test]
fn rejects_invalid_and_unrepresentable_input() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph(), heading(), bullet_list(), list_item()])
        .build()
        .unwrap();
    for node in [
        json!({"type":"heading","attrs":{"level":"2"}}),
        json!({"type":"bulletList","content":[{"type":"paragraph"}]}),
        json!({"type":"paragraph","content":[{"type":"image","attrs":{"src":"a.png"}}]}),
        json!({"type":"paragraph","content":[{"type":"text","text":""}]}),
        json!({"type":"paragraph","content":[{"type":"text","text":"x","marks":[{"type":"unknown"}]}]}),
    ] {
        assert!(
            gpdoc_plugin::core::Editor::from_json(
                &config,
                &json!({"type":"doc","content":[node]}).to_string()
            )
            .is_err()
        );
    }
    assert!(
        gpdoc_plugin::core::Editor::from_json(&config, r#"{"version":1,"blocks":[]}"#).is_err()
    );
}

#[test]
fn preserves_tiptap_list_names_and_nullable_mark_attributes() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([
            paragraph(),
            list_item(),
            ordered_list(),
            link(Default::default()),
            italic(),
            bold(),
        ])
        .build()
        .unwrap();
    let source = include_str!("tiptap-list.json");
    let editor = gpdoc_plugin::core::Editor::from_json(&config, source).unwrap();
    let list = &editor.document().blocks()[0];
    assert_eq!(list.attrs["start"], gpdoc_plugin::core::Value::from(3));
    let (_, paragraph) = editor.document().iter_blocks().last().unwrap();
    let ContentRef::Inline(runs) = paragraph.content() else {
        panic!("expected text")
    };
    assert_eq!(
        runs[0]
            .marks()
            .iter()
            .map(|m| m.kind.as_str())
            .collect::<Vec<_>>(),
        ["link", "italic", "bold"]
    );
    let saved: Value = serde_json::from_str(&editor.document().to_json().unwrap()).unwrap();
    let expected: Value = serde_json::from_str(source).unwrap();
    assert_eq!(saved, expected);
}

#[test]
fn deepest_supported_marked_text_reopens() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph(), blockquote(), link(Default::default())])
        .build()
        .unwrap();
    let mut node = Node::new(
        "paragraph",
        Default::default(),
        Content::Inline(vec![
            TextRun {
                text: "x".into(),
                marks: vec![Mark::new(
                    "link",
                    [("href".into(), gpdoc_plugin::core::Value::String("#".into()))].into(),
                )],
            }
            .into(),
        ]),
    );
    for _ in 1..40 {
        node = Node::container("blockquote", vec![node]);
    }
    let editor = gpdoc_plugin::core::Editor::from_document(&config, vec![node.clone()]).unwrap();
    let saved = editor.document().to_json().unwrap();
    assert_eq!(
        gpdoc_plugin::core::Editor::from_json(&config, &saved)
            .unwrap()
            .document(),
        editor.document()
    );

    let deeper = gpdoc_plugin::core::Editor::from_document(
        &config,
        vec![Node::container("blockquote", vec![node])],
    )
    .unwrap();
    assert!(matches!(
        deeper.document().to_json(),
        Err(JsonError::NestingLimit(40))
    ));
}

fn mention() -> Plugin {
    Plugin::new("mention", |p| {
        p.node(
            NodeSpec::inline("mention")
                .attrs([("id".into(), AttrSpec::required(AttrType::String))].into()),
        )
    })
}

#[test]
fn plugin_leaf_next_to_emoji_survives_edits_and_prosemirror_json() {
    use gpdoc_plugin::prelude::{Attrs, DocPos, Edit, Selection, Value};
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph(), code_block(), bold(), mention()])
        .build()
        .unwrap();
    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "😀!")])
            .unwrap();
    let mention = Node::leaf(
        "mention",
        Attrs::from([("id".into(), Value::String("minsu".into()))]),
    );
    let before = DocPos(3);
    let after = DocPos(4);
    let mut insert = editor.transaction();
    insert.push(Edit::ReplaceInline {
        range: before..before,
        nodes: vec![mention.clone()],
    });
    editor.apply(insert).unwrap();
    assert_eq!(editor.document().node_at(before), Some(&mention));
    assert_eq!(
        editor.document().text_byte_offset(after).unwrap(),
        "😀\u{fffc}".len()
    );
    let mut format = editor.transaction();
    format.push(Edit::SetMark {
        range: DocPos(1)..after,
        mark: Mark::new("bold", Attrs::new()),
    });
    editor.apply(format).unwrap();
    let saved = editor.document().to_json().unwrap();
    let expected = json!({"type":"doc","content":[{"type":"paragraph","content":[
        {"type":"text","text":"😀","marks":[{"type":"bold"}]},
        {"type":"mention","attrs":{"id":"minsu"},"marks":[{"type":"bold"}]},
        {"type":"text","text":"!"}
    ]}]});
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved).unwrap(),
        expected
    );
    let mut reopened = gpdoc_plugin::core::Editor::from_json(&config, &saved).unwrap();
    assert_eq!(reopened.document(), editor.document());
    reopened.select(Selection::Node { before }).unwrap();
    reopened.delete_selection().unwrap();
    assert!(reopened.document().node_at(before).is_none());
    reopened.undo().unwrap();
    assert_eq!(reopened.document(), editor.document());
    assert_eq!(reopened.selection(), Selection::Node { before });

    let mut joined = gpdoc_plugin::core::Editor::from_document(
        &config,
        vec![Node::text("paragraph", "A1"), Node::text("paragraph", "2B")],
    )
    .unwrap();
    joined
        .select(Selection::Text {
            anchor: DocPos(2),
            head: DocPos(6),
        })
        .unwrap();
    let mut tx = joined.transaction();
    tx.push(Edit::ReplaceInline {
        range: DocPos(2)..DocPos(6),
        nodes: vec![mention.clone()],
    });
    joined.apply(tx).unwrap();
    assert_eq!(joined.document().blocks().len(), 1);
    assert_eq!(joined.document().node_at(DocPos(2)), Some(&mention));
    assert_eq!(joined.selection(), Selection::caret(DocPos(3)));

    let mut code =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("codeBlock", "")])
            .unwrap();
    let mut insert = code.transaction();
    insert.push(Edit::ReplaceInline {
        range: DocPos(1)..DocPos(1),
        nodes: vec![mention],
    });
    assert!(code.apply(insert).is_err());
    assert_eq!(code.revision(), 0);
    let mut invalid = expected;
    invalid["content"][0]["type"] = "codeBlock".into();
    assert!(gpdoc_plugin::core::Editor::from_json(&config, &invalid.to_string()).is_err());
}

#[test]
fn schemas_own_their_mark_names() {
    fn strong() -> Plugin {
        Plugin::new("strong", |p| {
            p.mark(gpdoc_plugin::core::MarkSpec::new("strong"))
        })
    }

    let standard = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph(), bold()])
        .build()
        .unwrap();
    let custom = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph(), strong()])
        .build()
        .unwrap();
    for (name, owner, other) in [("bold", &standard, &custom), ("strong", &custom, &standard)] {
        let source = json!({"type":"doc","content":[{"type":"paragraph","content":[
            {"type":"text","text":"text","marks":[{"type":name}]}
        ]}]});
        let editor = gpdoc_plugin::core::Editor::from_json(&owner, &source.to_string()).unwrap();
        let saved: Value = serde_json::from_str(&editor.document().to_json().unwrap()).unwrap();
        assert_eq!(saved, source);
        assert_eq!(
            gpdoc_plugin::core::Editor::from_json(&owner, &saved.to_string())
                .unwrap()
                .document(),
            editor.document()
        );
        assert!(gpdoc_plugin::core::Editor::from_json(&other, &saved.to_string()).is_err());
    }
}
