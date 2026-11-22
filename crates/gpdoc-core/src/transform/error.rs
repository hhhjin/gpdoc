use crate::model::{DocPos, PositionError};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransformError {
    InvalidDocument(String),
    InvalidEdit(String),
    InvalidPosition(DocPos),
}

impl From<crate::model::Error> for TransformError {
    fn from(error: crate::model::Error) -> Self {
        Self::InvalidDocument(error.0)
    }
}

impl From<PositionError> for TransformError {
    fn from(error: PositionError) -> Self {
        match error {
            PositionError::InvalidPosition(pos) => Self::InvalidPosition(pos),
            PositionError::InvalidOffset { .. } => Self::InvalidEdit(error.to_string()),
        }
    }
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDocument(message) | Self::InvalidEdit(message) => f.write_str(message),
            Self::InvalidPosition(pos) => write!(f, "invalid document position {}", pos.0),
        }
    }
}

impl std::error::Error for TransformError {}
