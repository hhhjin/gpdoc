mod markdown;
#[cfg(feature = "gpui")]
mod popover;
use gpdoc_plugin::marks::{set_mark, toggle_mark, unset_mark};
use gpdoc_plugin::paste::pasted_textblocks;
use gpdoc_plugin::prelude::{
    AttrSpec, AttrType, Attrs, Command, CommandContext, ContentRef, DocPos, Document, Edit, Error,
    InputRuleContext, Mark, MarkSpec, Plugin, Selection, Transaction, Value, unit_command,
};
use linkify::{Link, LinkFinder, LinkKind};
#[cfg(feature = "gpui")]
pub use popover::{LinkAtCaret, link_popover};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct LinkOptions {
    pub autolink: bool,
    pub markdown_links: bool,
    pub link_on_paste: bool,
    pub default_protocol: String,
    pub protocols: Vec<String>,
}

impl Default for LinkOptions {
    fn default() -> Self {
        Self {
            autolink: true,
            markdown_links: false,
            link_on_paste: true,
            default_protocol: "http".into(),
            protocols: vec![],
        }
    }
}

/// A link attribute update. Omitted attributes keep their current values.
#[derive(Clone, Debug, Default)]
pub enum LinkAttr {
    #[default]
    Keep,
    Clear,
    Set(String),
}

#[derive(Clone, Debug, Default)]
pub struct LinkAttrs {
    pub href: LinkAttr,
    pub target: LinkAttr,
    pub rel: LinkAttr,
    pub class: LinkAttr,
    pub title: LinkAttr,
}

impl LinkAttrs {
    pub fn new(href: impl Into<String>) -> Self {
        Self {
            href: LinkAttr::Set(href.into()),
            ..Self::default()
        }
    }

    fn into_attrs(self) -> Attrs {
        [
            ("href", self.href),
            ("target", self.target),
            ("rel", self.rel),
            ("class", self.class),
            ("title", self.title),
        ]
        .into_iter()
        .filter_map(|(key, update)| {
            let value = match update {
                LinkAttr::Keep => return None,
                LinkAttr::Clear => Value::Null,
                LinkAttr::Set(text) => Value::String(text),
            };
            Some((key.to_owned(), value))
        })
        .collect()
    }
}

#[derive(Clone, Debug)]
pub struct Set(pub LinkAttrs);

impl Set {
    pub fn new(href: impl Into<String>) -> Self {
        Self(LinkAttrs::new(href))
    }
}

impl Command for Set {
    const NAME: &'static str = "setLink";
}

#[derive(Clone, Debug)]
pub struct Toggle(pub LinkAttrs);

impl Toggle {
    pub fn new(href: impl Into<String>) -> Self {
        Self(LinkAttrs::new(href))
    }
}

impl Command for Toggle {
    const NAME: &'static str = "toggleLink";
}

unit_command!(pub Unset, "unsetLink");

gpdoc_plugin::plugin_config! {
    pub Config {
        input_rules InputRules { markdown_link: mark("link"), autolink: mark("link") }
        paste_rules PasteRules { markdown_link: paste, autolink: paste, link_selection: handle_paste }
    }
}

