//! Tiptap's Yjs representation uses one XmlText for adjacent marked text runs.
use crate::Error;
use gpdoc_plugin::core::{Affinity, PositionMap};
use gpdoc_plugin::prelude::{Attrs, Content, ContentRef, DocPos, Mark, Node, Schema, TextRun};
use yrs::types::Attrs as Formats;
use yrs::{
    Any, Out, ReadTxn, Text, TransactionMut, Xml, XmlElementPrelim, XmlFragment, XmlOut,
    XmlTextPrelim, XmlTextRef,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Child {
    Element {
        kind: String,
        attrs: Formats,
        children: Vec<Child>,
    },
    Text(Vec<Run>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    text: String,
    marks: Formats,
}

fn encode_attrs(attrs: &Attrs) -> Result<Formats, Error> {
    attrs
        .iter()
        .map(|(k, v)| Ok((k.as_str().into(), serde_json::from_value(v.clone())?)))
        .collect()
}

fn decode_attrs(attrs: &Formats) -> Result<Attrs, Error> {
    attrs
        .iter()
        .map(|(k, v)| {
            let value = serde_json::to_value(v)?;
            Ok((k.to_string(), value))
        })
        .collect()
}

pub(crate) fn encode(nodes: &[Node]) -> Result<Vec<Child>, Error> {
    let mut result = Vec::new();
    for node in nodes {
        if let ContentRef::Text(run) = node.content() {
            let mut marks = Formats::new();
            for mark in run.marks {
                let attrs = encode_attrs(&mark.attrs)?
                    .into_iter()
                    .map(|(key, v)| (key.to_string(), v))
                    .collect();
                if marks
                    .insert(
                        mark.kind.as_str().into(),
                        Any::Map(std::sync::Arc::new(attrs)),
                    )
                    .is_some()
                {
                    return Err(Error::Content("overlapping marks of the same kind".into()));
                }
            }
            let run = Run {
                text: run.text.to_owned(),
                marks,
            };
            if let Some(Child::Text(text)) = result.last_mut() {
                text.push(run);
            } else {
                result.push(Child::Text(vec![run]));
            }
            continue;
        }
        if !node.marks().is_empty() {
            return Err(Error::Content(
                "Yjs XML elements cannot carry inline marks".into(),
            ));
        }
        let children = match node.content() {
            ContentRef::Blocks(nodes) | ContentRef::Inline(nodes) => encode(nodes)?,
            _ => vec![],
        };
        result.push(Child::Element {
            kind: node.kind.clone(),
            attrs: encode_attrs(&node.attrs)?,
            children,
        });
    }
    Ok(result)
}

pub(crate) fn decode(schema: &Schema, children: Vec<Child>) -> Result<Vec<Node>, Error> {
    let mut nodes = Vec::new();
    for child in children {
        match child {
            Child::Text(runs) => {
                for run in runs {
                    let mut marks = Vec::new();
                    for (key, attrs) in run.marks {
                        let Any::Map(values) = attrs else {
                            return Err(Error::Content("invalid Yjs mark".into()));
                        };
                        let attrs = values
                            .iter()
                            .map(|(k, v)| (k.as_str().into(), v.clone()))
                            .collect();
                        let name = key
                            .rsplit_once("--")
                            .and_then(|(name, hash)| {
                                (hash.len() == 8
                                    && hash.bytes().all(|b| {
                                        b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')
                                    }))
                                .then_some(name)
                            })
                            .unwrap_or(&key);
                        marks.push(Mark::new(name, decode_attrs(&attrs)?));
                    }
                    nodes.push(
                        TextRun {
                            text: run.text,
                            marks,
                        }
                        .into(),
                    );
                }
            }
            Child::Element {
                kind,
                attrs,
                children,
            } => {
                let ty = schema
                    .node_type(&kind)
                    .ok_or_else(|| Error::Content(format!("unknown node {kind}")))?;
                let nested = decode(schema, children)?;
                let content = if ty.is_leaf() && nested.is_empty() {
                    Content::Leaf(vec![])
                } else if ty.inline_content() {
                    Content::Inline(nested)
                } else {
                    Content::Blocks(nested)
                };
                nodes.push(Node::new(kind, decode_attrs(&attrs)?, content));
            }
        }
    }
    Ok(nodes)
}

pub(crate) fn read(
    tx: &impl ReadTxn,
    parent: &impl XmlFragment,
    depth: usize,
) -> Result<Vec<Child>, Error> {
    if depth > 128 {
        return Err(Error::Content("Yjs XML nesting exceeds 128".into()));
    }
    parent
        .children(tx)
        .map(|node| match node {
            XmlOut::Text(text) => {
                let runs = text
                    .diff(tx, |_| ())
                    .into_iter()
                    .map(|delta| {
                        let Out::Any(Any::String(value)) = delta.insert else {
                            return Err(Error::Content("unsupported inline Yjs embed".into()));
                        };
                        Ok(Run {
                            text: value.to_string(),
                            marks: delta.attributes.map(|attrs| *attrs).unwrap_or_default(),
                        })
                    })
                    .collect::<Result<_, _>>()?;
                Ok(Child::Text(runs))
            }
            XmlOut::Element(el) => {
                let attrs = el
                    .attributes(tx)
                    .map(|(k, v)| {
                        let Out::Any(value) = v else {
                            return Err(Error::Content(format!("invalid attribute {k}")));
                        };
                        Ok((k.into(), value))
                    })
                    .collect::<Result<_, _>>()?;
                Ok(Child::Element {
                    kind: el.tag().to_string(),
                    attrs,
                    children: read(tx, &el, depth + 1)?,
                })
            }
            XmlOut::Fragment(_) => Err(Error::Content("nested fragment".into())),
        })
        .collect()
}

pub(crate) fn seed(tx: &mut TransactionMut, parent: &impl XmlFragment, nodes: &[Child]) {
    for (i, child) in nodes.iter().enumerate() {
        insert(tx, parent, i as u32, child);
    }
}

struct Group<'a> {
    parts: &'a [Child],
    start: usize,
    end: usize,
}

fn groups<'a>(children: &'a [Child], mut start: usize, schema: &Schema) -> Vec<Group<'a>> {
    // Concurrent typing into an empty paragraph can produce adjacent XmlTexts. Together
    // they correspond to a single sequence of ProseMirror text runs.
    children
        .chunk_by(|a, b| matches!((a, b), (Child::Text(_), Child::Text(_))))
        .map(|parts| {
            let before = start;
            start += parts.iter().map(|c| size(c, schema)).sum::<usize>();
            Group {
                parts,
                start: before,
                end: start,
            }
        })
        .collect()
}

