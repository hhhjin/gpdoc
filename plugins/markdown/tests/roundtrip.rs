use gpdoc_plugin::prelude::{DocPos, Node, Selection};
use gpdoc_plugin_blockquote::blockquote;
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_code_block::code_block;
use gpdoc_plugin_heading::heading;
use gpdoc_plugin_horizontal_rule::horizontal_rule;
use gpdoc_plugin_inline_code::inline_code;
use gpdoc_plugin_italic::italic;
use gpdoc_plugin_link::link;
use gpdoc_plugin_list_item::list_item;
use gpdoc_plugin_markdown::{markdown, open, to_markdown};
use gpdoc_plugin_ordered_list::ordered_list;
use gpdoc_plugin_paragraph::paragraph;

#[test]
fn markdown_roundtrip_preserves_structure_markup_and_unicode() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugin(paragraph())
        .plugin(heading())
        .plugin(blockquote())
        .plugin(ordered_list())
        .plugin(list_item())
        .plugin(bold())
        .plugin(italic())
        .plugin(link(Default::default()))
        .plugin(inline_code())
        .plugin(code_block())
        .build()
        .unwrap();
    let source = "# Hello 😀\n\n> **bold**, *italic*, [link](https://example.org) and `x`\n\n3. first\n4. second\n\n```rust\nfn main() {}\n```";
    let first = open(&config, source).unwrap();
    assert!(first.warnings.is_empty());
    let doc = first.value.document();
    assert_eq!(
        doc.blocks()
            .iter()
            .map(|n| n.kind.as_str())
            .collect::<Vec<_>>(),
        ["heading", "blockquote", "orderedList", "codeBlock"]
    );
    let exported = to_markdown(&first.value).unwrap();
    assert!(exported.warnings.is_empty());
    let second = open(&config, &exported.value).unwrap();
    assert_eq!(second.value.document(), doc, "{}", exported.value);
}

#[test]
fn missing_plugins_warn_and_preserve_text() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugin(paragraph())
        .build()
        .unwrap();
    let result = open(&config, "# **Hello**").unwrap();
    assert_eq!(result.value.document().root().text_content(), "Hello");
    assert_eq!(result.warnings.len(), 2);
    assert_eq!(result.warnings[0].position, 2..11);
}

#[test]
fn set_markdown_replaces_content_in_one_undo_step() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugin(paragraph())
        .plugin(heading())
        .plugin(horizontal_rule())
        .plugin(markdown())
        .build()
        .unwrap();
    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "old")])
            .unwrap();
    editor.select(Selection::caret(DocPos(2))).unwrap();
    editor
        .run(gpdoc_plugin_markdown::Set {
            markdown: "## New\n\n---".into(),
        })
        .unwrap();
    assert_eq!(editor.document().blocks()[0].kind, "heading");
    assert_eq!(editor.document().blocks()[1].kind, "horizontalRule");
    editor.undo().unwrap();
    assert_eq!(editor.document().root().text_content(), "old");
    assert_eq!(editor.selection(), Selection::caret(DocPos(2)));
}

#[test]
fn conflicting_delimiters_fail_before_set_markdown_changes_history() {
    use gpdoc_plugin::prelude::{Error, MarkSpec, Plugin};
    use gpdoc_plugin_markdown::MarkdownSyntax;
    let marked = |name, token| {
        Plugin::<()>::new(name, |p| {
            p.mark(MarkSpec::new(name));
            p.metadata(MarkdownSyntax::default().mark(name, token));
        })
    };
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugin(paragraph())
        .plugin(markdown())
        .plugin(marked("one", "=="))
        .plugin(marked("two", "==="))
        .build()
        .unwrap();
    let mut editor = gpdoc_plugin::core::Editor::from_document(
        &config,
        vec![Node::text("paragraph", "original")],
    )
    .unwrap();
    let cmd = gpdoc_plugin_markdown::Set {
        markdown: "==new==".into(),
    };
    assert!(matches!(editor.run(cmd), Err(Error::InvalidSchema(_))));
    assert!(matches!(to_markdown(&editor), Err(Error::InvalidSchema(_))));
    assert_eq!(editor.document().root().text_content(), "original");
    assert!(matches!(editor.undo(), Err(Error::NoHistory)));
}

#[test]
fn custom_syntax_is_scoped_to_the_kit_and_preserves_literal_tokens() {
    use gpdoc_plugin::prelude::{MarkSpec, Plugin};
    use gpdoc_plugin_markdown::MarkdownSyntax;
    let note = |token| {
        Plugin::<()>::new("notes", |p| {
            p.mark(MarkSpec::new("note"));
            p.metadata(MarkdownSyntax::default().mark("note", token));
        })
    };
    let original = note("==");
    let variant = note("++");
    for (plugin, own, other) in [(original, "==", "++"), (variant, "++", "==")] {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugin(paragraph())
            .plugin(inline_code())
            .plugin(code_block())
            .plugin(plugin)
            .build()
            .unwrap();
        let source = format!(
            "{own}yes{own} {other}literal{other} {own}unclosed\n\n```\n{own}verbatim{own}\n```"
        );
        let imported = open(&config, &source).unwrap();
        let marked = imported
            .value
            .document()
            .iter_nodes()
            .filter(|(_, n)| n.marks().iter().any(|m| m.kind == "note"))
            .map(|(_, n)| n.projected_text())
            .collect::<String>();
        assert_eq!(marked, "yes");
        assert!(imported.warnings.is_empty());
        let exported = to_markdown(&imported.value).unwrap();
        assert!(exported.warnings.is_empty());
        assert_eq!(
            open(&config, &exported.value).unwrap().value.document(),
            imported.value.document()
        );
    }
}

#[test]
fn a_custom_delimiter_in_image_alt_text_is_literal() {
    use gpdoc_plugin::prelude::{MarkSpec, Plugin};
    use gpdoc_plugin_markdown::MarkdownSyntax;
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_image::image())
        .plugin(Plugin::<()>::new("notes", |p| {
            p.mark(MarkSpec::new("note"));
            p.metadata(MarkdownSyntax::default().mark("note", "=="));
        }))
        .build()
        .unwrap();
    let imported = open(&config, "![==caption==](/photo.png)").unwrap();
    let image = imported
        .value
        .document()
        .iter_nodes()
        .find(|(_, n)| n.kind == "image")
        .unwrap()
        .1;
    assert_eq!(image.attrs["alt"], "==caption==");
    let exported = to_markdown(&imported.value).unwrap();
    assert_eq!(
        open(&config, &exported.value).unwrap().value.document(),
        imported.value.document()
    );
}
