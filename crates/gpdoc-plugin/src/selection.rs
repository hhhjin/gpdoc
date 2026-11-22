//! Locate nodes and ranges covered by a selection.
use gpdoc_core::{CommandContext, ContentRef, DocPos, Node, Selection};

#[derive(Clone, Copy)]
pub struct Located<'a> {
    pub before: usize,
    pub node: &'a Node,
}
impl Located<'_> {
    pub fn end(&self) -> usize {
        self.before + self.node.size()
    }
}

pub fn children_at(nodes: &[Node], start: usize) -> impl Iterator<Item = Located<'_>> {
    nodes.iter().scan(start, |offset, node| {
        let before = *offset;
        *offset += node.size();
        Some(Located { before, node })
    })
}

// A position before an opening boundary belongs to that node for node selections.
pub fn path_at(mut nodes: &[Node], pos: DocPos) -> Vec<Located<'_>> {
    let mut start = 0;
    let mut path = Vec::new();
    while let Some(found) = children_at(nodes, start).find(|node| pos.0 < node.end()) {
        path.push(found);
        match found.node.content() {
            ContentRef::Blocks(children) if pos.0 > found.before => {
                nodes = children;
                start = found.before + 1;
            }
            _ => break,
        }
    }
    path
}

// Deepest sibling range covering the selection. A node selection always covers exactly that node.
pub fn selected_blocks(editor: CommandContext<'_>) -> Vec<Located<'_>> {
    let doc = editor.document();
    let (from, to) = match editor.selection() {
        Selection::Gap { .. } => return vec![],
        Selection::NodeRange { anchor, head } => {
            let mut selected = Vec::new();
            let mut before = anchor.min(head);
            while before < anchor.max(head) {
                let Some(node) = doc.node_at(before) else {
                    break;
                };
                selected.push(Located {
                    before: before.0,
                    node,
                });
                before.0 += node.size();
            }
            return selected;
        }
        Selection::Custom { anchor, head, .. } | Selection::Text { anchor, head } => {
            (anchor.min(head), anchor.max(head))
        }
        Selection::Node { before } => {
            return doc
                .node_at(before)
                .map(|node| Located {
                    before: before.0,
                    node,
                })
                .into_iter()
                .collect();
        }
    };
    let left = path_at(doc.blocks(), from);
    let right = path_at(doc.blocks(), to);
    let mut siblings = doc.blocks();
    let mut start = 0;
    for (a, b) in left.iter().zip(&right) {
        if a.before != b.before {
            return children_at(siblings, start)
                .filter(|child| (a.before..=b.before).contains(&child.before))
                .collect();
        }
        if let gpdoc_core::ContentRef::Blocks(children) = a.node.content() {
            siblings = children;
            start = a.before + 1;
        }
    }
    left.last().copied().into_iter().collect()
}
