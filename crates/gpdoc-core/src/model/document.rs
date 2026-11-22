use crate::model::{ContentRef, Node, ResolvedPos, TextRunRef};

/// Offset in a document. Text uses UTF-16. Leaves and container boundaries each occupy one unit.
/// The root's first child starts at zero. Use [`Document::text_pos_at_byte`] to translate UTF-8.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocPos(pub usize);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PositionError {
    #[error("invalid document position {0:?}")]
    InvalidPosition(DocPos),
    #[error("invalid UTF-8 byte offset {offset} in block at {block:?}")]
    InvalidOffset { block: DocPos, offset: usize },
}

/// An immutable, schema-validated document tree.
#[derive(Clone, Debug)]
pub struct Document {
    pub(super) root: Node,
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        self.schema() == other.schema() && self.root == other.root
    }
}
impl Eq for Document {}

fn byte_at_utf16(chars: impl Iterator<Item = char>, target: usize) -> Option<usize> {
    let mut units = 0;
    let mut bytes = 0;
    for ch in chars {
        if units >= target {
            break;
        }
        units += ch.len_utf16();
        bytes += ch.len_utf8();
    }
    (units == target).then_some(bytes)
}

impl Document {
    pub fn root(&self) -> &Node {
        &self.root
    }

    pub fn size(&self) -> usize {
        self.root.content.size()
    }

    pub fn resolve(&self, pos: DocPos) -> Result<ResolvedPos, PositionError> {
        self.root
            .resolve(pos.0)
            .map_err(|_| PositionError::InvalidPosition(pos))
    }

    pub fn schema(&self) -> &crate::model::Schema {
        self.root
            .schema()
            .expect("a document has a validated schema")
    }

