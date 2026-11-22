//! Transaction snapshots share immutable plugin values until a command replaces one.
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    sync::Arc,
};

trait StateValue: Any + Debug + Send + Sync {
    fn equals(&self, other: &dyn StateValue) -> bool;
}

impl<T: Any + Debug + PartialEq + Send + Sync> StateValue for T {
    fn equals(&self, other: &dyn StateValue) -> bool {
        (other as &dyn Any).downcast_ref() == Some(self)
    }
}

impl PartialEq for dyn StateValue {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct PluginStates {
    values: HashMap<TypeId, Arc<dyn StateValue>>,
}

impl PluginStates {
    pub(super) fn get<S: 'static>(&self) -> Option<&S> {
        let value = self.values.get(&TypeId::of::<S>())?;
        (value.as_ref() as &dyn Any).downcast_ref()
    }

    pub(super) fn set<S: Debug + PartialEq + Send + Sync + 'static>(&mut self, value: S) {
        self.values.insert(TypeId::of::<S>(), Arc::new(value));
    }
}
