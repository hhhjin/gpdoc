//! Folding a document range uses the same projection for display, hit testing, and IME.
use crate::EditorView;
use gpdoc_core::{DocPos, Document, Node, Selection};
use gpui::{App, Entity, Window};
use std::{collections::BTreeMap, ops::Range};

pub(crate) type FoldSpecs = BTreeMap<String, FoldSpec>;

/// A function identifying the collapsible range in an editable block.
pub type FoldSpec = fn(&Node, DocPos) -> Option<FoldRegion>;

#[derive(Clone, Debug)]
pub struct FoldRegion {
    /// Move a selection inside the hidden body to this position when folded.
    pub fallback: DocPos,
    pub body: Range<DocPos>,
    /// A boolean document attribute. With no attribute, openness is view-local.
    pub open_attribute: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct Fold {
    pub before: DocPos,
    pub region: FoldRegion,
    pub open: bool,
}

impl Fold {
    pub(crate) fn contains_endpoint(&self, selection: Selection, doc: &Document) -> bool {
        let bounds = selection.bounds(doc);
        self.region.body.contains(&bounds.start) || self.region.body.contains(&bounds.end)
    }
}

/// The revision and open state observed by a plugin's disclosure button.
pub struct FoldControl {
    view: Entity<EditorView>,
    before: DocPos,
    open: bool,
    revision: u64,
}

impl FoldControl {
    /// Capture the visible fold starting at the position before its node.
    pub fn at(view: &Entity<EditorView>, before: DocPos, cx: &App) -> Option<Self> {
        let editor = view.read(cx);
        let folds = editor.folds();
        let index = folds
            .binary_search_by_key(&before, |fold| fold.before)
            .ok()?;
        Some(Self {
            view: view.clone(),
            before,
            open: folds[index].open,
            revision: editor.editor().revision(),
        })
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Ignore stale controls after edits or composition. Restore editor focus on click.
    pub fn toggle(&self, window: &mut Window, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            view.focus_in(window, cx);
            let result = view
                .buffer
                .toggle_fold(self.before, self.open, self.revision);
            view.edited(result, window, cx);
        });
    }
}

impl EditorView {
    /// Visible fold regions in document order.
    pub fn folds(&self) -> &[Fold] {
        self.buffer.folds()
    }
}
