use crate::{
    Affinity, CommandContext, DocPos, Document, EditorConfig, Error, Node, PositionMap, Transaction,
};
use std::{any::Any, fmt::Debug, ops::Range};

/// A stateless selection kind contributed by a plugin. Its two endpoints are
/// document boundaries, so a host can bookmark them without knowing the plugin.
pub trait SelectionBehavior: Any + Debug + Send + Sync {
    fn validate(&self, doc: &Document, anchor: DocPos, head: DocPos) -> Result<(), Error>;
    fn bounds(&self, doc: &Document, anchor: DocPos, head: DocPos) -> Range<DocPos>;
    /// Ordered, disjoint content ranges covered by the selection.
    fn ranges(&self, doc: &Document, anchor: DocPos, head: DocPos) -> Vec<Range<DocPos>>;
    /// Content replaced by commands that operate on the active end.
    fn primary_range(&self, doc: &Document, anchor: DocPos, head: DocPos) -> Range<DocPos>;
    /// Return None when the selection no longer exists in the changed document.
    fn map(
        &self,
        doc: &Document,
        anchor: DocPos,
        head: DocPos,
        map: &PositionMap,
    ) -> Option<(DocPos, DocPos)>;
    /// Plan changes that leave a text selection ready for typing or deletion.
    fn prepare_input(
        &self,
        ctx: CommandContext<'_>,
        anchor: DocPos,
        head: DocPos,
    ) -> Result<Transaction, Error>;
}

impl PartialEq for dyn SelectionBehavior {
    fn eq(&self, other: &Self) -> bool {
        self.type_id() == other.type_id()
    }
}
impl Eq for dyn SelectionBehavior {}

impl EditorConfig {
    pub(crate) fn validate_selection(
        &self,
        selection: Selection,
        doc: &Document,
    ) -> Result<(), Error> {
        if let Selection::Custom { kind, .. } = selection
            && self.selection_plugin(kind).is_none()
        {
            return Err(Error::InvalidEdit(
                "selection requires an installed plugin".into(),
            ));
        }
        selection.validate(doc)
    }

    pub fn selection_behavior(&self, plugin: &str) -> Option<&'static dyn SelectionBehavior> {
        self.plugin_data::<&'static dyn SelectionBehavior>()
            .find_map(|(name, kind)| (name == plugin).then_some(*kind))
    }

    pub fn selection_plugin(&self, behavior: &'static dyn SelectionBehavior) -> Option<&str> {
        self.plugin_data::<&'static dyn SelectionBehavior>()
            .find_map(|(name, kind)| (*kind == behavior).then_some(name))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Text {
        anchor: DocPos,
        head: DocPos,
    },
    Node {
        before: DocPos,
    },
    /// Plugin-defined selection with bookmarkable endpoints.
    Custom {
        kind: &'static dyn SelectionBehavior,
        anchor: DocPos,
        head: DocPos,
    },
    /// A cursor between closed blocks, where typing creates a text block.
    Gap {
        at: DocPos,
    },
    /// A nonempty range of consecutive sibling blocks. Endpoints are block boundaries.
    NodeRange {
        anchor: DocPos,
        head: DocPos,
    },
}

impl Selection {
    pub fn caret(pos: DocPos) -> Self {
        Self::Text {
            anchor: pos,
            head: pos,
        }
    }

    pub fn bounds(self, doc: &Document) -> std::ops::Range<DocPos> {
        match self {
            Self::Text { anchor, head } | Self::NodeRange { anchor, head } => {
                anchor.min(head)..anchor.max(head)
            }
            Self::Custom { kind, anchor, head } => kind.bounds(doc, anchor, head),
            Self::Gap { at } => at..at,
            Self::Node { before } => {
                before..DocPos(before.0 + doc.node_at(before).map_or(0, Node::size))
            }
        }
    }

    pub fn primary_range(self, doc: &Document) -> Range<DocPos> {
        match self {
            Self::Custom { kind, anchor, head } => kind.primary_range(doc, anchor, head),
            _ => self.bounds(doc),
        }
    }

    pub(crate) fn ranges(self, doc: &Document) -> Vec<Range<DocPos>> {
        match self {
            Self::Custom { kind, anchor, head } => kind.ranges(doc, anchor, head),
            _ => vec![self.bounds(doc)],
        }
    }

    /// Inline content covered by this selection, with each run clipped to the selected ranges.
    pub fn inline_nodes(self, doc: &Document) -> impl Iterator<Item = (Range<DocPos>, &Node)> {
        let ranges = self.ranges(doc);
        let span = ranges
            .first()
            .zip(ranges.last())
            .map_or(DocPos(0)..DocPos(0), |(first, last)| first.start..last.end);
        let mut ranges = ranges.into_iter().peekable();
        let mut runs = doc.inline_nodes_in(span).peekable();
        std::iter::from_fn(move || {
            loop {
                let (run, node) = runs.peek()?;
                let range = ranges.peek()?;
                let overlap = run.start.max(range.start)..run.end.min(range.end);
                let node = *node;
                let run_end = run.end;
                let range_end = range.end;
                if run_end <= range_end {
                    runs.next();
                }
                if range_end <= run_end {
                    ranges.next();
                }
                if !overlap.is_empty() {
                    return Some((overlap, node));
                }
            }
        })
    }

