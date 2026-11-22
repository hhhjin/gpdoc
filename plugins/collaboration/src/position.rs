//! A fragment indexes children, while an XmlText indexes UTF-16 code units.
use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::prelude::{DocPos, Document, Schema, Selection};
use serde::{Deserialize, Serialize};
use yrs::{Assoc, IndexedSequence, ReadTxn, StickyIndex, Text, XmlFragment, XmlOut};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelativeSelection {
    pub anchor: StickyIndex,
    pub head: StickyIndex,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<BoundaryKind>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum BoundaryKind {
    Node,
    Gap,
    NodeRange,
    Plugin(String),
}

enum Offsets {
    Text { start: usize, len: usize },
    Children(Vec<usize>),
}

struct Entry {
    xml: XmlOut,
    positions: Offsets,
}

pub(crate) struct Positions(Vec<Entry>);

impl Positions {
    pub fn new(tx: &impl ReadTxn, root: yrs::XmlFragmentRef, schema: &Schema) -> Self {
        let mut out = Self(Vec::new());
        out.visit(tx, XmlOut::Fragment(root), 0, schema);
        out
    }

    fn visit(&mut self, tx: &impl ReadTxn, xml: XmlOut, pos: usize, schema: &Schema) -> usize {
        let (end, positions) = match &xml {
            XmlOut::Text(t) => {
                let len = t.len(tx) as usize;
                (pos + len, Offsets::Text { start: pos, len })
            }
            XmlOut::Element(el) if schema.node_type(el.tag()).is_some_and(|ty| ty.is_leaf()) => {
                return pos + 1;
            }
            parent => {
                let root = matches!(parent, XmlOut::Fragment(_));
                let mut at = pos + usize::from(!root);
                let mut boundaries = vec![at];
                let children = match parent {
                    XmlOut::Element(el) => el.children(tx),
                    XmlOut::Fragment(f) => f.children(tx),
                    _ => unreachable!(),
                };
                for child in children {
                    at = self.visit(tx, child, at, schema);
                    boundaries.push(at);
                }
                (at + usize::from(!root), Offsets::Children(boundaries))
            }
        };
        self.0.push(Entry { xml, positions });
        end
    }

    fn pin(&self, tx: &impl ReadTxn, pos: DocPos, node: bool) -> Option<StickyIndex> {
        for entry in &self.0 {
            let index = match &entry.positions {
                Offsets::Text { start, len }
                    if !node && (*start..=*start + len).contains(&pos.0) =>
                {
                    pos.0 - start
                }
                Offsets::Children(bounds) => match bounds.binary_search(&pos.0) {
                    Ok(index) => index,
                    Err(_) => continue,
                },
                _ => continue,
            } as u32;
            let assoc = if node { Assoc::After } else { Assoc::Before };
            return match &entry.xml {
                XmlOut::Text(t) => t.sticky_index(tx, index, assoc),
                XmlOut::Element(el) => el.sticky_index(tx, index, assoc),
                XmlOut::Fragment(f) => f.sticky_index(tx, index, assoc),
            }
            .or_else(|| Some(StickyIndex::from_type(tx, &entry.xml, assoc)));
        }
        None
    }

    fn resolve(&self, tx: &impl ReadTxn, pin: &StickyIndex) -> Option<DocPos> {
        let at = pin.get_offset(tx)?;
        let entry = self
            .0
            .iter()
            .find(|entry| entry.xml.as_ptr() == at.branch)?;
        match &entry.positions {
            Offsets::Text { start, len } if at.index as usize <= *len => {
                Some(DocPos(start + at.index as usize))
            }
            Offsets::Children(bounds) => bounds.get(at.index as usize).copied().map(DocPos),
            _ => None,
        }
    }

    pub fn relative(
        &self,
        tx: &impl ReadTxn,
        selection: Selection,
        doc: &Document,
        config: &EditorConfig,
    ) -> Option<RelativeSelection> {
        let (a, h, kind) = match selection {
            Selection::Text { anchor, head } => (anchor, head, None),
            Selection::Node { before } => {
                (before, selection.bounds(doc).end, Some(BoundaryKind::Node))
            }
            Selection::Gap { at } => (at, at, Some(BoundaryKind::Gap)),
            Selection::Custom { kind, anchor, head } => (
                anchor,
                head,
                Some(BoundaryKind::Plugin(config.selection_plugin(kind)?.into())),
            ),
            Selection::NodeRange { anchor, head } => (anchor, head, Some(BoundaryKind::NodeRange)),
        };
        Some(RelativeSelection {
            anchor: self.pin(tx, a, kind.is_some())?,
            head: self.pin(tx, h, kind.is_some())?,
            kind,
        })
    }

    pub fn absolute(
        &self,
        tx: &impl ReadTxn,
        relative: &RelativeSelection,
        config: &EditorConfig,
    ) -> Option<Selection> {
        let anchor = self.resolve(tx, &relative.anchor)?;
        let head = self.resolve(tx, &relative.head)?;
        Some(match &relative.kind {
            None => Selection::Text { anchor, head },
            Some(BoundaryKind::Node) => Selection::Node { before: anchor },
            Some(BoundaryKind::Gap) => Selection::Gap { at: anchor },
            Some(BoundaryKind::Plugin(name)) => Selection::Custom {
                kind: config.selection_behavior(name)?,
                anchor,
                head,
            },
            Some(BoundaryKind::NodeRange) => Selection::NodeRange { anchor, head },
        })
    }
}