    /// Platform text of an inline child. The schema's line-break node occupies one newline.
    pub fn inline_text<'a>(&self, node: &'a Node) -> &'a str {
        if node.is_leaf()
            && self
                .schema()
                .linebreak_replacement()
                .is_some_and(|ty| ty.name == node.kind)
        {
            "\n"
        } else {
            node.projected_text()
        }
    }

    pub fn blocks(&self) -> &[Node] {
        self.root.content.children()
    }

    pub fn nodes(&self) -> Vec<(DocPos, &Node)> {
        self.iter_nodes().collect()
    }

    /// Visits every block and inline child in document order.
    pub fn iter_nodes(&self) -> impl Iterator<Item = (DocPos, &Node)> {
        self.walk(true)
    }

    pub fn iter_blocks(&self) -> impl Iterator<Item = (DocPos, &Node)> {
        self.walk(false)
    }

    fn walk(&self, include_inline: bool) -> impl Iterator<Item = (DocPos, &Node)> {
        let mut stack = vec![(0, self.blocks().iter())];
        std::iter::from_fn(move || {
            loop {
                let (next_position, siblings) = stack.last_mut()?;
                if let Some(node) = siblings.next() {
                    let before = *next_position;
                    *next_position += node.size();
                    match node.content() {
                        ContentRef::Blocks(children) => stack.push((before + 1, children.iter())),
                        ContentRef::Inline(children) if include_inline => {
                            stack.push((before + 1, children.iter()))
                        }
                        _ => {}
                    }
                    return Some((DocPos(before), node));
                }
                stack.pop();
            }
        })
    }

    /// Text runs intersecting a half-open selection, with each whole run's document range.
    pub fn text_runs_in(
        &self,
        range: std::ops::Range<DocPos>,
    ) -> impl Iterator<Item = (std::ops::Range<DocPos>, TextRunRef<'_>)> {
        self.inline_nodes_in(range)
            .filter_map(|(span, node)| match node.content() {
                ContentRef::Text(run) => Some((span, run)),
                _ => None,
            })
    }

    pub fn inline_nodes_in(
        &self,
        range: std::ops::Range<DocPos>,
    ) -> impl Iterator<Item = (std::ops::Range<DocPos>, &Node)> {
        let from = range.start;
        let to = range.end;
        self.iter_blocks()
            .take_while(move |(before, _)| *before < to)
            .filter_map(move |(before, node)| match node.content() {
                ContentRef::Inline(runs) if before.0 + node.size() > from.0 => Some((before, runs)),
                _ => None,
            })
            .flat_map(move |(before, runs)| {
                runs.iter().scan(before.0 + 1, |pos, run| {
                    let start = DocPos(*pos);
                    *pos += run.size();
                    Some((start..DocPos(*pos), run))
                })
            })
            .filter(move |(span, _)| span.start.max(from) < span.end.min(to))
    }

    pub fn node_at(&self, before: DocPos) -> Option<&Node> {
        self.root
            .node_at(before.0)
            .ok()
            .flatten()
            .filter(|node| !node.is_text())
    }

    pub fn resolve_text(&self, pos: DocPos) -> Result<(ResolvedPos, usize), PositionError> {
        let resolved = self.resolve(pos)?;
        let invalid = PositionError::InvalidPosition(pos);
        let parent = resolved.parent();
        if !parent.inline_content() {
            return Err(invalid);
        }
        let bytes = byte_at_utf16(
            parent
                .content
                .children()
                .iter()
                .flat_map(|run| self.inline_text(run).chars()),
            resolved.parent_offset,
        )
        .ok_or(invalid)?;
        Ok((resolved, bytes))
    }

    /// Resolves a text position to its block and the position before that block.
    pub fn text_at(&self, pos: DocPos) -> Result<(DocPos, &Node), PositionError> {
        let (resolved, _) = self.resolve_text(pos)?;
        let before = resolved
            .before(resolved.depth())
            .map_err(|_| PositionError::InvalidPosition(pos))?;
        Ok((
            DocPos(before),
            self.node_at(DocPos(before))
                .ok_or(PositionError::InvalidPosition(pos))?,
        ))
    }

    /// Converts a text position to a UTF-8 byte offset in its block's platform text projection.
    pub fn text_byte_offset(&self, pos: DocPos) -> Result<usize, PositionError> {
        self.resolve_text(pos).map(|(_, bytes)| bytes)
    }

    /// Converts a UTF-8 byte boundary in a text block to a global position.
    pub fn text_pos_at_byte(
        &self,
        before: DocPos,
        byte_offset: usize,
    ) -> Result<DocPos, PositionError> {
        let Some(ContentRef::Inline(runs)) = self.node_at(before).map(Node::content) else {
            return Err(PositionError::InvalidPosition(before));
        };
        let mut bytes = 0;
        let mut units = 0;
        for ch in runs.iter().flat_map(|run| self.inline_text(run).chars()) {
            if bytes >= byte_offset {
                break;
            }
            bytes += ch.len_utf8();
            units += ch.len_utf16();
        }
        if bytes != byte_offset {
            return Err(PositionError::InvalidOffset {
                block: before,
                offset: byte_offset,
            });
        }
        Ok(DocPos(before.0 + 1 + units))
    }

    /// Returns the nearest text cursor. Equidistant cursors resolve to the later one.
    pub fn nearest_text(&self, pos: DocPos) -> Option<DocPos> {
        let mut previous = None;
        for (before, node) in self.iter_blocks() {
            let ContentRef::Inline(runs) = node.content() else {
                continue;
            };
            let start = before.0 + 1;
            let end = before.0 + node.size() - 1;
            if pos.0 < start {
                return match previous {
                    Some(earlier) if pos.0 - earlier < start - pos.0 => Some(DocPos(earlier)),
                    _ => Some(DocPos(start)),
                };
            }
            if pos.0 <= end {
                let chars = runs.iter().flat_map(|r| self.inline_text(r).chars());
                // The only interior UTF-16 position lies exactly between two surrogate halves.
                return Some(if byte_at_utf16(chars, pos.0 - start).is_some() {
                    pos
                } else {
                    DocPos(pos.0 + 1)
                });
            }
            previous = Some(end);
        }
        previous.map(DocPos)
    }
}
