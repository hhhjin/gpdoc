//! List commands can move several container boundaries in one edit. Copy complete
//! document subtrees and rebuild only the parents intersected by a boundary.
use super::{Boundary, BoundaryEdit, PositionMap, node_shell};
use crate::model::{DocPos, Document};
use crate::model::{Fragment, Node};
use crate::transform::TransformError as Error;

struct Frame {
    parent: Node,
    children: Vec<Node>,
}

struct Builder(Vec<Frame>);

impl Builder {
    fn open(&mut self, parent: Node) {
        self.0.push(Frame {
            parent,
            children: Vec::new(),
        });
    }

    fn append(&mut self, node: Node) {
        self.0.last_mut().expect("root frame").children.push(node);
    }

    fn close(&mut self) -> Result<(), Error> {
        if self.0.len() == 1 {
            return Err(Error::InvalidEdit("unmatched closing boundary".into()));
        }
        let frame = self.0.pop().expect("open parent");
        self.append(frame.parent.copy(Fragment::from(frame.children)));
        Ok(())
    }

    fn finish(mut self) -> Result<Node, Error> {
        if self.0.len() != 1 {
            return Err(Error::InvalidEdit("unmatched opening boundary".into()));
        }
        let root = self.0.pop().expect("root frame");
        Ok(root.parent.copy(Fragment::from(root.children)))
    }
}

enum Source<'a> {
    Node(&'a Node),
    TextTail(&'a Node, usize),
    Close,
}

struct Reader<'a> {
    pending: Vec<Source<'a>>,
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(root: &'a Node) -> Self {
        Self {
            pending: root
                .content
                .children()
                .iter()
                .rev()
                .map(Source::Node)
                .collect(),
            pos: 0,
        }
    }

    /// With no destination, only container opening and closing tokens may be skipped.
    fn advance(&mut self, until: usize, mut dest: Option<&mut Builder>) -> Result<(), Error> {
        while self.pos < until {
            let remaining = until - self.pos;
            let source = self
                .pending
                .pop()
                .ok_or(Error::InvalidPosition(DocPos(until)))?;
            match (source, dest.as_deref_mut()) {
                (Source::Node(node), Some(builder)) if node.size() <= remaining => {
                    builder.append(node.clone());
                    self.pos += node.size();
                }
                (Source::Node(node), _) if node.is_text() => {
                    self.pending.push(Source::TextTail(node, 0));
                }
                (Source::TextTail(node, offset), Some(builder)) => {
                    let end = offset + (node.size() - offset).min(remaining);
                    builder.append(
                        node.cut(offset, end)
                            .map_err(|_| Error::InvalidPosition(DocPos(until)))?,
                    );
                    self.pos += end - offset;
                    if end != node.size() {
                        self.pending.push(Source::TextTail(node, end));
                    }
                }
                (Source::TextTail(_, _), None) => {
                    return Err(Error::InvalidEdit("boundary edit would remove text".into()));
                }
                (Source::Node(node), _) if node.is_leaf() => {
                    return Err(Error::InvalidEdit(
                        "boundary edit would remove a leaf".into(),
                    ));
                }
                (Source::Node(node), builder) => {
                    if let Some(builder) = builder {
                        builder.open(node.clone());
                    }
                    self.pending.push(Source::Close);
                    self.pending
                        .extend(node.content.children().iter().rev().map(Source::Node));
                    self.pos += 1;
                }
                (Source::Close, builder) => {
                    if let Some(builder) = builder {
                        builder.close()?;
                    }
                    self.pos += 1;
                }
            }
        }
        Ok(())
    }
}

pub(super) fn apply_boundaries(
    doc: &Document,
    edits: &[BoundaryEdit],
) -> Result<(Document, PositionMap), Error> {
    let schema = doc.schema();
    let root = doc.root();
    let mut reader = Reader::new(root);
    let mut builder = Builder(Vec::new());
    builder.open(root.clone());
    let mut map = PositionMap::default();
    let mut written = 0usize;
    let overflow = || Error::InvalidDocument("document size exceeds position space".into());
    for edit in edits {
        let start = edit.range.start.0;
        let end = edit.range.end.0;
        if start < reader.pos || end < start || end > root.content.size() {
            return Err(Error::InvalidEdit(
                "unordered or out-of-bounds boundary edit".into(),
            ));
        }
        written = written
            .checked_add(start - reader.pos)
            .ok_or_else(overflow)?;
        reader.advance(start, Some(&mut builder))?;
        reader.advance(end, None)?;
        for boundary in &edit.insert {
            match boundary {
                Boundary::Open { kind, attrs } => {
                    let parent = node_shell(schema, kind, attrs)?;
                    if parent.is_leaf() {
                        return Err(Error::InvalidEdit("a leaf cannot have boundaries".into()));
                    }
                    builder.open(parent);
                }
                Boundary::Close => builder.close()?,
            }
        }
        map.record(written, end - start, edit.insert.len());
        written = written
            .checked_add(edit.insert.len())
            .ok_or_else(overflow)?;
    }
    if map.is_identity() {
        return Ok((doc.clone(), map));
    }
    written
        .checked_add(root.content.size() - reader.pos)
        .ok_or_else(overflow)?;
    reader.advance(root.content.size(), Some(&mut builder))?;
    let root = builder.finish()?;
    Ok((schema.with_root(Some(doc), root)?, map))
}
