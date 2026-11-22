//! Positions count UTF-16 code units. Text operations return an error if a
//! boundary would produce an unpaired surrogate, which Rust strings cannot store.
mod content;
pub mod declarations;
mod document;
mod fragment;
mod json;
mod node;
mod replace;
mod resolvedpos;
mod schema;
mod validation;

pub use content::ContentMatch;
pub use document::{DocPos, Document, PositionError};
pub use fragment::Fragment;
pub use json::JsonError;
pub use node::{Content, ContentRef, Mark, Node, NodeData, TextRun, TextRunRef};
pub use replace::Slice;
pub use resolvedpos::{NodeRange, ResolvedPos};
pub use schema::{AttributeSpec, MarkSpec, MarkType, NodeSpec, NodeType, Schema, SchemaSpec};

pub type Attrs = std::collections::BTreeMap<String, serde_json::Value>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
