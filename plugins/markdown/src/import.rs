use super::{Conversion, Warning, rules::Rules};
use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::prelude::{
    Attrs, Content, ContentRef, Error, Mark, Node, Schema, TextRun, Value,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag};
use std::ops::Range;

struct Frame<'a> {
    tag: Option<Tag<'a>>,
    children: Vec<Node>,
    source: Range<usize>,
    checked: Option<bool>,
    literal_text: bool,
    delimiters: Vec<(&'a str, usize)>,
}

struct Reader<'a> {
    schema: &'a Schema,
    default_text_block: &'a str,
    rules: Rules<'a>,
    warnings: Vec<Warning<Range<usize>>>,
}

fn text(value: &str) -> Node {
    TextRun {
        text: value.into(),
        marks: vec![],
    }
    .into()
}

fn formula(kind: &str, latex: &str) -> Node {
    Node::leaf(
        kind,
        [("latex".into(), Value::String(latex.trim().into()))].into(),
    )
}

fn plain(nodes: &[Node]) -> String {
    nodes
        .iter()
        .map(|n| match n.content() {
            ContentRef::Text(run) => run.text.to_owned(),
            ContentRef::Inline(kids) | ContentRef::Blocks(kids) => plain(kids),
            ContentRef::Leaf(_) => String::new(),
        })
        .collect()
}

impl<'a> Reader<'a> {
    fn warn(&mut self, span: Range<usize>, message: impl Into<String>) {
        self.warnings.push(Warning {
            position: span,
            message: message.into(),
        });
    }

    fn blocks(&self, nodes: Vec<Node>) -> Vec<Node> {
        let mut result = Vec::new();
        let mut inline = Vec::new();
        for node in nodes {
            let is_inline =
                node.kind == "text" || self.schema.node_spec(&node.kind).is_some_and(|s| s.inline);
            if is_inline {
                inline.push(node);
            } else {
                if !inline.is_empty() {
                    result.push(Node::new(
                        self.default_text_block,
                        Attrs::new(),
                        Content::Inline(std::mem::take(&mut inline)),
                    ));
                }
                result.push(node);
            }
        }
        if !inline.is_empty() {
            result.push(Node::new(
                self.default_text_block,
                Attrs::new(),
                Content::Inline(inline),
            ));
        }
        result
    }

    fn wrap(&mut self, kind: &str, attrs: Attrs, content: Content, pos: Range<usize>) -> Vec<Node> {
        if self.schema.node_spec(kind).is_some() {
            return vec![Node::new(kind, attrs, content)];
        }
        self.warn(pos, format!("missing {kind} plugin"));
        match content {
            Content::Blocks(kids) | Content::Inline(kids) => kids,
            _ => vec![],
        }
    }

    fn mark(
        &mut self,
        kind: &str,
        attrs: Attrs,
        children: Vec<Node>,
        pos: Range<usize>,
    ) -> Vec<Node> {
        if self.schema.mark_spec(kind).is_none() {
            self.warn(pos, format!("missing {kind} plugin"));
            return children;
        }
        let mark = Mark::new(kind, attrs);
        children
            .into_iter()
            .map(|node| match node.content() {
                ContentRef::Text(run) => {
                    let mut marks = run.marks.to_vec();
                    marks.push(mark.clone());
                    node.mark(marks)
                }
                ContentRef::Leaf(marks)
                    if self.schema.node_spec(&node.kind).is_some_and(|s| s.inline) =>
                {
                    let mut marks = marks.to_vec();
                    marks.push(mark.clone());
                    node.mark(marks)
                }
                _ => node,
            })
            .collect()
    }

    fn delimited_text(&mut self, frame: &mut Frame<'a>, source: &str, span: Range<usize>) {
        let raw = &source[span.clone()];
        let mut start = 0;
        let mut chars = raw.char_indices().peekable();
        while let Some((offset, _)) = chars.next() {
            let Some((&delimiter, &name)) = self
                .rules
                .mark_by_delimiter
                .iter()
                .find(|(delimiter, _)| raw[offset..].starts_with(**delimiter))
            else {
                continue;
            };
            let escapes = source[..span.start + offset]
                .bytes()
                .rev()
                .take_while(|&b| b == b'\\')
                .count();
            if escapes % 2 != 0 {
                continue;
            }
            if start < offset {
                frame.children.push(text(&raw[start..offset]));
            }
            start = offset + delimiter.len();
            while chars.next_if(|(i, _)| *i < start).is_some() {}
            if let Some(index) = frame
                .delimiters
                .iter()
                .rposition(|&(token, _)| token == delimiter)
            {
                let marker = frame.delimiters[index].1;
                frame.delimiters.truncate(index);
                if marker + 1 < frame.children.len() {
                    let content = frame.children.split_off(marker + 1);
                    frame.children.pop();
                    frame
                        .children
                        .extend(self.mark(name, Attrs::new(), content, span.clone()));
                } else {
                    frame.children.push(text(delimiter));
                }
            } else {
                frame.delimiters.push((delimiter, frame.children.len()));
                frame.children.push(text(delimiter));
            }
        }
        if start < raw.len() {
            frame.children.push(text(&raw[start..]));
        }
    }

