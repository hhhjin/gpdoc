use crate::model::node::BranchShape;
use crate::model::{Attrs, ContentMatch, Error, Fragment, Mark, Node};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

// A declared JSON null is a default; an absent `default` makes this attribute required.
fn present_default<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AttributeSpec {
    #[serde(default, deserialize_with = "present_default")]
    pub default: Option<Value>,
    pub validate: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NodeSpec {
    pub content: String,
    pub marks: Option<String>,
    pub group: String,
    pub inline: bool,
    pub atom: bool,
    pub attrs: BTreeMap<String, AttributeSpec>,
    pub selectable: Option<bool>,
    pub draggable: bool,
    pub code: bool,
    pub whitespace: Option<String>,
    pub defining_as_context: bool,
    pub defining_for_content: bool,
    pub defining: bool,
    pub isolating: bool,
    pub linebreak_replacement: bool,
}
impl NodeSpec {
    pub fn has_required_attrs(&self) -> bool {
        self.attrs
            .values()
            .any(|attr| attr.default.is_none() && !attr.allows_missing())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MarkSpec {
    pub attrs: BTreeMap<String, AttributeSpec>,
    pub inclusive: bool,
    pub excludes: Option<String>,
    pub group: String,
    pub spanning: bool,
    pub code: bool,
}
impl Default for MarkSpec {
    fn default() -> Self {
        Self {
            attrs: BTreeMap::new(),
            inclusive: true,
            excludes: None,
            group: String::new(),
            spanning: true,
            code: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SchemaSpec {
    /// Registration order determines which matching group member is generated first.
    pub nodes: Vec<(String, NodeSpec)>,
    /// Registration order determines the canonical order of a node's marks.
    pub marks: Vec<(String, MarkSpec)>,
    pub top_node: Option<String>,
}

#[derive(Debug)]
pub struct NodeType {
    pub name: String,
    pub spec: NodeSpec,
    pub content_match: ContentMatch,
    inline_content: bool,
    allowed_marks: Option<HashSet<String>>,
}
impl NodeType {
    fn shape(&self) -> BranchShape {
        let inline = self.is_inline();
        if self.is_leaf() {
            BranchShape::Leaf { inline }
        } else {
            BranchShape::Container {
                inline,
                inline_content: self.inline_content(),
            }
        }
    }

    pub fn is_inline(&self) -> bool {
        self.name == "text" || self.spec.inline
    }
    pub fn is_block(&self) -> bool {
        !self.is_inline()
    }
    pub fn is_text(&self) -> bool {
        self.name == "text"
    }
    pub fn inline_content(&self) -> bool {
        self.inline_content
    }
    pub fn is_textblock(&self) -> bool {
        self.is_block() && self.inline_content
    }
    pub fn is_leaf(&self) -> bool {
        self.spec.content.trim().is_empty()
    }
    pub fn is_atom(&self) -> bool {
        self.is_leaf() || self.spec.atom
    }
    pub fn in_group(&self, name: &str) -> bool {
        self.spec.group.split_whitespace().any(|g| g == name)
    }
    pub fn allows_mark(&self, kind: &str) -> bool {
        self.allowed_marks
            .as_ref()
            .is_none_or(|set| set.contains(kind))
    }
    pub fn allows_marks(&self, marks: &[Mark]) -> bool {
        marks.iter().all(|mark| {
            mark.schema()
                .is_some_and(|schema| self.content_match.belongs_to(schema))
                && self.allows_mark(&mark.kind)
        })
    }
    pub fn valid_content(&self, content: &Fragment) -> bool {
        self.content_match
            .match_fragment(content)
            .is_some_and(|state| state.valid_end())
            && content
                .children()
                .iter()
                .all(|n| self.allows_marks(n.marks()))
    }
}

#[derive(Debug)]
pub struct MarkType {
    pub name: String,
    pub rank: usize,
    pub spec: MarkSpec,
    excluded: HashSet<String>,
}
impl MarkType {
    pub fn excludes(&self, name: &str) -> bool {
        self.excluded.contains(name)
    }
}

impl AttributeSpec {
    fn allows_missing(&self) -> bool {
        self.validate
            .as_ref()
            .is_some_and(|types| types.split('|').any(|kind| kind == "undefined"))
    }
}

#[derive(Clone, Debug)]
pub struct Schema(Arc<Definitions>);
impl PartialEq for Schema {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Schema {}
#[derive(Debug)]
struct Definitions {
    id: Arc<()>,
    nodes: HashMap<String, NodeType>,
    marks: HashMap<String, MarkType>,
    top: String,
    linebreak: Option<String>,
}

impl Schema {
    pub fn node_types(&self) -> impl Iterator<Item = &NodeType> {
        self.0.nodes.values()
    }
    pub fn node_spec(&self, name: &str) -> Option<&NodeSpec> {
        self.node_type(name).map(|ty| &ty.spec)
    }
    pub fn mark_spec(&self, name: &str) -> Option<&MarkSpec> {
        self.mark_type(name).map(|ty| &ty.spec)
    }

    pub fn allows_children(&self, parent: &str, children: &[Node]) -> bool {
        self.node_type(parent)
            .and_then(|ty| {
                children
                    .iter()
                    .try_fold(ty.content_match.clone(), |state, child| {
                        state.match_type(&child.kind)
                    })
            })
            .is_some_and(|state| state.valid_end())
    }
    /// Deserialize a ProseMirror schema's declarative fields. `serde_json` preserves
    /// the member order of `nodes` and `marks` so generated content and mark ranks match.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        fn read_specs<T: serde::de::DeserializeOwned>(
            value: Option<&Value>,
            name: &str,
        ) -> Result<Vec<(String, T)>, Error> {
            let fields = value
                .and_then(Value::as_object)
                .ok_or_else(|| Error(format!("Expected a '{name}' object")))?;
            fields
                .iter()
                .map(|(key, value)| {
                    serde_json::from_value(value.clone())
                        .map(|spec| (key.clone(), spec))
                        .map_err(|e| Error(format!("Invalid {name} declaration '{key}': {e}")))
                })
                .collect()
        }
        let nodes = read_specs(value.get("nodes"), "nodes")?;
        let marks = if value.get("marks").is_some() {
            read_specs(value.get("marks"), "marks")?
        } else {
            Vec::new()
        };
        let top_node = value
            .get("topNode")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| Error("Expected topNode name".into()))
            })
            .transpose()?;
        Self::new(SchemaSpec {
            nodes,
            marks,
            top_node,
        })
    }
    pub fn new(spec: SchemaSpec) -> Result<Self, Error> {
        let top = spec
            .top_node
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "doc".into());
        if !spec.nodes.iter().any(|(name, _)| name == &top) {
            return Err(Error(format!(
                "Schema is missing its top node type '{top}'"
            )));
        }
        match spec.nodes.iter().find(|(name, _)| name == "text") {
            None => return Err(Error("Every schema needs a 'text' type".into())),
            Some((_, s)) if !s.attrs.is_empty() => {
                return Err(Error(
                    "The text node type should not have attributes".into(),
                ));
            }
            _ => {}
        }
        let mut nodes = HashMap::new();
        let id = Arc::new(());
        let mut matches = HashMap::<&str, ContentMatch>::new();
        let mut linebreak = None;
        for (name, s) in &spec.nodes {
            if nodes.contains_key(name) || spec.marks.iter().any(|(m, _)| m == name) {
                return Err(Error(format!("Duplicate schema type '{name}'")));
            }
            let content_match = if let Some(existing) = matches.get(s.content.as_str()) {
                existing.clone()
            } else {
                let matcher = ContentMatch::parse(&s.content, &spec.nodes, id.clone())?;
                matches.insert(&s.content, matcher.clone());
                matcher
            };
            let inline_content = content_match.edge(0).is_some_and(|(kind, _)| {
                kind == "text" || spec.nodes.iter().any(|(n, s)| n == kind && s.inline)
            });
            let allowed_marks = match s.marks.as_deref() {
                Some("_") => None,
                None if inline_content => None,
                Some(names) => Some(gather_marks(&spec.marks, names)?),
                None => Some(HashSet::new()),
            };
            let ty = NodeType {
                name: name.clone(),
                spec: s.clone(),
                content_match,
                inline_content,
                allowed_marks,
            };
            if s.linebreak_replacement {
                if linebreak.is_some() || !ty.is_inline() || !ty.is_leaf() {
                    return Err(Error(
                        "A schema may define one inline leaf as its linebreak replacement".into(),
                    ));
                }
                linebreak = Some(name.clone());
            }
            nodes.insert(name.clone(), ty);
        }
        let mut marks = HashMap::new();
        for (rank, (name, s)) in spec.marks.iter().enumerate() {
            let excluded = match s.excludes.as_deref() {
                None => HashSet::from([name.clone()]),
                Some(names) => gather_marks(&spec.marks, names)?,
            };
            let ty = MarkType {
                name: name.clone(),
                rank,
                spec: s.clone(),
                excluded,
            };
            if marks.insert(name.clone(), ty).is_some() {
                return Err(Error(format!("Duplicate mark type '{name}'")));
            }
        }
        Ok(Self(Arc::new(Definitions {
            id,
            nodes,
            marks,
            top,
            linebreak,
        })))
    }
    pub fn top_node_type(&self) -> &NodeType {
        &self.0.nodes[&self.0.top]
    }
    pub fn linebreak_replacement(&self) -> Option<&NodeType> {
        self.0.linebreak.as_ref().and_then(|n| self.node_type(n))
    }
    pub fn node_type(&self, name: &str) -> Option<&NodeType> {
        self.0.nodes.get(name)
    }
    pub fn mark_type(&self, name: &str) -> Option<&MarkType> {
        self.0.marks.get(name)
    }
    pub fn valid_content(&self, parent: &str, content: &Fragment) -> bool {
        self.node_type(parent)
            .is_some_and(|ty| ty.valid_content(content))
    }
    pub fn compatible_content(&self, a: &str, b: &str) -> bool {
        self.node_type(a)
            .zip(self.node_type(b))
            .is_some_and(|(ta, tb)| a == b || ta.content_match.compatible(&tb.content_match))
    }
    pub fn allows_marks(&self, parent: &str, marks: &[Mark]) -> bool {
        self.node_type(parent)
            .is_some_and(|ty| ty.allows_marks(marks))
    }
    pub fn excludes(&self, mark: &str, other: &str) -> bool {
        self.mark_type(mark).is_some_and(|ty| ty.excludes(other))
    }
    pub fn is_isolating(&self, name: &str) -> bool {
        self.node_type(name).is_some_and(|ty| ty.spec.isolating)
    }
    pub fn mark_rank(&self, name: &str) -> usize {
        self.mark_type(name).map_or(usize::MAX, |ty| ty.rank)
    }
    pub fn mark_inclusive(&self, name: &str) -> bool {
        self.mark_type(name).is_some_and(|ty| ty.spec.inclusive)
    }
    pub fn node(
        &self,
        name: &str,
        attrs: Attrs,
        content: Fragment,
        marks: Vec<Mark>,
    ) -> Result<Node, Error> {
        if !self.valid_content(name, &content) {
            return Err(Error(format!("Invalid content for node {name}")));
        }
        self.node_unchecked(name, attrs, content, marks)
    }
    pub fn node_unchecked(
        &self,
        name: &str,
        attrs: Attrs,
        content: Fragment,
        marks: Vec<Mark>,
    ) -> Result<Node, Error> {
        let ty = self.require_node_type(name)?;
        if ty.is_text() {
            return Err(Error("Use Schema::text to construct text nodes".into()));
        }
        self.check_ownership(&content, &marks)?;
        let attrs = compute_attrs(name, &ty.spec.attrs, &attrs)?;
        Ok(Node::branch(
            Some(self.clone()),
            name,
            attrs,
            content,
            Mark::set_from(self, marks),
            ty.shape(),
        ))
    }
    pub fn text(&self, text: &str, marks: Vec<Mark>) -> Result<Node, Error> {
        if marks.iter().any(|mark| mark.schema.as_ref() != Some(self)) {
            return Err(Error("Mark from a different schema".into()));
        }
        Node::checked_text(Some(self.clone()), text, Mark::set_from(self, marks))
    }
    pub fn mark(&self, name: &str, attrs: Attrs) -> Result<Mark, Error> {
        let ty = self.require_mark_type(name)?;
        Ok(Mark {
            schema: Some(self.clone()),
            kind: name.into(),
            attrs: compute_attrs(name, &ty.spec.attrs, &attrs)?,
        })
    }
    pub fn check_node_attrs(&self, name: &str, attrs: &Attrs) -> Result<(), Error> {
        check_attrs(name, &self.require_node_type(name)?.spec.attrs, attrs)
    }
    pub fn check_mark_attrs(&self, name: &str, attrs: &Attrs) -> Result<(), Error> {
        check_attrs(name, &self.require_mark_type(name)?.spec.attrs, attrs)
    }
    pub fn create_and_fill(
        &self,
        name: &str,
        attrs: Attrs,
        content: Fragment,
        marks: Vec<Mark>,
    ) -> Result<Option<Node>, Error> {
        let ty = self.require_node_type(name)?;
        self.check_ownership(&content, &marks)?;
        let attrs = compute_attrs(name, &ty.spec.attrs, &attrs)?;
        let mut generating = Vec::new();
        Ok(self.fill_node(ty, attrs, content, marks, &mut generating))
    }
    fn fill_node(
        &self,
        ty: &NodeType,
        attrs: Attrs,
        mut content: Fragment,
        marks: Vec<Mark>,
        generating: &mut Vec<String>,
    ) -> Option<Node> {
        if ty.is_text() {
            return None;
        }
        if content.size() > 0 {
            let before = ty
                .content_match
                .fill(self, &content, false, 0, generating)?;
            content = before.append(&content);
        }
        let matched = ty.content_match.match_fragment(&content)?;
        let after = matched.fill(self, &Fragment::default(), true, 0, generating)?;
        Some(Node::branch(
            Some(self.clone()),
            &ty.name,
            attrs,
            content.append(&after),
            Mark::set_from(self, marks),
            ty.shape(),
        ))
    }
    pub(super) fn generatable(&self, name: &str) -> bool {
        self.node_type(name)
            .is_some_and(|ty| !ty.is_text() && !ty.spec.has_required_attrs())
    }
    pub(super) fn has_id(&self, candidate: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.0.id, candidate)
    }
    fn check_ownership(&self, content: &Fragment, marks: &[Mark]) -> Result<(), Error> {
        if content
            .children()
            .iter()
            .any(|node| node.schema() != Some(self))
            || marks.iter().any(|mark| mark.schema.as_ref() != Some(self))
        {
            return Err(Error("Node or mark from a different schema".into()));
        }
        Ok(())
    }
    pub(super) fn default_child(&self, name: &str, generating: &mut Vec<String>) -> Option<Node> {
        // A recursive default (doc -> wrapper -> wrapper) has no finite filler.
        if generating.iter().any(|n| n == name) {
            return None;
        }
        let ty = self.node_type(name)?;
        let attrs = compute_attrs(name, &ty.spec.attrs, &Attrs::new()).ok()?;
        generating.push(name.to_owned());
        let result = self.fill_node(ty, attrs, Fragment::default(), vec![], generating);
        generating.pop();
        result
    }
    fn require_node_type(&self, name: &str) -> Result<&NodeType, Error> {
        self.node_type(name)
            .ok_or_else(|| Error(format!("Unknown node type '{name}'")))
    }
    fn require_mark_type(&self, name: &str) -> Result<&MarkType, Error> {
        self.mark_type(name)
            .ok_or_else(|| Error(format!("Unknown mark type '{name}'")))
    }
}

