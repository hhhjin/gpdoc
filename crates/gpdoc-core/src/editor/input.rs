use super::{AppliedChange, Draft, Editor, EditorConfig, Selection};
use crate::{DocPos, Document, Edit, Error, Mark};
use std::{ops::Range, sync::Arc};

/// An editable preview of a single text input. A host can display multiple IME
/// updates, then commit them together. Dropping the preview cancels all updates.
#[derive(Clone)]
pub struct TextInputTransaction {
    owner: Arc<()>,
    revision: u64,
    config: EditorConfig,
    draft: Draft,
    typed: Option<TypedInput>,
    pasted: Option<Range<DocPos>>,
    typing_marks: Option<Vec<Mark>>,
}

#[derive(Clone)]
struct TypedInput {
    range: Range<DocPos>,
    last_char: char,
}

impl Editor {
    pub fn begin_text_input(&self) -> TextInputTransaction {
        TextInputTransaction {
            owner: self.owner.clone(),
            revision: self.revision,
            config: self.config.clone(),
            draft: Draft::new(&self.state),
            typed: None,
            pasted: None,
            typing_marks: None,
        }
    }

    /// Paste clipboard text through plugin handlers, multiline insertion, and paste rules.
    pub fn paste_text(&mut self, text: &str) -> Result<AppliedChange, Error> {
        let mut input = self.begin_text_input();
        input.paste(self.selection(), text)?;
        self.commit_text_input(input)
    }

    /// Run input rules on the last single-line insertion, then publish the
    /// preview and its selection as one history entry.
    pub fn commit_text_input(
        &mut self,
        input: TextInputTransaction,
    ) -> Result<AppliedChange, Error> {
        if !Arc::ptr_eq(&self.owner, &input.owner) {
            return Err(Error::ForeignTransaction);
        }
        if self.revision != input.revision {
            return Err(Error::StaleRevision);
        }
        let mut draft = input.draft;
        if let Some(typed) = input.typed
            && let Some((steps, clear_mark)) = self.config.match_input(
                &draft.state.doc,
                draft.state.selection,
                typed.range,
                typed.last_char,
            )?
        {
            for step in steps {
                draft.step(&step)?;
            }
            if let Some(kind) = clear_mark
                && let Selection::Text { anchor, head } = draft.state.selection
                && anchor == head
            {
                let pos = draft.state.doc.resolve(head)?;
                let mut marks = pos.marks(self.schema());
                marks.retain(|m| m.kind != kind);
                draft.state.stored_marks = Some(marks);
            }
        }
        self.commit_with_paste(draft, input.pasted.as_ref())
    }
}

impl TextInputTransaction {
    /// Maps decorations and other view positions from the committed document into this preview.
    pub fn position_map(&self) -> &crate::PositionMap {
        &self.draft.map
    }

    pub fn document(&self) -> &Document {
        &self.draft.state.doc
    }

    pub fn selection(&self) -> Selection {
        self.draft.state.selection
    }

    pub fn select(&mut self, selection: Selection) -> Result<(), Error> {
        self.config.validate_selection(selection, self.document())?;
        self.draft.state.selection = selection;
        self.draft.state.stored_marks = None;
        Ok(())
    }

    /// Apply clipboard-specific rules to a selection in this preview.
    pub fn paste(&mut self, selection: Selection, text: &str) -> Result<Range<DocPos>, Error> {
        self.config.validate_selection(selection, self.document())?;
        let mut next = self.clone();
        next.draft.state.selection = selection;
        next.typed = None;
        let state = next.draft.state.clone();
        let owner = Arc::new(());
        let ctx = super::CommandContext::for_draft(&next.config, &state, &owner, next.revision);
        let mut range = selection.bounds(&state.doc);
        if let Some(tx) = next.config.handle_paste(ctx, text)? {
            ctx.apply_to(&mut next.draft, tx)?;
        } else {
            let text = text.replace("\r\n", "\n").replace('\r', "\n");
            range = next.replace(selection, &text.split('\n').collect::<Vec<_>>())?;
            next.typed = None;
            for rule in next.config.paste_rules() {
                let steps = rule(crate::InputRuleContext {
                    document: &next.draft.state.doc,
                    selection: next.selection(),
                    schema: next.config.schema(),
                    inserted: range.clone(),
                    last_typed_char: None,
                })?;
                if steps.is_empty() {
                    continue;
                }
                let mut patch = Draft::new(&next.draft.state);
                for step in steps {
                    patch.step(&step)?;
                }
                range = patch.map.map(range.start, crate::Affinity::Before).position
                    ..patch.map.map(range.end, crate::Affinity::After).position;
                next.draft.map.append(patch.map);
                next.draft.state = patch.state;
            }
            next.pasted = Some(range.clone());
        }
        *self = next;
        Ok(range)
    }