pub fn link(options: LinkOptions) -> Plugin<Config> {
    let attrs = ["href", "target", "rel", "class", "title"]
        .map(|key| {
            let value = match key {
                "target" => Value::String("_blank".into()),
                "rel" => Value::String("noopener noreferrer nofollow".into()),
                _ => Value::Null,
            };
            (
                key.into(),
                AttrSpec::with_default(AttrType::NullableString, value),
            )
        })
        .into();
    let protocols = options.protocols.clone();
    let toggle_protocols = protocols.clone();
    let plugin = Plugin::<Config>::new("link", |p| {
        p.mark(
            MarkSpec::new("link")
                .inclusive_end(options.autolink)
                .attrs(attrs),
        );

        p.command::<Set>(move |ctx, Set(link)| {
            if let LinkAttr::Set(href) = &link.href
                && !allowed(href, &protocols)
            {
                return Ok(None);
            }
            set_mark("link")(ctx, link.into_attrs())
        });
        p.command::<Unset>(|ctx, _| {
            if let Some(tx) = unlink_at_cursor(ctx)? {
                return Ok(Some(tx));
            }
            unset_mark("link")(ctx, Attrs::new())
        });
        p.command::<Toggle>(move |ctx, Toggle(link)| {
            if let LinkAttr::Set(href) = &link.href
                && !allowed(href, &toggle_protocols)
            {
                return Ok(None);
            }
            if let Some(tx) = unlink_at_cursor(ctx)? {
                return Ok(Some(tx));
            }
            toggle_mark("link")(ctx, link.into_attrs())
        });
        let options = Arc::new(options);
        let mut finder = LinkFinder::new();
        finder.url_must_have_scheme(false);
        let finder = Arc::new(finder);
        if options.markdown_links {
            markdown::rules(p, options.clone());
        }
        let options_for_paste = options.clone();
        let paste_finder = finder.clone();
        p.paste_rules().autolink().set(move |ctx| {
            let mut steps = Vec::new();
            pasted_textblocks(&ctx, "link", |from, text| {
                for link in paste_finder.links(text) {
                    let Some(href) = detected_href(&link, &options_for_paste) else {
                        continue;
                    };
                    let start = DocPos(from.0 + text[..link.start()].encode_utf16().count());
                    let range = start..DocPos(start.0 + link.as_str().encode_utf16().count());
                    if ctx
                        .document
                        .inline_nodes_in(range.clone())
                        .any(|(_, node)| {
                            node.marks()
                                .iter()
                                .any(|m| m.kind == "link" || ctx.schema.excludes(&m.kind, "link"))
                        })
                    {
                        continue;
                    }
                    steps.push(Edit::SetMark {
                        range,
                        mark: Mark::new("link", [("href".into(), Value::String(href))].into()),
                    });
                }
            })?;
            Ok(steps)
        });
        if options.link_on_paste {
            let options = options.clone();
            let finder = finder.clone();
            p.paste_rules().link_selection().set(move |ctx, text| {
                if ctx.selection().bounds(ctx.document()).is_empty() {
                    return Ok(None);
                }
                let Some(href) = finder
                    .links(text)
                    .next()
                    .filter(|l| l.as_str() == text)
                    .and_then(|l| detected_href(&l, &options))
                else {
                    return Ok(None);
                };
                set_mark("link")(ctx, [("href".into(), Value::String(href))].into())
            });
        }
        if options.autolink {
            p.input_rules()
                .autolink()
                .set(move |ctx| auto_link(ctx, &finder, &options));
        }
    })
    .priority(1000);
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, link_format);
    plugin
}

/// Underline links using the inherited text color.
#[cfg(feature = "gpui")]
pub fn link_format() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("linkFormat").mark_format("link", |_, style| {
        style.underline = true;
    })
}

/// Enable native link activation. The editor checks destination schemes when opening.
#[cfg(feature = "gpui")]
pub fn link_activation() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("linkActivation").mark_format("link", |attrs, style| {
        if let Some(Value::String(href)) = attrs.get("href")
            && allowed(href, &[])
        {
            style.link_href = Some(href.as_str().into());
        }
    })
}

fn detected_href(link: &Link<'_>, options: &LinkOptions) -> Option<String> {
    let text = link.as_str();
    let href = if *link.kind() == LinkKind::Email {
        format!("mailto:{text}")
    } else if text.contains("://") {
        text.to_owned()
    } else if text
        .split(['/', ':', '?', '#'])
        .next()
        .is_some_and(|h| h.parse::<std::net::IpAddr>().is_ok())
    {
        return None;
    } else {
        format!("{}://{text}", options.default_protocol)
    };
    allowed(&href, &options.protocols).then_some(href)
}

fn allowed(uri: &str, protocols: &[String]) -> bool {
    let normalized: String = uri
        .chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_ascii_control() && *ch != '\u{feff}')
        .collect();
    let Some((prefix, _)) = normalized.split_once(':') else {
        return true;
    };
    if !prefix.starts_with(|c: char| c.is_ascii_alphabetic())
        || prefix.contains(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    {
        return true;
    }
    [
        "http", "https", "ftp", "ftps", "mailto", "tel", "callto", "sms", "cid", "xmpp",
    ]
    .iter()
    .copied()
    .chain(protocols.iter().map(String::as_str))
    .any(|s| prefix.eq_ignore_ascii_case(s))
}

fn unlink_at_cursor(ctx: CommandContext<'_>) -> Result<Option<Transaction>, Error> {
    let Selection::Text { anchor, head } = ctx.selection() else {
        return Ok(None);
    };
    if head != anchor {
        return Ok(None);
    }
    let Some((range, _)) = link_at(ctx.document(), head)? else {
        return Ok(None);
    };
    let mut tx = ctx.transaction();
    tx.push(Edit::RemoveMark {
        range,
        kind: "link".into(),
    });
    let mut marks = ctx.cursor_marks()?.unwrap_or_default();
    marks.retain(|m| m.kind != "link");
    tx.set_stored_marks(marks);
    Ok(Some(tx))
}

