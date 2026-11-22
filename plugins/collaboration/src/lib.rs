//! A headless editor bound to a Yrs XML fragment compatible with Tiptap.
//! Seed once with [Collaboration::new]. Other peers join using [Collaboration::from_update].
//! Send V1 updates through the application's transport and use [Collaboration::undo] for local history.
#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::CollaborationView;

mod position;
mod tree;
pub use position::RelativeSelection;
pub use yrs::{StateVector, updates::decoder::Decode, updates::encoder::Encode};

use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::prelude::{Editor, Error as EditorError, Selection};
use position::Positions;
use std::sync::Arc;
use web_time::{SystemTime, UNIX_EPOCH};
use yrs::{Doc, OffsetKind, Options, ReadTxn, Transact, UndoManager, Update, XmlFragmentRef};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Editor(#[from] EditorError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("unsupported collaborative content: {0}")]
    Content(String),
    #[error("invalid Yjs update: {0}")]
    Update(String),
}

fn document() -> Doc {
    Doc::with_options(Options {
        offset_kind: OffsetKind::Utf16,
        ..Default::default()
    })
}

fn timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn apply(doc: &Doc, update: &[u8]) -> Result<(), Error> {
    let update = Update::decode_v1(update).map_err(|e| Error::Update(e.to_string()))?;
    doc.transact_mut_with("remote")
        .apply_update(update)
        .map_err(|e| Error::Update(e.to_string()))
}

pub struct Collaboration {
    doc: Doc,
    field: String,
    root: XmlFragmentRef,
    history: UndoManager,
    editor: Editor,
}

impl Collaboration {
    /// Tiptap uses the field name `default` unless the host configures a different fragment.
    pub fn new(editor: Editor, field: impl Into<String>) -> Result<Self, Error> {
        let field = field.into();
        let doc = document();
        let root = doc.get_or_insert_xml_fragment(field.as_str());
        let seed = tree::encode(editor.document().blocks())?;
        tree::seed(&mut doc.transact_mut(), &root, &seed);
        Ok(Self::bind(doc, root, field, editor))
    }

    pub fn from_update(
        config: &EditorConfig,
        field: impl Into<String>,
        update: &[u8],
    ) -> Result<Self, Error> {
        let field = field.into();
        let doc = document();
        let root = doc.get_or_insert_xml_fragment(field.as_str());
        apply(&doc, update)?;
        let blocks = tree::decode(config.schema(), tree::read(&doc.transact(), &root, 0)?)?;
        let mut editor = gpdoc_plugin::core::Editor::from_document(config, blocks.clone())?;
        editor.load_snapshot(blocks)?;
        Ok(Self::bind(doc, root, field, editor))
    }

    fn bind(doc: Doc, root: XmlFragmentRef, field: String, editor: Editor) -> Self {
        let mut history = UndoManager::with_options(yrs::undo::Options {
            capture_timeout_millis: 500,
            tracked_origins: Default::default(),
            capture_transaction: None,
            timestamp: Arc::new(timestamp_millis),
            init_undo_stack: Vec::new(),
            init_redo_stack: Vec::new(),
        });
        history.expand_scope(&doc, &root);
        history.include_origin("gpdoc");
        Self {
            doc,
            root,
            field,
            history,
            editor,
        }
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// Construct ephemeral Yjs awareness state using this peer's document identity.
    pub fn awareness(&self) -> yrs::sync::Awareness {
        let doc = Doc::with_options(Options::with_client_id(self.doc.client_id()));
        yrs::sync::Awareness::with_clock(doc, timestamp_millis)
    }

    /// Apply an editor action atomically and return only the update produced by this action.
    pub fn edit<R>(
        &mut self,
        edit: impl FnOnce(&mut Editor) -> Result<R, EditorError>,
    ) -> Result<(R, Vec<u8>), Error> {
        let mut editor = self.editor.fork();
        let result = edit(&mut editor)?;
        let update = self
            .record(&editor)?
            .unwrap_or_else(|| Update::EMPTY_V1.to_vec());
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
            let new = tree::encode(editor.document().blocks())?;
            let mut tx = self.doc.transact_mut_with("gpdoc");
            let old = tree::read(&tx, &self.root, 0)?;
            tree::reconcile(&mut tx, &self.root, &old, &new, editor.schema(), &map);
            Some(tx.encode_update_v1())
        };
        self.history.reset();
        self.editor = editor.fork();
        Ok(update)
    }

    pub fn select(&mut self, selection: Selection) -> Result<(), Error> {
        Ok(self.editor.select(selection)?)
    }

    pub fn relative_selection(&self) -> Option<RelativeSelection> {
        let tx = self.doc.transact();
        Positions::new(&tx, self.root.clone(), self.editor.schema()).relative(
            &tx,
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
        let tx = self.doc.transact();
        let selection = Positions::new(&tx, self.root.clone(), editor.schema()).absolute(
            &tx,
            relative,
            editor.config(),
        )?;
        selection.validate(editor.document()).ok()?;
        Some(selection)
    }

    pub fn state_vector(&self) -> StateVector {
        self.doc.transact().state_vector()
    }

    /// Pass the receiver's state vector, or an empty vector to export the whole document.
    pub fn update_since(&self, vector: &StateVector) -> Vec<u8> {
        self.doc.transact().encode_state_as_update_v1(vector)
    }

    pub fn apply_update(&mut self, update: &[u8]) -> Result<(), Error> {
        // A merge can yield schema-invalid content even when both starting editors were valid.
        // Validate on a replica before committing the update to the live CRDT and undo history.
        let check = document();
        let root = check.get_or_insert_xml_fragment(self.field.as_str());
        apply(&check, &self.update_since(&StateVector::default()))?;
        apply(&check, update)?;
        let blocks = tree::decode(
            self.editor.schema(),
            tree::read(&check.transact(), &root, 0)?,
        )?;
        let mut next = self.editor.fork();
        next.load_snapshot(blocks)?;
        let pin = self.relative_selection();
        apply(&self.doc, update)?;
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
        let vector = self.state_vector();
        let pin = self.relative_selection();
        let changed = if redo {
            self.history.redo_blocking()
        } else {
            self.history.undo_blocking()
        };
        if !changed {
            return Err(EditorError::NoHistory.into());
        }
        let next = (|| {
            let blocks = tree::decode(
                self.editor.schema(),
                tree::read(&self.doc.transact(), &self.root, 0)?,
            )?;
            let mut next = self.editor.fork();
            next.load_snapshot(blocks)?;
            Ok::<_, Error>(next)
        })();
        let mut next = match next {
            Ok(next) => next,
            Err(err) => {
                if redo {
                    self.history.undo_blocking();
                } else {
                    self.history.redo_blocking();
                }
                return Err(err);
            }
        };
        if let Some(selection) = pin.and_then(|p| self.resolve_selection_in(&p, &next)) {
            let _ = next.select(selection);
        }
        self.editor = next;
        Ok(self.update_since(&vector))
    }
}
