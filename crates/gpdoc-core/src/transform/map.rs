use std::{ops::Range, sync::Arc};

const DEL_BEFORE: u8 = 1;
const DEL_AFTER: u8 = 2;
const DEL_ACROSS: u8 = 4;
const DEL_SIDE: u8 = 8;

/// A pre-edit position mapped into the post-edit document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapResult {
    pub pos: usize,
    deletion: u8,
    recover: Option<Recovery>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Recovery {
    range: usize,
    offset: usize,
}

impl MapResult {
    pub fn deleted(self) -> bool {
        self.deletion & DEL_SIDE != 0
    }
    pub fn deleted_before(self) -> bool {
        self.deletion & (DEL_BEFORE | DEL_ACROSS) != 0
    }
    pub fn deleted_after(self) -> bool {
        self.deletion & (DEL_AFTER | DEL_ACROSS) != 0
    }
    pub fn deleted_across(self) -> bool {
        self.deletion & DEL_ACROSS != 0
    }
}

/// `assoc` is negative for left affinity and positive for right affinity.
pub trait Mappable {
    fn map_result(&self, pos: usize, assoc: i8) -> MapResult;
    fn map(&self, pos: usize, assoc: i8) -> usize {
        self.map_result(pos, assoc).pos
    }
}

/// Each triple is `[start in the original document, removed size, inserted size]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepMap {
    ranges: Arc<[[usize; 3]]>,
    inverted: bool,
}

impl StepMap {
    /// The changed ranges must be ordered and disjoint in document coordinates.
    pub fn new(ranges: Vec<[usize; 3]>) -> Self {
        Self {
            ranges: ranges.into(),
            inverted: false,
        }
    }

    pub fn offset(n: isize) -> Self {
        match n.cmp(&0) {
            std::cmp::Ordering::Equal => Self::default(),
            std::cmp::Ordering::Less => Self::new(vec![[0, n.unsigned_abs(), 0]]),
            std::cmp::Ordering::Greater => Self::new(vec![[0, 0, n as usize]]),
        }
    }

    pub fn invert(&self) -> Self {
        Self {
            ranges: self.ranges.clone(),
            inverted: !self.inverted,
        }
    }

    /// Visits changed ranges paired with their positions in the resulting document.
    pub fn changes(&self) -> impl Iterator<Item = (Range<usize>, Range<usize>)> + '_ {
        let mut old_end = 0;
        let mut new_end = 0;
        self.ranges.iter().map(move |&[start, removed, inserted]| {
            let new_start = new_end + (start - old_end);
            old_end = start + removed;
            new_end = new_start + inserted;
            let old = start..old_end;
            let new = new_start..new_end;
            if self.inverted {
                (new, old)
            } else {
                (old, new)
            }
        })
    }

    fn recover(&self, recovery: Recovery) -> usize {
        let (_, new) = self
            .changes()
            .nth(recovery.range)
            .expect("mirror has corresponding ranges");
        new.start + recovery.offset
    }

    pub fn map_result(&self, pos: usize, assoc: i8) -> MapResult {
        let mut old_end = 0;
        let mut new_end = 0;
        for (i, (old, new)) in self.changes().enumerate() {
            if pos < old.start {
                break;
            }
            if pos <= old.end {
                let left = if old.is_empty() {
                    assoc < 0
                } else if pos == old.start {
                    true
                } else if pos == old.end {
                    false
                } else {
                    assoc < 0
                };
                let side_deleted = if assoc < 0 {
                    pos != old.start
                } else {
                    pos != old.end
                };
                let deletion = if old.is_empty() {
                    0
                } else {
                    let edge = if pos == old.start {
                        DEL_AFTER
                    } else if pos == old.end {
                        DEL_BEFORE
                    } else {
                        DEL_ACROSS
                    };
                    edge | if side_deleted { DEL_SIDE } else { 0 }
                };
                return MapResult {
                    pos: if left { new.start } else { new.end },
                    deletion,
                    recover: side_deleted.then_some(Recovery {
                        range: i,
                        offset: pos - old.start,
                    }),
                };
            }
            old_end = old.end;
            new_end = new.end;
        }
        MapResult {
            pos: new_end.saturating_add(pos - old_end),
            deletion: 0,
            recover: None,
        }
    }

    pub fn map(&self, pos: usize, assoc: i8) -> usize {
        self.map_result(pos, assoc).pos
    }
}

impl Mappable for StepMap {
    fn map_result(&self, pos: usize, assoc: i8) -> MapResult {
        StepMap::map_result(self, pos, assoc)
    }
}

/// A series of step maps. A mirrored pair restores positions inside reinserted content.
#[derive(Clone, Debug, Default)]
pub struct Mapping {
    maps: Arc<Vec<StepMap>>,
    mirrors: Arc<Vec<(usize, usize)>>,
    from: usize,
    to: usize,
}

impl Mapping {
    pub fn new(maps: Vec<StepMap>) -> Self {
        Self {
            to: maps.len(),
            maps: Arc::new(maps),
            ..Self::default()
        }
    }

    pub fn maps(&self) -> &[StepMap] {
        &self.maps
    }

    pub fn slice(&self, from: usize, to: usize) -> Self {
        assert!(from <= to && to <= self.maps.len());
        Self {
            from,
            to,
            ..self.clone()
        }
    }

    pub fn append_map(&mut self, map: StepMap, mirrors: Option<usize>) {
        let index = self.maps.len();
        Arc::make_mut(&mut self.maps).push(map);
        self.to = self.maps.len();
        if let Some(mirror) = mirrors {
            Arc::make_mut(&mut self.mirrors).push((index, mirror));
        }
    }

    pub fn get_mirror(&self, index: usize) -> Option<usize> {
        self.mirrors.iter().find_map(|&(a, b)| {
            if a == index {
                Some(b)
            } else if b == index {
                Some(a)
            } else {
                None
            }
        })
    }

    pub fn append_mapping(&mut self, other: &Self) {
        let start = self.maps.len();
        for (i, map) in other.maps.iter().enumerate() {
            let mirror = other.get_mirror(i).filter(|&m| m < i).map(|m| start + m);
            self.append_map(map.clone(), mirror);
        }
    }

    pub fn append_mapping_inverted(&mut self, other: &Self) {
        let end = self.maps.len() + other.maps.len();
        for (i, map) in other.maps.iter().enumerate().rev() {
            let mirror = other.get_mirror(i).filter(|&m| m > i).map(|m| end - 1 - m);
            self.append_map(map.invert(), mirror);
        }
    }

    pub fn invert(&self) -> Self {
        let mut result = Self::default();
        result.append_mapping_inverted(self);
        result
    }

    pub fn map_result(&self, mut pos: usize, assoc: i8) -> MapResult {
        let mut i = self.from;
        let mut deletion = 0;
        while i < self.to {
            let result = self.maps[i].map_result(pos, assoc);
            if let Some(recover) = result.recover
                && let Some(mirror) = self.get_mirror(i)
                && mirror > i
                && mirror < self.to
            {
                pos = self.maps[mirror].recover(recover);
                i = mirror + 1;
                continue;
            }
            pos = result.pos;
            deletion |= result.deletion;
            i += 1;
        }
        MapResult {
            pos,
            deletion,
            recover: None,
        }
    }

    pub fn map(&self, pos: usize, assoc: i8) -> usize {
        self.map_result(pos, assoc).pos
    }
}

impl Mappable for Mapping {
    fn map_result(&self, pos: usize, assoc: i8) -> MapResult {
        Mapping::map_result(self, pos, assoc)
    }
}
