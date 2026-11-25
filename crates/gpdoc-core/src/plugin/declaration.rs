//! Collect schema declarations and editing behavior for one installable feature.
use super::{
    CommandResult, InputRules, PasteRules, RuleEditor, RuleGroup, TransactionHooks, rules::RuleList,
};
use crate::{
    AppendTransaction, AttrSpecs, Attrs, Command, CommandContext, CommandRegistration,
    DocumentChange, DragHandleScope, Edit, Error, FilterTransaction, Highlighter, InputRule,
    InputRuleContext, InstalledPlugin, MarkSpec, NodeSpec, PasteHandler, PasteRule,
    SelectionBehavior, StoredCommand, SyntaxToken,
};
use std::{any::Any, sync::Arc};

/// Node and mark names, or a predicate evaluated against every installed node.
#[derive(Clone)]
pub struct AttributeTargets(Targets);

#[derive(Clone)]
pub(super) enum Targets {
    Names(Vec<String>),
    Nodes(Arc<dyn Fn(&NodeSpec) -> bool + Send + Sync>),
}

impl AttributeTargets {
    /// Select installed nodes and marks by name.
    pub fn names(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self(Targets::Names(names.into_iter().map(Into::into).collect()))
    }

    /// Select installed nodes that satisfy the predicate.
    pub fn nodes(predicate: impl Fn(&NodeSpec) -> bool + Send + Sync + 'static) -> Self {
        Self(Targets::Nodes(Arc::new(predicate)))
    }
}

#[derive(Clone)]
pub(super) enum Declaration {
    GlobalAttributes(Targets, AttrSpecs),
    SyntaxHighlight(String, Highlighter),
    AtomLabel(String, crate::AtomLabel),
    DragHandle(String, DragHandleScope),
    Node(NodeSpec),
    Mark(MarkSpec),
    Break(String, crate::BreakHandler),
    DefaultBlock(String),
    DocumentContent(String),
}

/// An installable feature. A single feature may own any number of schema types and commands.
#[derive(Clone)]
#[must_use]
pub struct Plugin<S = ()> {
    pub(super) declarations: PluginDeclarations,
    config: S,
}

/// The configuration available both when declaring and when extending a plugin.
pub struct PluginSpec<'a, S = ()> {
    plugin: &'a mut PluginDeclarations,
    config: &'a mut S,
}

#[derive(Clone)]
pub(super) struct PluginDeclarations {
    pub(super) registration: InstalledPlugin,
    pub(super) priority: i32,
    pub(super) declarations: Vec<Declaration>,
    pub(super) commands: Vec<CommandRegistration>,
    pub(super) shortcuts: Vec<(crate::Shortcut, StoredCommand)>,
    pub(super) input_rules: RuleList<InputRule>,
    pub(super) paste_rules: RuleList<PasteRule>,
    pub(super) paste_handlers: RuleList<PasteHandler>,
    pub(super) filters: RuleList<FilterTransaction>,
    pub(super) append: RuleList<AppendTransaction>,
}

