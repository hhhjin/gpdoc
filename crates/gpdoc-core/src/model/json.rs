use crate::model::{Attrs, ContentRef, Document, Fragment, Mark, Node, Schema};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum JsonError {
    #[error("invalid document JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Document(#[from] crate::model::Error),
    #[error("{0}")]
    Format(String),
    #[error("gpdoc JSON supports at most {0} nested block levels")]
    NestingLimit(usize),
}

impl Document {
    /// Export this document as a ProseMirror JSON string.
    pub fn to_json(&self) -> Result<String, JsonError> {
        Ok(serde_json::to_string(&encode_node(self.root(), 0)?)?)
    }
}

impl Schema {
    /// Parse a complete ProseMirror document using this schema.
    pub fn document_from_json(&self, json: &str) -> Result<Document, JsonError> {
        let root = decode_node(self, serde_json::from_str(json)?, 0)?;
        Ok(self.with_root(None, root)?)
    }
}

const MAX_LEVELS: usize = 40;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonNode {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, skip_serializing_if = "Attrs::is_empty")]
    attrs: Attrs,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    content: Vec<JsonNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    marks: Vec<JsonMark>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonMark {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, skip_serializing_if = "Attrs::is_empty")]
    attrs: Attrs,
}

impl JsonMark {
    fn encode(mark: &Mark) -> Self {
        Self {
            kind: mark.kind.clone(),
            attrs: mark.attrs.clone(),
        }
    }
}
fn encode_node(node: &Node, block_level: usize) -> Result<JsonNode, JsonError> {
    if block_level > MAX_LEVELS {
        return Err(JsonError::NestingLimit(MAX_LEVELS));
    }
    let mut encoded = JsonNode {
        kind: node.kind.clone(),
        attrs: node.attrs.clone(),
        marks: node.marks().iter().map(JsonMark::encode).collect(),
        ..Default::default()
    };
    match node.content() {
        ContentRef::Text(run) => encoded.text = Some(run.text.to_owned()),
        ContentRef::Leaf(_) => {}
        ContentRef::Blocks(children) | ContentRef::Inline(children) => {
            encoded.content = children
                .iter()
                .map(|child| {
                    encode_node(
                        child,
                        block_level + usize::from(!child.is_inline() || !child.is_leaf()),
                    )
                })
                .collect::<Result<_, _>>()?;
        }
    }
    Ok(encoded)
}

fn decode_node(schema: &Schema, node: JsonNode, level: usize) -> Result<Node, JsonError> {
    if level > MAX_LEVELS {
        return Err(JsonError::NestingLimit(MAX_LEVELS));
    }
    let ty = schema
        .node_type(&node.kind)
        .ok_or_else(|| crate::model::Error(format!("unknown node '{}'", node.kind)))?;
    let marks = node
        .marks
        .into_iter()
        .map(|m| {
            schema.check_mark_attrs(&m.kind, &m.attrs)?;
            schema.mark(&m.kind, m.attrs)
        })
        .collect::<Result<Vec<_>, crate::model::Error>>()?;
    schema.check_node_attrs(&node.kind, &node.attrs)?;
    if ty.is_text() {
        if !node.content.is_empty() {
            return Err(JsonError::Format("text cannot have children".into()));
        }
        let text = node
            .text
            .ok_or_else(|| JsonError::Format("expected text".into()))?;
        return Ok(schema.text(&text, marks)?);
    }
    if node.text.is_some() {
        return Err(JsonError::Format("only text nodes may carry text".into()));
    }
    let children = node
        .content
        .into_iter()
        .map(|child| {
            let child_type = schema
                .node_type(&child.kind)
                .ok_or_else(|| crate::model::Error(format!("unknown node '{}'", child.kind)))?;
            let nested = !child_type.is_inline() || !child_type.is_leaf();
            decode_node(schema, child, level + usize::from(nested))
        })
        .collect::<Result<Vec<_>, JsonError>>()?;
    Ok(schema.node(&node.kind, node.attrs, Fragment::from(children), marks)?)
}
