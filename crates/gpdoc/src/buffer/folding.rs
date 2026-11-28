use super::*;
use crate::Fold;
use gpdoc_core::{Affinity, Edit, PositionMap, Value};
use std::collections::BTreeSet;

fn mapped(open: &BTreeSet<DocPos>, map: &PositionMap) -> BTreeSet<DocPos> {
    open.iter()
        .filter_map(|&pos| {
            let next = map.map(pos, Affinity::After);
            (!next.deleted).then_some(next.position)
        })
        .collect()
}

impl InputBuffer {
    pub(crate) fn folds(&self) -> &[Fold] {
        &self.projection.folds
    }

    pub(crate) fn hidden(&self, at: DocPos) -> bool {
        self.projection.hidden(at)
    }

    pub(crate) fn toggle_fold(
        &mut self,
        before: DocPos,
        was_open: bool,
        revision: u64,
    ) -> Result<(), Error> {
        if revision != self.editor.revision() || self.has_preedit() {
            return Err(Error::StaleRevision);
        }
        let Some(fold) = self
            .folds()
            .iter()
            .find(|d| d.before == before && d.open == was_open)
        else {
            return Err(Error::StaleRevision);
        };
        let next_selection = (was_open
            && fold.contains_endpoint(self.editor.selection(), self.editor.document()))
        .then(|| Selection::caret(fold.region.fallback));
        let node = self
            .editor
            .document()
            .node_at(before)
            .ok_or(Error::StaleRevision)?;
        if let Some(attr) = fold.region.open_attribute {
            let mut attrs = node.attrs.clone();
            attrs.insert(attr.into(), Value::Bool(!was_open));
            let mut tx = self.editor.transaction();
            tx.push(Edit::SetBlock {
                before,
                kind: node.kind.clone(),
                attrs,
            });
            if let Some(selection) = next_selection {
                tx.select(selection)
            }
            return self.apply_node_view(tx).map(|_| ());
        }
        if let Some(selection) = next_selection {
            self.editor.select(selection)?
        }
        let locals = &mut self.open_folds;
        if was_open {
            locals.remove(&before);
        } else {
            locals.insert(before);
        }
        self.refresh_projection(&PositionMap::default())
    }

    pub(crate) fn refresh_projection(&mut self, map: &PositionMap) -> Result<(), Error> {
        self.open_folds = mapped(&self.open_folds, map);
        self.projection = self.projection.rebuild(
            self.editor.document(),
            self.editor.config(),
            &self.open_folds,
            &self.fold_specs,
        );
        self.restore_visible_selection()
    }

    pub(super) fn restore_visible_selection(&mut self) -> Result<(), Error> {
        // A host command or undo can place a text cursor in a folded body.
        if let Some(fold) = self.folds().iter().find(|d| {
            !d.open && d.contains_endpoint(self.editor.selection(), self.editor.document())
        }) {
            let visible = Selection::caret(fold.region.fallback);
            self.editor.select(visible)?;
        }
        Ok(())
    }

    pub(super) fn preview_projection(&self, input: &TextInputTransaction) -> Projection {
        let opened = mapped(&self.open_folds, input.position_map());
        self.projection.rebuild(
            input.document(),
            self.editor.config(),
            &opened,
            &self.fold_specs,
        )
    }
}
