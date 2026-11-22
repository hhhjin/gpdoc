//! Document plugin declarations and configuration.
mod declaration;
mod registry;

pub use declaration::{AttributeTargets, Plugin, PluginSpec};
pub use registry::EditorConfigBuilder;

/// A planned transaction, or `Ok(None)` when the command does not apply.
pub type CommandResult = Result<Option<crate::Transaction>, crate::Error>;
