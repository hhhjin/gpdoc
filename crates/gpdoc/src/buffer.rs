mod folding;
mod projection;
use self::projection::Projection;
use gpdoc_core::{
    CommandOutcome, DocPos, Document, Editor, Error, Selection, TextInputTransaction,
};
pub(super) use projection::FormatSpan;
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Preedit {
    Marked,
    KoreanReplacement,
}

struct Composition {
    kind: Preedit,
    input: TextInputTransaction,
    marked: Range<usize>,
}

pub(super) struct InputBuffer {
    editor: Editor,
    pub(super) projection: Projection,
    pub(super) fold_specs: std::rc::Rc<crate::fold::FoldSpecs>,
    open_folds: std::collections::BTreeSet<DocPos>,
    composition: Option<Composition>,
}

impl InputBuffer {
    #[cfg(test)]
    pub(super) fn open(editor: Editor) -> Result<Self, Error> {
        Self::with_folds(editor, Default::default())
    }

    pub(super) fn with_folds(
        editor: Editor,
        fold_specs: std::rc::Rc<crate::fold::FoldSpecs>,
    ) -> Result<Self, Error> {
        let mut buffer = Self {
            projection: Projection::new(
                editor.document(),
                editor.config(),
                &Default::default(),
                &fold_specs,
            ),
            fold_specs,
            open_folds: Default::default(),
            editor,
            composition: None,
        };
        buffer.restore_visible_selection()?;
        Ok(buffer)
    }

    pub(super) fn editor(&self) -> &Editor {
        &self.editor
    }

    pub(super) fn text(&self) -> &str {
        &self.projection.text
    }

    pub(super) fn object_bytes(&self) -> &[Range<usize>] {
        &self.projection.object_bytes
    }

    pub(super) fn display_text(&self) -> String {
        self.projection.display_text()
    }

    pub(super) fn dividers(&self) -> &[(DocPos, usize)] {
        &self.projection.dividers
    }

    pub(super) fn alignments(&self) -> &[(usize, gpdoc_core::TextAlignment)] {
        &self.projection.alignments
    }

    pub(super) fn formats(&self) -> &[FormatSpan] {
        &self.projection.formats
    }

    pub(super) fn execute(
        &mut self,
        command: impl FnOnce(&mut Editor) -> Result<CommandOutcome, Error>,
    ) -> Result<CommandOutcome, Error> {
        self.unmark()?;
        let outcome = command(&mut self.editor)?;
        if let CommandOutcome::Applied(change) = &outcome {
            self.refresh_projection(&change.map)?;
        }
        Ok(outcome)
    }

    pub(super) fn apply_node_view(&mut self, tx: gpdoc_core::Transaction) -> Result<bool, Error> {
        if self.has_preedit() {
            return Err(Error::InvalidEdit(
                "node view edit during composition".into(),
            ));
        }
        let Some(change) = self.editor.apply_if_accepted(tx)? else {
            return Ok(false);
        };
        self.refresh_projection(&change.map)?;
        Ok(true)
    }

    pub(super) fn block_offsets(&self, range: Range<DocPos>) -> Option<Range<usize>> {
        self.projection.block_offsets(range)
    }

    pub(super) fn node_range(&self, before: DocPos) -> Range<usize> {
        let (start, end) = self
            .projection
            .selection(self.state().0, Selection::Node { before });
        start..end
    }

    /// Project a range in the displayed document. A collapsed, hidden range has no geometry.
    pub(super) fn visible_range(&self, range: Range<DocPos>) -> Option<Range<usize>> {
        if range.start > range.end || range.end.0 > self.state().0.size() {
            return None;
        }
        let offsets = self.offset(range.start)..self.offset(range.end);
        (!offsets.is_empty() || !self.hidden(range.start)).then_some(offsets)
    }

    /// Map an editor decoration through the current IME preview and visible document ranges.
    pub(super) fn decoration_range(&self, range: Range<DocPos>) -> Option<Range<usize>> {
        if range.is_empty() || range.end.0 > self.editor.document().size() {
            return None;
        }
        let range = if let Some(preview) = &self.composition {
            let map = preview.input.position_map();
            map.map(range.start, gpdoc_core::Affinity::After).position
                ..map.map(range.end, gpdoc_core::Affinity::Before).position
        } else {
            range
        };
        let start = self.offset(range.start);
        let end = self.offset(range.end);
        if start >= end {
            return None;
        }
        // A folded body and the preceding line end share an offset. Start at the
        // next visible line to omit its preceding newline when the match begins in the fold.
        let start = start + usize::from(self.position(start).ok()? < range.start);
        (start < end).then_some(start..end)
    }

    pub(super) fn offset(&self, pos: DocPos) -> usize {
        self.projection.offset(pos)
    }

