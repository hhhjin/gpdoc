use crate::model::{Error, Node, Schema, Slice};
use crate::transform::{Mappable, StepMap};

/// An atomic replacement. Open ends of the slice must join the document at `from` and `to`.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplaceStep {
    pub from: usize,
    pub to: usize,
    pub slice: Slice,
    pub structure: bool,
}

impl ReplaceStep {
    pub(super) fn check_ranges(&self) -> Result<(), Error> {
        if self.from > self.to || self.from.checked_add(self.slice.size()).is_none() {
            return Err(Error("Invalid replacement range".into()));
        }
        Ok(())
    }

    pub fn apply(&self, schema: &Schema, doc: &Node) -> Result<Node, Error> {
        self.check_ranges()?;
        if self.structure && content_between(doc, self.from, self.to)? {
            return Err(Error("Structure replace would overwrite content".into()));
        }
        doc.replace(schema, self.from, self.to, &self.slice)
    }

    pub fn get_map(&self) -> StepMap {
        StepMap::new(vec![[self.from, self.to - self.from, self.slice.size()]])
    }

    pub fn invert(&self, doc: &Node) -> Result<Self, Error> {
        self.check_ranges()?;
        Ok(Self {
            from: self.from,
            to: self.from + self.slice.size(),
            slice: doc.slice(self.from, self.to, false)?,
            structure: false,
        })
    }

    pub fn map(&self, mapping: &impl Mappable) -> Option<Self> {
        self.map_with_bias(mapping, 1)
    }

    /// A negative insertion bias keeps rebased insertions before an insertion at the same point.
    pub fn map_with_bias(&self, mapping: &impl Mappable, bias: i8) -> Option<Self> {
        let to = mapping.map_result(self.to, -1);
        let from = if self.from == self.to && bias < 0 {
            to
        } else {
            mapping.map_result(self.from, 1)
        };
        if from.deleted_across() && to.deleted_across() {
            return None;
        }
        Some(Self {
            from: from.pos,
            to: from.pos.max(to.pos),
            ..self.clone()
        })
    }

    pub fn merge(&self, other: &Self) -> Option<Self> {
        if self.structure || other.structure {
            return None;
        }
        if self.from.checked_add(self.slice.size())? == other.from
            && self.slice.open_end == 0
            && other.slice.open_start == 0
        {
            Some(Self {
                from: self.from,
                to: self.to.checked_add(other.to.checked_sub(other.from)?)?,
                slice: append_slices(&self.slice, &other.slice),
                structure: false,
            })
        } else if other.to == self.from && self.slice.open_start == 0 && other.slice.open_end == 0 {
            Some(Self {
                from: other.from,
                to: self.to,
                slice: append_slices(&other.slice, &self.slice),
                structure: false,
            })
        } else {
            None
        }
    }
}

fn append_slices(left: &Slice, right: &Slice) -> Slice {
    if left.size() == 0 && right.size() == 0 {
        Slice::empty()
    } else {
        Slice::new(
            left.content.append(&right.content),
            left.open_start,
            right.open_end,
        )
    }
}

/// A replacement that moves `gap_from..gap_to` from the old document into the slice.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplaceAroundStep {
    pub from: usize,
    pub to: usize,
    pub gap_from: usize,
    pub gap_to: usize,
    pub slice: Slice,
    pub insert: usize,
    pub structure: bool,
}

impl ReplaceAroundStep {
    pub(super) fn check_ranges(&self) -> Result<(), Error> {
        if self.from > self.gap_from
            || self.gap_from > self.gap_to
            || self.gap_to > self.to
            || self.insert > self.slice.size()
            || self
                .from
                .checked_add(self.slice.size())
                .and_then(|end| end.checked_add(self.gap_to - self.gap_from))
                .is_none()
        {
            return Err(Error("Invalid ranges in replace-around step".into()));
        }
        Ok(())
    }

    pub fn apply(&self, schema: &Schema, doc: &Node) -> Result<Node, Error> {
        self.check_ranges()?;
        if self.structure
            && (content_between(doc, self.from, self.gap_from)?
                || content_between(doc, self.gap_to, self.to)?)
        {
            return Err(Error(
                "Structure gap-replace would overwrite content".into(),
            ));
        }
        let gap = doc.slice(self.gap_from, self.gap_to, false)?;
        if gap.open_start != 0 || gap.open_end != 0 {
            return Err(Error("Gap is not a flat range".into()));
        }
        let inserted = self
            .slice
            .insert_at(schema, self.insert, &gap.content)?
            .ok_or_else(|| Error("Content does not fit in gap".into()))?;
        doc.replace(schema, self.from, self.to, &inserted)
    }

    pub fn get_map(&self) -> StepMap {
        StepMap::new(vec![
            [self.from, self.gap_from - self.from, self.insert],
            [
                self.gap_to,
                self.to - self.gap_to,
                self.slice.size() - self.insert,
            ],
        ])
    }

    pub fn invert(&self, doc: &Node) -> Result<Self, Error> {
        self.check_ranges()?;
        let gap = self.gap_to - self.gap_from;
        let slice = doc
            .slice(self.from, self.to, false)?
            .remove_between(self.gap_from - self.from, self.gap_to - self.from)?;
        Ok(Self {
            from: self.from,
            to: self.from + self.slice.size() + gap,
            gap_from: self.from + self.insert,
            gap_to: self.from + self.insert + gap,
            slice,
            insert: self.gap_from - self.from,
            structure: self.structure,
        })
    }

    pub fn map(&self, mapping: &impl Mappable) -> Option<Self> {
        let from = mapping.map_result(self.from, 1);
        let to = mapping.map_result(self.to, -1);
        let gap_from = if self.from == self.gap_from {
            from.pos
        } else {
            mapping.map(self.gap_from, -1)
        };
        let gap_to = if self.to == self.gap_to {
            to.pos
        } else {
            mapping.map(self.gap_to, 1)
        };
        if (from.deleted_across() && to.deleted_across())
            || gap_from < from.pos
            || gap_to > to.pos
            || gap_from > gap_to
        {
            return None;
        }
        Some(Self {
            from: from.pos,
            to: to.pos,
            gap_from,
            gap_to,
            ..self.clone()
        })
    }
}

fn content_between(doc: &Node, from: usize, to: usize) -> Result<bool, Error> {
    let start = doc.resolve(from)?;
    let mut distance = to - from;
    let mut depth = start.depth();
    while distance > 0 && depth > 0 && start.index_after(depth) == start.node(depth).child_count() {
        depth -= 1;
        distance -= 1;
    }
    let mut next = start
        .node(depth)
        .content
        .maybe_child(start.index_after(depth));
    while distance > 0 {
        match next {
            Some(node) if !node.is_leaf() => next = node.content.maybe_child(0),
            _ => return Ok(true),
        }
        distance -= 1;
    }
    Ok(false)
}
