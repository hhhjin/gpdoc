//! Markdown link typing and paste rules, including one level of balanced parentheses.
use super::{LinkOptions, allowed};
use gpdoc_plugin::paste::pasted_textblocks;
use gpdoc_plugin::prelude::{Attrs, DocPos, Edit, Mark, PluginSpec, Selection, Value};
use regex::Regex;
use std::{
    ops::Range,
    sync::{Arc, LazyLock},
};

struct Match {
    span: Range<DocPos>,
    label_end: DocPos,
    attrs: Attrs,
}

fn matches(text: &str, from: DocPos, options: &LinkOptions) -> Vec<Match> {
    static SYNTAX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
        r#"\[([^\[\]]+)\]\(((?:[^\s()]|\([^\s()]*\))+)(?:\s+(?:"([^"]*)"|'([^']*)'|“([^”]*)”|‘([^’]*)’))?\)"#
    ).expect("markdown link syntax")
    });
    let mut out = vec![];
    let mut scanner = 0;
    let mut open_fence = 0;
    for capture in SYNTAX.captures_iter(text) {
        let syntax = capture.get(0).unwrap();
        let start = syntax.start();
        let bytes = text.as_bytes();
        while scanner < start {
            if bytes[scanner] != b'`' {
                scanner += 1;
                continue;
            }
            let run = bytes[scanner..start]
                .iter()
                .take_while(|&&c| c == b'`')
                .count();
            if open_fence == run {
                open_fence = 0
            } else if open_fence == 0 && !escaped(bytes, scanner) {
                open_fence = run
            }
            scanner += run;
        }
        let label = capture.get(1).unwrap();
        let href = capture.get(2).unwrap().as_str();
        if open_fence != 0
            || text[..start].ends_with('!')
            || escaped(bytes, start)
            || label.as_str().trim().is_empty()
            || !allowed(href, &options.protocols)
        {
            continue;
        }
        let title = (3..=6)
            .find_map(|i| capture.get(i))
            .map(|m| m.as_str())
            .filter(|t| !t.is_empty());
        let start = DocPos(from.0 + text[..start].encode_utf16().count());
        out.push(Match {
            span: start..DocPos(start.0 + syntax.as_str().encode_utf16().count()),
            label_end: DocPos(start.0 + 1 + label.as_str().encode_utf16().count()),
            attrs: [
                ("href".into(), Value::String(href.into())),
                (
                    "title".into(),
                    title.map_or(Value::Null, |t| Value::String(t.into())),
                ),
            ]
            .into(),
        });
    }
    out
}

fn escaped(bytes: &[u8], at: usize) -> bool {
    bytes[..at]
        .iter()
        .rev()
        .take_while(|&&c| c == b'\\')
        .count()
        % 2
        == 1
}

fn convert(m: Match) -> [Edit; 3] {
    [
        Edit::ReplaceText {
            range: m.label_end..m.span.end,
            text: String::new(),
        },
        Edit::ReplaceText {
            range: m.span.start..DocPos(m.span.start.0 + 1),
            text: String::new(),
        },
        Edit::SetMark {
            range: m.span.start..DocPos(m.label_end.0 - 1),
            mark: Mark::new("link", m.attrs),
        },
    ]
}

pub(super) fn rules(p: &mut PluginSpec, options: Arc<LinkOptions>) {
    let typed = options.clone();
    p.mark_input_rule("link", move |ctx| {
        let Selection::Text { anchor, head } = ctx.selection else {
            return Ok(None);
        };
        if anchor != head || head != ctx.inserted.end {
            return Ok(None);
        }
        let pos = ctx.document.resolve(head)?;
        let schema = ctx.schema;
        if !schema
            .node_type(&pos.parent().kind)
            .is_some_and(|ty| ty.allows_mark("link"))
        {
            return Ok(None);
        }
        let prefix = pos
            .parent()
            .text_between(0, pos.parent_offset, "", "\u{fffc}")?;
        if !prefix.ends_with(')') {
            return Ok(None);
        }
        let m = matches(&prefix, DocPos(pos.start(pos.depth())), &typed)
            .pop()
            .filter(|m| m.span.end == head);
        let Some(m) = m else { return Ok(None) };
        if ctx.document.inline_nodes_in(m.span.clone()).any(|(_, n)| {
            n.marks()
                .iter()
                .any(|m| schema.excludes(&m.kind, "link") && m.kind != "link")
        }) {
            return Ok(None);
        }
        Ok(Some(convert(m).into()))
    });
    p.paste_rule(move |ctx| {
        let mut found = Vec::new();
        pasted_textblocks(&ctx, "link", |pos, text| {
            found.extend(matches(text, pos, &options))
        })?;
        Ok(found.into_iter().rev().flat_map(convert).collect())
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin::core::Node;
    use gpdoc_plugin_paragraph::paragraph;
    #[test]
    fn markdown_links_preserve_the_label_title_and_escaped_text() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugins([
                paragraph(),
                crate::link(LinkOptions {
                    markdown_links: true,
                    ..Default::default()
                }),
            ])
            .build()
            .unwrap();
        let mut editor =
            gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "")])
                .unwrap();
        editor
            .paste_text(r#"A [site](/page_(v2) "title") and \[literal](/text)"#)
            .unwrap();
        let labels: Vec<_> = editor
            .document()
            .iter_nodes()
            .flat_map(|(_, node)| node.marks().iter().map(move |m| (node.projected_text(), m)))
            .collect();
        assert_eq!(labels.len(), 1);
        let (label, mark) = labels[0];
        assert_eq!(label, "site");
        assert_eq!(mark.attrs["href"], Value::String("/page_(v2)".into()));
        assert_eq!(mark.attrs["title"], Value::String("title".into()));
        editor.undo().unwrap();
        for ch in "[😀](/doc)".chars() {
            editor.insert_text(&ch.to_string()).unwrap();
        }
        let (_, node) = editor.document().iter_nodes().nth(1).unwrap();
        assert_eq!(node.projected_text(), "😀");
        assert_eq!(node.marks()[0].attrs["href"], Value::String("/doc".into()));
    }
}
