//! Named declarations for assembling a document schema from plugins.
use crate::model::{Error, Schema};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrType {
    Json,
    NullableString,
    String,
    Integer,
    Bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttrSpec {
    pub kind: AttrType,
    pub required: bool,
    pub default: Option<Value>,
}

impl AttrSpec {
    pub(super) fn accepts(&self, value: &Value) -> bool {
        match self.kind {
            AttrType::String => value.is_string(),
            AttrType::NullableString => value.is_string() || value.is_null(),
            AttrType::Integer => value.is_i64(),
            AttrType::Bool => value.is_boolean(),
            AttrType::Json => true,
        }
    }

    pub fn required(kind: AttrType) -> Self {
        Self {
            kind,
            required: true,
            default: None,
        }
    }

    pub fn optional(kind: AttrType) -> Self {
        Self {
            kind,
            required: false,
            default: None,
        }
    }

    pub fn with_default(kind: AttrType, value: Value) -> Self {
        Self {
            kind,
            required: false,
            default: Some(value),
        }
    }
}
pub type AttrSpecs = BTreeMap<String, AttrSpec>;

#[derive(Clone, Debug)]
pub enum AllowedMarks {
    All,
    Only(Vec<String>),
}

#[derive(Clone, Debug)]
pub enum ContentSpec {
    /// Inline leaf that represents a newline in plain text.
    LineBreak,
    Leaf,
    Expression {
        content: String,
        marks: Option<AllowedMarks>,
    },
}

impl ContentSpec {
    /// A schema content expression. Mark permissions follow the parent type's defaults.
    pub fn expression(content: impl Into<String>) -> Self {
        Self::Expression {
            content: content.into(),
            marks: None,
        }
    }

    pub fn with_marks(content: impl Into<String>, marks: AllowedMarks) -> Self {
        Self::Expression {
            content: content.into(),
            marks: Some(marks),
        }
    }

    pub fn inline(marks: AllowedMarks) -> Self {
        Self::with_marks("inline*", marks)
    }

    pub fn text(marks: AllowedMarks) -> Self {
        Self::with_marks("text*", marks)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Block,
    Inline,
}

#[derive(Clone, Debug)]
pub struct NodeSpec {
    pub placement: Placement,
    pub name: String,
    pub groups: Vec<String>,
    pub attrs: AttrSpecs,
    pub content: ContentSpec,
}
impl NodeSpec {
    /// An ungrouped leaf. Use `.group("block")` for a top-level block.
    pub fn block(name: impl Into<String>) -> Self {
        Self::new(name, Placement::Block)
    }

    pub fn inline(name: impl Into<String>) -> Self {
        Self::new(name, Placement::Inline)
    }

    pub fn new(name: impl Into<String>, placement: Placement) -> Self {
        Self {
            name: name.into(),
            placement,
            groups: vec![],
            attrs: AttrSpecs::new(),
            content: ContentSpec::Leaf,
        }
    }

    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.groups.push(group.into());
        self
    }

    pub fn content(mut self, content: impl Into<ContentSpec>) -> Self {
        self.content = content.into();
        self
    }

    pub fn attrs(mut self, attrs: AttrSpecs) -> Self {
        self.attrs = attrs;
        self
    }
}

impl From<&str> for ContentSpec {
    fn from(expression: &str) -> Self {
        Self::expression(expression)
    }
}

#[derive(Clone, Debug)]
pub struct MarkSpec {
    pub name: String,
    pub groups: Vec<String>,
    /// Mark names and groups this mark excludes. An underscore excludes all marks.
    /// None excludes its own type. An empty string permits distinct marks of the same type.
    pub excludes: Option<String>,
    /// Continue the mark when typing at its end, or before it at the very start of a text block.
    pub inclusive_end: bool,
    pub attrs: AttrSpecs,
}

impl MarkSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            groups: vec![],
            excludes: None,
            inclusive_end: true,
            attrs: AttrSpecs::new(),
        }
    }

    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.groups.push(group.into());
        self
    }

    pub fn attrs(mut self, attrs: AttrSpecs) -> Self {
        self.attrs = attrs;
        self
    }

    pub fn excludes(mut self, expression: impl Into<String>) -> Self {
        self.excludes = Some(expression.into());
        self
    }

    pub fn inclusive_end(mut self, inclusive: bool) -> Self {
        self.inclusive_end = inclusive;
        self
    }
}

