//! Declare editing plugins and compile them into a headless editor config.
//!
//! Import the authoring types from [`prelude`]. The [`core`] module exposes
//! the engine's document model and editing operations. Import [`EditorConfig`]
//! from the crate root when building a headless editor.
pub use gpdoc_core as core;

pub mod atom;
pub mod marks;
pub mod paste;
pub mod selection;
pub mod textblock;

pub use gpdoc_core::plugin::{AttributeTargets, CommandResult, Plugin, PluginSpec};
pub use gpdoc_core::{Command, EditorConfig, StoredCommand, unit_command};

/// Types commonly used when authoring a plugin.
pub mod prelude {
    pub use crate::{
        AttributeTargets, Command, CommandResult, Plugin, PluginSpec, StoredCommand, unit_command,
    };
    pub use gpdoc_core::{
        AllowedMarks, AttrSpec, AttrSpecs, AttrType, Attrs, CommandContext, CommandOutcome,
        Content, ContentRef, ContentSpec, DocPos, Document, Edit, Editor, Error, Fragment,
        InputRuleContext, Mark, MarkSpec, Node, NodeSpec, Placement, Schema, Selection, Slice,
        Step, TextFormat, TextRun, Transaction, Value,
    };
}