fn gather_marks(types: &[(String, MarkSpec)], names: &str) -> Result<HashSet<String>, Error> {
    let mut result = HashSet::new();
    for name in names.split_whitespace() {
        if types.iter().any(|(registered, _)| registered == name) {
            result.insert(name.into());
            continue;
        }
        let mut found = false;
        for (registered, spec) in types {
            if name == "_" || spec.group.split_whitespace().any(|g| g == name) {
                found = true;
                result.insert(registered.clone());
            }
        }
        if !found {
            return Err(Error(format!("Unknown mark type '{name}'")));
        }
    }
    Ok(result)
}
fn validate(
    owner: &str,
    name: &str,
    spec: &AttributeSpec,
    value: Option<&Value>,
) -> Result<(), Error> {
    if let Some(types) = spec.validate.as_deref() {
        let kind = match value {
            None => "undefined",
            Some(Value::Null) => "null",
            Some(Value::String(_)) => "string",
            Some(Value::Bool(_)) => "boolean",
            Some(Value::Number(_)) => "number",
            Some(Value::Array(_) | Value::Object(_)) => "object",
        };
        if !types
            .split('|')
            .any(|t| t == kind || (t == "integer" && value.is_some_and(Value::is_i64)))
        {
            return Err(Error(format!(
                "Expected {types} for attribute {name} of {owner}; got {kind}"
            )));
        }
    }
    Ok(())
}
fn compute_attrs(
    owner: &str,
    specs: &BTreeMap<String, AttributeSpec>,
    values: &Attrs,
) -> Result<Attrs, Error> {
    let mut attrs = Attrs::new();
    for (name, spec) in specs {
        if let Some(given) = values.get(name) {
            if let Err(error) = validate(owner, name, spec, Some(given)) {
                // Attribute step inverses use null for a previously missing value.
                if given.is_null() && spec.default.is_none() && spec.allows_missing() {
                    continue;
                }
                return Err(error);
            }
            attrs.insert(name.clone(), given.clone());
        } else if let Some(default) = &spec.default {
            attrs.insert(name.clone(), default.clone());
        } else if !spec.allows_missing() {
            return Err(Error(format!(
                "No value supplied for attribute {name} of {owner}"
            )));
        }
    }
    Ok(attrs)
}
fn check_attrs(
    owner: &str,
    specs: &BTreeMap<String, AttributeSpec>,
    values: &Attrs,
) -> Result<(), Error> {
    for name in values.keys() {
        if !specs.contains_key(name) {
            return Err(Error(format!("Unsupported attribute {name} for {owner}")));
        }
    }
    for (name, spec) in specs {
        let value = values.get(name).or(spec.default.as_ref());
        if value.is_none() && !spec.allows_missing() {
            return Err(Error(format!("Missing attribute {name} for {owner}")));
        }
        validate(owner, name, spec, value)?;
    }
    Ok(())
}
