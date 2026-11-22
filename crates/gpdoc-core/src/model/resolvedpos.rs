use crate::model::{Error, Mark, Node, Schema};

#[derive(Clone, Debug)]
struct PathEntry {
    node: Node,
    index: usize,
    child_start: usize,
}

/// A document position with the ancestors, child indices, and offsets on its path.
#[derive(Clone, Debug)]
pub struct ResolvedPos {
    pub pos: usize,
    pub parent_offset: usize,
    path: Vec<PathEntry>,
}

impl ResolvedPos {
    pub(super) fn resolve(doc: &Node, pos: usize) -> Result<Self, Error> {
        let mut path = Vec::new();
        let mut node = doc;
        let mut start = 0;
        let mut parent_offset = pos;
        loop {
            let (index, offset) = node.content.find_index(parent_offset)?;
            let remainder = parent_offset - offset;
            path.push(PathEntry {
                node: node.clone(),
                index,
                child_start: start + offset,
            });
            if remainder == 0 {
                break;
            }
            node = node.child(index);
            if node.is_text() {
                break;
            }
            parent_offset = remainder - 1;
            start += offset + 1;
        }
        Ok(Self {
            pos,
            parent_offset,
            path,
        })
    }

    pub fn depth(&self) -> usize {
        self.path.len() - 1
    }
    pub fn node(&self, depth: usize) -> &Node {
        &self.path[depth].node
    }
    pub fn index(&self, depth: usize) -> usize {
        self.path[depth].index
    }
    pub fn parent(&self) -> &Node {
        self.node(self.depth())
    }
    pub fn doc(&self) -> &Node {
        self.node(0)
    }
    pub fn text_offset(&self) -> usize {
        self.pos - self.path[self.depth()].child_start
    }
    pub fn index_after(&self, depth: usize) -> usize {
        self.index(depth) + usize::from(depth != self.depth() || self.text_offset() != 0)
    }
    pub fn start(&self, depth: usize) -> usize {
        if depth == 0 {
            0
        } else {
            self.path[depth - 1].child_start + 1
        }
    }
    pub fn end(&self, depth: usize) -> usize {
        self.start(depth) + self.node(depth).content.size()
    }
    pub fn before(&self, depth: usize) -> Result<usize, Error> {
        if depth == self.depth() + 1 {
            return Ok(self.pos);
        }
        if depth == 0 || depth > self.depth() {
            return Err(Error("invalid depth for before".into()));
        }
        Ok(self.path[depth - 1].child_start)
    }
    pub fn after(&self, depth: usize) -> Result<usize, Error> {
        if depth == self.depth() + 1 {
            return Ok(self.pos);
        }
        Ok(self.before(depth)? + self.node(depth).size())
    }
    pub fn pos_at_index(&self, index: usize, depth: usize) -> usize {
        self.start(depth)
            + self.node(depth).content.children()[..index]
                .iter()
                .map(Node::size)
                .sum::<usize>()
    }
    pub fn node_before(&self) -> Result<Option<Node>, Error> {
        let index = self.index(self.depth());
        let offset = self.text_offset();
        if offset > 0 {
            return self.parent().child(index).cut(0, offset).map(Some);
        }
        Ok(index.checked_sub(1).map(|i| self.parent().child(i).clone()))
    }
    pub fn node_after(&self) -> Result<Option<Node>, Error> {
        let Some(child) = self.parent().content.maybe_child(self.index(self.depth())) else {
            return Ok(None);
        };
        let offset = self.text_offset();
        if offset > 0 {
            return child.cut(offset, child.size()).map(Some);
        }
        Ok(Some(child.clone()))
    }

    pub fn shared_depth(&self, pos: usize) -> usize {
        (1..=self.depth())
            .rev()
            .find(|&depth| (self.start(depth)..=self.end(depth)).contains(&pos))
            .unwrap_or(0)
    }
    pub fn same_parent(&self, other: &Self) -> bool {
        self.pos - self.parent_offset == other.pos - other.parent_offset
    }

    pub fn marks(&self, schema: &Schema) -> Vec<Mark> {
        let parent = self.parent();
        let index = self.index(self.depth());
        if self.text_offset() > 0 {
            return parent.child(index).marks.clone();
        }
        let before = index.checked_sub(1).map(|i| parent.child(i));
        let after = parent.content.maybe_child(index);
        match (before, after) {
            (Some(main), other) => inclusive_marks(schema, main, other),
            (None, Some(main)) => inclusive_marks(schema, main, None),
            _ => Vec::new(),
        }
    }
    pub fn marks_across(&self, schema: &Schema, end: &Self) -> Option<Vec<Mark>> {
        let after = self
            .parent()
            .content
            .maybe_child(self.index(self.depth()))?;
        if !after.is_inline() {
            return None;
        }
        Some(inclusive_marks(
            schema,
            after,
            end.parent().content.maybe_child(end.index(end.depth())),
        ))
    }

    pub fn block_range<'a>(
        &'a self,
        other: &'a Self,
        schema: &Schema,
        mut accept: impl FnMut(&Node) -> bool,
    ) -> Option<NodeRange<'a>> {
        let (from, to) = if self.pos <= other.pos {
            (self, other)
        } else {
            (other, self)
        };
        let inline = schema.node_type(from.parent().kind())?.inline_content();
        let max_depth = from
            .depth()
            .checked_sub(usize::from(inline || from.pos == to.pos))?;
        let depth = (0..=max_depth)
            .rev()
            .find(|&depth| to.pos <= from.end(depth) && accept(from.node(depth)))?;
        Some(NodeRange { from, to, depth })
    }
}

fn inclusive_marks(schema: &Schema, main: &Node, other: Option<&Node>) -> Vec<Mark> {
    main.marks
        .iter()
        .filter(|mark| {
            schema.mark_inclusive(&mark.kind) || other.is_some_and(|n| n.marks.contains(mark))
        })
        .cloned()
        .collect()
}

#[derive(Debug)]
pub struct NodeRange<'a> {
    from: &'a ResolvedPos,
    to: &'a ResolvedPos,
    pub depth: usize,
}

impl NodeRange<'_> {
    pub fn start(&self) -> usize {
        self.from.before(self.depth + 1).expect("valid block range")
    }
    pub fn end(&self) -> usize {
        self.to.after(self.depth + 1).expect("valid block range")
    }
    pub fn parent(&self) -> &Node {
        self.from.node(self.depth)
    }
    pub fn start_index(&self) -> usize {
        self.from.index(self.depth)
    }
    pub fn end_index(&self) -> usize {
        self.to.index_after(self.depth)
    }
}
