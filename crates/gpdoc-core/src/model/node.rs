use crate::model::{Attrs, ContentMatch, Error, Fragment, ResolvedPos, Schema, Slice};
use serde_json::{Value, json};
use std::{ops::Deref, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub(super) schema: Option<Schema>,
    pub kind: String,
    pub attrs: Attrs,
}

impl Mark {
    /// Build a mark before it is attached to an editor schema.
    pub fn new(kind: impl Into<String>, attrs: Attrs) -> Self {
        Self {
            schema: None,
            kind: kind.into(),
            attrs,
        }
    }

    pub fn schema(&self) -> Option<&Schema> {
        self.schema.as_ref()
    }

    pub fn with_attrs(mut self, attrs: Attrs) -> Self {
        self.attrs = attrs;
        self
    }

    /// Apply this mark in schema order and remove marks it excludes.
    pub fn add_to_set(&self, schema: &Schema, set: &[Mark]) -> Vec<Mark> {
        let mut result = Vec::with_capacity(set.len() + 1);
        let mut inserted = false;
        for other in set {
            if self == other {
                return set.to_vec();
            }
            if schema.excludes(&self.kind, &other.kind) {
                continue;
            }
            if schema.excludes(&other.kind, &self.kind) {
                return set.to_vec();
            }
            if !inserted && schema.mark_rank(&other.kind) > schema.mark_rank(&self.kind) {
                result.push(self.clone());
                inserted = true;
            }
            result.push(other.clone());
        }
        if !inserted {
            result.push(self.clone());
        }
        result
    }

    pub fn remove_from_set(&self, set: &[Mark]) -> Vec<Mark> {
        let mut result = set.to_vec();
        if let Some(index) = set.iter().position(|mark| mark == self) {
            result.remove(index);
        }
        result
    }

    pub fn set_from(schema: &Schema, mut marks: Vec<Mark>) -> Vec<Mark> {
        marks.sort_by_key(|mark| schema.mark_rank(&mark.kind));
        marks
    }

    pub fn to_json(&self) -> Value {
        let mut value = json!({"type": self.kind});
        if !self.attrs.is_empty() {
            value["attrs"] = Value::Object(self.attrs.clone().into_iter().collect());
        }
        value
    }
}

#[derive(Clone, Debug)]
pub struct Node(Arc<NodeData>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    pub marks: Vec<Mark>,
}

#[derive(Clone, Copy, Debug)]
pub struct TextRunRef<'a> {
    pub text: &'a str,
    pub marks: &'a [Mark],
}

/// Content supplied when building a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Inline(Vec<Node>),
    Text(TextRun),
    Leaf(Vec<Mark>),
    Blocks(Vec<Node>),
}