    fn close(&mut self, frame: Frame, parent: Option<&Tag>) -> Result<Vec<Node>, Error> {
        let Frame {
            tag,
            children,
            source,
            checked,
            ..
        } = frame;
        let Some(tag) = tag else {
            return Ok(self.blocks(children));
        };
        let empty = Attrs::new();
        Ok(match tag {
            Tag::Paragraph => self.blocks(children),
            Tag::Heading { level, .. } => self.wrap(
                "heading",
                [("level".into(), Value::from(level as i64))].into(),
                Content::Inline(children),
                source,
            ),
            Tag::BlockQuote(_) => {
                let blocks = self.blocks(children);
                self.wrap("blockquote", empty, Content::Blocks(blocks), source)
            }
            Tag::CodeBlock(kind) => {
                let raw = plain(&children);
                let code = raw.strip_suffix('\n').unwrap_or(&raw);
                if let CodeBlockKind::Fenced(info) = &kind
                    && let Some(&(name, fence)) = self.rules.node_by_info.get(info.as_ref())
                {
                    let node = (fence.parse)(code)?;
                    if node.kind != name {
                        return Err(Error::InvalidDocument(format!(
                            "Markdown fence '{}' returned '{}', expected '{name}'",
                            fence.info(),
                            node.kind
                        )));
                    }
                    return Ok(vec![node]);
                }
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .map(|s| Value::String(s.into())),
                    _ => None,
                };
                let attrs = [("language".into(), lang.unwrap_or(Value::Null))].into();
                self.wrap(
                    "codeBlock",
                    attrs,
                    Content::Inline(if code.is_empty() {
                        vec![]
                    } else {
                        vec![text(code)]
                    }),
                    source,
                )
            }
            Tag::List(start) => {
                let mut lists = Vec::new();
                for group in
                    children.chunk_by(|a, b| (a.kind == "taskItem") == (b.kind == "taskItem"))
                {
                    let kind = if group[0].kind == "taskItem" {
                        "taskList"
                    } else if start.is_some() {
                        "orderedList"
                    } else {
                        "bulletList"
                    };
                    let attrs = start
                        .map(|n| [("start".into(), Value::from(n as i64))].into())
                        .unwrap_or_default();
                    lists.extend(self.wrap(
                        kind,
                        attrs,
                        Content::Blocks(group.to_vec()),
                        source.clone(),
                    ));
                }
                lists
            }
            Tag::Item => {
                let mut blocks = self.blocks(children);
                if blocks.first().is_none_or(|n| n.kind != "paragraph") {
                    blocks.insert(0, Node::text("paragraph", ""));
                }
                let task = checked.is_some()
                    && self.schema.node_spec("taskList").is_some()
                    && self.schema.node_spec("taskItem").is_some();
                let (kind, attrs) = if task {
                    (
                        "taskItem",
                        [("checked".into(), Value::Bool(checked.unwrap_or(false)))].into(),
                    )
                } else {
                    ("listItem", empty)
                };
                if checked.is_some() && !task {
                    self.warn(source.clone(), "missing task list plugins");
                }
                self.wrap(kind, attrs, Content::Blocks(blocks), source)
            }
            Tag::Table(alignments) => {
                let rows = children
                    .into_iter()
                    .map(|row| {
                        let ContentRef::Blocks(cells) = row.content() else {
                            return row;
                        };
                        let cells = cells
                            .iter()
                            .enumerate()
                            .map(|(i, cell)| {
                                let align = match alignments.get(i) {
                                    Some(Alignment::Left) => "left",
                                    Some(Alignment::Right) => "right",
                                    Some(Alignment::Center) => "center",
                                    _ => return cell.clone(),
                                };
                                let mut attrs = cell.attrs.clone();
                                attrs.insert("align".into(), Value::String(align.into()));
                                Node::new(&cell.kind, attrs, cell.content().to_owned())
                            })
                            .collect();
                        Node::container(&row.kind, cells)
                    })
                    .collect();
                self.wrap("table", empty, Content::Blocks(rows), source)
            }
            Tag::TableHead | Tag::TableRow => {
                self.wrap("tableRow", empty, Content::Blocks(children), source)
            }
            Tag::TableCell => {
                let kind = if matches!(parent, Some(Tag::TableHead)) {
                    "tableHeader"
                } else {
                    "tableCell"
                };
                self.wrap(
                    kind,
                    empty,
                    Content::Blocks(vec![Node::new(
                        "paragraph",
                        Attrs::new(),
                        Content::Inline(children),
                    )]),
                    source,
                )
            }
            Tag::Emphasis => self.mark("italic", empty, children, source),
            Tag::Strong => self.mark("bold", empty, children, source),
            Tag::Strikethrough => self.mark("strike", empty, children, source),
            Tag::Link {
                dest_url, title, ..
            } => {
                let mut attrs =
                    Attrs::from([("href".into(), Value::String(dest_url.into_string()))]);
                if !title.is_empty() {
                    attrs.insert("title".into(), Value::String(title.into_string()));
                }
                self.mark("link", attrs, children, source)
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                if self.schema.node_spec("image").is_none() {
                    self.warn(source, "missing image plugin");
                    return Ok(children);
                }
                let attrs = [
                    ("src".into(), Value::String(dest_url.into_string())),
                    ("alt".into(), Value::String(plain(&children))),
                    (
                        "title".into(),
                        if title.is_empty() {
                            Value::Null
                        } else {
                            Value::String(title.into_string())
                        },
                    ),
                ]
                .into();
                self.wrap("image", attrs, Content::Leaf(vec![]), source)
            }
            _ => children,
        })
    }
}

