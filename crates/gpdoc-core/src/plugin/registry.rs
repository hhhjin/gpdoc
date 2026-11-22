use super::{
    Plugin,
    declaration::{Declaration, Targets},
};
use crate::{
    AttrSpecs, EditorConfig, EditorHooks, Error, MarkSpec, NodeSpec, Presentation, Schema, Shortcut,
};
use std::collections::BTreeMap;

/// Collects plugin declarations during config construction.
#[derive(Default)]
struct Registry {
    current_plugin: String,
    owners: BTreeMap<&'static str, BTreeMap<String, String>>,
    global_attributes: Vec<(Targets, AttrSpecs)>,
    nodes: Vec<NodeSpec>,
    marks: Vec<MarkSpec>,
    default_block: Option<String>,
    document_content: Option<String>,
    hooks: EditorHooks,
}

impl Registry {
    fn reserve(&mut self, kind: &'static str, name: &str) -> Result<(), Error> {
        let namespace = self.owners.entry(kind).or_default();
        let error = if name.is_empty() {
            Error::InvalidSchema(format!(
                "empty {kind} name in plugin '{}'",
                self.current_plugin
            ))
        } else if let Some(first) = namespace.get(name) {
            Error::Duplicate {
                kind,
                name: name.to_owned(),
                first: first.clone(),
                second: self.current_plugin.clone(),
            }
        } else {
            namespace.insert(name.to_owned(), self.current_plugin.clone());
            return Ok(());
        };
        Err(error)
    }

    fn collect(&mut self, declaration: Declaration) -> Result<(), Error> {
        match declaration {
            Declaration::GlobalAttributes(types, attrs) => {
                self.global_attributes.push((types, attrs))
            }
            Declaration::Append(handler) => self.hooks.append.push(handler),
            Declaration::Filter(handler) => self.hooks.filters.push(handler),
            Declaration::Node(spec) => {
                self.reserve("node", &spec.name)?;
                self.nodes.push(spec);
            }
            Declaration::Mark(spec) => {
                self.reserve("mark", &spec.name)?;
                self.marks.push(spec);
            }
            Declaration::AtomLabel(kind, label) => {
                self.reserve("atom label", &kind)?;
                self.hooks.atom_labels.insert(kind, label);
            }
            Declaration::SyntaxHighlight(kind, highlight) => {
                self.reserve("syntax highlighter", &kind)?;
                self.hooks.presentation.highlighters.insert(kind, highlight);
            }
            Declaration::Break(kind, handler) => {
                self.reserve("break handler", &kind)?;
                self.hooks.break_handlers.insert(kind, handler);
            }
            Declaration::DefaultBlock(name) => {
                self.reserve("schema role", "default_text_block")?;
                self.default_block = Some(name);
            }
            Declaration::DocumentContent(content) => {
                self.reserve("schema role", "document_content")?;
                self.document_content = Some(content);
            }
        }
        Ok(())
    }
}

/// Collect document plugins before validating their declarations together.
#[derive(Default)]
#[must_use]
pub struct EditorConfigBuilder {
    plugins: Vec<Plugin>,
}

impl EditorConfig {
    pub fn builder() -> EditorConfigBuilder {
        EditorConfigBuilder::default()
    }
}

impl EditorConfigBuilder {
    pub fn plugin(mut self, plugin: Plugin) -> Self {
        self.plugins.push(plugin);
        self
    }

    pub fn plugins(mut self, plugins: impl IntoIterator<Item = Plugin>) -> Self {
        self.plugins.extend(plugins);
        self
    }

    pub fn build(self) -> Result<EditorConfig, Error> {
        self.build_with_presentation(|_, _| {})
    }

    /// Resolve host formatters against the complete document schema.
    pub fn build_with_presentation(
        self,
        configure: impl FnOnce(&Schema, &mut Presentation),
    ) -> Result<EditorConfig, Error> {
        let mut registry = Registry::default();
        let mut ordered = self.plugins;
        registry.current_plugin = "editor config".into();
        for plugin in &ordered {
            registry.reserve("plugin", plugin.name())?;
        }
        ordered.sort_by_key(|plugin| std::cmp::Reverse(plugin.priority));
        for plugin in &mut ordered {
            registry.hooks.installed.push(plugin.registration.clone());
            registry.current_plugin = plugin.registration.name().to_owned();
            for declaration in std::mem::take(&mut plugin.declarations) {
                registry.collect(declaration)?;
            }
            for command in std::mem::take(&mut plugin.commands) {
                registry.reserve("command", command.name())?;
                registry
                    .hooks
                    .commands
                    .insert(command.command_type(), command);
            }
        }
        // Break key and rule dispatch ties in favor of later registrations.
        ordered.reverse();
        ordered.sort_by_key(|plugin| std::cmp::Reverse(plugin.priority));
        for plugin in ordered {
            let mut keys = BTreeMap::new();
            for (key, command) in plugin.shortcuts {
                keys.insert(key.parse::<Shortcut>()?, command);
            }
            for (key, binding) in keys {
                registry
                    .hooks
                    .shortcuts
                    .entry(key)
                    .or_default()
                    .push(binding);
            }
            registry.hooks.rules.extend(plugin.input_rules);
            registry.hooks.paste_rules.extend(plugin.paste_rules);
            registry.hooks.paste_handlers.extend(plugin.paste_handlers);
        }
        let default = registry.default_block.ok_or_else(|| {
            Error::InvalidSchema("register a default text block before building the config".into())
        })?;
        for (targets, attributes) in registry.global_attributes {
            for node in &mut registry.nodes {
                let matches = match &targets {
                    Targets::Names(names) => names.contains(&node.name),
                    Targets::Nodes(predicate) => predicate(node),
                };
                if matches {
                    merge_attributes(&node.name, &mut node.attrs, &attributes)?;
                }
            }
            if let Targets::Names(names) = targets {
                for mark in registry
                    .marks
                    .iter_mut()
                    .filter(|mark| names.contains(&mark.name))
                {
                    merge_attributes(&mark.name, &mut mark.attrs, &attributes)?;
                }
            }
        }
        let schema = Schema::from_declarations(
            registry.nodes,
            registry.marks,
            registry.document_content.as_deref().unwrap_or("block+"),
        )
        .map_err(|err| Error::InvalidSchema(err.to_string()))?;
        configure(&schema, &mut registry.hooks.presentation);
        EditorConfig::with_hooks(schema, default, registry.hooks)
    }
}

fn merge_attributes(
    owner: &str,
    attrs: &mut AttrSpecs,
    additions: &AttrSpecs,
) -> Result<(), Error> {
    for (key, spec) in additions {
        if attrs
            .insert(key.clone(), spec.clone())
            .is_some_and(|previous| previous != *spec)
        {
            return Err(Error::InvalidSchema(format!(
                "conflicting definitions of attribute '{key}' on '{owner}'"
            )));
        }
    }
    Ok(())
}
