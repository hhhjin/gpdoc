use super::{AppliedChange, Editor, Selection};
use crate::{DocPos, Edit, EditorConfig, Error, Node, ResolvedPos};

impl Editor {
    /// Break the current paragraph. Container handlers run from innermost to
    /// outermost before the text block's handler and the default split.
    pub fn paragraph_break(&mut self) -> Result<AppliedChange, Error> {
        let Selection::Text { anchor, head } = self.selection() else {
            // Structural selections use their native text-input replacement behavior.
            let mut input = self.begin_text_input();
            input.replace(self.selection(), &["", ""])?;
            return self.commit_text_input(input);
        };
        let from = anchor.min(head);
        let to = anchor.max(head);
        let pos = self.document().resolve(from)?;
        let shared = pos.shared_depth(to.0);
        let containers = (0..=shared.min(pos.depth() - 1)).rev();
        let text_block = (shared == pos.depth()).then_some(shared);
        for depth in containers.chain(text_block) {
            if let Some(handler) = self.config().break_handler(&pos.node(depth).kind)
                && let Some(tx) = handler(self.command_context())?
            {
                return self.apply(tx);
            }
        }

        let mut tx = self.transaction();
        if from != to {
            tx.push(Edit::ReplaceText {
                range: from..to,
                text: String::new(),
            });
        }
        let pos = tx.document()?.resolve(from)?;
        if let Some((edit, caret)) = boundary_break(self.config(), &pos)? {
            tx.push(edit);
            tx.select(Selection::caret(caret));
        } else {
            tx.push(Edit::Split { at: from });
            tx.select(Selection::caret(DocPos(from.0 + 2)));
        }
        self.apply(tx)
    }
}

/// A boundary creates an empty sibling. An empty non-default block can instead
/// become the default text block allowed in its existing slot.
fn boundary_break(
    config: &EditorConfig,
    pos: &ResolvedPos,
) -> Result<Option<(Edit, DocPos)>, Error> {
    let depth = pos.depth();
    let block = pos.parent();
    let at_start = pos.parent_offset == 0;
    let at_end = pos.parent_offset == block.content.size();
    if !at_start && !at_end {
        return Ok(None);
    }

    let schema = config.schema();
    let parent = pos.node(depth - 1);
    let index = pos.index(depth - 1);
    let before = pos.before(depth)?;
    let empty_block_at = |index, remove| -> Result<Option<Node>, Error> {
        let allowed = parent.content_match_at(schema, index)?;
        let candidates =
            (0..allowed.edge_count()).filter_map(|i| allowed.edge(i).map(|(name, _)| name));
        Ok(std::iter::once(config.default_text_block())
            .chain(candidates)
            .find(|name| {
                schema.node_type(name).is_some_and(|ty| {
                    ty.is_textblock()
                        && ty.content_match.valid_end()
                        && !ty.spec.has_required_attrs()
                }) && parent.can_replace_with(schema, index, index + remove, name, &[])
            })
            .map(|kind| Node::text(kind, "")))
    };

    if at_start
        && at_end
        && block.kind != config.default_text_block()
        && let Some(default) = empty_block_at(index, 1)?
        && default.kind != block.kind
    {
        let range = DocPos(before)..DocPos(before + block.size());
        return Ok(Some((
            Edit::ReplaceNodes {
                range,
                nodes: vec![default],
            },
            DocPos(before + 1),
        )));
    }

    let after = usize::from(at_end);
    let Some(default) = empty_block_at(index + after, 0)? else {
        return Ok(None);
    };
    let at = DocPos(if at_end {
        before + block.size()
    } else {
        before
    });
    let caret = DocPos(if at_end { at.0 + 1 } else { before + 3 });
    Ok(Some((
        Edit::ReplaceNodes {
            range: at..at,
            nodes: vec![default],
        },
        caret,
    )))
}
