use crate::model::Schema;
use crate::model::{Attrs, ContentRef, Document, Error, Fragment, Mark, Node};
use std::collections::{HashMap, HashSet};

impl Schema {
    pub fn document(&self, blocks: Vec<Node>) -> Result<Document, Error> {
        let blocks = self.bind_nodes(&blocks)?;
        let root = self.node(&self.top_node_type().name, Attrs::new(), blocks, vec![])?;
        self.with_root(None, root)
    }

    /// Attach a node and its descendants to this schema.
    pub fn bind_node(&self, node: &Node) -> Result<Node, Error> {
        self.bind(node, &mut HashMap::new())
    }

    pub fn bind_nodes(&self, nodes: &[Node]) -> Result<Fragment, Error> {
        let mut bound = HashMap::new();
        let nodes = nodes
            .iter()
            .map(|node| self.bind(node, &mut bound))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Fragment::from(nodes))
    }

    fn bind(
        &self,
        node: &Node,
        bound: &mut HashMap<*const crate::model::NodeData, Node>,
    ) -> Result<Node, Error> {
        if node.schema() == Some(self) {
            return Ok(node.clone());
        }
        let id = std::ptr::from_ref(&**node);
        if let Some(node) = bound.get(&id) {
            return Ok(node.clone());
        }
        if node.size() == usize::MAX {
            return Err(Error("document exceeds the position range".into()));
        }
        let ty = self
            .node_type(&node.kind)
            .ok_or_else(|| Error(format!("unknown node '{}'", node.kind)))?;
        let marks = node
            .marks()
            .iter()
            .map(|mark| self.bind_mark(mark))
            .collect::<Result<Vec<_>, _>>()?;
        let marks = Mark::set_from(self, marks);
        self.check_marks(&marks)?;
        let result = match node.content() {
            ContentRef::Text(run) if ty.is_text() && node.attrs.is_empty() => {
                self.text(run.text, marks)?
            }
            content if !ty.is_text() => {
                let children = match content {
                    ContentRef::Inline(children) if ty.inline_content() => children,
                    ContentRef::Blocks(children) if !ty.inline_content() && !ty.is_leaf() => {
                        children
                    }
                    ContentRef::Leaf(_) if ty.is_leaf() && (ty.is_inline() || marks.is_empty()) => {
                        &[]
                    }
                    _ => {
                        return Err(Error(format!("wrong content type for '{}'", node.kind)));
                    }
                };
                let attrs = self.node_attributes(&node.kind, &node.attrs)?;
                let children = children
                    .iter()
                    .map(|child| self.bind(child, bound))
                    .collect::<Result<Vec<_>, _>>()?;
                self.node(&node.kind, attrs, Fragment::from(children), marks)?
            }
            _ => {
                return Err(Error(format!("unexpected node '{}'", node.kind)));
            }
        };
        bound.insert(id, result.clone());
        Ok(result)
    }

    pub fn bind_mark(&self, mark: &Mark) -> Result<Mark, Error> {
        self.check_mark_attrs(&mark.kind, &mark.attrs)?;
        self.mark(&mark.kind, mark.attrs.clone())
    }

    pub fn node_attributes(&self, kind: &str, attrs: &Attrs) -> Result<Attrs, Error> {
        self.check_node_attrs(kind, attrs)?;
        Ok(attrs.clone())
    }

    pub fn check_marks(&self, marks: &[Mark]) -> Result<(), Error> {
        let mut canonical = Vec::new();
        for mark in marks {
            if mark.schema() != Some(self) {
                return Err(Error("mark belongs to another schema".into()));
            }
            self.check_mark_attrs(&mark.kind, &mark.attrs)?;
            canonical = mark.add_to_set(self, &canonical);
        }
        if canonical != marks {
            return Err(Error("unordered, duplicate, or excluded marks".into()));
        }
        Ok(())
    }

    pub fn with_root(&self, old: Option<&Document>, root: Node) -> Result<Document, Error> {
        if root.kind != self.top_node_type().name || !root.marks().is_empty() {
            return Err(Error("invalid document root".into()));
        }
        let previous = old.filter(|doc| doc.schema() == self).map(Document::root);
        self.check_tree(&root, previous)?;
        Ok(Document { root })
    }

    /// Shared subtrees of a previously validated document need no further checks.
    pub(super) fn check_tree(&self, root: &Node, previous: Option<&Node>) -> Result<(), Error> {
        let mut pending = vec![(root, previous)];
        let mut seen = HashSet::new();
        while let Some((node, previous)) = pending.pop() {
            if previous.is_some_and(|old| old.ptr_eq(node))
                || !seen.insert(std::ptr::from_ref(&**node))
            {
                continue;
            }
            if node.schema() != Some(self)
                || node.size() == usize::MAX
                || !self.valid_content(&node.kind, &node.content)
            {
                return Err(Error(format!("invalid content for '{}'", node.kind)));
            }
            self.check_marks(node.marks())?;
            self.check_node_attrs(&node.kind, &node.attrs)?;
            let before = previous.map_or(&[][..], |old| old.content.children());
            let after = node.content.children();
            let suffix = before
                .iter()
                .rev()
                .zip(after.iter().rev())
                .take_while(|(a, b)| a.ptr_eq(b))
                .count();
            pending.extend(
                after[..after.len() - suffix]
                    .iter()
                    .enumerate()
                    .map(|(index, child)| (child, before.get(index))),
            );
        }
        Ok(())
    }
}
