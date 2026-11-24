use crate::{Command, PluginSpec, paste::pasted_textblocks};
use gpdoc_core::{
    Attrs, CommandContext, DocPos, Edit, Error, Mark, MarkSpec, Selection, Transaction,
};
use std::ops::Range;

/// Register an attribute-free mark and its set, unset, and toggle commands.
pub fn register_formatting_mark<Set: Command, Unset: Command, Toggle: Command>(
    p: &mut PluginSpec<'_, impl Sized>,
    kind: &'static str,
) {
    p.mark(MarkSpec::new(kind));
    p.command::<Set>(move |ctx, _| set_mark(kind)(ctx, Attrs::new()));
    p.command::<Unset>(move |ctx, _| unset_mark(kind)(ctx, Attrs::new()));
    p.command::<Toggle>(move |ctx, _| toggle_mark(kind)(ctx, Attrs::new()));
}

#[derive(Clone, Copy)]
enum MarkChange {
    Set,
    Unset,
    Toggle,
}

fn at_cursor(
    ctx: CommandContext<'_>,
    kind: &str,
    attrs: Attrs,
    change: MarkChange,
) -> Result<Option<Transaction>, Error> {
    let Some(mut marks) = ctx.cursor_marks()? else {
        return Ok(None);
    };
    let Selection::Text { head, .. } = ctx.selection() else {
        return Ok(None);
    };
    let schema = ctx.schema();
    let parent = ctx.document().resolve(head)?;
    if !schema
        .node_type(&parent.parent().kind)
        .is_some_and(|ty| ty.allows_mark(kind))
    {
        return Ok(None);
    }
    let old = marks.iter().find(|m| m.kind == kind);
    let active = old.is_some_and(|mark| {
        attrs
            .iter()
            .all(|(key, value)| mark.attrs.get(key) == Some(value))
    });
    if matches!(change, MarkChange::Unset) || matches!(change, MarkChange::Toggle) && active {
        marks.retain(|m| m.kind != kind);
    } else {
        let mut merged: Attrs = old
            .into_iter()
            .flat_map(|m| &m.attrs)
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        merged.extend(attrs);
        let mark = ctx.schema().bind_mark(&Mark::new(kind, merged))?;
        marks = mark.add_to_set(schema, &marks);
        if !marks.contains(&mark) {
            return Ok(None);
        }
    }
    let mut tx = ctx.transaction();
    tx.set_stored_marks(marks);
    Ok(Some(tx))
}

pub fn toggle_mark(
    kind: &'static str,
) -> impl Fn(CommandContext<'_>, Attrs) -> Result<Option<Transaction>, Error> {
    move |editor, args| {
        let Some(runs) = selected_runs(editor) else {
            return at_cursor(editor, kind, args, MarkChange::Toggle);
        };
        let mut found = false;
        let mut all_active = true;
        let schema = editor.schema();
        for (span, node) in runs {
            let parent = editor.document().resolve(span.start)?;
            if !schema
                .node_type(&parent.parent().kind)
                .is_some_and(|ty| ty.allows_mark(kind))
                || node
                    .marks()
                    .iter()
                    .any(|m| m.kind != kind && schema.excludes(&m.kind, kind))
            {
                continue;
            }
            found = true;
            all_active &= node.marks().iter().any(|m| {
                m.kind == kind
                    && args
                        .iter()
                        .all(|(key, value)| m.attrs.get(key) == Some(value))
            });
        }
        if !found {
            return Ok(None);
        }
        if all_active {
            unset_mark(kind)(editor, args)
        } else {
            set_mark(kind)(editor, args)
        }
    }
}

pub fn set_mark(
    kind: &'static str,
) -> impl Fn(CommandContext<'_>, Attrs) -> Result<Option<Transaction>, Error> {
    move |editor, args| {
        let Some(runs) = selected_runs(editor) else {
            return at_cursor(editor, kind, args, MarkChange::Set);
        };
        let schema = editor.schema();
        let mut tx = editor.transaction();
        let mut found = false;
        for (span, node) in runs {
            let pos = editor.document().resolve(span.start)?;
            if !schema
                .node_type(&pos.parent().kind)
                .is_some_and(|ty| ty.allows_mark(kind))
                || node
                    .marks()
                    .iter()
                    .any(|m| m.kind != kind && schema.excludes(&m.kind, kind))
            {
                continue;
            }
            let mut attrs = node
                .marks()
                .iter()
                .find(|m| m.kind == kind)
                .map(|m| m.attrs.clone())
                .unwrap_or_default();
            attrs.extend(args.iter().map(|(key, value)| (key.clone(), value.clone())));
            tx.push(Edit::SetMark {
                range: span,
                mark: Mark::new(kind, attrs),
            });
            found = true;
        }
        Ok(found.then_some(tx))
    }
}

pub fn unset_mark(
    kind: &'static str,
) -> impl Fn(CommandContext<'_>, Attrs) -> Result<Option<Transaction>, Error> {
    move |editor, args| {
        let Some(runs) = selected_runs(editor) else {
            return at_cursor(editor, kind, args, MarkChange::Unset);
        };
        let mut tx = editor.transaction();
        for (range, node) in runs {
            if node.marks().iter().any(|mark| mark.kind == kind) {
                tx.push(Edit::RemoveMark {
                    range,
                    kind: kind.into(),
                });
            }
        }
        Ok(Some(tx))
    }
}

