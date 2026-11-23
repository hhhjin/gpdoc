mod commands;
mod paragraph_break;
pub use commands::{CommandChain, CommandContext};
mod input;
pub use input::TextInputTransaction;

mod plugin_state;
use plugin_state::PluginStates;
use std::sync::Arc;

use crate::transform::{apply_edit, replace_text};
use crate::{
    Affinity, Attrs, DocPos, Document, Edit, EditorConfig, Error, Mark, Node, PositionMap, Schema,
};

mod selection;
pub use selection::{Selection, SelectionBehavior};

/// A private document draft. Each edit sees the result of the preceding edit.
#[derive(Clone, Debug)]
pub struct Transaction {
    owner: Arc<()>,
    revision: u64,
    initial_selection: Selection,
    draft: Draft,
    error: Option<Error>,
    final_selection: Option<Selection>,
    final_marks: Option<Vec<Mark>>,
}

impl Transaction {
    /// The document after every edit pushed so far.
    pub fn document(&self) -> Result<&Document, Error> {
        match &self.error {
            Some(error) => Err(error.clone()),
            None => Ok(&self.draft.state.doc),
        }
    }

    /// Edit a plugin's private Rust state in this draft. Each state type has one slot
    /// per editor. State commits with the transaction and survives document undo.
    pub fn update_plugin_state<S>(&mut self, update: impl FnOnce(&mut S))
    where
        S: Default + Clone + std::fmt::Debug + PartialEq + Send + Sync + 'static,
    {
        let states = Arc::make_mut(&mut self.draft.state.plugin_states);
        let mut value = states.get::<S>().cloned().unwrap_or_default();
        update(&mut value);
        states.set(value);
    }

    /// Apply an edit to the draft. Applying the transaction reports the first failed edit.
    pub fn push(&mut self, edit: impl Into<Edit>) {
        if self.error.is_none() {
            self.error = self.draft.step(&edit.into()).err();
        }
        self.final_marks = None;
    }

    /// Override the mapped selection using positions in the final document.
    pub fn select(&mut self, selection: Selection) {
        self.final_selection = Some(selection);
        self.final_marks = None;
    }

    /// Override marks for the next typed text at the final cursor. An empty set disables inherited marks.
    pub fn set_stored_marks(&mut self, marks: Vec<Mark>) {
        self.final_marks = Some(marks);
    }
}

#[derive(Clone, Debug)]
pub struct AppliedChange {
    pub revision: u64,
    pub selection: Selection,
    pub map: PositionMap,
}

#[derive(Clone, Debug)]
pub enum CommandOutcome {
    Applied(AppliedChange),
    NotApplicable,
}

#[derive(Clone, Debug)]
struct Snapshot {
    plugin_states: Arc<PluginStates>,
    stored_marks: Option<Vec<Mark>>,
    doc: Document,
    selection: Selection,
}

struct HistoryEntry {
    before: Snapshot,
    after: Snapshot,
    forward: PositionMap,
    backward: PositionMap,
}

/// Temporary state used by both explicit transactions and the text input path.
/// No partially applied edit is visible to the editor or its history.
#[derive(Clone, Debug)]
struct Draft {
    state: Snapshot,
    map: PositionMap,
}

impl Draft {
    fn new(state: &Snapshot) -> Self {
        Self {
            state: state.clone(),
            map: PositionMap::default(),
        }
    }

    fn step(&mut self, step: &Edit) -> Result<(), Error> {
        let (doc, map) = match (step, &self.state.stored_marks) {
            (Edit::ReplaceText { range, text }, Some(marks))
                if matches!(self.state.selection,
                    Selection::Text { anchor, head } if (anchor.min(head)..anchor.max(head)) == *range
                ) =>
            {
                replace_text(&self.state.doc, range, text, Some(marks))?
            }
            _ => apply_edit(&self.state.doc, step)?,
        };
        let selection = match (step, self.state.selection) {
            (
                Edit::ReplaceText { range, .. } | Edit::ReplaceInline { range, .. },
                Selection::Text { anchor, head },
            ) if range.start == anchor.min(head) && range.end == anchor.max(head) => {
                Selection::caret(map.map(range.end, Affinity::After).position)
            }
            (_, old) => old.mapped(&doc, &map)?,
        };
        self.state.doc = doc;
        self.state.selection = selection;
        self.state.stored_marks = None;
        self.map.append(map);
        Ok(())
    }

