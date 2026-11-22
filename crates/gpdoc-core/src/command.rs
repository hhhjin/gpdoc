//! Typed editing commands and their plugin handlers.
use crate::{CommandContext, Error, Transaction};
use std::{
    any::{Any, TypeId, type_name},
    fmt,
    sync::Arc,
};

/// A plugin-defined editing operation and its arguments.
pub trait Command: Clone + Send + Sync + 'static {
    /// Name used in configuration errors and diagnostics.
    const NAME: &'static str;
}

/// Declare a command that takes no arguments.
#[macro_export]
macro_rules! unit_command {
    ($(#[$meta:meta])* $vis:vis $name:ident, $id:literal $(,)?) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis struct $name;

        impl $crate::Command for $name {
            const NAME: &'static str = $id;
        }
    };
}

/// An owned command for heterogeneous collections such as shortcuts and menus.
#[derive(Clone)]
pub struct StoredCommand {
    name: &'static str,
    value: Arc<dyn Any + Send + Sync>,
}

impl StoredCommand {
    pub fn new<C: Command>(command: C) -> Self {
        Self {
            name: C::NAME,
            value: Arc::new(command),
        }
    }

    pub(crate) fn command_type(&self) -> TypeId {
        self.value.as_ref().type_id()
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl fmt::Debug for StoredCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredCommand")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

type Handler =
    dyn Fn(CommandContext<'_>, &dyn Any) -> Result<Option<Transaction>, Error> + Send + Sync;

/// One plugin's handler for a concrete command type.
#[derive(Clone)]
pub(crate) struct CommandRegistration {
    name: &'static str,
    command_type: TypeId,
    handler: Arc<Handler>,
}

impl CommandRegistration {
    pub fn new<C: Command>(
        run: impl Fn(CommandContext<'_>, C) -> Result<Option<Transaction>, Error>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            name: C::NAME,
            command_type: TypeId::of::<C>(),
            handler: Arc::new(move |ctx, value| {
                let command = value.downcast_ref::<C>().cloned().ok_or_else(|| {
                    Error::InvalidCommandArguments {
                        command: C::NAME.into(),
                        reason: format!("expected command type {}", type_name::<C>()),
                    }
                })?;
                run(ctx, command)
            }),
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn command_type(&self) -> TypeId {
        self.command_type
    }

    pub(crate) fn run_stored(
        &self,
        ctx: CommandContext<'_>,
        command: &StoredCommand,
    ) -> Result<Option<Transaction>, Error> {
        self.run_typed(ctx, command.value.as_ref())
    }

    pub(crate) fn run_typed(
        &self,
        ctx: CommandContext<'_>,
        value: &dyn Any,
    ) -> Result<Option<Transaction>, Error> {
        (self.handler)(ctx, value)
    }
}