fn size(child: &Child, schema: &Schema) -> usize {
    match child {
        Child::Text(runs) => runs.iter().map(|r| r.text.encode_utf16().count()).sum(),
        Child::Element { kind, .. } if schema.node_type(kind).is_some_and(|ty| ty.is_leaf()) => 1,
        Child::Element { children, .. } => {
            2 + children.iter().map(|c| size(c, schema)).sum::<usize>()
        }
    }
}

pub(crate) fn reconcile(
    tx: &mut TransactionMut,
    root: &impl XmlFragment,
    old: &[Child],
    new: &[Child],
    schema: &Schema,
    map: &PositionMap,
) {
    let mut edit = Reconcile { tx, schema, map };
    edit.children(root, old, 0, new, 0);
}

struct Reconcile<'a, 'doc> {
    tx: &'a mut TransactionMut<'doc>,
    schema: &'a Schema,
    map: &'a PositionMap,
}

impl Reconcile<'_, '_> {
    fn children(
        &mut self,
        parent: &impl XmlFragment,
        old: &[Child],
        old_start: usize,
        new: &[Child],
        new_start: usize,
    ) {
        let old = groups(old, old_start, self.schema);
        let new = groups(new, new_start, self.schema);
        let (mut cursor, mut next) = (0, 0);
        for group in &old {
            let mapped = self.map.map(DocPos(group.start), Affinity::After);
            // Insertion at a text group's first character keeps the XmlText itself.
            let candidate = if matches!(group.parts.first(), Some(Child::Text(_))) {
                new[next..].iter().position(|g| {
                    matches!(g.parts.first(), Some(Child::Text(_)))
                        && g.start <= mapped.position.0
                        && mapped.position.0 <= g.end
                })
            } else if mapped.deleted {
                None
            } else {
                new[next..]
                    .binary_search_by_key(&mapped.position.0, |group| group.start)
                    .ok()
                    .filter(|i| match (&group.parts[0], &new[next + *i].parts[0]) {
                        (Child::Element { kind, .. }, Child::Element { kind: other, .. }) => {
                            kind == other
                        }
                        _ => false,
                    })
            };
            let Some(skip) = candidate else {
                parent.remove_range(self.tx, cursor, group.parts.len() as u32);
                continue;
            };
            for g in &new[next..next + skip] {
                for child in g.parts {
                    insert(self.tx, parent, cursor, child);
                    cursor += 1;
                }
            }
            next += skip;
            self.patch(parent, cursor, group, &new[next]);
            cursor += group.parts.len() as u32;
            next += 1;
        }
        for g in &new[next..] {
            for child in g.parts {
                insert(self.tx, parent, cursor, child);
                cursor += 1;
            }
        }
    }