    pub(super) fn state(&self) -> (&Document, Selection) {
        match &self.composition {
            Some(preview) => (preview.input.document(), preview.input.selection()),
            None => (self.editor.document(), self.editor.selection()),
        }
    }

    pub(super) fn selection(&self) -> (usize, usize) {
        let (doc, sel) = self.state();
        self.projection.selection(doc, sel)
    }

    pub(super) fn marked_range(&self) -> Option<Range<usize>> {
        self.composition
            .as_ref()
            .filter(|c| c.kind == Preedit::Marked)
            .map(|c| c.marked.clone())
    }

    pub(super) fn has_preedit(&self) -> bool {
        self.composition.is_some()
    }

    fn korean_range(&self) -> Option<Range<usize>> {
        self.composition
            .as_ref()
            .filter(|c| c.kind == Preedit::KoreanReplacement)
            .map(|c| c.marked.clone())
    }

    pub(super) fn text_for_range(&self, range: Range<usize>) -> Option<(String, Range<usize>)> {
        self.projection.substring(range)
    }

    pub(super) fn position(&self, offset: usize) -> Result<DocPos, Error> {
        self.projection.position(offset)
    }

    pub(super) fn select_doc(&mut self, sel: Selection) -> Result<(), Error> {
        self.unmark()?;
        let revision = self.editor.revision();
        let mut tx = self.editor.transaction();
        tx.select(sel);
        let change = self.editor.apply(tx)?;
        if change.revision != revision {
            self.refresh_projection(&change.map)
        } else {
            self.restore_visible_selection()
        }
    }

    fn selection_at(&self, anchor: usize, head: usize) -> Result<Selection, Error> {
        let start = self.projection.position(anchor)?;
        let end = self.projection.position(head)?;
        let first_leaf = self.projection.block_leaf_at(anchor);
        let last_leaf = self.projection.block_leaf_at(head);
        if let Some(before) = first_leaf.or(last_leaf) {
            if first_leaf == last_leaf || anchor.abs_diff(head) <= 1 {
                return Ok(Selection::Node { before });
            }
            let (doc, _) = self.state();
            let endpoint = |pos| doc.nearest_text(pos).ok_or(Error::InvalidPosition(pos));
            return Ok(Selection::Text {
                anchor: endpoint(start)?,
                head: endpoint(end)?,
            });
        }
        Ok(Selection::Text {
            anchor: start,
            head: end,
        })
    }

    pub(super) fn select(&mut self, anchor: usize, head: usize) -> Result<(), Error> {
        let selection = self.selection_at(anchor, head)?;
        if !self.has_preedit() {
            return self.select_doc(selection);
        }
        let (visible, _) = self.state();
        if let Selection::Text { anchor, head } = selection {
            visible.text_at(anchor)?;
            visible.text_at(head)?;
        }
        self.unmark()?;
        // An input rule may have shortened or reshaped the text while committing.
        let nearest = |offset| {
            self.projection
                .substring(offset..offset)
                .map_or(self.projection.len(), |(_, actual)| actual.end)
        };
        self.select_doc(self.selection_at(nearest(anchor), nearest(head))?)
    }

    fn replacement_context(
        &self,
        range: Option<Range<usize>>,
    ) -> Result<(TextInputTransaction, Selection), Error> {
        let input = self
            .composition
            .as_ref()
            .map_or_else(|| self.editor.begin_text_input(), |c| c.input.clone());
        let selection = match range.or_else(|| self.marked_range()) {
            Some(r) if r.start > r.end => {
                return Err(Error::InvalidEdit(
                    "reversed native replacement range".into(),
                ));
            }
            Some(r) => {
                let (anchor, head) = self.selection();
                // AppKit can return the UTF-16 range it just queried. Several structural
                // selections can share those offsets, especially beside block leaves.
                if !self.has_preedit()
                    && !matches!(input.selection(), Selection::Text { .. })
                    && r == (anchor.min(head)..anchor.max(head))
                {
                    input.selection()
                } else {
                    self.selection_at(r.start, r.end)?
                }
            }
            None => input.selection(),
        };
        Ok((input, selection))
    }

    fn replacement(
        &self,
        range: Option<Range<usize>>,
        text: &str,
    ) -> Result<(TextInputTransaction, Range<DocPos>), Error> {
        let (mut input, selection) = self.replacement_context(range)?;
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let lines: Vec<_> = normalized.split('\n').collect();
        let inserted = input.replace(selection, &lines)?;
        Ok((input, inserted))
    }

    fn commit(&mut self, input: TextInputTransaction) -> Result<(), Error> {
        let change = self.editor.commit_text_input(input)?;
        self.composition = None;
        self.refresh_projection(&change.map)?;
        Ok(())
    }

    pub(super) fn paragraph_break(&mut self) -> Result<(), Error> {
        self.unmark()?;
        let change = self.editor.paragraph_break()?;
        self.refresh_projection(&change.map)
    }

