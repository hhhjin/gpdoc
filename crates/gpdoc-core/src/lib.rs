//! Headless rich-text documents, editing transactions and editor sessions.
mod command;
mod config;
mod editor;
mod json;
mod presentation;
mod shortcut;

pub mod model;
pub mod plugin;
pub use plugin::EditorConfigBuilder;
pub mod transform;

pub(crate) use command::CommandRegistration;
pub use command::{Command, StoredCommand};
pub use config::*;
pub use editor::*;
pub use json::{JsonError, OpenJsonError};
pub use model::declarations::*;
pub use model::{
    Attrs, Content, ContentRef, DocPos, Document, Fragment, Mark, Node, ResolvedPos, Schema, Slice,
    TextRun, TextRunRef,
};
pub use presentation::*;
pub use serde_json::Value;
pub use shortcut::Shortcut;
#[doc(hidden)]
pub use shortcut::ShortcutParts;
pub use transform::{
    Affinity, Boundary, BoundaryEdit, Edit, MappedPosition, PositionMap, ReplaceAroundStep,
    ReplaceStep, Step,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidSchema(String),
    InvalidDocument(String),
    InvalidPosition(DocPos),
    InvalidEdit(String),
    Duplicate {
        kind: &'static str,
        name: String,
        first: String,
        second: String,
    },
    ForeignTransaction,
    StaleRevision,
    NoHistory,
    UnknownCommand(String),
    InvalidCommandArguments {
        command: String,
        reason: String,
    },
}

impl From<model::Error> for Error {
    fn from(error: model::Error) -> Self {
        Self::InvalidDocument(error.0)
    }
}

impl From<model::PositionError> for Error {
    fn from(error: model::PositionError) -> Self {
        match error {
            model::PositionError::InvalidPosition(pos) => Self::InvalidPosition(pos),
            model::PositionError::InvalidOffset { .. } => Self::InvalidEdit(error.to_string()),
        }
    }
}

impl From<transform::TransformError> for Error {
    fn from(error: transform::TransformError) -> Self {
        match error {
            transform::TransformError::InvalidDocument(reason) => Self::InvalidDocument(reason),
            transform::TransformError::InvalidEdit(reason) => Self::InvalidEdit(reason),
            transform::TransformError::InvalidPosition(pos) => Self::InvalidPosition(pos),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSchema(msg) | Self::InvalidDocument(msg) | Self::InvalidEdit(msg) => {
                f.write_str(msg)
            }
            Self::InvalidPosition(pos) => write!(f, "invalid document position {}", pos.0),
            Self::Duplicate {
                kind,
                name,
                first,
                second,
            } => write!(
                f,
                "{kind} '{name}' registered by both '{first}' and '{second}'"
            ),
            Self::ForeignTransaction => f.write_str("transaction belongs to another editor"),
            Self::StaleRevision => f.write_str("transaction is based on an earlier editor state"),
            Self::NoHistory => f.write_str("no history entry available"),
            Self::InvalidCommandArguments { command, reason } => {
                write!(f, "invalid arguments for {command}: {reason}")
            }
            Self::UnknownCommand(name) => write!(f, "unknown command '{name}'"),
        }
    }
}
impl std::error::Error for Error {}
