use gpdoc_core::{ContentSpec, EditorConfig, Node, NodeSpec, Schema};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paragraph = NodeSpec::block("paragraph").content(ContentSpec::expression("text*"));
    let schema = Schema::from_declarations(vec![paragraph], vec![], "paragraph+")?;
    let config = EditorConfig::new(schema, "paragraph")?;
    let mut editor = gpdoc_core::Editor::from_config(&config)?;

    editor.insert_text("Hello")?;
    assert_eq!(
        editor.document().blocks(),
        [Node::text("paragraph", "Hello")]
    );
    editor.undo()?;
    assert_eq!(editor.document().blocks(), [Node::text("paragraph", "")]);
    Ok(())
}
