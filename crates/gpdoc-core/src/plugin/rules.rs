//! Named editing rules retain their first registration position across extensions.
use super::{CommandResult, PluginDeclarations};
use crate::{
    AppendTransaction, CommandContext, DocumentChange, Edit, Error, FilterTransaction, InputRule,
    InputRuleContext, PasteHandler, PasteRule,
};
use std::sync::Arc;

#[derive(Clone)]
enum Entry<T> {
    Internal(T),
    Named {
        name: &'static str,
        handler: Option<T>,
    },
}

#[derive(Clone)]
pub(super) struct RuleList<T>(Vec<Entry<T>>);

impl<T> Default for RuleList<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> RuleList<T> {
    pub(super) fn push(&mut self, handler: T) {
        self.0.push(Entry::Internal(handler));
    }

    fn named(&mut self, name: &'static str) -> NamedRule<'_, T> {
        NamedRule { list: self, name }
    }

    pub(super) fn active(self) -> impl Iterator<Item = T> {
        self.0.into_iter().filter_map(|entry| match entry {
            Entry::Internal(handler) => Some(handler),
            Entry::Named { handler, .. } => handler,
        })
    }
}

struct NamedRule<'a, T> {
    list: &'a mut RuleList<T>,
    name: &'static str,
}

impl<T> NamedRule<'_, T> {
    fn update(&mut self, replacement: Option<T>) {
        for entry in &mut self.list.0 {
            if let Entry::Named { name, handler } = entry
                && *name == self.name
            {
                *handler = replacement;
                return;
            }
        }
        if replacement.is_some() {
            self.list.0.push(Entry::Named {
                name: self.name,
                handler: replacement,
            });
        }
    }
}

/// A typed group of rule accessors supplied by a plugin.
pub trait RuleGroup<Category> {
    type Access<'a>: From<RuleEditor<'a>>;
}

pub enum InputRules {}
pub enum PasteRules {}
pub enum TransactionHooks {}

// Used by plugin_config! to project a plugin's named accessor onto its declaration list.
#[doc(hidden)]
pub struct RuleEditor<'a>(pub(super) &'a mut PluginDeclarations);

impl RuleEditor<'_> {
    pub fn input(&mut self, name: &'static str, mark: Option<&'static str>) -> InputRuleSlot<'_> {
        InputRuleSlot {
            handler: self.0.input_rules.named(name),
            mark,
        }
    }
    pub fn paste(&mut self, name: &'static str) -> PasteRuleSlot<'_> {
        PasteRuleSlot(self.0.paste_rules.named(name))
    }
    pub fn handle_paste(&mut self, name: &'static str) -> PasteHandlerSlot<'_> {
        PasteHandlerSlot(self.0.paste_handlers.named(name))
    }
    pub fn filter(&mut self, name: &'static str) -> FilterSlot<'_> {
        FilterSlot(self.0.filters.named(name))
    }
    pub fn append(&mut self, name: &'static str) -> AppendSlot<'_> {
        AppendSlot(self.0.append.named(name))
    }
}

pub struct InputRuleSlot<'a> {
    handler: NamedRule<'a, InputRule>,
    mark: Option<&'static str>,
}

impl InputRuleSlot<'_> {
    pub fn set(
        &mut self,
        handler: impl Fn(InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error>
        + Send
        + Sync
        + 'static,
    ) {
        self.handler.update(Some(InputRule {
            handler: Arc::new(handler),
            clear_mark: self.mark.map(str::to_owned),
        }));
    }
    pub fn disable(&mut self) {
        self.handler.update(None);
    }
}

macro_rules! handler_slot {
    ($name:ident, $handler:ty, ($($arg:ty),*) -> $result:ty) => {
        pub struct $name<'a>(NamedRule<'a, $handler>);
        impl $name<'_> {
            pub fn set(&mut self, handler: impl Fn($($arg),*) -> $result + Send + Sync + 'static) {
                self.0.update(Some(Arc::new(handler)));
            }
            pub fn disable(&mut self) {
                self.0.update(None);
            }
        }
    };
}

handler_slot!(PasteRuleSlot, PasteRule, (InputRuleContext<'_>) -> Result<Vec<Edit>, Error>);
handler_slot!(PasteHandlerSlot, PasteHandler, (CommandContext<'_>, &str) -> CommandResult);
handler_slot!(FilterSlot, FilterTransaction, (DocumentChange<'_>) -> Result<bool, Error>);
handler_slot!(AppendSlot, AppendTransaction, (DocumentChange<'_>) -> Result<Vec<Edit>, Error>);