    /// Replace a selection in the preview. Adjacent entries in `lines` produce
    /// a block split. A single empty string deletes the selection.
    pub fn replace(
        &mut self,
        selection: Selection,
        lines: &[&str],
    ) -> Result<Range<DocPos>, Error> {
        let Some((first, rest)) = lines.split_first() else {
            return Err(Error::InvalidEdit(
                "text input needs at least one line".into(),
            ));
        };
        self.config.validate_selection(selection, self.document())?;
        // Validate every step before replacing the previous, still usable preview.
        let mut next = Draft::new(&self.draft.state);
        let typing_marks = match selection {
            Selection::Text { anchor, head }
                if self
                    .typed
                    .as_ref()
                    .is_some_and(|typed| typed.range == (anchor.min(head)..anchor.max(head))) =>
            {
                self.typing_marks.clone()
            }
            _ if selection == next.state.selection => next.state.stored_marks.clone(),
            _ => None,
        };
        next.state.selection = selection;
        next.state.stored_marks.clone_from(&typing_marks);
        match selection {
            Selection::NodeRange { .. } if !first.is_empty() || !rest.is_empty() => {
                let range = selection.bounds(&next.state.doc);
                let caret = DocPos(range.start.0 + 1);
                let paragraph = crate::Node::text(self.config.default_text_block(), "");
                next.step(&Edit::ReplaceNodes {
                    range,
                    nodes: vec![paragraph],
                })?;
                next.state.selection = Selection::caret(caret);
            }
            Selection::Node { .. } | Selection::NodeRange { .. } => {
                let range = selection.bounds(&next.state.doc);
                next.delete_nodes(&self.config, range)?;
            }
            Selection::Gap { at } => {
                if first.is_empty() && rest.is_empty() {
                    return Ok(at..at);
                }
                let kind = super::selection::gap_textblock(&next.state.doc, at)
                    .ok_or(Error::InvalidPosition(at))?;
                next.step(&Edit::ReplaceNodes {
                    range: at..at,
                    nodes: vec![crate::Node::text(kind, "")],
                })?;
                next.state.selection = Selection::caret(DocPos(at.0 + 1));
            }
            Selection::Custom { kind, anchor, head } => {
                let state = next.state.clone();
                let owner = Arc::new(());
                let ctx =
                    super::CommandContext::for_draft(&self.config, &state, &owner, self.revision);
                let tx = kind.prepare_input(ctx, anchor, head)?;
                ctx.apply_to(&mut next, tx)?;
            }
            Selection::Text { .. } => {}
        }
        let Selection::Text { anchor, head } = next.state.selection else {
            return Err(Error::InvalidEdit(
                "text input requires an editable position".into(),
            ));
        };
        let start = anchor.min(head);
        next.step(&Edit::ReplaceText {
            range: start..anchor.max(head),
            text: (*first).into(),
        })?;
        let mut end = DocPos(start.0 + first.encode_utf16().count());
        for line in rest {
            next.step(&Edit::Split { at: end })?;
            // The closing and opening block boundaries are each one document unit.
            end.0 += 2;
            next.step(&Edit::ReplaceText {
                range: end..end,
                text: (*line).into(),
            })?;
            end.0 += line.encode_utf16().count();
        }
        next.state.selection = Selection::caret(end);
        self.draft.state = next.state;
        self.draft.map.append(next.map);
        let inserted = start..end;
        self.pasted = None;
        self.typing_marks = typing_marks;
        self.typed = if rest.is_empty() {
            first.chars().next_back().map(|last_char| TypedInput {
                range: inserted.clone(),
                last_char,
            })
        } else {
            None
        };
        Ok(inserted)
    }
}
