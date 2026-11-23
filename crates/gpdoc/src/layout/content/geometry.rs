use super::*;

/// All sections use offsets in the same projected UTF-16 string.
pub(crate) struct TextLayout {
    pub(crate) bounds: Bounds<Pixels>,
    pub(in super::super) sections: Vec<Rc<TextSection>>,
    pub(in super::super) children: Vec<ChildBounds>,
}

impl TextLayout {
    pub(crate) fn url_at(
        &self,
        point: Point<Pixels>,
        formats: &[FormatSpan],
        base: Option<&url::Url>,
    ) -> Option<url::Url> {
        let mut start = 0;
        for span in formats {
            let range = start..span.utf16_end;
            start = span.utf16_end;
            if let Some(source) = span
                .format
                .media_source
                .as_ref()
                .or(span.format.link_href.as_ref())
                && self.rects(range).any(|r| r.contains(&point))
            {
                return crate::layout::activation_url(
                    source,
                    base,
                    span.format.link_href.is_some(),
                );
            }
        }
        None
    }
    fn section_index(&self, offset: usize) -> Option<usize> {
        self.sections
            .partition_point(|s| s.rows[0].start <= offset)
            .checked_sub(1)
    }

    fn section(&self, offset: usize) -> Option<&TextSection> {
        Some(&self.sections[self.section_index(offset)?])
    }

    pub(crate) fn slot_bounds(&self, offset: usize) -> Option<Bounds<Pixels>> {
        Some(self.section(offset)?.bounds)
    }

    pub(crate) fn bounds_for_range(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        self.section(range.start)?.bounds_for_range(range)
    }

    pub(crate) fn caret_bounds(&self, caret: Caret) -> Option<Bounds<Pixels>> {
        self.section(caret.offset)?.caret_bounds(caret)
    }

    pub(crate) fn line_edge(&self, head: Caret, right: bool) -> Option<Caret> {
        let section = self.section(head.offset)?;
        Some(section.rows[section.row_for_caret(head)?].edge(right))
    }

    /// First editable section strictly beyond a boundary, and its facing text position.
    pub(crate) fn section_beyond(
        &self,
        boundary: DocPos,
        down: bool,
        buffer: &InputBuffer,
    ) -> Option<(usize, DocPos)> {
        let edge = |section: &TextSection| {
            let offset = if down {
                section.rows.first()?.start
            } else {
                section.rows.last()?.end()
            };
            buffer.position(offset).ok()
        };
        let gap = self.sections.partition_point(|s| {
            edge(s).is_some_and(|pos| {
                if down {
                    pos <= boundary
                } else {
                    pos < boundary
                }
            })
        });
        let index = if down { gap } else { gap.checked_sub(1)? };
        Some((index, edge(self.sections.get(index)?)?))
    }

    pub(crate) fn vertical_destination(
        &self,
        head: Caret,
        preferred_x: Option<Pixels>,
        down: bool,
        redirect: impl FnOnce(Option<usize>) -> Option<usize>,
    ) -> Option<(Caret, Pixels)> {
        let section_index = self.section_index(head.offset)?;
        let current = self.sections[section_index].as_ref();
        let index = current.row_for_caret(head)?;
        let row = &current.rows[index];
        let x = preferred_x.unwrap_or_else(|| current.bounds.left() + row.x(head));
        let adjacent = if down {
            (index + 1 < current.rows.len()).then_some(index + 1)
        } else {
            index.checked_sub(1)
        };
        let (section, index) = if let Some(index) = adjacent {
            (current, index)
        } else {
            let next = if down {
                (section_index + 1 < self.sections.len()).then_some(section_index + 1)
            } else {
                section_index.checked_sub(1)
            };
            let Some(next) = redirect(next) else {
                let edge = if down {
                    Caret::trailing(row.end())
                } else {
                    Caret::leading(row.start)
                };
                return Some((edge, x));
            };
            let section = self.sections[next].as_ref();
            (section, if down { 0 } else { section.rows.len() - 1 })
        };
        Some((section.rows[index].nearest(x - section.bounds.left()), x))
    }

    pub(crate) fn rects(&self, range: Range<usize>) -> impl Iterator<Item = Bounds<Pixels>> + '_ {
        let first = self
            .sections
            .partition_point(|section| section.rows[0].start <= range.start)
            .saturating_sub(1);
        let end = range.end;
        self.sections[first..]
            .iter()
            .take_while(move |section| section.rows[0].start < end)
            .flat_map(move |s| s.rects(range.clone()))
    }

    pub(crate) fn index_for_point(&self, at: Point<Pixels>) -> usize {
        // Empty structural boxes have no editable caret. Use the nearest rendered
        // section, while an empty paragraph has a real (zero-length) text span.
        let sections = match self
            .child_at(at)
            .and_then(|child| child.slot.offsets.as_ref())
        {
            Some(offsets) => {
                let first = self
                    .sections
                    .partition_point(|s| s.rows[0].start < offsets.start);
                let end = self
                    .sections
                    .partition_point(|s| s.rows[0].start <= offsets.end);
                &self.sections[first..end]
            }
            None => &self.sections[..],
        };
        sections
            .iter()
            .min_by_key(|s| {
                let b = s.bounds;
                (
                    (at.y - at.y.clamp(b.top(), b.bottom())).abs(),
                    (at.x - at.x.clamp(b.left(), b.right())).abs(),
                )
            })
            .map_or(0, |section| section.index_for_point(at))
    }

    fn child_at(&self, at: Point<Pixels>) -> Option<&ChildBounds> {
        self.children
            .iter()
            .filter(|child| child.bounds.contains(&at) && child.clip.contains(&at))
            .max_by_key(|child| child.prepaint_order)
    }

    pub(crate) fn child_bounds_at(&self, at: Point<Pixels>) -> Option<(DocPos, Bounds<Pixels>)> {
        self.child_at(at)
            .map(|child| (child.slot.before, child.bounds))
    }

    pub(crate) fn child_bounds(&self, before: DocPos) -> Option<Bounds<Pixels>> {
        Some(self.child(before)?.bounds)
    }

    pub(crate) fn child_content_top(&self, before: DocPos) -> Option<Pixels> {
        Some(self.child(before)?.content_top)
    }

    fn child(&self, before: DocPos) -> Option<&ChildBounds> {
        let index = self
            .children
            .binary_search_by_key(&before, |child| child.slot.before)
            .ok()?;
        self.children.get(index)
    }

    /// Shape native input at each slot's last GPUI allocation. GPUI places the
    /// slots again next frame, including those inside fixed-height or grid layouts.
    pub(crate) fn refresh(
        &mut self,
        buffer: &InputBuffer,
        views: &BlockViews,
        window: &mut Window,
    ) -> bool {
        let mut spans = Vec::new();
        let mut slots = Vec::new();
        for part in Part::document(buffer, views) {
            part.geometry(&mut spans, &mut slots);
        }
        if spans.len() != self.sections.len() {
            return false;
        }
        for child in &mut self.children {
            let Some(slot) = slots.get(child.slot.index) else {
                return false;
            };
            if slot.node.kind != child.slot.node.kind {
                return false;
            }
            child.slot = slot.clone();
        }
        let source = Rc::new(TextSource::read(buffer));
        for (old, span) in self.sections.iter_mut().zip(spans) {
            let mut measure = Measure {
                typography: old.typography.clone(),
                span,
                source: source.clone(),
                wrapped: None,
            };
            *old = Rc::new(measure.layout(old.bounds, window));
        }
        true
    }
}
