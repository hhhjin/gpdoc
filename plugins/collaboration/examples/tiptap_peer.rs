//! Counterpart to tests/tiptap/collaboration.mjs. stdin/stdout carry Yjs V1 updates.
use gpdoc_plugin::prelude::{DocPos, Selection};
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_collaboration::Collaboration;
use gpdoc_plugin_paragraph::paragraph;
use std::io::{Read, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugin(paragraph())
        .plugin(bold())
        .build()?;
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input)?;
    let mut peer = Collaboration::from_update(&config, "default", &input)?;
    let expected = "Hello 😀";
    assert_eq!(peer.editor().document().root().text_content(), expected);
    let vector = peer.state_vector();
    peer.select(Selection::Text {
        anchor: DocPos(1),
        head: DocPos(6),
    })?;
    peer.edit(|ed| ed.run(gpdoc_plugin_bold::Unset))?;
    peer.select(Selection::caret(DocPos(9)))?;
    peer.edit(|ed| ed.insert_text("!"))?;
    std::io::stdout().write_all(&peer.update_since(&vector)?)?;
    Ok(())
}
