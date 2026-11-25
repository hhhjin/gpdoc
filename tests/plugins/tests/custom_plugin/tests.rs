use super::{InsertSnippet, RepeatSnippet, snippets};
use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::prelude::*;
use gpdoc_plugin_paragraph::paragraph;

#[test]
fn repeat_the_last_snippet() -> Result<(), Error> {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(snippets())
        .build()?;
    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "")])?;
    assert!(matches!(
        editor.run(RepeatSnippet)?,
        CommandOutcome::NotApplicable
    ));

    editor.run(InsertSnippet { text: "Hi!".into() })?;
    editor.run(RepeatSnippet)?;
    assert_eq!(
        editor.document().blocks(),
        [Node::text("paragraph", "Hi!Hi!")]
    );

    editor.undo()?;
    assert_eq!(editor.document().blocks(), [Node::text("paragraph", "Hi!")]);
    editor.undo()?;
    assert_eq!(editor.document().blocks(), [Node::text("paragraph", "")]);
    editor.run(RepeatSnippet)?;
    assert_eq!(editor.document().blocks(), [Node::text("paragraph", "Hi!")]);
    Ok(())
}
