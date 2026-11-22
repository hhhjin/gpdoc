use crate::model::{Error, Fragment, Node, Schema};
use crate::transform::{Mappable, mark_step::replace_node_start};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct AttrStep {
    pub pos: usize,
    pub attr: String,
    pub value: Value,
}

impl AttrStep {
    fn target<'a>(&self, doc: &'a Node) -> Result<&'a Node, Error> {
        doc.node_at(self.pos)?
            .ok_or_else(|| Error("No node at attribute step's position".into()))
    }
    pub fn apply(&self, schema: &Schema, doc: &Node) -> Result<Node, Error> {
        let node = self.target(doc)?;
        let mut attrs = node.attrs.clone();
        attrs.insert(self.attr.clone(), self.value.clone());
        let updated =
            schema.node_unchecked(&node.kind, attrs, Fragment::default(), node.marks.clone())?;
        replace_node_start(schema, doc, self.pos, updated)
    }
    pub fn invert(&self, doc: &Node) -> Result<Self, Error> {
        Ok(Self {
            value: self
                .target(doc)?
                .attrs
                .get(&self.attr)
                .cloned()
                .unwrap_or_default(),
            ..self.clone()
        })
    }
    pub fn map(&self, mapping: &impl Mappable) -> Option<Self> {
        let pos = mapping.map_result(self.pos, 1);
        (!pos.deleted_after()).then(|| Self {
            pos: pos.pos,
            ..self.clone()
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocAttrStep {
    pub attr: String,
    pub value: Value,
}

impl DocAttrStep {
    pub fn apply(&self, schema: &Schema, doc: &Node) -> Result<Node, Error> {
        let mut attrs = doc.attrs.clone();
        attrs.insert(self.attr.clone(), self.value.clone());
        schema.node_unchecked(&doc.kind, attrs, doc.content.clone(), doc.marks.clone())
    }
    pub fn invert(&self, doc: &Node) -> Self {
        Self {
            value: doc.attrs.get(&self.attr).cloned().unwrap_or_default(),
            ..self.clone()
        }
    }
}