fn selected_runs<'a>(
    editor: CommandContext<'a>,
) -> Option<impl Iterator<Item = (Range<DocPos>, &'a gpdoc_core::Node)>> {
    let selection = editor.selection();
    (!selection.bounds(editor.document()).is_empty())
        .then(|| selection.inline_nodes(editor.document()))
}

/// Convert a trailing delimiter pair to a mark. Edit only the delimiters so
/// existing marks and inline objects inside the span retain their identity.
pub fn delimited_mark_input(
    kind: &'static str,
    delimiter: &'static str,
) -> impl Fn(gpdoc_core::InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error> {
    let closing_char = delimiter
        .chars()
        .next_back()
        .expect("nonempty mark delimiter");
    move |ctx| {
        if ctx.last_typed_char.is_some_and(|ch| ch != closing_char) {
            return Ok(None);
        }
        let Selection::Text { head, anchor } = ctx.selection else {
            return Ok(None);
        };
        if head != anchor || head != ctx.inserted.end || ctx.inserted.is_empty() {
            return Ok(None);
        }
        let schema = ctx.schema;
        let pos = ctx.document.resolve(head)?;
        if !schema
            .node_type(&pos.parent().kind)
            .is_some_and(|ty| ty.allows_mark(kind))
        {
            return Ok(None);
        }
        let prefix = pos
            .parent()
            .text_between(0, pos.parent_offset, "", "\u{fffc}")?;
        let Some(body) = prefix.strip_suffix(delimiter) else {
            return Ok(None);
        };
        let Some((before, content)) = body.rsplit_once(delimiter) else {
            return Ok(None);
        };
        let marker = delimiter.chars().next().expect("nonempty mark delimiter");
        if content.is_empty() || content.contains(marker) {
            return Ok(None);
        }
        if kind == "code" {
            if before.ends_with(marker) {
                return Ok(None);
            }
        } else if (!before.is_empty() && !before.ends_with(char::is_whitespace))
            || content.trim().is_empty()
        {
            return Ok(None);
        }
        let width = delimiter.encode_utf16().count();
        let inside_end = DocPos(head.0 - width);
        let inside_start = DocPos(inside_end.0 - content.encode_utf16().count());
        let start = DocPos(inside_start.0 - width);
        if ctx
            .document
            .inline_nodes_in(inside_start..head)
            .any(|(_, node)| {
                node.marks()
                    .iter()
                    .any(|m| m.kind != kind && schema.excludes(&m.kind, kind))
            })
        {
            return Ok(None);
        }
        Ok(Some(vec![
            Edit::ReplaceText {
                range: inside_end..head,
                text: String::new(),
            },
            Edit::ReplaceText {
                range: start..inside_start,
                text: String::new(),
            },
            Edit::SetMark {
                range: start..DocPos(inside_end.0 - width),
                mark: Mark::new(kind, Attrs::new()),
            },
        ]))
    }
}

/// Transform matching delimiter pairs in pasted text, from right to left.
pub fn delimited_mark_paste(
    kind: &'static str,
    delimiter: &'static str,
) -> impl Fn(gpdoc_core::InputRuleContext<'_>) -> Result<Vec<Edit>, Error> {
    let d = regex::escape(delimiter);
    let marker = delimiter.chars().next().expect("nonempty delimiter");
    let escaped_marker = regex::escape(&marker.to_string());
    let matcher =
        regex::Regex::new(&format!(r"{d}([^{escaped_marker}]+){d}")).expect("escaped delimiter");
    move |ctx| {
        let mut spans = Vec::new();
        pasted_textblocks(&ctx, kind, |start, text| {
            for captures in matcher.captures_iter(text) {
                let matched = captures.get(0).unwrap();
                let inner = captures.get(1).unwrap();
                let before = &text[..matched.start()];
                if kind == "code" {
                    if before.ends_with(marker) || text[matched.end()..].starts_with(marker) {
                        continue;
                    }
                } else if (!before.is_empty() && !before.ends_with(char::is_whitespace))
                    || inner.as_str().trim().is_empty()
                {
                    continue;
                }
                let from = DocPos(start.0 + before.encode_utf16().count());
                let to = DocPos(from.0 + matched.as_str().encode_utf16().count());
                let excludes = ctx.document.inline_nodes_in(from..to).any(|(_, node)| {
                    node.marks()
                        .iter()
                        .any(|m| m.kind != kind && ctx.schema.excludes(&m.kind, kind))
                });
                if !excludes {
                    spans.push(from..to)
                }
            }
        })?;
        let mut steps = Vec::new();
        let width = delimiter.encode_utf16().count();
        for span in spans.into_iter().rev() {
            steps.push(Edit::ReplaceText {
                range: DocPos(span.end.0 - width)..span.end,
                text: String::new(),
            });
            steps.push(Edit::ReplaceText {
                range: span.start..DocPos(span.start.0 + width),
                text: String::new(),
            });
            steps.push(Edit::SetMark {
                range: span.start..DocPos(span.end.0 - 2 * width),
                mark: Mark::new(kind, Attrs::new()),
            });
        }
        Ok(steps)
    }
}
