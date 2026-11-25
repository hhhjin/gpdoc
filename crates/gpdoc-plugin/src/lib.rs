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

pub use gpdoc_core::plugin::{
    AttributeTargets, CommandResult, Plugin, PluginConfig, PluginShortcuts, PluginSpec,
    ShortcutBinding, ShortcutKeys,
};
pub use gpdoc_core::{Command, EditorConfig, StoredCommand, key, unit_command};

/// Types commonly used when authoring a plugin.
pub mod prelude {
    pub use crate::{
        AttributeTargets, Command, CommandResult, Plugin, PluginConfig, PluginShortcuts,
        PluginSpec, ShortcutBinding, ShortcutKeys, StoredCommand, key, plugin_config,
        plugin_shortcuts, unit_command,
    };
    pub use gpdoc_core::{
        AllowedMarks, AttrSpec, AttrSpecs, AttrType, Attrs, CommandContext, CommandOutcome,
        Content, ContentRef, ContentSpec, DocPos, Document, Edit, Editor, Error, Fragment,
        InputRuleContext, Mark, MarkSpec, Node, NodeSpec, Placement, Schema, Selection, Slice,
        Step, TextFormat, TextRun, Transaction, Value,
    };
}

/// Declare a plugin's shortcut accessors together with the commands and their default keys.
#[macro_export]
macro_rules! plugin_shortcuts {
    ($vis:vis $name:ident { $($action:ident : $command:expr => [$($key:literal),*]),+ $(,)? }) => {
        #[derive(Clone)]
        $vis struct $name {
            $($action: $crate::ShortcutBinding),+
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $($action: $crate::ShortcutBinding::new($command, [$($crate::key!($key)),*])),+ }
            }
        }

        impl $name {
            $(pub fn $action(&mut self) -> &mut $crate::ShortcutKeys { self.$action.keys() })+
        }

        impl $crate::PluginShortcuts for $name {
            fn into_bindings(self) -> impl IntoIterator<Item = $crate::ShortcutBinding> {
                [$(self.$action),+]
            }
        }
    };
}

/// Declare the shortcuts and named rule accessors that an application can customize.
/// Use `impl ExistingConfig` to add groups to a manually defined shortcut configuration.
///
/// ```
/// use gpdoc_plugin::prelude::*;
///
/// plugin_config! {
///     pub Config {
///         input_rules Inputs { marker: input }
///         paste_rules Paste { marker: paste }
///         hooks Hooks { limit: filter }
///     }
/// }
///
/// let plugin = Plugin::<Config>::new("limited", |p| {
///     p.hooks().limit().set(|change| Ok(change.document.size() <= 100));
/// });
/// let _custom = plugin.extend(|p| p.hooks().limit().disable());
/// ```
///
/// ```compile_fail,E0308
/// use gpdoc_plugin::prelude::*;
/// plugin_config! { Config { hooks Hooks { limit: filter } } }
/// let _plugin = Plugin::<Config>::new("limited", |p| {
///     p.hooks().limit().set(|_| Ok(Vec::<Edit>::new()));
/// });
/// ```
#[macro_export]
macro_rules! plugin_config {
    (impl $config:ident { $($groups:tt)* }) => {
        $crate::plugin_config!(@groups pub $config { $($groups)* });
    };
    ($vis:vis $config:ident { shortcuts { $($keys:tt)* } $($groups:tt)* }) => {
        $crate::plugin_shortcuts! { $vis $config { $($keys)* } }
        $crate::plugin_config!(@groups $vis $config { $($groups)* });
    };
    ($vis:vis $config:ident { $($groups:tt)* }) => {
        #[derive(Clone, Default)]
        $vis struct $config;
        impl $crate::PluginConfig for $config {}
        $crate::plugin_config!(@groups $vis $config { $($groups)* });
    };
    (@groups $vis:vis $config:ident {
        $($category:ident $group:ident { $($action:ident : $kind:ident $(($mark:literal))?),+ $(,)? })*
    }) => {
        $(
            $vis struct $group<'a>($crate::core::plugin::RuleEditor<'a>);

            impl<'a> From<$crate::core::plugin::RuleEditor<'a>> for $group<'a> {
                fn from(editor: $crate::core::plugin::RuleEditor<'a>) -> Self { Self(editor) }
            }

            impl $crate::core::plugin::RuleGroup<$crate::plugin_config!(@category $category)> for $config {
                type Access<'a> = $group<'a>;
            }

            impl $group<'_> {
                $($crate::plugin_config!(@slot $category $action $kind $(($mark))?);)+
            }
        )*
    };
    (@category input_rules) => { $crate::core::plugin::InputRules };
    (@category paste_rules) => { $crate::core::plugin::PasteRules };
    (@category hooks) => { $crate::core::plugin::TransactionHooks };
    (@slot input_rules $action:ident input) => {
        pub fn $action(&mut self) -> $crate::core::plugin::InputRuleSlot<'_> {
            self.0.input(stringify!($action), None)
        }
    };
    (@slot input_rules $action:ident mark($mark:literal)) => {
        pub fn $action(&mut self) -> $crate::core::plugin::InputRuleSlot<'_> {
            self.0.input(stringify!($action), Some($mark))
        }
    };
    (@slot paste_rules $action:ident paste) => {
        $crate::plugin_config!(@access $action paste PasteRuleSlot);
    };
    (@slot paste_rules $action:ident handle_paste) => {
        $crate::plugin_config!(@access $action handle_paste PasteHandlerSlot);
    };
    (@slot hooks $action:ident filter) => {
        $crate::plugin_config!(@access $action filter FilterSlot);
    };
    (@slot hooks $action:ident append) => {
        $crate::plugin_config!(@access $action append AppendSlot);
    };
    (@access $action:ident $kind:ident $slot:ident) => {
        pub fn $action(&mut self) -> $crate::core::plugin::$slot<'_> {
            self.0.$kind(stringify!($action))
        }
    };
}