    pub fn validate(self, doc: &Document) -> Result<(), Error> {
        match self {
            Self::Custom { kind, anchor, head } => kind.validate(doc, anchor, head),
            Self::Gap { at } => gap_textblock(doc, at)
                .map(|_| ())
                .ok_or(Error::InvalidPosition(at)),
            Self::NodeRange { anchor, head } => {
                let root = doc.root();
                let from = root.resolve(anchor.0)?;
                let to = root.resolve(head.0)?;
                if anchor != head
                    && from.same_parent(&to)
                    && !doc
                        .schema()
                        .node_type(&from.parent().kind)
                        .is_some_and(|ty| ty.inline_content())
                {
                    Ok(())
                } else {
                    Err(Error::InvalidEdit(
                        "node range must cover sibling blocks".into(),
                    ))
                }
            }
            Self::Text { anchor, head } => {
                doc.text_at(anchor)?;
                doc.text_at(head)?;
                Ok(())
            }
            Self::Node { before } => doc
                .node_at(before)
                .map(|_| ())
                .ok_or(Error::InvalidPosition(before)),
        }
    }

    pub(super) fn near(doc: &Document, pos: DocPos) -> Result<Self, Error> {
        if let Some(caret) = doc.nearest_text(pos) {
            return Ok(Self::caret(caret));
        }
        // A plugin can define a valid document consisting solely of non-text nodes.
        let before = DocPos(0);
        doc.node_at(before)
            .map(|_| Self::Node { before })
            .ok_or_else(|| Error::InvalidDocument("document has no selectable node".into()))
    }

    pub(super) fn mapped(self, doc: &Document, map: &PositionMap) -> Result<Self, Error> {
        match self {
            Self::Custom { kind, anchor, head } => match kind.map(doc, anchor, head, map) {
                Some((anchor, head)) => Ok(Self::Custom { kind, anchor, head }),
                None => Self::near(doc, map.map(head, Affinity::After).position),
            },
            Self::Gap { at } => {
                let at = map.map(at, Affinity::After).position;
                let gap = Self::Gap { at };
                if gap.validate(doc).is_ok() {
                    Ok(gap)
                } else {
                    Self::near(doc, at)
                }
            }
            Self::NodeRange { anchor, head } => {
                let start = map.map(anchor.min(head), Affinity::After).position;
                let end = map.map(anchor.max(head), Affinity::Before).position;
                let range = if anchor < head {
                    Self::NodeRange {
                        anchor: start,
                        head: end,
                    }
                } else {
                    Self::NodeRange {
                        anchor: end,
                        head: start,
                    }
                };
                if start < end && range.validate(doc).is_ok() {
                    Ok(range)
                } else {
                    Self::near(doc, start)
                }
            }
            Self::Node { before } => {
                let mapped = map.map(before, Affinity::After);
                if !mapped.deleted && doc.node_at(mapped.position).is_some() {
                    Ok(Self::Node {
                        before: mapped.position,
                    })
                } else {
                    Self::near(doc, mapped.position)
                }
            }
            Self::Text { anchor, head } => {
                let low = anchor.min(head);
                let high = anchor.max(head);
                let start = map.map(low, Affinity::After).position;
                if low == high {
                    return Self::near(doc, start);
                }
                let end = map.map(high, Affinity::Before).position;
                if start >= end {
                    return Self::near(doc, start);
                }
                let (Some(start), Some(end)) = (doc.nearest_text(start), doc.nearest_text(end))
                else {
                    return Self::near(doc, start);
                };
                Ok(if anchor < head {
                    Self::Text {
                        anchor: start,
                        head: end,
                    }
                } else {
                    Self::Text {
                        anchor: end,
                        head: start,
                    }
                })
            }
        }
    }
}

/// Find the default text block that typing at this gap would create.
pub(super) fn gap_textblock(doc: &Document, at: DocPos) -> Option<String> {
    let root = doc.root();
    let pos = root.resolve(at.0).ok()?;
    let parent = pos.parent();
    if doc.schema().node_type(&parent.kind)?.inline_content() {
        return None;
    }
    // Each side must end in a closed block or the edge of the document.
    for right in [false, true] {
        for depth in (0..=pos.depth()).rev() {
            let ancestor = pos.node(depth);
            let index = if right {
                Some(pos.index_after(depth))
            } else {
                pos.index(depth).checked_sub(1)
            };
            let Some(mut child) = index.and_then(|i| ancestor.content.maybe_child(i)) else {
                continue;
            };
            loop {
                if doc.schema().node_type(&child.kind)?.inline_content() {
                    return None;
                }
                let children = child.content.children();
                let Some(next) = (if right {
                    children.first()
                } else {
                    children.last()
                }) else {
                    break;
                };
                child = next;
            }
            break;
        }
    }
    let index = pos.index(pos.depth());
    let matched = parent.content_match_at(doc.schema(), index).ok()?;
    let kind = matched.default_type(doc.schema())?;
    let ty = doc.schema().node_type(kind)?;
    (ty.is_textblock() && parent.can_replace_with(doc.schema(), index, index, kind, &[]))
        .then(|| kind.to_owned())
}