    pub(super) fn paste(&mut self, range: Option<Range<usize>>, text: &str) -> Result<(), Error> {
        if self.korean_range().is_some() {
            self.unmark()?;
        }
        let (mut input, selection) = self.replacement_context(range)?;
        input.paste(selection, text)?;
        self.commit(input)
    }

    pub(super) fn replace(&mut self, range: Option<Range<usize>>, text: &str) -> Result<(), Error> {
        if self.korean_range().is_some() {
            self.unmark()?;
        }
        let (input, _) = self.replacement(range, text)?;
        self.commit(input)
    }

    /// Some macOS two-set input sources use successive insertText callbacks:
    /// first a consonant at the caret, then syllable updates that replace it.
    /// Keep this syllable provisional without reporting an AppKit marked range.
    pub(super) fn native_replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
    ) -> Result<(), Error> {
        let mut chars = text.chars();
        let ch = chars.next().filter(|_| chars.next().is_none());
        // Hangul Jamo and Compatibility Jamo used by the two-set input source.
        let jamo = matches!(ch, Some('\u{1100}'..='\u{11FF}' | '\u{3131}'..='\u{318E}'));
        let hangul = jamo || matches!(ch, Some('\u{AC00}'..='\u{D7A3}'));
        let pending = self.korean_range();

        if let Some(active) = &pending
            && range.as_ref() == Some(active)
            && (hangul || text.is_empty())
        {
            if self
                .projection
                .substring(active.clone())
                .is_some_and(|(old, _)| old == text)
            {
                return Ok(());
            }
            return self.stage(Preedit::KoreanReplacement, range, text, None);
        }
        if pending.is_some() {
            self.unmark()?;
        }
        // After Korean resyllabification the next syllable may arrive already composed.
        if self.composition.is_none() && range.is_none() && (jamo || (pending.is_some() && hangul))
        {
            return self.stage(Preedit::KoreanReplacement, range, text, None);
        }
        self.replace(range, text)
    }

    pub(super) fn mark(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) -> Result<(), Error> {
        if let Some(pending) = self.korean_range()
            && range.as_ref() != Some(&pending)
        {
            self.unmark()?;
        }
        self.stage(Preedit::Marked, range, text, selected)
    }

    fn stage(
        &mut self,
        kind: Preedit,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) -> Result<(), Error> {
        let (mut input, inserted) = self.replacement(range, text)?;
        let projection = self.preview_projection(&input);
        let marked = projection.offset(inserted.start)..projection.offset(inserted.end);
        if marked.is_empty() {
            // macOS also represents withdrawal of the current preedit as an empty marked string.
            if input.document() == self.editor.document() {
                self.cancel_mark();
                return Ok(());
            }
            return self.commit(input);
        }
        if let Some(selected) = selected {
            if selected.start > selected.end || selected.end > marked.len() {
                return Err(Error::InvalidEdit(
                    "selection outside the marked text".into(),
                ));
            }
            input.select(Selection::Text {
                anchor: projection.position(marked.start + selected.start)?,
                head: projection.position(marked.start + selected.end)?,
            })?;
        }
        self.projection = projection;
        self.composition = Some(Composition {
            kind,
            input,
            marked,
        });
        Ok(())
    }

    pub(super) fn unmark(&mut self) -> Result<(), Error> {
        if let Some(c) = &self.composition {
            self.commit(c.input.clone())?;
        }
        Ok(())
    }

    pub(super) fn cancel_mark(&mut self) {
        if self.composition.take().is_some() {
            self.projection = Projection::new(
                self.editor.document(),
                self.editor.config(),
                &self.open_folds,
                &self.fold_specs,
            );
        }
    }

    pub(super) fn undo(&mut self) -> Result<(), Error> {
        if self.marked_range().is_some() {
            self.cancel_mark();
        } else {
            self.unmark()?;
            let change = self.editor.undo()?;
            self.refresh_projection(&change.map)?;
        }
        Ok(())
    }

    pub(super) fn redo(&mut self) -> Result<(), Error> {
        if self.korean_range().is_some() {
            self.unmark()?;
        }
        let change = self.editor.redo()?;
        self.composition = None;
        self.refresh_projection(&change.map)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

/// Segment each text span independently so a combining character cannot absorb an inline object.
pub(crate) fn editor_graphemes<'a>(
    text: &'a str,
    objects: &'a [Range<usize>],
) -> impl Iterator<Item = (usize, &'a str)> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut previous = 0;
    objects
        .iter()
        .cloned()
        .chain(std::iter::once(text.len()..text.len()))
        .flat_map(move |object| {
            let start = previous;
            previous = object.end;
            text[start..object.start]
                .grapheme_indices(true)
                .map(move |(i, g)| (start + i, g))
                .chain((!object.is_empty()).then(|| (object.start, &text[object])))
        })
}