    fn append(
        &mut self,
        config: &EditorConfig,
        before: Option<&Document>,
        pasted: Option<&std::ops::Range<DocPos>>,
    ) -> Result<(), Error> {
        let schema = config.schema();
        // A later hook can introduce work for an earlier hook (e.g. a new
        // trailing paragraph needs an ID). Reject non-converging compositions.
        for _ in 0..32 {
            let mut changed = false;
            for handler in config.appenders() {
                let steps = handler(crate::DocumentChange {
                    default_text_block: config.default_text_block(),
                    before,
                    pasted,
                    schema,
                    document: &self.state.doc,
                })?;
                if steps.is_empty() {
                    continue;
                }
                let previous = self.state.doc.clone();
                let marks = self.state.stored_marks.clone();
                for step in steps {
                    self.step(&step)?;
                }
                if let (Some(marks), Selection::Text { anchor, head }) =
                    (marks, self.state.selection)
                    && anchor == head
                    && schema.allows_marks(&self.state.doc.resolve(head)?.parent().kind, &marks)
                {
                    self.state.stored_marks = Some(marks);
                }
                changed |= previous != self.state.doc;
            }
            if !changed {
                return Ok(());
            }
        }
        Err(Error::InvalidEdit(
            "appended transactions did not converge".into(),
        ))
    }

    fn delete_nodes(
        &mut self,
        config: &EditorConfig,
        range: std::ops::Range<DocPos>,
    ) -> Result<(), Error> {
        let doc = &self.state.doc;
        let schema = config.schema();
        let mut before = range.start;
        let mut end = range.end;
        while let Some(parent_pos) = before.0.checked_sub(1).map(DocPos)
            && let Some(parent) = doc.node_at(parent_pos)
            && !parent.is_leaf()
            && !parent.inline_content()
            && end.0 + 1 == parent_pos.0 + parent.size()
            && !schema.allows_children(&parent.kind, &[])
        {
            before = parent_pos;
            end.0 += 1;
        }
        let remaining_text = doc
            .iter_blocks()
            .any(|(pos, node)| (pos < before || pos >= end) && node.inline_content());
        let pos = doc
            .root()
            .resolve(before.0)
            .map_err(|_| Error::InvalidPosition(before))?;
        let parent = pos.parent();
        let index = pos.index(pos.depth());
        let after = doc.resolve(end)?.index(pos.depth());
        let default = Node::text(config.default_text_block(), "");
        let default_fragment = crate::model::Fragment::from(vec![schema.bind_node(&default)?]);

        let replacement =
            if !remaining_text && parent.can_replace(schema, index, after, &default_fragment) {
                default_fragment.clone()
            } else {
                // Use the prefix and the surviving suffix to find what this slot requires.
                // For a list item the filler may be a paragraph; for a document it may be a title.
                let prefix = parent.content_match_at(schema, index)?;
                prefix
                    .fill_before(schema, &parent.content, true, after)
                    .ok_or_else(|| {
                        Error::InvalidEdit("selection cannot be deleted within this schema".into())
                    })?
            };
        self.step(&Edit::Atomic(crate::transform::Step::Replace(
            crate::transform::ReplaceStep {
                from: before.0,
                to: end.0,
                slice: crate::model::Slice::new(replacement, 0, 0),
                structure: false,
            },
        )))?;

        if self.state.doc.nearest_text(before).is_none() {
            let root = self.state.doc.root();
            let mut boundary = 0usize;
            let mut nearest = None;
            for index in 0..=root.child_count() {
                if root.can_replace(schema, index, index, &default_fragment)
                    && nearest.is_none_or(|previous| {
                        boundary.abs_diff(before.0) <= before.0.abs_diff(previous)
                    })
                {
                    nearest = Some(boundary);
                }
                if let Some(node) = root.content.maybe_child(index) {
                    boundary += node.size();
                }
            }
            if let Some(nearest) = nearest {
                let at = DocPos(nearest);
                self.step(&Edit::ReplaceNodes {
                    range: at..at,
                    nodes: vec![default],
                })?;
            }
        }
        self.state.selection = Selection::near(&self.state.doc, before)?;
        Ok(())
    }
}