/// Borrowed content from a node.
#[derive(Clone, Copy, Debug)]
pub enum ContentRef<'a> {
    Inline(&'a [Node]),
    Text(TextRunRef<'a>),
    Leaf(&'a [Mark]),
    Blocks(&'a [Node]),
}

impl ContentRef<'_> {
    pub fn to_owned(self) -> Content {
        match self {
            Self::Inline(nodes) => Content::Inline(nodes.to_vec()),
            Self::Blocks(nodes) => Content::Blocks(nodes.to_vec()),
            Self::Leaf(marks) => Content::Leaf(marks.to_vec()),
            Self::Text(run) => Content::Text(TextRun {
                text: run.text.to_owned(),
                marks: run.marks.to_vec(),
            }),
        }
    }
}

impl From<TextRun> for Node {
    fn from(run: TextRun) -> Self {
        Self::new("text", Attrs::new(), Content::Text(run))
    }
}

#[derive(Clone, Debug)]
pub struct NodeData {
    schema: Option<Schema>,
    pub kind: String,
    pub attrs: Attrs,
    pub content: Fragment,
    pub marks: Vec<Mark>,
    pub text: Option<String>,
    size: usize,
    shape: BranchShape,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum BranchShape {
    Leaf { inline: bool },
    Container { inline: bool, inline_content: bool },
}

impl Deref for Node {
    type Target = NodeData;

    fn deref(&self) -> &NodeData {
        &self.0
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other)
            || (self.kind == other.kind
                && self.attrs == other.attrs
                && self.is_leaf() == other.is_leaf()
                && self.inline_content() == other.inline_content()
                && self.text == other.text
                && self.content == other.content
                && self.marks.len() == other.marks.len()
                && self
                    .marks
                    .iter()
                    .zip(&other.marks)
                    .all(|(a, b)| a.kind == b.kind && a.attrs == b.attrs))
    }
}
impl Eq for Node {}

impl Node {
    /// Build a node before it is checked against an editor's schema.
    pub fn new(kind: impl Into<String>, attrs: Attrs, content: Content) -> Self {
        match content {
            Content::Text(run) => Self::text_node(None, kind, attrs, run.text, run.marks),
            Content::Leaf(marks) => Self::branch(
                None,
                kind,
                attrs,
                Fragment::default(),
                marks,
                BranchShape::Leaf { inline: false },
            ),
            Content::Inline(children) => Self::branch(
                None,
                kind,
                attrs,
                Fragment::from_exact(children),
                vec![],
                BranchShape::Container {
                    inline: false,
                    inline_content: true,
                },
            ),
            Content::Blocks(children) => Self::branch(
                None,
                kind,
                attrs,
                Fragment::from_exact(children),
                vec![],
                BranchShape::Container {
                    inline: false,
                    inline_content: false,
                },
            ),
        }
    }

    pub fn text(kind: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let children = if text.is_empty() {
            vec![]
        } else {
            vec![
                TextRun {
                    text,
                    marks: vec![],
                }
                .into(),
            ]
        };
        Self::new(kind, Attrs::new(), Content::Inline(children))
    }

    pub fn container(kind: impl Into<String>, children: Vec<Node>) -> Self {
        Self::new(kind, Attrs::new(), Content::Blocks(children))
    }

    pub fn leaf(kind: impl Into<String>, attrs: Attrs) -> Self {
        Self::new(kind, attrs, Content::Leaf(vec![]))
    }

    pub fn content(&self) -> ContentRef<'_> {
        if let Some(text) = &self.text {
            ContentRef::Text(TextRunRef {
                text,
                marks: self.marks(),
            })
        } else if self.is_leaf() {
            ContentRef::Leaf(self.marks())
        } else if self.inline_content() {
            ContentRef::Inline(self.content.children())
        } else {
            ContentRef::Blocks(self.content.children())
        }
    }

    /// Text and the platform placeholder for a non-text leaf.
    pub fn projected_text(&self) -> &str {
        match self.text.as_deref() {
            Some(text) => text,
            None if self.is_leaf() => "\u{fffc}",
            None => "",
        }
    }

    pub fn inline_content(&self) -> bool {
        matches!(
            self.shape,
            BranchShape::Container {
                inline_content: true,
                ..
            }
        )
    }

    pub(super) fn branch(
        schema: Option<Schema>,
        kind: impl Into<String>,
        attrs: Attrs,
        content: Fragment,
        marks: Vec<Mark>,
        shape: BranchShape,
    ) -> Self {
        let size = if matches!(shape, BranchShape::Leaf { .. }) {
            1
        } else {
            content.size().saturating_add(2)
        };
        Self(Arc::new(NodeData {
            schema,
            kind: kind.into(),
            attrs,
            content,
            marks,
            text: None,
            size,
            shape,
        }))
    }

    pub(super) fn checked_text(
        schema: Option<Schema>,
        text: &str,
        marks: Vec<Mark>,
    ) -> Result<Self, Error> {
        if text.is_empty() {
            return Err(Error("empty text nodes are not allowed".into()));
        }
        Ok(Self::text_node(
            schema,
            "text",
            Attrs::new(),
            text.to_owned(),
            marks,
        ))
    }

    fn text_node(
        schema: Option<Schema>,
        kind: impl Into<String>,
        attrs: Attrs,
        text: String,
        marks: Vec<Mark>,
    ) -> Self {
        let size = text.encode_utf16().count();
        Self(Arc::new(NodeData {
            schema,
            kind: kind.into(),
            attrs,
            content: Fragment::default(),
            marks,
            text: Some(text),
            size,
            shape: BranchShape::Leaf { inline: true },
        }))
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn schema(&self) -> Option<&Schema> {
        self.schema.as_ref()
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }
    pub fn size(&self) -> usize {
        self.size
    }
    pub fn is_text(&self) -> bool {
        self.text.is_some()
    }
    pub fn is_leaf(&self) -> bool {
        matches!(self.shape, BranchShape::Leaf { .. })
    }
    pub fn is_inline(&self) -> bool {
        match self.shape {
            BranchShape::Leaf { inline } | BranchShape::Container { inline, .. } => inline,
        }
    }
    pub fn child_count(&self) -> usize {
        self.content.child_count()
    }
    pub fn child(&self, index: usize) -> &Node {
        self.content.child(index)
    }

    /// Visit descendants intersecting [from, to). Returning false skips a subtree.
    pub fn nodes_between(
        &self,
        from: usize,
        to: usize,
        mut visit: impl FnMut(&Node, usize, &Node, usize) -> bool,
    ) -> Result<(), Error> {
        if from > to || to > self.content.size() {
            return Err(Error("invalid traversal range".into()));
        }
        let mut stack = vec![(self, 0usize, 0usize)];
        while let Some((parent, index, offset)) = stack.pop() {
            let Some(node) = parent.content.maybe_child(index) else {
                continue;
            };
            if offset >= to {
                continue;
            }
            let end = offset.saturating_add(node.size());
            stack.push((parent, index + 1, end));
            if end > from && visit(node, offset, parent, index) && node.child_count() > 0 {
                stack.push((node, 0, offset + 1));
            }
        }
        Ok(())
    }

    pub fn text_content(&self) -> String {
        if let Some(text) = &self.text {
            return text.clone();
        }
        let mut result = String::new();
        self.nodes_between(0, self.content.size(), |child, _, _, _| {
            if let Some(text) = &child.text {
                result.push_str(text);
            }
            true
        })
        .expect("whole content is a valid range");
        result
    }

    pub fn text_between(
        &self,
        from: usize,
        to: usize,
        block_separator: &str,
        leaf_text: &str,
    ) -> Result<String, Error> {
        if let Some(text) = &self.text {
            return Ok(text_range(text, from, to)?.to_owned());
        }
        let mut result = String::new();
        let mut first = true;
        let mut error = None;
        self.nodes_between(from, to, |node, pos, _, _| {
            if let Some(text) = &node.text {
                match text_range(
                    text,
                    from.saturating_sub(pos),
                    to.saturating_sub(pos).min(node.size()),
                ) {
                    Ok(part) => result.push_str(part),
                    Err(e) => error = Some(e),
                }
            } else {
                let textblock = node.inline_content();
                if !node.is_inline() && (textblock || (node.is_leaf() && !leaf_text.is_empty())) {
                    if !first {
                        result.push_str(block_separator);
                    }
                    first = false;
                }
                if node.is_leaf() {
                    result.push_str(leaf_text);
                }
            }
            true
        })?;
        error.map_or(Ok(result), Err)
    }

    pub fn range_has_mark(&self, from: usize, to: usize, kind: &str) -> Result<bool, Error> {
        let mut found = false;
        if from < to {
            self.nodes_between(from, to, |node, _, _, _| {
                found |= node.marks.iter().any(|mark| mark.kind == kind);
                !found
            })?;
        }
        Ok(found)
    }

    pub fn same_markup(&self, other: &Node) -> bool {
        self.schema == other.schema
            && self.kind == other.kind
            && self.attrs == other.attrs
            && self.marks == other.marks
    }

    pub fn copy(&self, content: Fragment) -> Node {
        if self.is_text() || self.content.ptr_eq(&content) {
            return self.clone();
        }
        Self::branch(
            self.schema.clone(),
            &self.kind,
            self.attrs.clone(),
            content,
            self.marks.clone(),
            self.shape,
        )
    }

    pub fn mark(&self, marks: Vec<Mark>) -> Node {
        if marks == self.marks {
            return self.clone();
        }
        let mut data = (*self.0).clone();
        data.marks = marks;
        Node(Arc::new(data))
    }

    pub fn with_attrs(&self, attrs: Attrs) -> Node {
        if attrs == self.attrs {
            return self.clone();
        }
        let mut data = (*self.0).clone();
        data.attrs = attrs;
        Node(Arc::new(data))
    }

    pub fn with_text(&self, text: &str) -> Result<Node, Error> {
        if self.text.as_deref() == Some(text) {
            return Ok(self.clone());
        }
        if !self.is_text() {
            return Err(Error("expected a text node".into()));
        }
        Self::checked_text(self.schema.clone(), text, self.marks.clone())
    }

    pub fn cut(&self, from: usize, to: usize) -> Result<Node, Error> {
        if let Some(text) = &self.text {
            if from == 0 && to == self.size {
                return Ok(self.clone());
            }
            return self.with_text(text_range(text, from, to)?);
        }
        Ok(self.copy(self.content.cut(from, to)?))
    }

    pub fn resolve(&self, pos: usize) -> Result<ResolvedPos, Error> {
        ResolvedPos::resolve(self, pos)
    }

    pub fn node_at(&self, mut pos: usize) -> Result<Option<&Node>, Error> {
        let mut node = self;
        loop {
            let (index, offset) = node.content.find_index(pos)?;
            let Some(child) = node.content.maybe_child(index) else {
                return Ok(None);
            };
            if offset == pos || child.is_text() {
                return Ok(Some(child));
            }
            pos -= offset + 1;
            node = child;
        }
    }

    pub fn slice(&self, from: usize, to: usize, include_parents: bool) -> Result<Slice, Error> {
        if from > to || to > self.content.size() {
            return Err(Error("invalid slice range".into()));
        }
        if from == to {
            return Ok(Slice::empty());
        }
        let start = self.resolve(from)?;
        let end = self.resolve(to)?;
        let depth = if include_parents {
            0
        } else {
            start.shared_depth(to)
        };
        let offset = start.start(depth);
        let content = start.node(depth).content.cut(from - offset, to - offset)?;
        Ok(Slice::new(
            content,
            start.depth() - depth,
            end.depth() - depth,
        ))
    }

    pub fn replace(
        &self,
        schema: &Schema,
        from: usize,
        to: usize,
        slice: &Slice,
    ) -> Result<Node, Error> {
        if from > to {
            return Err(Error("reversed replacement range".into()));
        }
        if self.schema() != Some(schema)
            || slice
                .content
                .children()
                .iter()
                .any(|n| n.schema() != Some(schema))
        {
            return Err(Error("replacement belongs to another schema".into()));
        }
        crate::model::replace::replace(schema, &self.resolve(from)?, &self.resolve(to)?, slice)
    }

    pub fn content_match_at(&self, schema: &Schema, index: usize) -> Result<ContentMatch, Error> {
        if self.schema() != Some(schema) {
            return Err(Error("node belongs to another schema".into()));
        }
        schema
            .node_type(&self.kind)
            .and_then(|ty| {
                ty.content_match
                    .match_fragment_range(&self.content, 0, index)
            })
            .ok_or_else(|| Error(format!("invalid child prefix for '{}'", self.kind)))
    }

    pub fn can_replace(
        &self,
        schema: &Schema,
        from: usize,
        to: usize,
        replacement: &Fragment,
    ) -> bool {
        let Some(ty) = schema.node_type(&self.kind) else {
            return false;
        };
        from <= to
            && self
                .content_match_at(schema, from)
                .ok()
                .and_then(|m| m.match_fragment(replacement))
                .and_then(|m| m.match_fragment_range(&self.content, to, self.child_count()))
                .is_some_and(|end| end.valid_end())
            && replacement
                .children()
                .iter()
                .all(|node| node.schema() == Some(schema) && ty.allows_marks(node.marks()))
    }

    pub fn can_replace_with(
        &self,
        schema: &Schema,
        from: usize,
        to: usize,
        kind: &str,
        marks: &[Mark],
    ) -> bool {
        let Some(ty) = schema.node_type(&self.kind) else {
            return false;
        };
        from <= to
            && marks.iter().all(|mark| mark.schema() == Some(schema))
            && ty.allows_marks(marks)
            && self
                .content_match_at(schema, from)
                .ok()
                .and_then(|m| m.match_type(kind))
                .and_then(|m| m.match_fragment_range(&self.content, to, self.child_count()))
                .is_some_and(|end| end.valid_end())
    }

    pub fn can_append(&self, schema: &Schema, other: &Node) -> bool {
        if self.schema() != Some(schema) || other.schema() != Some(schema) {
            return false;
        }
        if other.child_count() == 0 {
            schema.compatible_content(&self.kind, &other.kind)
        } else {
            self.can_replace(
                schema,
                self.child_count(),
                self.child_count(),
                &other.content,
            )
        }
    }

    pub fn check(&self, schema: &Schema) -> Result<(), Error> {
        schema.check_tree(self, None)
    }

    pub fn to_json(&self) -> Value {
        let mut obj = json!({"type": self.kind});
        if !self.attrs.is_empty() {
            obj["attrs"] = Value::Object(self.attrs.clone().into_iter().collect());
        }
        if self.child_count() != 0 {
            obj["content"] = self.content.to_json();
        }
        if !self.marks.is_empty() {
            obj["marks"] = self.marks.iter().map(Mark::to_json).collect();
        }
        if let Some(text) = &self.text {
            obj["text"] = Value::String(text.clone());
        }
        obj
    }
}

/// Convert a UTF-16 range to a borrowed Rust string without changing boundary semantics.
pub(super) fn text_range(text: &str, from: usize, to: usize) -> Result<&str, Error> {
    let mut units = 0;
    let mut start_byte = (from == 0).then_some(0);
    if from == 0 && to == 0 {
        return Ok("");
    }
    for (byte, ch) in text.char_indices() {
        units += ch.len_utf16();
        let next_byte = byte + ch.len_utf8();
        if units == from {
            start_byte = Some(next_byte);
        }
        if units == to
            && let Some(start) = start_byte
        {
            return Ok(&text[start..next_byte]);
        }
        if units >= to {
            break;
        }
    }
    Err(Error(
        "range is outside text or splits a UTF-16 surrogate pair".into(),
    ))
}

fn json_attrs(
    value: &Value,
    spec: &std::collections::BTreeMap<String, crate::model::AttributeSpec>,
) -> Result<Attrs, Error> {
    match value.get("attrs") {
        // In the pinned source, null input uses the cached defaults when all
        // attributes have defaults. Otherwise computeAttrs reads every key as null.
        None | Some(Value::Null) if spec.values().any(|attr| attr.default.is_none()) => {
            Ok(spec.keys().map(|key| (key.clone(), Value::Null)).collect())
        }
        None | Some(Value::Null) => Ok(Attrs::new()),
        Some(Value::Object(attrs)) => {
            Ok(attrs.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        }
        _ => Err(Error("expected an attribute object".into())),
    }
}

fn json_type(value: &Value) -> Result<&str, Error> {
    value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| Error("missing type name in JSON".into()))
}

impl Schema {
    pub fn mark_from_json(&self, value: &Value) -> Result<Mark, Error> {
        let kind = json_type(value)?;
        let ty = self
            .mark_type(kind)
            .ok_or_else(|| Error(format!("unknown mark type '{kind}'")))?;
        self.mark(kind, json_attrs(value, &ty.spec.attrs)?)
    }

    /// Like Node.fromJSON, this constructs possibly-open nodes. Use Node::check
    /// at a document boundary to validate descendant content and mark sets.
    pub fn node_from_json(&self, value: &Value) -> Result<Node, Error> {
        let kind = json_type(value)?;
        let marks = match value.get("marks") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| self.mark_from_json(item))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(Error("invalid mark array".into())),
        };
        if kind == "text" {
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| Error("invalid text node in JSON".into()))?;
            self.text(text, marks)
        } else {
            let ty = self
                .node_type(kind)
                .ok_or_else(|| Error(format!("unknown node type '{kind}'")))?;
            self.node_unchecked(
                kind,
                json_attrs(value, &ty.spec.attrs)?,
                Fragment::from_json(self, &value["content"])?,
                marks,
            )
        }
    }
}