impl<S: Default> Plugin<S> {
    /// Start with the configuration defaults of the plugin's return type.
    pub fn new(name: impl Into<String>, declare: impl FnOnce(&mut PluginSpec<'_, S>)) -> Self {
        Self {
            declarations: PluginDeclarations {
                registration: InstalledPlugin::new(name),
                priority: 100,
                declarations: vec![],
                commands: vec![],
                shortcuts: vec![],
                input_rules: RuleList::default(),
                paste_rules: RuleList::default(),
                paste_handlers: RuleList::default(),
                filters: RuleList::default(),
                append: RuleList::default(),
            },
            config: S::default(),
        }
        .extend(declare)
    }
}

impl<S> Plugin<S> {
    /// Customize this plugin before installation. Extensions compose in call order.
    pub fn extend(mut self, configure: impl FnOnce(&mut PluginSpec<'_, S>)) -> Self {
        configure(&mut PluginSpec {
            plugin: &mut self.declarations,
            config: &mut self.config,
        });
        self
    }

    pub fn name(&self) -> &str {
        self.declarations.registration.name()
    }

    /// Read host-specific metadata before compiling the editor config.
    pub fn metadata<T: Any>(&self) -> Option<&T> {
        self.declarations.registration.data()
    }

    /// Attach a typed host integration to this plugin.
    pub fn with_metadata<T: Any + Send + Sync>(mut self, value: T) -> Self {
        self.declarations.registration.register_metadata(value);
        self
    }

    /// Set priority when composing a config. Higher numbers run first; the default is 100.
    /// Equal-priority shortcut and input-rule ties favor later plugins.
    pub fn priority(mut self, priority: i32) -> Self {
        self.declarations.priority = priority;
        self
    }
}

impl<S> PluginSpec<'_, S> {
    /// Edit the typing rules exposed by this plugin.
    pub fn input_rules(&mut self) -> <S as RuleGroup<InputRules>>::Access<'_>
    where
        S: RuleGroup<InputRules>,
    {
        RuleEditor(self.plugin).into()
    }

    /// Edit the clipboard handlers and paste rules exposed by this plugin.
    pub fn paste_rules(&mut self) -> <S as RuleGroup<PasteRules>>::Access<'_>
    where
        S: RuleGroup<PasteRules>,
    {
        RuleEditor(self.plugin).into()
    }

    /// Edit the transaction filters and append handlers exposed by this plugin.
    pub fn hooks(&mut self) -> <S as RuleGroup<TransactionHooks>>::Access<'_>
    where
        S: RuleGroup<TransactionHooks>,
    {
        RuleEditor(self.plugin).into()
    }

    pub fn node(&mut self, spec: NodeSpec) {
        self.plugin.declarations.push(Declaration::Node(spec));
    }

    pub fn mark(&mut self, spec: MarkSpec) {
        self.plugin.declarations.push(Declaration::Mark(spec));
    }

    /// Extend matching types after every plugin has registered its nodes and marks.
    pub fn attributes(&mut self, targets: AttributeTargets, attrs: AttrSpecs) {
        self.plugin
            .declarations
            .push(Declaration::GlobalAttributes(targets.0, attrs));
    }

    pub fn default_text_block(&mut self, name: impl Into<String>) {
        self.plugin
            .declarations
            .push(Declaration::DefaultBlock(name.into()));
    }

    pub fn document_content(&mut self, expression: impl Into<String>) {
        self.plugin
            .declarations
            .push(Declaration::DocumentContent(expression.into()));
    }

    /// Register a selection kind under this plugin's name.
    pub fn selection(&mut self, kind: &'static dyn SelectionBehavior) {
        self.metadata(kind);
    }

    /// Expose a typed declaration to other integrations installed in this config.
    pub fn metadata<T: Any + Send + Sync>(&mut self, value: T) {
        self.plugin.registration.register_metadata(value);
    }

