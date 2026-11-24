use crate::{Command, Shortcut, StoredCommand};

/// The keys for one command invocation, including its plugin-defined arguments.
#[derive(Clone, Debug)]
pub struct ShortcutBinding {
    pub(super) keys: ShortcutKeys,
    pub(super) command: StoredCommand,
}

impl ShortcutBinding {
    pub fn new(command: impl Command, keys: impl IntoIterator<Item = Shortcut>) -> Self {
        let mut bindings = ShortcutKeys::default();
        for key in keys {
            bindings.add(key);
        }
        Self {
            keys: bindings,
            command: StoredCommand::new(command),
        }
    }

    /// Edit the keys without changing the command or its arguments.
    pub fn keys(&mut self) -> &mut ShortcutKeys {
        &mut self.keys
    }
}

/// The editable shortcuts for one plugin action.
#[derive(Clone, Debug, Default)]
pub struct ShortcutKeys(pub(super) Vec<Shortcut>);

impl ShortcutKeys {
    /// Replace all of this action's shortcuts with one key.
    pub fn set(&mut self, key: Shortcut) {
        self.0.clear();
        self.0.push(key);
    }

    pub fn add(&mut self, key: Shortcut) {
        if !self.0.contains(&key) {
            self.0.push(key);
        }
    }

    pub fn remove(&mut self, key: Shortcut) {
        self.0.retain(|existing| existing != &key);
    }

    pub fn disable(&mut self) {
        self.0.clear();
    }
}

/// A plugin's typed shortcut actions, flattened when installed in an editor.
pub trait PluginShortcuts {
    fn into_bindings(self) -> impl IntoIterator<Item = ShortcutBinding>;
}

/// Configuration kept by a plugin until installation. Rule-only configs have no shortcuts.
pub trait PluginConfig: Sized {
    fn into_shortcuts(self) -> impl IntoIterator<Item = ShortcutBinding> {
        []
    }
}

impl<S: PluginShortcuts> PluginConfig for S {
    fn into_shortcuts(self) -> impl IntoIterator<Item = ShortcutBinding> {
        self.into_bindings()
    }
}
