mod map;
mod splice;

use super::TransformError as Error;
use crate::model::{Attrs, DocPos, Document, Fragment, Mark, Node, ResolvedPos, Schema, Slice};
use crate::transform::{
    AddMarkStep, AttrStep, RemoveMarkStep, ReplaceAroundStep, ReplaceStep, Step,
};
pub use map::{Affinity, MappedPosition, PositionMap};
use serde_json::Value;
use std::ops::Range;

// Replace structural tokens while retaining the content between edits (e.g. list commands).
#[derive(Clone, Debug)]
pub enum Boundary {
    Open { kind: String, attrs: Attrs },
    Close,
}
#[derive(Clone, Debug)]
pub struct BoundaryEdit {
    pub range: Range<DocPos>,
    pub insert: Vec<Boundary>,
}

/// A document edit whose ranges address the document supplied at application time.
#[derive(Clone, Debug)]
pub enum Edit {
    Atomic(Step),
    ReplaceInline {
        range: Range<DocPos>,
        nodes: Vec<Node>,
    },
    ReplaceText {
        range: Range<DocPos>,
        text: String,
    },
    ReplaceNodes {
        range: Range<DocPos>,
        nodes: Vec<Node>,
    },
    Split {
        at: DocPos,
    },
    Join {
        at: DocPos,
    }, // boundary before the right sibling
    Wrap {
        range: Range<DocPos>,
        kind: String,
        attrs: Attrs,
    },
    Unwrap {
        before: DocPos,
    },
    SetBlock {
        before: DocPos,
        kind: String,
        attrs: Attrs,
    },
    SetMark {
        range: Range<DocPos>,
        mark: Mark,
    },
    RemoveMark {
        range: Range<DocPos>,
        kind: String,
    },
    RemoveExactMark {
        range: Range<DocPos>,
        mark: Mark,
    },
    Boundaries(Vec<BoundaryEdit>), // ascending, disjoint ranges in the input
}

impl Step {
    /// Fit the supplied slice at the requested range, including its open boundaries.
    pub fn fit_replace(
        schema: &Schema,
        doc: &Node,
        from: usize,
        to: usize,
        slice: &Slice,
    ) -> Result<Option<Self>, Error> {
        Ok(crate::transform::replace_step(
            schema, doc, from, to, slice,
        )?)
    }

    pub fn set_node_attribute(before: DocPos, attr: impl Into<String>, value: Value) -> Self {
        Self::Attr(AttrStep {
            pos: before.0,
            attr: attr.into(),
            value,
        })
    }

    /// Apply one edit for inspection without updating an editor's state or history.
    pub fn preview(&self, doc: &Document) -> Result<(Document, PositionMap), Error> {
        let schema = doc.schema();
        let root = self.apply(schema, doc.root())?;
        let document = schema.with_root(Some(doc), root)?;
        Ok((document, PositionMap::from_step_map(self.get_map())))
    }
}

impl From<Step> for Edit {
    fn from(step: Step) -> Self {
        Self::Atomic(step)
    }
}

impl Edit {
    pub fn preview(&self, doc: &Document) -> Result<(Document, PositionMap), Error> {
        apply_edit(doc, self)
    }
}

