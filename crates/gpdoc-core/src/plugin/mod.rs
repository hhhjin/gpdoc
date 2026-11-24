//! Document plugin declarations and configuration.
mod bindings;
mod rules;
use declaration::PluginDeclarations;
pub use rules::{
    AppendSlot, FilterSlot, InputRuleSlot, InputRules, PasteHandlerSlot, PasteRuleSlot, PasteRules,
    RuleEditor, RuleGroup, TransactionHooks,
};
mod declaration;
pub use bindings::{PluginConfig, PluginShortcuts, ShortcutBinding, ShortcutKeys};
mod registry;

pub use declaration::{AttributeTargets, Plugin, PluginSpec};
pub use registry::EditorConfigBuilder;

/// A planned transaction, or `Ok(None)` when the command does not apply.
pub type CommandResult = Result<Option<crate::Transaction>, crate::Error>;
