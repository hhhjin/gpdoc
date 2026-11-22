use gpdoc::{Command, DocPos, Editor, EditorConfig, Error, Node, Selection};
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_italic::italic;
use gpdoc_plugin_paragraph::paragraph;
use wasm_bindgen::prelude::*;

fn example_config() -> Result<EditorConfig, Error> {
    EditorConfig::builder()
        .plugins([paragraph(), bold(), italic()])
        .build()
}

/// A one-paragraph document with zero-based UTF-16 text offsets.
#[wasm_bindgen]
pub struct ParagraphExample {
    editor: Editor,
}

impl ParagraphExample {
    fn select(&mut self, from: usize, to: usize) -> Result<(), Error> {
        // The opening paragraph boundary is one document unit.
        let position = |offset: usize| {
            offset
                .checked_add(1)
                .map(DocPos)
                .ok_or_else(|| Error::InvalidEdit(format!("invalid paragraph offset {offset}")))
        };
        self.editor.select(Selection::Text {
            anchor: position(from)?,
            head: position(to)?,
        })
    }

    fn toggle_mark(&mut self, command: impl Command, from: usize, to: usize) -> Result<(), Error> {
        self.select(from, to)?;
        self.editor.run(command)?;
        Ok(())
    }
}

#[wasm_bindgen]
impl ParagraphExample {
    #[wasm_bindgen(constructor)]
    pub fn new(text: &str) -> Result<ParagraphExample, JsError> {
        let config = example_config()?;
        let editor = Editor::from_document(&config, vec![Node::text("paragraph", text)])?;
        Ok(Self { editor })
    }

    pub fn toggle_bold(&mut self, from: usize, to: usize) -> Result<(), JsError> {
        Ok(self.toggle_mark(gpdoc_plugin_bold::Toggle, from, to)?)
    }

    pub fn toggle_italic(&mut self, from: usize, to: usize) -> Result<(), JsError> {
        Ok(self.toggle_mark(gpdoc_plugin_italic::Toggle, from, to)?)
    }

    pub fn document_json(&self) -> Result<String, JsError> {
        Ok(self.editor.document().to_json()?)
    }
}

#[wasm_bindgen]
pub fn round_trip(source: &str) -> Result<String, JsError> {
    let config = example_config()?;
    let original = Editor::from_json(&config, source)?;
    let saved = original.document().to_json()?;
    let reopened = Editor::from_json(&config, &saved)?;
    if original.document() != reopened.document() {
        return Err(JsError::new("The round trip changed the document."));
    }
    Ok(saved)
}