pub struct Editor {
    owner: Arc<()>,
    config: EditorConfig,
    revision: u64,
    state: Snapshot,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl Editor {
    /// Create the initial document, filling required content from the schema.
    pub fn from_config(config: &EditorConfig) -> Result<Self, Error> {
        let schema = config.schema();
        let root = schema.top_node_type();
        let start = &root.content_match;
        let candidates =
            (0..start.edge_count()).filter_map(|i| start.edge(i).map(|(name, _)| name));
        let preferred = config.default_text_block();
        let candidates = std::iter::once(preferred)
            .filter(|name| start.match_type(name).is_some())
            .chain(candidates.filter(|name| *name != preferred));
        for name in candidates {
            if schema
                .node_type(name)
                .is_some_and(|ty| ty.spec.has_required_attrs())
            {
                continue;
            }
            let Some(child) =
                schema.create_and_fill(name, Attrs::new(), Default::default(), vec![])?
            else {
                continue;
            };
            if let Some(root) =
                schema.create_and_fill(&root.name, Attrs::new(), vec![child].into(), vec![])?
            {
                return Editor::open_document(config.clone(), schema.with_root(None, root)?);
            }
        }
        Err(Error::InvalidDocument(
            "schema cannot create a selectable initial document".into(),
        ))
    }

    /// Start an independent editor with the supplied top-level blocks.
    pub fn from_document(config: &EditorConfig, blocks: Vec<Node>) -> Result<Self, Error> {
        let doc = config.schema().document(blocks)?;
        Self::open_document(config.clone(), doc)
    }

    pub(crate) fn open_document(config: EditorConfig, doc: Document) -> Result<Self, Error> {
        let selection = Selection::near(&doc, DocPos(0))?;
        let state = Snapshot {
            doc,
            selection,
            stored_marks: None,
            plugin_states: Default::default(),
        };
        let mut initial = Draft::new(&state);
        initial.append(&config, None, None)?;
        Ok(Self {
            owner: Arc::new(()),
            config,
            revision: 0,
            state: initial.state,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// Start an independent editing session at this snapshot with an empty history.
    /// Transactions from either session belong to their original editor.
    pub fn fork(&self) -> Self {
        Self {
            owner: Arc::new(()),
            config: self.config.clone(),
            revision: self.revision,
            state: self.state.clone(),
            undo: vec![],
            redo: vec![],
        }
    }

    /// Compose the edits after a retained snapshot. A missing snapshot belongs to a
    /// different branch or predates the retained history.
    pub fn map_since(&self, snapshot: &Document) -> Option<PositionMap> {
        if self.document().root().ptr_eq(snapshot.root()) {
            return Some(PositionMap::default());
        }
        let start = self
            .undo
            .iter()
            .rposition(|entry| entry.before.doc.root().ptr_eq(snapshot.root()))?;
        let mut map = PositionMap::default();
        for entry in &self.undo[start..] {
            map.append(entry.forward.clone());
        }
        Some(map)
    }

    /// Load a schema-validated snapshot from an external history (for example, Yjs).
    /// Local input rules and append hooks run on the originating editor. Loading a snapshot
    /// clears local undo history and typing marks, and keeps editor-local plugin state.
    pub fn load_snapshot(&mut self, blocks: Vec<Node>) -> Result<(), Error> {
        let doc = self.schema().document(blocks)?;
        if doc == self.state.doc {
            return Ok(());
        }
        if !self.config.accepts(crate::DocumentChange {
            default_text_block: self.config.default_text_block(),
            before: Some(self.document()),
            pasted: None,
            document: &doc,
            schema: self.schema(),
        })? {
            return Err(Error::InvalidEdit(
                "a document filter rejected the shared snapshot".into(),
            ));
        }
        let previous = self.state.selection.bounds(&self.state.doc).start;
        let selection = Selection::near(&doc, previous)?;
        self.revision = self.next_revision()?;
        self.state.doc = doc;
        self.state.selection = selection;
        self.state.stored_marks = None;
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }

    pub fn document(&self) -> &Document {
        &self.state.doc
    }

    pub fn schema(&self) -> &Schema {
        self.config.schema()
    }

    pub fn config(&self) -> &EditorConfig {
        &self.config
    }

    pub fn selection(&self) -> Selection {
        self.state.selection
    }

    /// Read the last committed value of a plugin's state type.
    pub fn plugin_state<S: 'static>(&self) -> Option<&S> {
        self.state.plugin_states.get()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Move the selection and clear pending typing marks without adding an undo entry.
    pub fn select(&mut self, selection: Selection) -> Result<(), Error> {
        let mut tx = self.transaction();
        tx.select(selection);
        self.apply(tx)?;
        Ok(())
    }

    pub fn transaction(&self) -> Transaction {
        self.command_context().transaction()
    }

    pub fn apply(&mut self, tx: Transaction) -> Result<AppliedChange, Error> {
        Ok(self
            .apply_if_accepted(tx)?
            .unwrap_or_else(|| self.unchanged()))
    }

    /// Apply a transaction, returning None if an editor filter rejects it.
    pub fn apply_if_accepted(&mut self, tx: Transaction) -> Result<Option<AppliedChange>, Error> {
        let mut draft = Draft::new(&self.state);
        self.command_context().apply_to(&mut draft, tx)?;
        self.commit_if_accepted(draft, None)
    }

    /// Insert committed text and apply the first matching input rule as one edit.
    pub fn insert_text(&mut self, text: &str) -> Result<AppliedChange, Error> {
        let mut input = self.begin_text_input();
        input.replace(self.selection(), &[text])?;
        self.commit_text_input(input)
    }

    pub fn delete_selection(&mut self) -> Result<AppliedChange, Error> {
        self.insert_text("")
    }

    fn apply_command(&mut self, planned: Option<Transaction>) -> Result<CommandOutcome, Error> {
        match planned {
            Some(tx) => self.apply(tx).map(CommandOutcome::Applied),
            None => Ok(CommandOutcome::NotApplicable),
        }
    }

    fn next_revision(&self) -> Result<u64, Error> {
        self.revision
            .checked_add(1)
            .ok_or_else(|| Error::InvalidEdit("editor exhausted its revision numbers".into()))
    }

    fn commit(&mut self, draft: Draft) -> Result<AppliedChange, Error> {
        self.commit_with_paste(draft, None)
    }

    fn commit_with_paste(
        &mut self,
        draft: Draft,
        pasted: Option<&std::ops::Range<DocPos>>,
    ) -> Result<AppliedChange, Error> {
        Ok(self
            .commit_if_accepted(draft, pasted)?
            .unwrap_or_else(|| self.unchanged()))
    }

    fn unchanged(&self) -> AppliedChange {
        AppliedChange {
            revision: self.revision,
            selection: self.selection(),
            map: PositionMap::default(),
        }
    }

    fn commit_if_accepted(
        &mut self,
        mut draft: Draft,
        pasted: Option<&std::ops::Range<DocPos>>,
    ) -> Result<Option<AppliedChange>, Error> {
        let before = Some(&self.state.doc);
        draft.append(&self.config, before, pasted)?;
        if !self.config.accepts(crate::DocumentChange {
            default_text_block: self.config.default_text_block(),
            before,
            pasted,
            document: &draft.state.doc,
            schema: self.schema(),
        })? {
            return Ok(None);
        }
        // Moving a selection or applying an already-active mark preserves the redo branch.
        if draft.map.is_identity() && draft.state.doc == self.state.doc {
            if self.state.stored_marks != draft.state.stored_marks
                || self.state.plugin_states != draft.state.plugin_states
            {
                self.revision = self.next_revision()?;
            }
            self.state = draft.state;
            return Ok(Some(AppliedChange {
                revision: self.revision,
                selection: self.selection(),
                map: draft.map,
            }));
        }
        let revision = self.next_revision()?;
        let after = draft.state.clone();
        let before = std::mem::replace(&mut self.state, draft.state);
        self.undo.push(HistoryEntry {
            before,
            after,
            forward: draft.map.clone(),
            backward: draft.map.inverse(),
        });
        self.redo.clear();
        self.revision = revision;
        Ok(Some(AppliedChange {
            revision,
            selection: self.selection(),
            map: draft.map,
        }))
    }

    pub fn undo(&mut self) -> Result<AppliedChange, Error> {
        let revision = self.next_revision()?;
        let entry = self.undo.pop().ok_or(Error::NoHistory)?;
        let plugin_states = self.state.plugin_states.clone();
        self.state = entry.before.clone();
        self.state.plugin_states = plugin_states;
        self.state.stored_marks = None;
        self.revision = revision;
        let change = AppliedChange {
            revision,
            selection: self.selection(),
            map: entry.backward.clone(),
        };
        self.redo.push(entry);
        Ok(change)
    }

    pub fn redo(&mut self) -> Result<AppliedChange, Error> {
        let revision = self.next_revision()?;
        let entry = self.redo.pop().ok_or(Error::NoHistory)?;
        let plugin_states = self.state.plugin_states.clone();
        self.state = entry.after.clone();
        self.state.plugin_states = plugin_states;
        self.state.stored_marks = None;
        self.revision = revision;
        let change = AppliedChange {
            revision,
            selection: self.selection(),
            map: entry.forward.clone(),
        };
        self.undo.push(entry);
        Ok(change)
    }
}

#[cfg(test)]
mod tests;