pub fn apply_edit(doc: &Document, edit: &Edit) -> Result<(Document, PositionMap), Error> {
    let schema = doc.schema();
    match edit {
        Edit::Atomic(step) => step.preview(doc),
        Edit::ReplaceText { range, text } => replace_text(doc, range, text, None),
        Edit::ReplaceNodes { range, nodes } => {
            sibling_position(doc, range)?;
            let content = schema.bind_nodes(nodes)?;
            replace_slice(doc, range.clone(), Slice::new(content, 0, 0))
        }
        Edit::ReplaceInline { range, nodes } => {
            let range = resolve_text_range(doc, range)?;
            let content = schema.bind_nodes(nodes)?;
            if content.children().iter().any(|n| !n.is_inline()) {
                return Err(Error::InvalidEdit(
                    "inline replacement needs inline nodes".into(),
                ));
            }
            replace_inline(doc, range, content)
        }
        Edit::Split { at } => split(doc, *at)?.preview(doc),
        Edit::Join { at } => join(doc, *at)?.preview(doc),
        Edit::Wrap { range, kind, attrs } => wrap(doc, range.clone(), kind, attrs)?.preview(doc),
        Edit::Unwrap { before } => {
            let node = doc
                .node_at(*before)
                .ok_or(Error::InvalidPosition(*before))?;
            if node.is_leaf() || node.inline_content() {
                return Err(Error::InvalidEdit("unwrapping requires a container".into()));
            }
            let end = before.0 + node.size();
            let step = ReplaceAroundStep {
                from: before.0,
                to: end,
                gap_from: before.0 + 1,
                gap_to: end - 1,
                slice: Slice::empty(),
                insert: 0,
                structure: true,
            };
            Step::ReplaceAround(step).preview(doc)
        }
        Edit::SetBlock {
            before,
            kind,
            attrs,
        } => {
            let target = doc
                .root()
                .node_at(before.0)
                .map_err(|_| Error::InvalidPosition(*before))?
                .filter(|node| !node.is_text())
                .ok_or(Error::InvalidPosition(*before))?;
            let end = before.0 + target.size();
            let shell = node_shell(schema, kind, attrs)?.mark(target.marks().to_vec());
            if target.is_leaf() != shell.is_leaf() || target.is_inline() != shell.is_inline() {
                return Err(Error::InvalidEdit(
                    "changing markup must preserve node boundaries".into(),
                ));
            }
            let slice = Slice::new(Fragment::from(vec![shell]), 0, 0);
            let (document, _) = if target.is_leaf() {
                replace_slice(doc, *before..DocPos(end), slice)?
            } else {
                let step = ReplaceAroundStep {
                    from: before.0,
                    to: end,
                    gap_from: before.0 + 1,
                    gap_to: end - 1,
                    slice,
                    insert: 1,
                    structure: true,
                };
                Step::ReplaceAround(step).preview(doc)?
            };
            // Changing markup preserves positions and a selection on this node.
            Ok((document, PositionMap::default()))
        }
        Edit::SetMark { range, mark } => {
            resolve_text_range(doc, range)?;
            let step = AddMarkStep {
                from: range.start.0,
                to: range.end.0,
                mark: schema.bind_mark(mark)?,
            };
            Step::AddMark(step).preview(doc)
        }
        Edit::RemoveExactMark { range, mark } => {
            let step = RemoveMarkStep {
                from: range.start.0,
                to: range.end.0,
                mark: schema.bind_mark(mark)?,
            };
            Step::RemoveMark(step).preview(doc)
        }
        Edit::RemoveMark { range, kind } => {
            resolve_text_range(doc, range)?;
            if schema.mark_type(kind).is_none() {
                return Err(Error::InvalidDocument(format!("unknown mark '{kind}'")));
            }
            // RemoveMarkStep addresses one mark value. A selection may contain links
            // with different attributes, so collect the values from the original document.
            let mut removals = Vec::new();
            for (span, node) in doc.inline_nodes_in(range.clone()) {
                if let Some(mark) = node.marks().iter().find(|m| m.kind == *kind) {
                    let step = Step::RemoveMark(RemoveMarkStep {
                        from: span.start.max(range.start).0,
                        to: span.end.min(range.end).0,
                        mark: mark.clone(),
                    });
                    if let Some(merged) = removals.last().and_then(|last: &Step| last.merge(&step))
                    {
                        *removals.last_mut().unwrap() = merged;
                    } else {
                        removals.push(step);
                    }
                }
            }
            Step::mark_set(removals)?.preview(doc)
        }
        Edit::Boundaries(edits) => splice::apply_boundaries(doc, edits),
    }
}

/// Split the text block at a valid UTF-16 text position.
pub fn split(doc: &Document, at: DocPos) -> Result<Step, Error> {
    let (pos, _) = doc.resolve_text(at)?;
    let shell = pos.parent().copy(Fragment::default());
    Ok(Step::Replace(ReplaceStep {
        from: at.0,
        to: at.0,
        slice: Slice::new(Fragment::from(vec![shell.clone(), shell]), 1, 1),
        structure: false,
    }))
}

