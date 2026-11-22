use crate::model::DocPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappedPosition {
    pub position: DocPos,
    /// The content on the requested side of the position was removed.
    pub deleted: bool,
}

/// Composes the position maps produced by successive edits.
#[derive(Clone, Debug, Default)]
pub struct PositionMap(crate::transform::Mapping);

impl PositionMap {
    pub fn is_identity(&self) -> bool {
        self.0.maps().is_empty()
    }

    pub(super) fn from_step_map(map: crate::transform::StepMap) -> Self {
        if map
            .changes()
            .all(|(before, after)| before.is_empty() && after.is_empty())
        {
            return Self::default();
        }
        let mut mapping = crate::transform::Mapping::default();
        mapping.append_map(map, None);
        Self(mapping)
    }

    pub(super) fn record(&mut self, start: usize, removed: usize, inserted: usize) {
        if removed != 0 || inserted != 0 {
            let map = crate::transform::StepMap::new(vec![[start, removed, inserted]]);
            self.0.append_map(map, None);
        }
    }

    pub fn map(&self, pos: DocPos, affinity: Affinity) -> MappedPosition {
        let assoc = match affinity {
            Affinity::Before => -1,
            Affinity::After => 1,
        };
        let result = self.0.map_result(pos.0, assoc);
        MappedPosition {
            position: DocPos(result.pos),
            deleted: result.deleted(),
        }
    }

    /// The changed ranges for each step, in the coordinates of that step.
    pub fn changes(
        &self,
    ) -> impl Iterator<Item = (std::ops::Range<DocPos>, std::ops::Range<DocPos>)> + '_ {
        self.0
            .maps()
            .iter()
            .flat_map(|map| map.changes())
            .map(|(before, after)| {
                (
                    DocPos(before.start)..DocPos(before.end),
                    DocPos(after.start)..DocPos(after.end),
                )
            })
    }

    pub fn append(&mut self, next: PositionMap) {
        self.0.append_mapping(&next.0);
    }

    pub fn inverse(&self) -> Self {
        Self(self.0.invert())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_edges_follow_the_original_gap_and_report_the_deleted_side() {
        let mut map = PositionMap::default();
        map.record(2, 3, 1);
        let at = |p, affinity| {
            let r = map.map(DocPos(p), affinity);
            (r.position.0, r.deleted)
        };
        assert_eq!(at(2, Affinity::Before), (2, false));
        assert_eq!(at(2, Affinity::After), (2, true));
        assert_eq!(at(3, Affinity::After), (3, true));
        assert_eq!(at(5, Affinity::After), (3, false));
        assert_eq!(at(5, Affinity::Before), (3, true));
    }

    #[test]
    fn later_steps_use_updated_coordinates_and_inverse_reverses_the_order() {
        let mut map = PositionMap::default();
        map.record(2, 0, 3);
        let mut next = PositionMap::default();
        next.record(7, 2, 0);
        map.append(next);
        assert_eq!(map.map(DocPos(2), Affinity::Before).position, DocPos(2));
        assert_eq!(map.map(DocPos(2), Affinity::After).position, DocPos(5));
        for p in [0, 1, 2, 3, 7, 8, 9] {
            for affinity in [Affinity::Before, Affinity::After] {
                let mapped = map.map(DocPos(p), affinity);
                assert_eq!(
                    map.inverse().map(mapped.position, affinity).position,
                    DocPos(p)
                );
            }
        }
    }
}
