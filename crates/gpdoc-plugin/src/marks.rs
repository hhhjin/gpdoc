use crate::{Command, Plugin, PluginSpec, paste::pasted_textblocks};
use gpdoc_core::{Attrs, CommandContext, DocPos, Edit, Error, Mark, Selection, Transaction};
use std::ops::Range;

/// A text formatting mark with standard editing commands and delimiter rules.
pub struct FormattingMark<S, U, T> {
    pub name: &'static str,
    pub set: S,
    pub unset: U,
    pub toggle: T,
    pub shortcut: Option<&'static str>,
    pub delimiters: &'static [&'static str],
}

impl<S: Command, U: Command, T: Command> From<FormattingMark<S, U, T>> for crate::Plugin {
    fn from(format: FormattingMark<S, U, T>) -> Self {
        let kind = format.name;
        Plugin::new(kind, |p| {
            p.mark(gpdoc_core::MarkSpec::new(kind));
            p.command::<S>(move |ctx, _| set_mark(kind)(ctx, Attrs::new()));
            p.command::<U>(move |ctx, _| unset_mark(kind)(ctx, Attrs::new()));
            p.command::<T>(move |ctx, _| toggle_mark(kind)(ctx, Attrs::new()));
            if let Some(key) = format.shortcut {
                p.shortcut(key, format.toggle);
            }
            if !format.delimiters.is_empty() {
                delimited_mark_rules(p, kind, format.delimiters);
            }
        })
    }
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
fn delimited_mark_input(
    kind: &'static str,
    delimiters: &'static [&'static str],
) -> impl Fn(gpdoc_core::InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error> {
    move |ctx| {
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
        for &delimiter in delimiters {
            let Some(body) = prefix.strip_suffix(delimiter) else {
                continue;
            };
            let Some((before, content)) = body.rsplit_once(delimiter) else {
                continue;
            };
            let marker = delimiter.chars().next().expect("nonempty mark delimiter");
            if content.is_empty() || content.contains(marker) {
                continue;
            }
            if kind == "code" {
                if before.ends_with(marker) {
                    continue;
                }
            } else if (!before.is_empty() && !before.ends_with(char::is_whitespace))
                || content.trim().is_empty()
            {
                continue;
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
                continue;
            }
            return Ok(Some(vec![
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
            ]));
        }
        Ok(None)
    }
}

/// Install the typing rule and a clipboard rule for each delimiter supported by the mark.
pub fn delimited_mark_rules(
    p: &mut PluginSpec,
    kind: &'static str,
    delimiters: &'static [&'static str],
) {
    p.mark_input_rule(kind, delimited_mark_input(kind, delimiters));
    for &delimiter in delimiters {
        let d = regex::escape(delimiter);
        let marker = regex::escape(
            &delimiter
                .chars()
                .next()
                .expect("nonempty delimiter")
                .to_string(),
        );
        let matcher =
            regex::Regex::new(&format!(r"{d}([^{marker}]+){d}")).expect("escaped delimiter");
        p.paste_rule(move |ctx| {
            let mut spans = Vec::new();
            pasted_textblocks(&ctx, kind, |start, text| {
                for captures in matcher.captures_iter(text) {
                    let matched = captures.get(0).unwrap();
                    let inner = captures.get(1).unwrap();
                    let before = &text[..matched.start()];
                    if kind == "code" {
                        if before.ends_with('`') || text[matched.end()..].starts_with('`') {
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
        });
    }
}