/// Join siblings at the boundary before the right node.
pub fn join(doc: &Document, at: DocPos) -> Result<Step, Error> {
    let right = doc.node_at(at).ok_or(Error::InvalidPosition(at))?;
    let pos = sibling_position(doc, &(at..DocPos(at.0 + right.size())))?;
    let index = pos.index(pos.depth());
    let left = index
        .checked_sub(1)
        .map(|i| pos.parent().child(i))
        .ok_or_else(|| Error::InvalidEdit("no left sibling to join".into()))?;
    if left.is_leaf() || right.is_leaf() {
        return Err(Error::InvalidEdit(
            "cannot join text content with block content".into(),
        ));
    }

    Ok(Step::Replace(ReplaceStep {
        from: at.0 - 1,
        to: at.0 + 1,
        slice: Slice::empty(),
        structure: false,
    }))
}

/// Wrap a non-empty range of complete siblings in a container.
pub fn wrap(
    doc: &Document,
    range: Range<DocPos>,
    kind: &str,
    attrs: &Attrs,
) -> Result<Step, Error> {
    sibling_position(doc, &range)?;
    if range.is_empty() {
        return Err(Error::InvalidEdit(
            "wrapping requires at least one block".into(),
        ));
    }
    let shell = node_shell(doc.schema(), kind, attrs)?;
    let step = ReplaceAroundStep {
        from: range.start.0,
        to: range.end.0,
        gap_from: range.start.0,
        gap_to: range.end.0,
        slice: Slice::new(Fragment::from(vec![shell]), 0, 0),
        insert: 1,
        structure: true,
    };
    Ok(Step::ReplaceAround(step))
}

fn sibling_position(doc: &Document, range: &Range<DocPos>) -> Result<ResolvedPos, Error> {
    let from = doc.resolve(range.start)?;
    let to = doc.resolve(range.end)?;
    if range.start > range.end
        || !from.same_parent(&to)
        || from.text_offset() != 0
        || to.text_offset() != 0
    {
        return Err(Error::InvalidEdit(
            "range must span complete siblings".into(),
        ));
    }
    Ok(from)
}

pub fn replace_text(
    doc: &Document,
    range: &Range<DocPos>,
    text: &str,
    marks: Option<&[Mark]>,
) -> Result<(Document, PositionMap), Error> {
    let schema = doc.schema();
    let range = resolve_text_range(doc, range)?;
    let content = if text.is_empty() {
        Fragment::default()
    } else {
        let marks = marks.map_or_else(|| range.start.marks(schema), <[_]>::to_vec);
        Fragment::from(vec![schema.text(text, marks)?])
    };
    replace_inline(doc, range, content)
}

fn resolve_text_range(doc: &Document, range: &Range<DocPos>) -> Result<Range<ResolvedPos>, Error> {
    if range.start > range.end {
        return Err(Error::InvalidEdit("reversed edit range".into()));
    }
    let (start, _) = doc.resolve_text(range.start)?;
    let end = if range.is_empty() {
        start.clone()
    } else {
        doc.resolve_text(range.end)?.0
    };
    Ok(start..end)
}

fn node_shell(schema: &Schema, kind: &str, attrs: &Attrs) -> Result<Node, Error> {
    let attrs = schema.node_attributes(kind, attrs)?;
    // Required children will come from the gap in the replace-around step.
    Ok(schema.node_unchecked(kind, attrs, Fragment::default(), vec![])?)
}

fn replace_slice(
    doc: &Document,
    range: Range<DocPos>,
    slice: Slice,
) -> Result<(Document, PositionMap), Error> {
    let step = ReplaceStep {
        from: range.start.0,
        to: range.end.0,
        slice,
        structure: false,
    };
    Step::Replace(step).preview(doc)
}

fn replace_inline(
    doc: &Document,
    range: Range<ResolvedPos>,
    content: Fragment,
) -> Result<(Document, PositionMap), Error> {
    let slice = Slice::new(content, 0, 0);
    let from = range.start.pos;
    let to = range.end.pos;
    if from == to && slice.size() == 0 {
        return Ok((doc.clone(), PositionMap::default()));
    }
    // Plugin inline edits inside a single text block must satisfy that block's
    // rules. Fitting here could move a rejected inline object into a new paragraph.
    if range.start.same_parent(&range.end) {
        return replace_slice(doc, DocPos(from)..DocPos(to), slice);
    }
    let step = crate::transform::replace_step(doc.schema(), doc.root(), from, to, &slice)?
        .ok_or_else(|| Error::InvalidEdit("inline content does not fit at this position".into()))?;
    step.preview(doc)
}

#[cfg(test)]
mod tests;
