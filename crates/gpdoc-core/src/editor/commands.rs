//! Commands plan against the selection and document at their position in the chain.
use super::{CommandOutcome, Draft, Editor, Snapshot, Transaction};
use crate::{Command, Document, Edit, EditorConfig, Error, Mark, Schema, Selection, StoredCommand};
use std::{any::TypeId, sync::Arc};

#[derive(Clone, Copy)]
pub struct CommandContext<'a> {
    config: &'a EditorConfig,
    state: &'a Snapshot,
    owner: &'a Arc<()>,
    revision: u64,
}

impl<'a> CommandContext<'a> {
    pub(super) fn for_draft(
        config: &'a EditorConfig,
        state: &'a Snapshot,
        owner: &'a Arc<()>,
        revision: u64,
    ) -> Self {
        Self {
            config,
            state,
            owner,
            revision,
        }
    }
    pub fn document(self) -> &'a Document {
        &self.state.doc
    }
    pub fn selection(self) -> Selection {
        self.state.selection
    }
    pub fn config(self) -> &'a EditorConfig {
        self.config
    }

    pub fn schema(self) -> &'a Schema {
        self.config.schema()
    }
    /// Effective marks at a text cursor, including any explicit typing override.
    pub fn cursor_marks(self) -> Result<Option<Vec<Mark>>, Error> {
        let Selection::Text { anchor, head } = self.selection() else {
            return Ok(None);
        };
        if anchor != head {
            return Ok(None);
        }
        if let Some(marks) = &self.state.stored_marks {
            return Ok(Some(marks.clone()));
        }
        let pos = self.document().resolve(head)?;
        Ok(Some(pos.marks(self.schema())))
    }
    /// Read this plugin's state, including changes earlier in the current command chain.
    pub fn plugin_state<S: 'static>(self) -> Option<&'a S> {
        self.state.plugin_states.get()
    }

    pub fn transaction(self) -> Transaction {
        Transaction {
            owner: self.owner.clone(),
            revision: self.revision,
            initial_selection: self.selection(),
            draft: Draft::new(self.state),
            error: None,
            final_selection: None,
            final_marks: None,
        }
    }
    /// Plan a transaction containing one edit against the current draft.
    pub fn one_step(self, edit: impl Into<Edit>) -> Transaction {
        let mut tx = self.transaction();
        tx.push(edit);
        tx
    }

    /// Plan a plugin command against the current draft.
    pub fn plan<C: Command>(self, command: C) -> Result<Option<Transaction>, Error> {
        self.config
            .command(TypeId::of::<C>(), C::NAME)?
            .run_typed(self, &command)
    }

    /// Plan a typed command against the current draft, including earlier chained edits.
    pub fn plan_stored(self, command: &StoredCommand) -> Result<Option<Transaction>, Error> {
        self.config
            .command(command.command_type(), command.name())?
            .run_stored(self, command)
    }

    /// Compose typed commands against successive drafts of a single transaction.
    pub fn plan_chain(
        self,
        commands: impl IntoIterator<Item = StoredCommand>,
    ) -> Result<Option<Transaction>, Error> {
        Ok(self.evaluate(commands)?.map(|draft| Transaction {
            draft,
            ..self.transaction()
        }))
    }

    fn evaluate(
        self,
        commands: impl IntoIterator<Item = StoredCommand>,
    ) -> Result<Option<Draft>, Error> {
        let mut draft = Draft::new(self.state);
        for command in commands {
            let state = draft.state.clone();
            // Positions in a command result belong to this particular draft snapshot.
            let ctx = CommandContext {
                owner: &Arc::new(()),
                state: &state,
                ..self
            };
            let Some(tx) = ctx.plan_stored(&command)? else {
                return Ok(None);
            };
            ctx.apply_to(&mut draft, tx)?;
        }
        Ok(Some(draft))
    }

    pub(super) fn apply_to(self, draft: &mut Draft, tx: Transaction) -> Result<(), Error> {
        if !Arc::ptr_eq(self.owner, &tx.owner) {
            return Err(Error::ForeignTransaction);
        }
        if tx.revision != self.revision || tx.initial_selection != self.state.selection {
            return Err(Error::StaleRevision);
        }
        if let Some(error) = tx.error {
            return Err(error);
        }
        let mut next = tx.draft;
        if let Some(selection) = tx.final_selection {
            self.config.validate_selection(selection, &next.state.doc)?;
            next.state.selection = selection;
            next.state.stored_marks = None;
        }
        if let Some(marks) = tx.final_marks {
            let Selection::Text { anchor, head } = next.state.selection else {
                return Err(Error::InvalidEdit("stored marks need a text cursor".into()));
            };
            let schema = self.schema();
            let marks = marks
                .iter()
                .map(|mark| schema.bind_mark(mark))
                .collect::<Result<Vec<_>, _>>()?;
            let pos = next.state.doc.resolve(head)?;
            if anchor != head || !schema.allows_marks(&pos.parent().kind, &marks) {
                return Err(Error::InvalidEdit(
                    "stored marks do not apply at this selection".into(),
                ));
            }
            schema.check_marks(&marks)?;
            next.state.stored_marks = Some(marks);
        }
        draft.state = next.state;
        draft.map.append(next.map);
        Ok(())
    }
}

