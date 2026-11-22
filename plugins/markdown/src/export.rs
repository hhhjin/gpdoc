use super::{Conversion, Warning, rules::Rules};
use gpdoc_plugin::prelude::{Attrs, ContentRef, DocPos, Document, Mark, Node, Value};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, LinkType, Tag};
use pulldown_cmark_to_cmark::{
    Error, Options, calculate_code_block_token_count, cmark_with_options,
};

fn string<'a>(attrs: &'a Attrs, key: &str) -> &'a str {
    match attrs.get(key) {
        Some(Value::String(s)) => s,
        _ => "",
    }
}
fn link<'a>(attrs: &'a Attrs, image: bool) -> Tag<'a> {
    let dest_url = string(attrs, if image { "src" } else { "href" }).into();
    let title = string(attrs, "title").into();
    if image {
        Tag::Image {
            link_type: LinkType::Inline,
            dest_url,
            title,
            id: "".into(),
        }
    } else {
        Tag::Link {
            link_type: LinkType::Inline,
            dest_url,
            title,
            id: "".into(),
        }
    }
}

#[derive(PartialEq)]
enum InlineTag<'a> {
    Common(Tag<'a>),
    Delimiter(&'a str),
}

impl<'a> InlineTag<'a> {
    fn start(&self) -> Event<'a> {
        match self {
            Self::Common(tag) => Event::Start(tag.clone()),
            Self::Delimiter(token) => Event::InlineHtml((*token).into()),
        }
    }
    fn end(&self) -> Event<'a> {
        match self {
            Self::Common(tag) => Event::End(tag.to_end()),
            Self::Delimiter(token) => Event::InlineHtml((*token).into()),
        }
    }
}

struct Writer<'a> {
    rules: Rules<'a>,
    events: Vec<Event<'a>>,
    warnings: Vec<Warning<DocPos>>,
}

impl<'a> Writer<'a> {
    fn warn(&mut self, pos: usize, message: impl Into<String>) {
        self.warnings.push(Warning {
            position: DocPos(pos),
            message: message.into(),
        });
    }

    fn tag(&mut self, tag: Tag<'a>, body: impl FnOnce(&mut Self)) {
        let end = tag.to_end();
        self.events.push(Event::Start(tag));
        body(self);
        self.events.push(Event::End(end));
    }