    fn patch(&mut self, parent: &impl XmlFragment, cursor: u32, old: &Group, new: &Group) {
        match (&old.parts[0], &new.parts[0]) {
            (
                Child::Element {
                    attrs, children, ..
                },
                Child::Element {
                    attrs: values,
                    children: kids,
                    ..
                },
            ) => {
                let Some(XmlOut::Element(el)) = parent.get(self.tx, cursor) else {
                    unreachable!("read from the same transaction")
                };
                for key in attrs.keys().filter(|k| !values.contains_key(*k)) {
                    el.remove_attribute(self.tx, key);
                }
                for (key, value) in values {
                    if attrs.get(key) != Some(value) {
                        el.insert_attribute(self.tx, key.clone(), value.clone());
                    }
                }
                self.children(&el, children, old.start + 1, kids, new.start + 1);
            }
            (Child::Text(_), Child::Text(runs)) => {
                let chars = characters(runs);
                let mut target = 0;
                let mut old_pos = old.start;
                let mut new_pos = new.start;
                for (i, part) in old.parts.iter().enumerate() {
                    let Child::Text(prev) = part else {
                        unreachable!()
                    };
                    let Some(XmlOut::Text(text)) = parent.get(self.tx, cursor + i as u32) else {
                        unreachable!("same transaction")
                    };
                    let end = old_pos
                        + prev
                            .iter()
                            .map(|r| r.text.encode_utf16().count())
                            .sum::<usize>();
                    let split = if i + 1 == old.parts.len() {
                        new.end
                    } else {
                        self.map
                            .map(DocPos(end), Affinity::Before)
                            .position
                            .0
                            .clamp(new_pos, new.end)
                    };
                    let mut limit = target;
                    let mut next_pos = new_pos;
                    while limit < chars.len() && next_pos + chars[limit].0.len_utf16() <= split {
                        next_pos += chars[limit].0.len_utf16();
                        limit += 1;
                    }
                    update_text(
                        self.tx,
                        &text,
                        &characters(prev),
                        old_pos,
                        &chars[target..limit],
                        new_pos,
                        self.map,
                    );
                    target = limit;
                    old_pos = end;
                    new_pos = next_pos;
                }
            }
            _ => unreachable!("matched XML type"),
        }
    }
}

