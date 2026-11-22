use crate::{
    Attrs, CommandContext, CommandRegistration, DocPos, Document, Edit, Error, Node, Presentation,
    Schema, Selection, Shortcut, StoredCommand, Transaction,
};
use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    ops::Range,
    sync::Arc,
};

/// The post-insertion document, before input-rule steps are committed.
#[non_exhaustive]
pub struct InputRuleContext<'a> {
    pub document: &'a Document,
    pub schema: &'a Schema,
    pub selection: Selection,
    pub inserted: Range<DocPos>,
}
#[derive(Clone)]
pub(crate) struct InputRule {
    pub handler: RuleHandler,
    pub clear_mark: Option<String>,
}
type RuleHandler =
    Arc<dyn Fn(InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error> + Send + Sync>;

/// The initial document (before is absent), or the proposed result of an edit.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct DocumentChange<'a> {
    pub before: Option<&'a Document>,
    pub default_text_block: &'a str,
    /// The inserted text in the post-paste document, before appended steps.
    pub pasted: Option<&'a Range<DocPos>>,
    pub document: &'a Document,
    pub schema: &'a Schema,
}

pub(crate) type BreakHandler =
    Arc<dyn Fn(CommandContext<'_>) -> crate::plugin::CommandResult + Send + Sync>;
pub(crate) type PasteHandler =
    Arc<dyn Fn(CommandContext<'_>, &str) -> Result<Option<Transaction>, Error> + Send + Sync>;
pub(crate) type PasteRule =
    Arc<dyn Fn(InputRuleContext<'_>) -> Result<Vec<Edit>, Error> + Send + Sync>;
pub(crate) type AppendTransaction =
    Arc<dyn Fn(DocumentChange<'_>) -> Result<Vec<Edit>, Error> + Send + Sync>;
pub(crate) type FilterTransaction =
    Arc<dyn Fn(DocumentChange<'_>) -> Result<bool, Error> + Send + Sync>;

/// Named integration data shared by all sessions opened with a config.
#[derive(Clone)]
pub struct InstalledPlugin {
    name: String,
    data: std::collections::HashMap<std::any::TypeId, Arc<dyn Any + Send + Sync>>,
}

impl InstalledPlugin {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            data: Default::default(),
        }
    }

    pub fn register_metadata<T: Any + Send + Sync>(&mut self, value: T) {
        self.data
            .insert(std::any::TypeId::of::<T>(), Arc::new(value));
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn data<T: Any>(&self) -> Option<&T> {
        self.data.get(&std::any::TypeId::of::<T>())?.downcast_ref()
    }
}

pub(crate) type AtomLabel = Arc<dyn Fn(&Attrs) -> String + Send + Sync>;

/// Ordered runtime handlers and host data for an editor config. All are optional.
#[derive(Default)]
pub(crate) struct EditorHooks {
    pub installed: Vec<InstalledPlugin>,
    pub commands: BTreeMap<TypeId, CommandRegistration>,
    /// The first matching rule handles the insertion.
    pub rules: Vec<InputRule>,
    pub break_handlers: BTreeMap<String, BreakHandler>,
    pub paste_handlers: Vec<PasteHandler>,
    pub paste_rules: Vec<PasteRule>,
    /// Each handler sees the document produced by earlier handlers in the same pass.
    pub append: Vec<AppendTransaction>,
    pub filters: Vec<FilterTransaction>,
    pub shortcuts: BTreeMap<Shortcut, Vec<StoredCommand>>,
    pub presentation: Presentation,
    pub atom_labels: BTreeMap<String, AtomLabel>,
}

struct ConfigData {
    schema: Schema,
    default_text_block: String,
    hooks: EditorHooks,
}

/// The schema and editing behavior shared by editors with this configuration.
#[derive(Clone)]
pub struct EditorConfig(Arc<ConfigData>);
impl Default for EditorConfig {
    fn default() -> Self {
        let paragraph = crate::NodeSpec::block("paragraph")
            .group("block")
            .content("inline*");
        let schema = Schema::from_declarations(vec![paragraph], vec![], "block+")
            .expect("valid built-in paragraph schema");
        Self::new(schema, "paragraph").expect("valid built-in editor config")
    }
}

impl EditorConfig {
    /// Create an editor config directly from a schema, with no editing hooks.
    pub fn new(schema: Schema, default_text_block: impl Into<String>) -> Result<Self, Error> {
        Self::with_hooks(schema, default_text_block, EditorHooks::default())
    }

    /// Bind runtime handlers to the schema before opening any sessions.
    pub(crate) fn with_hooks(
        schema: Schema,
        default_text_block: impl Into<String>,
        hooks: EditorHooks,
    ) -> Result<Self, Error> {
        let default_text_block = default_text_block.into();
        if schema.top_node_type().inline_content()
            || schema
                .node_types()
                .any(|ty| ty.is_inline() && !ty.is_leaf())
        {
            return Err(Error::InvalidSchema(
                "the editor requires block root content and inline leaves".into(),
            ));
        }
        if !schema.node_type(&default_text_block).is_some_and(|ty| {
            ty.is_textblock() && ty.content_match.valid_end() && !ty.spec.has_required_attrs()
        }) {
            return Err(Error::InvalidSchema(format!(
                "default text block '{default_text_block}' must accept empty content and attributes"
            )));
        }
        for (key, bindings) in &hooks.shortcuts {
            for binding in bindings {
                let name = binding.name();
                if !hooks.commands.contains_key(&binding.command_type()) {
                    return Err(Error::InvalidSchema(format!(
                        "shortcut '{key}' references unknown command '{name}'"
                    )));
                }
            }
        }
        for kind in hooks.break_handlers.keys() {
            if !schema
                .node_type(kind)
                .is_some_and(|ty| ty.is_block() && !ty.is_leaf())
            {
                return Err(Error::InvalidSchema(format!(
                    "on_break requires a block with content: '{kind}'"
                )));
            }
        }
        for kind in hooks.atom_labels.keys() {
            if !schema
                .node_type(kind)
                .is_some_and(|node| !node.is_text() && node.is_atom())
            {
                return Err(Error::InvalidSchema(format!(
                    "atom label requires an atomic node '{kind}'"
                )));
            }
        }
        hooks.presentation.validate(&schema)?;
        Ok(Self(Arc::new(ConfigData {
            schema,
            default_text_block,
            hooks,
        })))
    }

    /// Whether both handles refer to the same compiled schema and editing behaviors.
    pub fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Named integrations in registration order.
    pub fn installed_plugins(&self) -> impl Iterator<Item = &InstalledPlugin> {
        self.0.hooks.installed.iter()
    }

    /// Installed data of one type, in registration order.
    pub fn plugin_data<T: Any>(&self) -> impl Iterator<Item = (&str, &T)> {
        self.installed_plugins()
            .filter_map(|plugin| Some((plugin.name(), plugin.data()?)))
    }

    /// Return a plugin-provided text alternative for an atomic node.
    pub fn atom_label(&self, node: &Node) -> Option<String> {
        self.0
            .hooks
            .atom_labels
            .get(&node.kind)
            .map(|label| label(&node.attrs))
            .filter(|label| !label.is_empty())
    }

    /// Node and mark formatting and syntax highlighting supplied by the plugins.
    pub fn presentation(&self) -> &Presentation {
        &self.0.hooks.presentation
    }

    pub fn default_text_block(&self) -> &str {
        &self.0.default_text_block
    }

    pub fn schema(&self) -> &Schema {
        &self.0.schema
    }
    /// Native key dispatch tries each matching binding until a command handles it.
    pub fn shortcuts(&self, key: &Shortcut) -> impl Iterator<Item = &StoredCommand> {
        self.0.hooks.shortcuts.get(key).into_iter().flatten()
    }
    pub(crate) fn break_handler(&self, kind: &str) -> Option<&BreakHandler> {
        self.0.hooks.break_handlers.get(kind)
    }
    pub(crate) fn paste_rules(&self) -> &[PasteRule] {
        &self.0.hooks.paste_rules
    }
    pub(crate) fn handle_paste(
        &self,
        ctx: CommandContext<'_>,
        text: &str,
    ) -> Result<Option<Transaction>, Error> {
        for handler in &self.0.hooks.paste_handlers {
            if let Some(tx) = handler(ctx, text)? {
                return Ok(Some(tx));
            }
        }
        Ok(None)
    }
    pub(crate) fn appenders(&self) -> &[AppendTransaction] {
        &self.0.hooks.append
    }
    pub(crate) fn accepts(&self, change: DocumentChange<'_>) -> Result<bool, Error> {
        for filter in &self.0.hooks.filters {
            if !filter(change)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn command(&self, id: TypeId, name: &str) -> Result<&CommandRegistration, Error> {
        self.0
            .hooks
            .commands
            .get(&id)
            .ok_or_else(|| Error::UnknownCommand(name.into()))
    }
    pub(crate) fn match_input(
        &self,
        doc: &Document,
        selection: Selection,
        inserted: Range<DocPos>,
    ) -> Result<Option<(Vec<Edit>, Option<&str>)>, Error> {
        for rule in &self.0.hooks.rules {
            if let Some(steps) = (rule.handler)(InputRuleContext {
                document: doc,
                schema: self.schema(),
                selection,
                inserted: inserted.clone(),
            })? {
                return Ok(Some((steps, rule.clear_mark.as_deref())));
            }
        }
        Ok(None)
    }
}