    fn mark(&mut self, mark: &'a Mark, pos: usize) -> Option<InlineTag<'a>> {
        if let Some(&(delimiter, ref defaults)) =
            self.rules.delimiter_by_mark.get(mark.kind.as_str())
        {
            if &mark.attrs != defaults {
                self.warn(
                    pos,
                    format!(
                        "Markdown delimiter omits attributes of '{}' mark",
                        mark.kind
                    ),
                );
            }
            return Some(InlineTag::Delimiter(delimiter));
        }
        match mark.kind.as_str() {
            "bold" => Some(Tag::Strong),
            "italic" => Some(Tag::Emphasis),
            "strike" => Some(Tag::Strikethrough),
            "link" => Some(link(&mark.attrs, false)),
            "code" => None,
            kind => {
                self.warn(pos, format!("omitted {kind} mark"));
                None
            }
        }
        .map(InlineTag::Common)
    }

    fn text(&mut self, value: &'a str) {
        // Entities preserve literal punctuation, including a single character next
        // to an opening/closing delimiter. The reader only matches source tokens.
        if self.rules.mark_by_delimiter.is_empty() {
            self.events.push(Event::Text(value.into()));
            return;
        }
        let mut start = 0;
        for (offset, ch) in value.char_indices() {
            if !self
                .rules
                .mark_by_delimiter
                .keys()
                .any(|d| d.starts_with(ch))
            {
                continue;
            }
            if start < offset {
                self.events.push(Event::Text(value[start..offset].into()));
            }
            self.events
                .push(Event::InlineHtml(format!("&#{};", u32::from(ch)).into()));
            start = offset + ch.len_utf8();
        }
        if start < value.len() {
            self.events.push(Event::Text(value[start..].into()));
        }
    }

    fn inline(&mut self, children: &'a [Node], mut pos: usize) {
        let mut open: Vec<InlineTag> = Vec::new();
        for node in children {
            let next: Vec<_> = node
                .marks()
                .iter()
                .filter_map(|m| self.mark(m, pos))
                .collect();
            let prefix = open.iter().zip(&next).take_while(|(a, b)| a == b).count();
            for t in open.drain(prefix..).rev() {
                self.events.push(t.end());
            }
            for t in next.into_iter().skip(prefix) {
                self.events.push(t.start());
                open.push(t);
            }
            match (node.content(), node.kind.as_str()) {
                (ContentRef::Text(run), _) if run.marks.iter().any(|m| m.kind == "code") => {
                    self.events.push(Event::Code(run.text.into()))
                }
                (ContentRef::Text(run), _) => self.text(run.text),
                (_, "inlineMath") => self
                    .events
                    .push(Event::InlineMath(string(&node.attrs, "latex").into())),
                (_, "hardBreak") => self.events.push(Event::HardBreak),
                (_, "image") => self.image(node),
                _ => self.fallback(node, pos),
            }
            pos += node.size();
        }
        for t in open.iter().rev() {
            self.events.push(t.end());
        }
    }

    fn image(&mut self, node: &'a Node) {
        self.tag(link(&node.attrs, true), |w| {
            w.events
                .push(Event::Text(string(&node.attrs, "alt").into()))
        });
    }

    fn fallback(&mut self, node: &'a Node, pos: usize) {
        self.warn(pos, format!("no Markdown representation for {}", node.kind));
        match node.content() {
            ContentRef::Blocks(kids) => self.blocks(kids, pos + 1),
            ContentRef::Inline(kids) => self.tag(Tag::Paragraph, |w| w.inline(kids, pos + 1)),
            ContentRef::Text(run) => self.text(run.text),
            ContentRef::Leaf(_) => {
                let label = ["label", "alt", "name", "latex", "src"]
                    .iter()
                    .map(|key| string(&node.attrs, key))
                    .find(|v| !v.is_empty())
                    .unwrap_or(&node.kind);
                self.events.push(Event::Text(label.into()));
            }
        }
    }

    fn blocks(&mut self, nodes: &'a [Node], mut at: usize) {
        for node in nodes {
            self.block(node, at);
            at += node.size();
        }
    }

    fn block(&mut self, node: &'a Node, at: usize) {
        if let Some(&fence) = self.rules.fence_by_node.get(node.kind.as_str()) {
            self.tag(
                Tag::CodeBlock(CodeBlockKind::Fenced(fence.info().into())),
                |w| {
                    let body = (fence.render)(node);
                    w.events.push(Event::Text(format!("{body}\n").into()));
                },
            );
            return;
        }
        match (node.content(), node.kind.as_str()) {
            (ContentRef::Inline(runs), "paragraph") => {
                self.tag(Tag::Paragraph, |w| w.inline(runs, at + 1))
            }
            (ContentRef::Inline(runs), "heading") => {
                let level = match node.attrs.get("level") {
                    Some(value) => value
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .and_then(|n| HeadingLevel::try_from(n).ok())
                        .unwrap_or(HeadingLevel::H1),
                    _ => HeadingLevel::H1,
                };
                self.tag(
                    Tag::Heading {
                        level,
                        id: None,
                        classes: vec![],
                        attrs: vec![],
                    },
                    |w| w.inline(runs, at + 1),
                );
            }
            (ContentRef::Inline(runs), "codeBlock") => {
                let language = string(&node.attrs, "language");
                if self.rules.node_by_info.contains_key(language) {
                    self.warn(at, format!("fence '{language}' imports as a custom block"));
                }
                self.tag(
                    Tag::CodeBlock(CodeBlockKind::Fenced(language.into())),
                    |w| {
                        for run in runs {
                            if let ContentRef::Text(r) = run.content() {
                                w.events.push(Event::Text(r.text.into()));
                            }
                        }
                        w.events.push(Event::Text("\n".into()));
                    },
                );
            }
            (ContentRef::Blocks(kids), "blockquote") => {
                self.tag(Tag::BlockQuote(None), |w| w.blocks(kids, at + 1))
            }
            (ContentRef::Blocks(kids), "orderedList" | "bulletList" | "taskList") => {
                let start = (node.kind == "orderedList").then(|| match node.attrs.get("start") {
                    Some(value) => value.as_u64().unwrap_or(1),
                    _ => 1,
                });
                self.tag(Tag::List(start), |w| w.blocks(kids, at + 1));
            }
            (ContentRef::Blocks(kids), "listItem" | "taskItem") => self.tag(Tag::Item, |w| {
                let mut pos = at + 1;
                for (i, child) in kids.iter().enumerate() {
                    if i == 0
                        && child.kind == "paragraph"
                        && let ContentRef::Inline(runs) = child.content()
                    {
                        if node.kind == "taskItem" {
                            w.events.push(Event::TaskListMarker(
                                node.attrs.get("checked") == Some(&Value::Bool(true)),
                            ));
                        }
                        w.inline(runs, pos + 1);
                    } else {
                        w.block(child, pos);
                    }
                    pos += child.size();
                }
            }),
            (ContentRef::Blocks(rows), "table") => self.table(rows, at),
            (_, "blockMath") => self.tag(Tag::Paragraph, |w| {
                w.events
                    .push(Event::DisplayMath(string(&node.attrs, "latex").into()))
            }),
            (_, "horizontalRule") => self.events.push(Event::Rule),
            (_, "image") => self.tag(Tag::Paragraph, |w| w.image(node)),
            _ => self.fallback(node, at),
        }
    }

    fn table(&mut self, rows: &'a [Node], at: usize) {
        let cells = |row: &'a Node| match row.content() {
            ContentRef::Blocks(c) => c,
            _ => &[],
        };
        let Some(first) = rows.first() else {
            self.warn(at, "empty Markdown table");
            return;
        };
        let alignment = cells(first)
            .iter()
            .map(|c| match string(&c.attrs, "align") {
                "left" => Alignment::Left,
                "right" => Alignment::Right,
                "center" => Alignment::Center,
                _ => Alignment::None,
            })
            .collect();
        self.tag(Tag::Table(alignment), |w| {
            let mut row_at = at + 1;
            for (i, row) in rows.iter().enumerate() {
                w.tag(
                    if i == 0 {
                        Tag::TableHead
                    } else {
                        Tag::TableRow
                    },
                    |w| {
                        let mut cell_at = row_at + 1;
                        for cell in cells(row) {
                            if i == 0 && cell.kind != "tableHeader" {
                                w.warn(cell_at, "Markdown uses the first table row as headers");
                            }
                            if ["colspan", "rowspan"].iter().any(|k| {
                                cell.attrs
                                    .get(*k)
                                    .and_then(Value::as_i64)
                                    .is_some_and(|n| n > 1)
                            }) {
                                w.warn(cell_at, "Markdown cannot preserve merged table cells");
                            }
                            w.tag(Tag::TableCell, |w| {
                                let mut child_pos = cell_at + 1;
                                for (j, child) in cells(cell).iter().enumerate() {
                                    if j > 0 {
                                        w.events.push(Event::Text(" ".into()));
                                    }
                                    if let ContentRef::Inline(runs) = child.content() {
                                        w.inline(runs, child_pos + 1);
                                    } else {
                                        w.fallback(child, child_pos);
                                    }
                                    child_pos += child.size();
                                }
                            });
                            cell_at += cell.size();
                        }
                    },
                );
                row_at += row.size();
            }
        });
    }
}

pub(super) fn render<'a>(
    document: &'a Document,
    rules: Rules<'a>,
) -> Result<Conversion<String, DocPos>, Error> {
    let mut writer = Writer {
        rules,
        events: vec![],
        warnings: vec![],
    };
    writer.blocks(document.blocks(), 0);
    let mut output = String::new();
    let options = Options {
        code_block_token_count: calculate_code_block_token_count(&writer.events)
            .unwrap_or(3)
            .max(3),
        ..Default::default()
    };
    cmark_with_options(writer.events.into_iter(), &mut output, options)?;
    Ok(Conversion {
        value: output,
        warnings: writer.warnings,
    })
}
