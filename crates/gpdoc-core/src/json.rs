pub use crate::model::JsonError;
use crate::{Editor, EditorConfig, Error};

#[derive(Debug, thiserror::Error)]
pub enum OpenJsonError {
    #[error(transparent)]
    Document(#[from] JsonError),
    #[error(transparent)]
    Editor(#[from] Error),
}

impl Editor {
    /// Open ProseMirror JSON using the plugins installed in this config.
    pub fn from_json(config: &EditorConfig, json: &str) -> Result<Editor, OpenJsonError> {
        let document = config.schema().document_from_json(json)?;
        Ok(Editor::open_document(config.clone(), document)?)
    }
}
