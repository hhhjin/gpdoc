use crate::model::{Error, Node, Schema};
use serde_json::Value;
use std::sync::Arc;

/// A persistent sequence of children with its total size in document positions.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Fragment {
    children: Arc<[Node]>,
    size: usize,
}

impl Fragment {
    pub(super) fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.children, &other.children)
    }

    pub(super) fn from_exact(children: Vec<Node>) -> Self {
        let size = children
            .iter()
            .map(Node::size)
            .fold(0, usize::saturating_add);
        Self {
            children: children.into(),
            size,
        }
    }

    pub fn children(&self) -> &[Node] {
        &self.children
    }
    pub fn size(&self) -> usize {
        self.size
    }
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
    pub fn child(&self, index: usize) -> &Node {
        &self.children[index]
    }
    pub fn maybe_child(&self, index: usize) -> Option<&Node> {
        self.children.get(index)
    }

    pub fn find_index(&self, pos: usize) -> Result<(usize, usize), Error> {
        if pos == self.size {
            return Ok((self.child_count(), pos));
        }
        if pos > self.size {
            return Err(Error(format!(
                "position {pos} outside fragment of size {}",
                self.size
            )));
        }
        let mut offset = 0usize;
        for (i, node) in self.children.iter().enumerate() {
            let end = offset.saturating_add(node.size());
            if pos < end {
                return Ok((i, offset));
            }
            offset = end;
        }
        unreachable!("a valid interior position is contained in some child")
    }

    pub fn cut(&self, from: usize, to: usize) -> Result<Self, Error> {
        if from > to || to > self.size {
            return Err(Error("invalid fragment cut range".into()));
        }
        if from == 0 && to == self.size {
            return Ok(self.clone());
        }
        if from == to {
            return Ok(Self::default());
        }
        let (index, mut pos) = self.find_index(from)?;
        let mut result = Vec::new();
        for child in &self.children[index..] {
            if pos >= to {
                break;
            }
            let end = pos.saturating_add(child.size());
            let cut = if pos < from || end > to {
                let boundary = usize::from(!child.is_text());
                let width = if child.is_text() {
                    child.size()
                } else {
                    child.content.size()
                };
                let start = pos + boundary;
                child.cut(
                    from.saturating_sub(start),
                    to.saturating_sub(start).min(width),
                )?
            } else {
                child.clone()
            };
            result.push(cut);
            pos = end;
        }
        Ok(Self::from_exact(result))
    }

    pub fn append(&self, other: &Self) -> Self {
        if self.children.is_empty() {
            return other.clone();
        }
        if other.children.is_empty() {
            return self.clone();
        }
        let mut result = self.children.to_vec();
        let tail = other.children();
        let last = result.last_mut().expect("non-empty fragment");
        if last.is_text() && last.same_markup(&tail[0]) {
            let mut text = last.text.as_ref().unwrap().clone();
            text.push_str(tail[0].text.as_deref().unwrap());
            *last = last.with_text(&text).expect("joined text is non-empty");
            result.extend_from_slice(&tail[1..]);
        } else {
            result.extend_from_slice(tail);
        }
        Self::from_exact(result)
    }

    pub fn replace_child(&self, index: usize, node: Node) -> Self {
        if self.child(index).ptr_eq(&node) {
            return self.clone();
        }
        let mut result = self.children.to_vec();
        result[index] = node;
        Self::from_exact(result)
    }

    pub fn cut_by_index(&self, from: usize, to: usize) -> Self {
        if from == 0 && to == self.child_count() {
            return self.clone();
        }
        Self::from_exact(self.children[from..to].to_vec())
    }

    pub fn find_diff_start(&self, other: &Fragment) -> Option<usize> {
        let mut pos = 0;
        for (a, b) in self.children.iter().zip(other.children.iter()) {
            if !a.ptr_eq(b) {
                if !a.same_markup(b) {
                    return Some(pos);
                }
                if let (Some(left), Some(right)) = (&a.text, &b.text)
                    && left != right
                {
                    let common: usize = left
                        .chars()
                        .zip(right.chars())
                        .take_while(|(x, y)| x == y)
                        .map(|(ch, _)| ch.len_utf16())
                        .sum();
                    return Some(pos + common);
                }
                if let Some(inner) = a.content.find_diff_start(&b.content) {
                    return Some(pos + 1 + inner);
                }
            }
            pos += a.size();
        }
        (self.child_count() != other.child_count()).then_some(pos)
    }

    pub fn find_diff_end(&self, other: &Fragment) -> Option<(usize, usize)> {
        let (mut a_end, mut b_end) = (self.size(), other.size());
        for (a, b) in self.children.iter().rev().zip(other.children.iter().rev()) {
            if !a.ptr_eq(b) {
                if !a.same_markup(b) {
                    return Some((a_end, b_end));
                }
                if let (Some(left), Some(right)) = (&a.text, &b.text)
                    && left != right
                {
                    let common: usize = left
                        .chars()
                        .rev()
                        .zip(right.chars().rev())
                        .take_while(|(x, y)| x == y)
                        .map(|(ch, _)| ch.len_utf16())
                        .sum();
                    return Some((a_end - common, b_end - common));
                }
                if let Some((left, right)) = a.content.find_diff_end(&b.content) {
                    return Some((
                        a_end - 1 - a.content.size() + left,
                        b_end - 1 - b.content.size() + right,
                    ));
                }
            }
            a_end -= a.size();
            b_end -= b.size();
        }
        (self.child_count() != other.child_count()).then_some((a_end, b_end))
    }

    pub fn from_json(schema: &Schema, value: &Value) -> Result<Self, Error> {
        match value {
            Value::Null => Ok(Self::default()),
            Value::Array(items) => Ok(items
                .iter()
                .map(|v| schema.node_from_json(v))
                .collect::<Result<Vec<_>, _>>()?
                .into()),
            _ => Err(Error("invalid JSON for fragment".into())),
        }
    }
    pub fn to_json(&self) -> Value {
        if self.child_count() == 0 {
            return Value::Null;
        }
        self.children.iter().map(Node::to_json).collect()
    }
}

impl From<Vec<Node>> for Fragment {
    fn from(nodes: Vec<Node>) -> Self {
        if !nodes
            .windows(2)
            .any(|pair| pair[1].is_text() && pair[0].same_markup(&pair[1]))
        {
            return Self::from_exact(nodes);
        }
        let mut result = Vec::with_capacity(nodes.len());
        let mut remaining = nodes.into_iter().peekable();
        while let Some(node) = remaining.next() {
            if node.is_text() && remaining.peek().is_some_and(|next| next.same_markup(&node)) {
                let mut text = node.text.as_ref().unwrap().clone();
                while let Some(next) = remaining.next_if(|next| next.same_markup(&node)) {
                    text.push_str(next.text.as_deref().unwrap());
                }
                result.push(node.with_text(&text).expect("joined text is non-empty"));
            } else {
                result.push(node);
            }
        }
        Self::from_exact(result)
    }
}

impl From<Node> for Fragment {
    fn from(node: Node) -> Self {
        Self::from_exact(vec![node])
    }
}
