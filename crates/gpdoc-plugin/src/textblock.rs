use crate::selection::selected_blocks;
use gpdoc_core::{
    Attrs, CommandContext, ContentRef, DocPos, Edit, Error, InputRuleContext, Selection,
    Transaction,
};
use regex::{Captures, Regex};

pub fn set_node(
    kind: &str,
    defaults: Attrs,
) -> impl Fn(CommandContext<'_>, Attrs) -> Result<Option<Transaction>, Error> {
    move |editor, args| {
        let selected = selected_blocks(editor);
        if selected.is_empty() || !selected.iter().all(|block| block.node.inline_content()) {
            return Ok(None);
        }
        let schema = editor.schema();
        let Some(target) = schema.node_type(kind) else {
            return Ok(None);
        };
        let mut attrs = defaults.clone();
        if selected.len() == 1 {
            attrs.extend(
                selected[0]
                    .node
                    .attrs
                    .iter()
                    .filter(|(key, _)| target.spec.attrs.contains_key(*key))
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
        attrs.extend(args);
        let mut tx = editor.transaction();
        let mut changed = false;
        for block in selected.into_iter().rev() {
            let pos = editor.document().root().resolve(block.before)?;
            let index = pos.index(pos.depth());
            if !pos
                .parent()
                .can_replace_with(schema, index, index + 1, kind, &[])
            {
                continue;
            }
            let ContentRef::Inline(runs) = block.node.content() else {
                continue;
            };
            let mut end = block.end() - 1;
            for run in runs.iter().rev() {
                let start = end - run.size();
                let range = DocPos(start)..DocPos(end);
                if kind == "codeBlock" && !run.is_text() {
                    let nodes = if editor.document().inline_text(run) == "\n" {
                        vec![
                            gpdoc_core::TextRun {
                                text: "\n".into(),
                                marks: Vec::new(),
                            }
                            .into(),
                        ]
                    } else {
                        Vec::new()
                    };
                    tx.push(Edit::ReplaceInline { range, nodes });
                } else {
                    for mark in run.marks().iter().filter(|m| !target.allows_mark(&m.kind)) {
                        tx.push(Edit::RemoveMark {
                            range: range.clone(),
                            kind: mark.kind.clone(),
                        });
                    }
                }
                end = start;
            }
            tx.push(Edit::SetBlock {
                before: DocPos(block.before),
                kind: kind.into(),
                attrs: attrs.clone(),
            });
            if block.node.kind == "codeBlock"
                && kind != "codeBlock"
                && let Some(linebreak) = schema.linebreak_replacement()
                && target.content_match.match_type(&linebreak.name).is_some()
            {
                let mut offset = block.before + 1;
                for ch in runs.iter().flat_map(|run| run.projected_text().chars()) {
                    if ch == '\n' {
                        tx.push(Edit::ReplaceInline {
                            range: DocPos(offset)..DocPos(offset + 1),
                            nodes: vec![gpdoc_core::Node::leaf(&linebreak.name, Attrs::new())],
                        });
                    }
                    offset += ch.len_utf16();
                }
            }
            changed = true;
        }
        if !changed {
            return Ok(None);
        }
        Ok(Some(tx))
    }
}

/// Switch the selected blocks back to the default text block if all already match.
pub fn toggle_node(
    kind: &str,
) -> impl Fn(CommandContext<'_>, Attrs) -> Result<Option<Transaction>, Error> {
    move |ctx, attrs| {
        let blocks = selected_blocks(ctx);
        if blocks.is_empty() {
            return Ok(None);
        }
        let active = blocks.iter().all(|block| {
            block.node.kind == kind
                && attrs
                    .iter()
                    .all(|(key, val)| block.node.attrs.get(key) == Some(val))
        });
        if active {
            set_node(ctx.config().default_text_block(), Attrs::new())(ctx, Attrs::new())
        } else {
            set_node(kind, Attrs::new())(ctx, attrs)
        }
    }
}

/// Match a whole text-block prefix at the caret. Captures provide the new node's attributes.
/// Returning None leaves the typed text in place and lets subsequent rules try.
pub fn textblock_type_input_rule(
    find: Regex,
    source: &'static str,
    target: &'static str,
    get_attributes: impl Fn(&Captures<'_>) -> Option<Attrs>,
) -> impl Fn(InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error> {
    move |ctx| {
        let Selection::Text { anchor, head } = ctx.selection else {
            return Ok(None);
        };
        if anchor != head || head != ctx.inserted.end || ctx.inserted.is_empty() {
            return Ok(None);
        }
        let pos = ctx
            .document
            .root()
            .resolve(head.0)
            .map_err(|_| Error::InvalidPosition(head))?;
        if pos.parent().kind != source || pos.depth() == 0 {
            return Ok(None);
        }
        let schema = ctx.schema;
        let prefix = pos
            .parent()
            .text_between(0, pos.parent_offset, "", "\u{fffc}")
            .map_err(|err| Error::InvalidEdit(err.to_string()))?;
        let Some(captures) = find.captures(&prefix) else {
            return Ok(None);
        };
        if captures.get(0).unwrap().range() != (0..prefix.len()) {
            return Ok(None);
        }
        let Some(attrs) = get_attributes(&captures) else {
            return Ok(None);
        };
        let parent_depth = pos.depth() - 1;
        let index = pos.index(parent_depth);
        if !pos
            .node(parent_depth)
            .can_replace_with(schema, index, index + 1, target, &[])
        {
            return Ok(None);
        }
        let content = &pos.parent().content;
        let suffix = content
            .cut(pos.parent_offset, content.size())
            .map_err(|err| Error::InvalidEdit(err.to_string()))?;
        if !schema.valid_content(target, &suffix) {
            return Ok(None);
        }
        let start = pos.start(pos.depth());
        Ok(Some(vec![
            Edit::ReplaceText {
                range: DocPos(start)..head,
                text: String::new(),
            },
            Edit::SetBlock {
                before: DocPos(start - 1),
                kind: target.into(),
                attrs,
            },
        ]))
    }
}
