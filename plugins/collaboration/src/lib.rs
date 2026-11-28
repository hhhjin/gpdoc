//! A headless editor bound to a Yjs XML fragment compatible with Tiptap.
//! Seed once with [Collaboration::new]. Other peers join using [Collaboration::from_update].
//! Send V1 updates through the application's transport and use [Collaboration::undo] for local history.
#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::CollaborationView;

mod position;
mod tree;
pub use position::RelativeSelection;
pub use y_octo::StateVector;

use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::prelude::{Editor, Error as EditorError, Selection};
use once_cell::sync::OnceCell;
use position::Positions;
use y_octo::{
    Doc, DocOptions, DocSession, LocalUpdate, SessionConfig, SessionTxOrigin, SessionUndoConfig,
    TypeIdentity, TypeTarget, UndoManager, UndoOptions, Update, XMLFragment, YTypeKind,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Editor(#[from] EditorError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("unsupported collaborative content: {0}")]
    Content(String),
    #[error(transparent)]
    Crdt(#[from] y_octo::JwstCodecError),
}

pub struct Collaboration {
    doc: Doc,
    root: XMLFragment,
    field: String,
    history: UndoManager,
    // This replica authors a single edit at a time. A failed edit can discard
    // it without changing the committed document or its undo/redo stacks.
    writer: OnceCell<DocSession>,
    editor: Editor,
}

fn writer_config() -> SessionConfig {
    SessionConfig {
        undo: SessionUndoConfig::Disabled,
    }
}

impl Collaboration {
    /// Tiptap uses the field name `default` unless the host configures a different fragment.
    pub fn new(editor: Editor, field: impl Into<String>) -> Result<Self, Error> {
        let field = field.into();
        let mut writer = DocSession::with_config(DocOptions::default(), writer_config());
        let target = writer.ensure_root(&field, YTypeKind::XMLFragment)?.into();
        tree::seed(
            &mut writer,
            &target,
            &tree::encode(editor.document().blocks(), 0)?,
        )?;
        let mut doc = Doc::new();
        doc.apply_update(Update::decode_v1(writer.encode_update_v1()?)?)?;
        let mut session = Self::bind(doc, field, editor)?;
        session.writer = OnceCell::with_value(writer);
        Ok(session)
    }

    pub fn from_update(
        config: &EditorConfig,
        field: impl Into<String>,
        update: &[u8],
    ) -> Result<Self, Error> {
        let field = field.into();
        let mut doc = Doc::new();
        let root = doc.get_or_create_xml_fragment(&field)?;
        doc.apply_update(Update::decode_v1(update)?)?;
        let blocks = tree::decode(config.schema(), tree::read(&doc, &root)?)?;
        let mut editor = Editor::from_document(config, blocks.clone())?;
        editor.load_snapshot(blocks)?;
        Self::bind(doc, field, editor)
    }

    fn bind(doc: Doc, field: String, editor: Editor) -> Result<Self, Error> {
        let root = doc.get_or_create_xml_fragment(&field)?;
        let history = UndoManager::new(
            &doc,
            [(&root).into()],
            UndoOptions {
                capture_timeout: 0,
                ..Default::default()
            },
        )?;
        Ok(Self {
            doc,
            root,
            field,
            history,
            writer: OnceCell::new(),
            editor,
        })
    }

    fn target(&self) -> TypeTarget {
        TypeIdentity::Root(self.field.clone()).into()
    }

    fn fresh_writer(&self) -> Result<DocSession, Error> {
        let mut writer = DocSession::from_update_v1(
            &self.doc.encode_update_v1()?,
            DocOptions::default(),
            writer_config(),
        )?;
        writer.ensure_root(&self.field, YTypeKind::XMLFragment)?;
        Ok(writer)
    }

    fn writer(&self) -> Result<&DocSession, Error> {
        self.writer.get_or_try_init(|| self.fresh_writer())
    }

    fn sync_writer(&mut self, update: &[u8]) {
        if let Some(writer) = self.writer.get_mut()
            && writer.apply_update_v1(update).is_err()
        {
            self.writer.take();
        }
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// Construct ephemeral Yjs awareness state using this peer's document identity.
    pub fn awareness(&self) -> y_octo::Awareness {
        y_octo::Awareness::new(self.doc.client())
    }

    /// Apply an editor action atomically and return only the update produced by this action.
    pub fn edit<R>(
        &mut self,
        edit: impl FnOnce(&mut Editor) -> Result<R, EditorError>,
    ) -> Result<(R, Vec<u8>), Error> {
        let mut editor = self.editor.fork();
        let result = edit(&mut editor)?;
        let update = self.record(&editor)?.unwrap_or_else(|| vec![0, 0]);
        Ok((result, update))
    }

    /// Record committed edits from a view displaying a fork of this session's editor.
    /// An editor from a stale branch cannot overwrite newly received remote updates.
    pub fn record(&mut self, editor: &Editor) -> Result<Option<Vec<u8>>, Error> {
        let map = editor
            .map_since(self.editor.document())
            .ok_or(EditorError::StaleRevision)?;
        let update = if editor
            .document()
            .root()
            .ptr_eq(self.editor.document().root())
        {
            None
        } else {
            let new = tree::encode(editor.document().blocks(), 0)?;
            let old = tree::read(&self.doc, &self.root)?;
            let mut writer = match self.writer.take() {
                Some(writer) => writer,
                None => self.fresh_writer()?,
            };
            writer.begin_tx(SessionTxOrigin::Untracked)?;
            tree::reconcile(
                &mut writer,
                &self.target(),
                &old,
                &new,
                editor.schema(),
                &map,
            )?;
            let update = writer.commit_tx()?.map(|commit| commit.update);
            if let Some(bytes) = &update {
                let applied = self
                    .doc
                    .apply_local_update(Update::decode_v1(bytes)?, None)?;
                if let LocalUpdate::MissingDependencies(_) = applied {
                    return Err(Error::Content(
                        "local edit depends on missing document updates".into(),
                    ));
                }
            }
            self.writer = OnceCell::with_value(writer);
            update
        };
        self.editor = editor.fork();
        Ok(update)
    }

    pub fn select(&mut self, selection: Selection) -> Result<(), Error> {
        Ok(self.editor.select(selection)?)
    }

    pub fn relative_selection(&self) -> Option<RelativeSelection> {
        let writer = self.writer().ok()?;
        Positions::new(writer, &self.target(), self.editor.schema())?.relative(
            writer,
            self.editor.selection(),
            self.editor.document(),
            self.editor.config(),
        )
    }

    pub fn resolve_selection(&self, relative: &RelativeSelection) -> Option<Selection> {
        self.resolve_selection_in(relative, &self.editor)
    }

    fn resolve_selection_in(
        &self,
        relative: &RelativeSelection,
        editor: &Editor,
    ) -> Option<Selection> {
        let selection = Positions::new(self.writer().ok()?, &self.target(), editor.schema())?
            .absolute(&self.doc, relative, editor.config())?;
        selection.validate(editor.document()).ok()?;
        Some(selection)
    }

    pub fn state_vector(&self) -> StateVector {
        self.doc.get_state_vector()
    }

    /// Pass the receiver's state vector, or an empty vector to export the whole document.
    pub fn update_since(&self, vector: &StateVector) -> Result<Vec<u8>, Error> {
        Ok(self.doc.encode_state_as_update_v1(vector)?)
    }

    pub fn apply_update(&mut self, update: &[u8]) -> Result<(), Error> {
        // A merge can yield schema-invalid content even when both starting editors were valid.
        // Validate on a replica before committing the update to the live CRDT and undo history.
        let update_data = Update::decode_v1(update)?;
        let mut check = Doc::try_from_binary_v1(self.doc.encode_update_v1()?)?;
        let root = check.get_or_create_xml_fragment(&self.field)?;
        check.apply_update(update_data.clone())?;
        let blocks = tree::decode(self.editor.schema(), tree::read(&check, &root)?)?;
        let mut next = self.editor.fork();
        next.load_snapshot(blocks)?;
        let pin = self.relative_selection();
        self.doc.apply_update(update_data)?;
        self.sync_writer(update);
        if next.document() != self.editor.document() {
            if let Some(selection) = pin.and_then(|p| self.resolve_selection_in(&p, &next)) {
                let _ = next.select(selection);
            }
            self.editor = next;
        }
        Ok(())
    }

    pub fn undo(&mut self) -> Result<Vec<u8>, Error> {
        self.history_step(false)
    }
    pub fn redo(&mut self) -> Result<Vec<u8>, Error> {
        self.history_step(true)
    }

    fn history_step(&mut self, redo: bool) -> Result<Vec<u8>, Error> {
        let pin = self.relative_selection();
        let vector = self.state_vector();
        if redo {
            self.history.redo()?
        } else {
            self.history.undo()?
        }
        .ok_or(EditorError::NoHistory)?;
        let next = (|| {
            let blocks = tree::decode(self.editor.schema(), tree::read(&self.doc, &self.root)?)?;
            let mut next = self.editor.fork();
            next.load_snapshot(blocks)?;
            Ok::<_, Error>(next)
        })();
        let mut next = match next {
            Ok(next) => next,
            Err(err) => {
                if redo {
                    self.history.undo()?;
                } else {
                    self.history.redo()?;
                }
                self.writer.take();
                return Err(err);
            }
        };
        let update = self.update_since(&vector)?;
        self.sync_writer(&update);
        if let Some(selection) = pin.and_then(|p| self.resolve_selection_in(&p, &next)) {
            let _ = next.select(selection);
        }
        self.editor = next;
        Ok(update)
    }
}
