//! A fragment indexes children, while an XmlText indexes UTF-16 code units.
use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::prelude::{DocPos, Document, Schema, Selection};
use serde::{Deserialize, Serialize};
use y_octo::{DocSession, RelativePosition, SessionReadValue, TypeTarget};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelativeSelection {
    pub anchor: RelativePosition,
    pub head: RelativePosition,
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
    xml: TypeTarget,
    positions: Offsets,
}

pub(crate) struct Positions(Vec<Entry>);

impl Positions {
    pub fn new(doc: &DocSession, root: &TypeTarget, schema: &Schema) -> Option<Self> {
        let mut out = Self(Vec::new());
        out.visit(doc, root, 0, schema, true)?;
        Some(out)
    }

    fn visit(
        &mut self,
        doc: &DocSession,
        parent: &TypeTarget,
        pos: usize,
        schema: &Schema,
        root: bool,
    ) -> Option<usize> {
        let mut at = pos + usize::from(!root);
        let mut boundaries = vec![at];
        for child in doc.read_array(parent).ok()? {
            match child {
                SessionReadValue::XmlText(id) => {
                    let xml = id.into();
                    let len = doc.read_text_len(&xml).ok()? as usize;
                    self.0.push(Entry {
                        xml,
                        positions: Offsets::Text { start: at, len },
                    });
                    at += len;
                }
                SessionReadValue::XmlElement { tag, identity } => {
                    if schema.node_type(&tag).is_some_and(|ty| ty.is_leaf()) {
                        at += 1;
                    } else {
                        at = self.visit(doc, &identity.into(), at, schema, false)?;
                    }
                }
                _ => return None,
            }
            boundaries.push(at);
        }
        self.0.push(Entry {
            xml: parent.clone(),
            positions: Offsets::Children(boundaries),
        });
        Some(at + usize::from(!root))
    }

    fn pin(&self, tx: &DocSession, pos: DocPos, node: bool) -> Option<RelativePosition> {
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
            } as u64;
            return tx
                .create_relative_position(&entry.xml, index, if node { 0 } else { -1 })
                .ok();
        }
        None
    }

    fn resolve(&self, tx: &y_octo::Doc, pin: &RelativePosition) -> Option<DocPos> {
        let at = tx.resolve_relative_position(pin, true)?;
        let target = TypeTarget::from(at.r#type);
        let entry = self.0.iter().find(|entry| entry.xml == target)?;
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
        tx: &DocSession,
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
        tx: &y_octo::Doc,
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