    pub fn command<C: Command>(
        &mut self,
        run: impl Fn(CommandContext<'_>, C) -> CommandResult + Send + Sync + 'static,
    ) {
        self.plugin.commands.push(CommandRegistration::new(run));
    }

    pub fn shortcut(&mut self, key: crate::Shortcut, command: impl Command) {
        self.shortcut_stored(key, StoredCommand::new(command));
    }

    pub fn shortcut_stored(&mut self, key: crate::Shortcut, command: StoredCommand) {
        self.plugin.shortcuts.push((key, command));
    }

    /// Handle a paragraph break in this block. Return None to use the next handler.
    pub fn on_break(
        &mut self,
        kind: impl Into<String>,
        handler: impl Fn(CommandContext<'_>) -> CommandResult + Send + Sync + 'static,
    ) {
        self.plugin
            .declarations
            .push(Declaration::Break(kind.into(), Arc::new(handler)));
    }

    /// Transform newly typed text. Return `Ok(None)` to try the next rule.
    pub fn input_rule(
        &mut self,
        handler: impl Fn(InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error>
        + Send
        + Sync
        + 'static,
    ) {
        self.plugin.input_rules.push(InputRule {
            handler: Arc::new(handler),
            clear_mark: None,
        });
    }

    /// Transform newly typed text and clear the given mark for subsequent typing on a match.
    pub fn mark_input_rule(
        &mut self,
        mark: impl Into<String>,
        handler: impl Fn(InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error>
        + Send
        + Sync
        + 'static,
    ) {
        self.plugin.input_rules.push(InputRule {
            handler: Arc::new(handler),
            clear_mark: Some(mark.into()),
        });
    }

    /// Handle pasted text before the editor replaces the selection.
    pub fn handle_paste(
        &mut self,
        handler: impl Fn(CommandContext<'_>, &str) -> CommandResult + Send + Sync + 'static,
    ) {
        self.plugin.paste_handlers.push(Arc::new(handler));
    }

    /// Apply edits to the newly pasted range after insertion.
    pub fn paste_rule(
        &mut self,
        handler: impl Fn(InputRuleContext<'_>) -> Result<Vec<Edit>, Error> + Send + Sync + 'static,
    ) {
        self.plugin.paste_rules.push(Arc::new(handler));
    }

    /// Append schema-checked edits to the triggering edit's undo entry.
    pub fn append_edits(
        &mut self,
        handler: impl Fn(DocumentChange<'_>) -> Result<Vec<Edit>, Error> + Send + Sync + 'static,
    ) {
        self.plugin.append.push(Arc::new(handler));
    }

    pub fn filter_transaction(
        &mut self,
        handler: impl Fn(DocumentChange<'_>) -> Result<bool, Error> + Send + Sync + 'static,
    ) {
        self.plugin.filters.push(Arc::new(handler));
    }

    /// Give this block a drag handle. Dragging any of its named direct children
    /// moves the owner. Other children keep their own drag behavior.
    pub fn drag_handle(
        &mut self,
        owner: impl Into<String>,
        children: impl IntoIterator<Item = impl Into<String>>,
    ) {
        self.plugin.declarations.push(Declaration::DragHandle(
            owner.into(),
            DragHandleScope::Children(children.into_iter().map(Into::into).collect()),
        ));
    }

    /// Use the owner's drag handle everywhere inside this block, at any nesting depth.
    pub fn drag_handle_for_subtree(&mut self, owner: impl Into<String>) {
        self.plugin.declarations.push(Declaration::DragHandle(
            owner.into(),
            DragHandleScope::Subtree,
        ));
    }

    /// Supply an accessible text representation for a document atom.
    pub fn atom_label(
        &mut self,
        kind: impl Into<String>,
        label: impl Fn(&Attrs) -> String + Send + Sync + 'static,
    ) {
        self.plugin
            .declarations
            .push(Declaration::AtomLabel(kind.into(), Arc::new(label)));
    }

    pub fn syntax_highlighter(
        &mut self,
        kind: impl Into<String>,
        highlight: impl Fn(&Attrs, &str) -> Vec<SyntaxToken> + Send + Sync + 'static,
    ) {
        self.plugin.declarations.push(Declaration::SyntaxHighlight(
            kind.into(),
            Arc::new(highlight),
        ));
    }
}

impl<S: super::PluginShortcuts> PluginSpec<'_, S> {
    /// The plugin-specific actions exposed for rebinding.
    pub fn shortcuts(&mut self) -> &mut S {
        self.config
    }
}

impl<S: super::PluginConfig> From<Plugin<S>> for Plugin {
    fn from(plugin: Plugin<S>) -> Self {
        let mut declarations = plugin.declarations;
        for action in plugin.config.into_shortcuts() {
            for key in action.keys.0 {
                declarations.shortcuts.push((key, action.command.clone()));
            }
        }
        Plugin {
            declarations,
            config: (),
        }
    }
}