impl Editor {
    /// Whether this mark applies to typed text at the caret or appears in the selection.
    pub fn mark_active(&self, kind: &str) -> Result<bool, Error> {
        if let Some(marks) = self.command_context().cursor_marks()? {
            return Ok(marks.iter().any(|mark| mark.kind == kind));
        }
        let doc = self.document();
        for range in self.selection().ranges(doc) {
            if doc
                .root()
                .range_has_mark(range.start.0, range.end.0, kind)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn command_context(&self) -> CommandContext<'_> {
        CommandContext {
            config: self.config(),
            state: &self.state,
            owner: &self.owner,
            revision: self.revision(),
        }
    }
    pub fn chain(&mut self) -> CommandChain<'_> {
        CommandChain {
            editor: self,
            commands: Vec::new(),
        }
    }
    /// Run a command exported by an installed plugin.
    pub fn run<C: Command>(&mut self, command: C) -> Result<CommandOutcome, Error> {
        let planned = self.command_context().plan(command)?;
        self.apply_command(planned)
    }

    /// Check whether a command applies at the current selection.
    pub fn can<C: Command>(&self, command: C) -> Result<bool, Error> {
        let planned = self.command_context().plan(command)?;
        self.can_apply(planned)
    }

    /// Execute a command stored by a shortcut, menu, or other heterogeneous collection.
    pub fn run_stored(&mut self, command: &StoredCommand) -> Result<CommandOutcome, Error> {
        let planned = self.command_context().plan_stored(command)?;
        self.apply_command(planned)
    }

    /// Check a stored command against the current document without dispatching it.
    pub fn can_stored(&self, command: &StoredCommand) -> Result<bool, Error> {
        let planned = self.command_context().plan_stored(command)?;
        self.can_apply(planned)
    }

    fn can_apply(&self, planned: Option<Transaction>) -> Result<bool, Error> {
        let ctx = self.command_context();
        let Some(tx) = planned else {
            return Ok(false);
        };
        let mut draft = Draft::new(&self.state);
        ctx.apply_to(&mut draft, tx)?;
        self.config.accepts(crate::DocumentChange {
            default_text_block: self.config.default_text_block(),
            before: Some(self.document()),
            pasted: None,
            document: &draft.state.doc,
            schema: self.schema(),
        })
    }
}

/// Commit one history entry. A declined or failing command discards the whole chain.
#[must_use]
pub struct CommandChain<'a> {
    editor: &'a mut Editor,
    commands: Vec<StoredCommand>,
}

impl CommandChain<'_> {
    pub fn command(self, command: impl Command) -> Self {
        self.stored(StoredCommand::new(command))
    }

    pub fn stored(mut self, command: StoredCommand) -> Self {
        self.commands.push(command);
        self
    }

    pub fn run(self) -> Result<CommandOutcome, Error> {
        let Some(draft) = self.editor.command_context().evaluate(self.commands)? else {
            return Ok(CommandOutcome::NotApplicable);
        };
        self.editor.commit(draft).map(CommandOutcome::Applied)
    }
}