fn insert(tx: &mut TransactionMut, parent: &impl XmlFragment, at: u32, child: &Child) {
    match child {
        Child::Element {
            kind,
            attrs,
            children,
        } => {
            let el = parent.insert(tx, at, XmlElementPrelim::empty(kind.as_str()));
            for (key, v) in attrs {
                el.insert_attribute(tx, key.clone(), v.clone());
            }
            for (i, c) in children.iter().enumerate() {
                insert(tx, &el, i as u32, c);
            }
        }
        Child::Text(runs) => {
            let text = parent.insert(tx, at, XmlTextPrelim::new(""));
            append_runs(tx, &text, 0, runs);
        }
    }
}

fn append_runs(tx: &mut TransactionMut, text: &XmlTextRef, mut offset: u32, runs: &[Run]) {
    for run in runs {
        text.insert_with_attributes(tx, offset, &run.text, run.marks.clone());
        offset += run.text.encode_utf16().count() as u32;
    }
}

fn characters(runs: &[Run]) -> Vec<(char, &Formats)> {
    runs.iter()
        .flat_map(|run| run.text.chars().map(|ch| (ch, &run.marks)))
        .collect()
}

fn update_text(
    tx: &mut TransactionMut,
    text: &XmlTextRef,
    old: &[(char, &Formats)],
    mut old_pos: usize,
    new: &[(char, &Formats)],
    new_pos: usize,
    map: &PositionMap,
) {
    let mut positions = Vec::with_capacity(new.len());
    let mut pos = new_pos;
    for (ch, _) in new {
        positions.push(pos);
        pos += ch.len_utf16();
    }
    let mut anchors = Vec::new();
    for (index, (ch, _)) in old.iter().enumerate() {
        let mapped = map.map(DocPos(old_pos), Affinity::After);
        if !mapped.deleted
            && let Ok(i) = positions.binary_search(&mapped.position.0)
            && new[i].0 == *ch
        {
            anchors.push((index, i));
        }
        old_pos += ch.len_utf16();
    }
    let (mut prev, mut next, mut offset) = (0, 0, 0);
    for group in anchors.chunk_by(|a, b| a.0 + 1 == b.0 && a.1 + 1 == b.1) {
        let (a, b) = group[0];
        replace_chars(tx, text, &mut offset, &old[prev..a], &new[next..b]);
        prev = a + group.len();
        next = b + group.len();
        format_changed(tx, text, &mut offset, &old[a..prev], &new[b..next]);
    }
    replace_chars(tx, text, &mut offset, &old[prev..], &new[next..]);
}

fn replace_chars(
    tx: &mut TransactionMut,
    text: &XmlTextRef,
    offset: &mut u32,
    old: &[(char, &Formats)],
    new: &[(char, &Formats)],
) {
    let removed = old.iter().map(|(ch, _)| ch.len_utf16() as u32).sum();
    if removed > 0 {
        text.remove_range(tx, *offset, removed);
    }
    for run in new.chunk_by(|a, b| a.1 == b.1) {
        let string: String = run.iter().map(|(ch, _)| *ch).collect();
        text.insert_with_attributes(tx, *offset, &string, run[0].1.clone());
        *offset += string.encode_utf16().count() as u32;
    }
}

fn format_changed(
    tx: &mut TransactionMut,
    text: &XmlTextRef,
    offset: &mut u32,
    old: &[(char, &Formats)],
    new: &[(char, &Formats)],
) {
    let mut i = 0;
    while i < old.len() {
        let len = (i + 1..old.len())
            .take_while(|j| old[*j].1 == old[i].1 && new[*j].1 == new[i].1)
            .count()
            + 1;
        let units = old[i..i + len]
            .iter()
            .map(|(ch, _)| ch.len_utf16() as u32)
            .sum();
        if old[i].1 != new[i].1 {
            let mut changed = Formats::new();
            for key in old[i].1.keys() {
                if !new[i].1.contains_key(key) {
                    changed.insert(key.clone(), Any::Null);
                }
            }
            for (key, val) in new[i].1 {
                if old[i].1.get(key) != Some(val) {
                    changed.insert(key.clone(), val.clone());
                }
            }
            text.format(tx, *offset, units, changed);
        }
        *offset += units;
        i += len;
    }
}
