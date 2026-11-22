use gpdoc_plugin::prelude::{Attrs, ContentRef, DocPos, Node, Selection};

#[test]
fn mention_suggestion_replaces_text_and_leaves_one_space_after_an_emoji() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([
            gpdoc_plugin_paragraph::paragraph(),
            gpdoc_plugin_mention::mention(),
        ])
        .build()
        .unwrap();
    let input = "😀 @민수 next";
    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", input)])
            .unwrap();
    let end = "😀 @민수".encode_utf16().count() + 1;
    editor.select(Selection::caret(DocPos(end))).unwrap();

    editor
        .run(gpdoc_plugin_mention::Accept(
            gpdoc_plugin_mention::Mention {
                id: None,
                label: Some("민수".into()),
                trigger: '@',
                attrs: Attrs::new(),
            },
        ))
        .unwrap();
    let ContentRef::Inline(content) = editor.document().blocks()[0].content() else {
        panic!("expected inline content")
    };
    assert_eq!(content[1].kind, "mention");
    assert_eq!(content[2].projected_text(), " next");
    assert_eq!(editor.selection(), Selection::caret(DocPos(6)));
    editor.undo().unwrap();
    assert_eq!(
        editor.document().blocks()[0],
        Node::text("paragraph", input)
    );
}