impl Schema {
    /// Compile named node and mark declarations with built-in document and text types.
    pub fn from_declarations(
        nodes: Vec<NodeSpec>,
        marks: Vec<MarkSpec>,
        document_content: &str,
    ) -> Result<Self, Error> {
        for (owner, attrs) in nodes
            .iter()
            .map(|spec| (&spec.name, &spec.attrs))
            .chain(marks.iter().map(|spec| (&spec.name, &spec.attrs)))
        {
            for (key, spec) in attrs {
                if let Some(default) = &spec.default
                    && (spec.required || !spec.accepts(default))
                {
                    return Err(Error(format!(
                        "invalid default for attribute '{key}' on '{owner}'"
                    )));
                }
            }
        }
        let mut node_types = vec![
            (
                "doc".to_string(),
                crate::model::NodeSpec {
                    content: document_content.into(),
                    ..Default::default()
                },
            ),
            (
                "text".to_string(),
                crate::model::NodeSpec {
                    inline: true,
                    group: "inline".into(),
                    ..Default::default()
                },
            ),
        ];
        for spec in &nodes {
            if matches!(spec.name.as_str(), "doc" | "text") {
                return Err(Error(format!("reserved node type '{}'", spec.name)));
            }
            let inline = spec.placement == Placement::Inline;
            if (inline && spec.groups.iter().any(|g| g == "block"))
                || (!inline && spec.groups.iter().any(|g| g == "inline"))
            {
                return Err(Error(format!("inconsistent placement of '{}'", spec.name)));
            }
            let mut groups = spec.groups.clone();
            if inline && !groups.iter().any(|g| g == "inline") {
                groups.push("inline".into());
            }
            let (content, marks) = match &spec.content {
                ContentSpec::Leaf | ContentSpec::LineBreak => (String::new(), None),
                ContentSpec::Expression { content, marks } => {
                    let marks = marks.as_ref().map(|allowed| match allowed {
                        AllowedMarks::All => "_".to_string(),
                        AllowedMarks::Only(names) => names.join(" "),
                    });
                    (content.clone(), marks)
                }
            };
            node_types.push((
                spec.name.clone(),
                crate::model::NodeSpec {
                    content,
                    marks,
                    inline,
                    linebreak_replacement: matches!(spec.content, ContentSpec::LineBreak),
                    group: groups.join(" "),
                    attrs: attribute_specs(&spec.attrs),
                    ..Default::default()
                },
            ));
        }
        let mark_types = marks
            .iter()
            .map(|spec| {
                (
                    spec.name.clone(),
                    crate::model::MarkSpec {
                        attrs: attribute_specs(&spec.attrs),
                        inclusive: spec.inclusive_end,
                        group: spec.groups.join(" "),
                        excludes: spec.excludes.clone(),
                        ..Default::default()
                    },
                )
            })
            .collect();
        crate::model::Schema::new(crate::model::SchemaSpec {
            nodes: node_types,
            marks: mark_types,
            top_node: None,
        })
    }
}

fn attribute_specs(specs: &AttrSpecs) -> BTreeMap<String, crate::model::AttributeSpec> {
    specs
        .iter()
        .map(|(name, spec)| {
            let mut validate = match spec.kind {
                AttrType::String => "string",
                AttrType::NullableString => "string|null",
                AttrType::Bool => "boolean",
                AttrType::Json => "string|number|boolean|object|null",
                AttrType::Integer => "integer",
            }
            .to_string();
            if !spec.required {
                validate.push_str("|undefined");
            }
            (
                name.clone(),
                crate::model::AttributeSpec {
                    default: spec.default.clone(),
                    validate: Some(validate),
                },
            )
        })
        .collect()
}