/// At a shared boundary prefer the link beginning at the caret.
fn link_at(
    document: &Document,
    head: DocPos,
) -> Result<Option<(std::ops::Range<DocPos>, &Mark)>, Error> {
    let (before, block) = document.text_at(head)?;
    let ContentRef::Inline(children) = block.content() else {
        return Ok(None);
    };
    let mut offset = before.0 + 1;
    let mut spans: Vec<(usize, usize, &Mark)> = vec![];
    for child in children {
        let end = offset + child.size();
        if let Some(mark) = child.marks().iter().find(|m| m.kind == "link") {
            if let Some((_, last, previous)) = spans.last_mut()
                && *last == offset
                && *previous == mark
            {
                *last = end;
            } else {
                spans.push((offset, end, mark));
            }
        }
        offset = end;
    }
    let found = spans
        .iter()
        .find(|(start, end, _)| (*start..*end).contains(&head.0))
        .or_else(|| spans.iter().find(|(_, end, _)| *end == head.0));
    Ok(found.map(|&(start, end, mark)| (DocPos(start)..DocPos(end), mark)))
}

fn auto_link(
    ctx: InputRuleContext<'_>,
    finder: &LinkFinder,
    options: &LinkOptions,
) -> Result<Option<Vec<Edit>>, Error> {
    let Selection::Text { anchor, head } = ctx.selection else {
        return Ok(None);
    };
    if head != anchor || head != ctx.inserted.end {
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
    let trimmed = prefix.trim_end();
    if trimmed.len() == prefix.len() {
        return Ok(None);
    }
    let start_space = head.0 - prefix[trimmed.len()..].encode_utf16().count();
    let mut steps = Vec::new();
    if ctx
        .document
        .inline_nodes_in(DocPos(start_space)..head)
        .any(|(_, n)| n.marks().iter().any(|m| m.kind == "link"))
    {
        steps.push(Edit::RemoveMark {
            range: DocPos(start_space)..head,
            kind: "link".into(),
        });
    }
    if let Some(word) = trimmed.split_whitespace().last() {
        let inner = word
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .or_else(|| word.strip_prefix('[').and_then(|s| s.strip_suffix(']')))
            .unwrap_or(word);
        if let Some(link) = finder.links(inner).next().filter(|l| l.as_str() == inner) {
            let mut end = start_space;
            if word != inner {
                end -= 1
            }
            let start = DocPos(end - inner.encode_utf16().count());
            let range = start..DocPos(end);
            if let Some(href) = detected_href(&link, options)
                && !ctx
                    .document
                    .inline_nodes_in(range.clone())
                    .any(|(_, node)| {
                        node.marks()
                            .iter()
                            .any(|m| m.kind == "link" || schema.excludes(&m.kind, "link"))
                    })
            {
                steps.push(Edit::SetMark {
                    range,
                    mark: Mark::new("link", [("href".into(), Value::String(href))].into()),
                });
            }
        }
    }
    Ok((!steps.is_empty()).then_some(steps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin::prelude::{CommandOutcome, Node};
    use gpdoc_plugin_paragraph::paragraph;
    #[test]
    fn link_updates_preserve_omitted_and_null_attributes() {
        let args = Attrs::from([
            ("href".into(), Value::String("https://example.org".into())),
            ("target".into(), Value::Null),
        ]);
        let attrs = LinkAttrs {
            target: LinkAttr::Clear,
            ..LinkAttrs::new("https://example.org")
        };
        assert_eq!(attrs.into_attrs(), args);
    }

    #[test]
    fn paste_link_keeps_selected_text_and_checks_schemes() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugin(paragraph())
            .plugin(link(Default::default()))
            .build()
            .unwrap();
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "site")],
        )
        .unwrap();
        editor
            .select(Selection::Text {
                anchor: DocPos(1),
                head: DocPos(5),
            })
            .unwrap();
        for href in [
            "javascript:example",
            "DaTa:text/html,hello",
            "java\tscript:example",
        ] {
            assert!(matches!(
                editor.run(Set::new(href)).unwrap(),
                CommandOutcome::NotApplicable
            ));
        }
        editor.paste_text("https://example.org").unwrap();
        let (_, node) = editor.document().iter_nodes().nth(1).unwrap();
        assert_eq!(node.projected_text(), "site");
        assert_eq!(
            node.marks()[0].attrs["href"],
            Value::String("https://example.org".into())
        );
        editor.undo().unwrap();
        editor
            .paste_text("visit example.org and mail team@example.org")
            .unwrap();
        let links = editor
            .document()
            .iter_nodes()
            .flat_map(|(_, node)| node.marks())
            .filter_map(|m| m.attrs.get("href"))
            .collect::<Vec<_>>();
        assert_eq!(
            links,
            [
                &Value::String("http://example.org".into()),
                &Value::String("mailto:team@example.org".into())
            ]
        );
    }
}