pub(super) fn parse(
    config: &EditorConfig,
    source: &str,
) -> Result<Conversion<Vec<Node>, Range<usize>>, Error> {
    let schema = config.schema();
    let mut reader = Reader {
        rules: Rules::from_config(config)?,
        default_text_block: config.default_text_block(),
        schema,
        warnings: vec![],
    };
    let mut stack = vec![Frame {
        tag: None,
        source: 0..source.len(),
        children: vec![],
        checked: None,
        literal_text: false,
        delimiters: vec![],
    }];
    let mut options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    if ["table", "tableRow", "tableHeader", "tableCell"]
        .iter()
        .all(|ty| schema.node_spec(ty).is_some())
    {
        options.insert(Options::ENABLE_TABLES);
    }
    if schema.node_spec("inlineMath").is_some() || schema.node_spec("blockMath").is_some() {
        options.insert(Options::ENABLE_MATH);
    }
    for (event, pos) in Parser::new_ext(source, options).into_offset_iter() {
        let new = match event {
            Event::Start(tag) => {
                if stack.len() > 128 {
                    return Err(Error::InvalidDocument(
                        "Markdown nesting exceeds 128".into(),
                    ));
                }
                let literal_text = matches!(tag, Tag::CodeBlock(_) | Tag::Image { .. })
                    || stack.last().is_some_and(|frame| frame.literal_text);
                stack.push(Frame {
                    literal_text,
                    tag: Some(tag),
                    children: vec![],
                    source: pos,
                    checked: None,
                    delimiters: vec![],
                });
                continue;
            }
            Event::End(_) => {
                let frame = stack.pop().expect("balanced parser events");
                reader.close(frame, stack.last().and_then(|p| p.tag.as_ref()))?
            }
            Event::Text(raw)
                if !reader.rules.mark_by_delimiter.is_empty()
                    && raw.as_ref() == &source[pos.clone()]
                    && !stack.last().expect("root frame").literal_text =>
            {
                reader.delimited_text(stack.last_mut().unwrap(), source, pos);
                continue;
            }
            Event::Text(raw) => {
                if raw.is_empty() {
                    vec![]
                } else {
                    vec![text(&raw)]
                }
            }
            Event::Html(raw) | Event::InlineHtml(raw) => {
                reader.warn(pos, "raw HTML preserved as text");
                vec![text(&raw)]
            }
            Event::SoftBreak => vec![text("\n")],
            Event::HardBreak => {
                if schema.node_spec("hardBreak").is_some() {
                    vec![Node::leaf("hardBreak", Attrs::new())]
                } else {
                    reader.warn(pos, "missing hardBreak plugin");
                    vec![text("\n")]
                }
            }
            Event::Code(raw) => reader.mark("code", Attrs::new(), vec![text(&raw)], pos),
            Event::InlineMath(raw) if schema.node_spec("inlineMath").is_some() => {
                vec![formula("inlineMath", &raw)]
            }
            Event::DisplayMath(raw) if schema.node_spec("blockMath").is_some() => {
                vec![formula("blockMath", &raw)]
            }
            Event::Rule => reader.wrap("horizontalRule", Attrs::new(), Content::Leaf(vec![]), pos),
            Event::TaskListMarker(checked) => {
                if let Some(item) = stack
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f.tag, Some(Tag::Item)))
                {
                    item.checked = Some(checked);
                }
                continue;
            }
            _ => {
                reader.warn(pos.clone(), "unsupported Markdown token preserved as text");
                vec![text(&source[pos])]
            }
        };
        stack.last_mut().expect("root frame").children.extend(new);
    }
    let mut nodes = reader.blocks(stack.pop().expect("root frame").children);
    if nodes.is_empty() {
        nodes.push(Node::text(config.default_text_block(), ""));
    }
    Ok(Conversion {
        value: nodes,
        warnings: reader.warnings,
    })
}
