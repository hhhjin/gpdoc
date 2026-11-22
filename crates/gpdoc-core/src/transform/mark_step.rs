use crate::model::{Error, Fragment, Mark, Node, Schema, Slice};
use crate::transform::{Mappable, Step};

#[derive(Clone, Debug, PartialEq)]
pub struct RangeMarkStep<const ADD: bool> {
    pub from: usize,
    pub to: usize,
    pub mark: Mark,
}
pub type AddMarkStep = RangeMarkStep<true>;
pub type RemoveMarkStep = RangeMarkStep<false>;

impl<const ADD: bool> RangeMarkStep<ADD> {
    pub fn apply(&self, schema: &Schema, doc: &Node) -> Result<Node, Error> {
        let old = doc.slice(self.from, self.to, false)?;
        let from = doc.resolve(self.from)?;
        let parent = from.node(from.shared_depth(self.to));
        let content = map_fragment(&old.content, parent, &|child, parent| {
            if ADD {
                if !schema.node_type(&child.kind).is_some_and(|ty| ty.is_atom())
                    || !schema
                        .node_type(&parent.kind)
                        .is_some_and(|ty| ty.allows_mark(&self.mark.kind))
                {
                    return child.clone();
                }
                child.mark(self.mark.add_to_set(schema, &child.marks))
            } else {
                child.mark(self.mark.remove_from_set(&child.marks))
            }
        });
        doc.replace(
            schema,
            self.from,
            self.to,
            &Slice::new(content, old.open_start, old.open_end),
        )
    }

    pub fn invert(&self, schema: &Schema, doc: &Node) -> Result<Step, Error> {
        let mut steps = Vec::new();
        doc.nodes_between(self.from, self.to, |node, pos, _, _| {
            if !node.is_inline() || !schema.node_type(&node.kind).is_some_and(|ty| ty.is_atom()) {
                return true;
            }
            let from = self.from.max(pos);
            let to = self.to.min(pos + node.size());
            if ADD {
                let added = self.mark.add_to_set(schema, &node.marks);
                if added != node.marks {
                    push_mark_step(
                        &mut steps,
                        Step::RemoveMark(RemoveMarkStep {
                            from,
                            to,
                            mark: self.mark.clone(),
                        }),
                    );
                    for excluded in node.marks.iter().filter(|m| !added.contains(m)) {
                        push_mark_step(
                            &mut steps,
                            Step::AddMark(AddMarkStep {
                                from,
                                to,
                                mark: excluded.clone(),
                            }),
                        );
                    }
                }
            } else if node.marks.contains(&self.mark) {
                push_mark_step(
                    &mut steps,
                    Step::AddMark(AddMarkStep {
                        from,
                        to,
                        mark: self.mark.clone(),
                    }),
                );
            }
            true
        })?;
        if steps.len() == 1 {
            Ok(steps.pop().unwrap())
        } else {
            Step::mark_set(steps)
        }
    }

    pub fn map(&self, mapping: &impl Mappable) -> Option<Self> {
        let from = mapping.map(self.from, 1);
        let to = mapping.map(self.to, -1);
        (from < to).then(|| Self {
            from,
            to,
            mark: self.mark.clone(),
        })
    }

    pub fn merge(&self, other: &Self) -> Option<Self> {
        (self.mark == other.mark && self.from <= other.to && self.to >= other.from).then(|| Self {
            from: self.from.min(other.from),
            to: self.to.max(other.to),
            mark: self.mark.clone(),
        })
    }
}

fn map_fragment(
    content: &Fragment,
    parent: &Node,
    apply: &impl Fn(&Node, &Node) -> Node,
) -> Fragment {
    let mut nodes = Vec::with_capacity(content.child_count());
    for child in content.children() {
        let mut mapped = child.clone();
        if child.content.size() > 0 {
            mapped = child.copy(map_fragment(&child.content, child, apply));
        }
        if child.is_inline() {
            mapped = apply(&mapped, parent);
        }
        nodes.push(mapped);
    }
    Fragment::from(nodes)
}

fn push_mark_step(steps: &mut Vec<Step>, next: Step) {
    match (steps.last_mut(), &next) {
        (Some(Step::AddMark(last)), Step::AddMark(curr))
            if last.to == curr.from && last.mark == curr.mark =>
        {
            last.to = curr.to
        }
        (Some(Step::RemoveMark(last)), Step::RemoveMark(curr))
            if last.to == curr.from && last.mark == curr.mark =>
        {
            last.to = curr.to
        }
        _ => steps.push(next),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeMarkStep<const ADD: bool> {
    pub pos: usize,
    pub mark: Mark,
}
pub type AddNodeMarkStep = NodeMarkStep<true>;
pub type RemoveNodeMarkStep = NodeMarkStep<false>;

impl<const ADD: bool> NodeMarkStep<ADD> {
    pub fn apply(&self, schema: &Schema, doc: &Node) -> Result<Node, Error> {
        let node = doc
            .node_at(self.pos)?
            .ok_or_else(|| Error("No node at mark step's position".into()))?;
        let marks = if ADD {
            self.mark.add_to_set(schema, &node.marks)
        } else {
            self.mark.remove_from_set(&node.marks)
        };
        let updated =
            schema.node_unchecked(&node.kind, node.attrs.clone(), Fragment::default(), marks)?;
        replace_node_start(schema, doc, self.pos, updated)
    }

    pub fn invert(&self, schema: &Schema, doc: &Node) -> Result<Step, Error> {
        let node = doc.node_at(self.pos)?;
        if ADD {
            if let Some(node) = node {
                let new = self.mark.add_to_set(schema, &node.marks);
                if new.len() == node.marks.len() {
                    let mark = node
                        .marks
                        .iter()
                        .find(|old| !new.contains(old))
                        .unwrap_or(&self.mark);
                    return Ok(Step::AddNodeMark(AddNodeMarkStep {
                        pos: self.pos,
                        mark: mark.clone(),
                    }));
                }
            }
        } else if node.is_some_and(|node| node.marks.contains(&self.mark)) {
            return Ok(Step::AddNodeMark(AddNodeMarkStep {
                pos: self.pos,
                mark: self.mark.clone(),
            }));
        }
        Ok(Step::RemoveNodeMark(RemoveNodeMarkStep {
            pos: self.pos,
            mark: self.mark.clone(),
        }))
    }

    pub fn map(&self, mapping: &impl Mappable) -> Option<Self> {
        let mapped = mapping.map_result(self.pos, 1);
        (!mapped.deleted_after()).then(|| Self {
            pos: mapped.pos,
            mark: self.mark.clone(),
        })
    }
}

pub(super) fn replace_node_start(
    schema: &Schema,
    doc: &Node,
    pos: usize,
    updated: Node,
) -> Result<Node, Error> {
    let open_end = usize::from(!updated.is_leaf());
    let slice = Slice::new(Fragment::from(vec![updated]), 0, open_end);
    doc.replace(schema, pos, pos + 1, &slice)
}
