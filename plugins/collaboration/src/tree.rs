//! Tiptap's Yjs representation uses one XmlText for adjacent marked text runs.
use crate::Error;
use gpdoc_plugin::core::{Affinity, PositionMap};
use gpdoc_plugin::prelude::{Attrs, Content, ContentRef, DocPos, Mark, Node, Schema, TextRun};
use y_octo::{
    Any, Doc, DocSession, SessionReadValue, SessionValue, TextAttributes as Formats, TextDeltaOp,
    TextInsert, Transaction, TypeTarget, Value, XMLFragment,
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
        .map(|(k, v)| Ok((k.as_str().into(), attribute_value(v)?)))
        .collect()
}

// JSON numbers represent JavaScript Numbers in Tiptap. Preserve integers that
// cannot be represented exactly as a Number using the Yjs bigint encoding.
fn attribute_value(value: &serde_json::Value) -> Result<Any, Error> {
    use serde_json::Value as Json;
    Ok(match value {
        Json::Number(number) if number.is_f64() => Any::from(number.as_f64().unwrap()),
        Json::Number(number) => {
            let integer = number
                .as_i64()
                .ok_or_else(|| Error::Content(format!("attribute integer {number} exceeds i64")))?;
            let float = integer as f64;
            if float as i128 == i128::from(integer) {
                Any::from(float)
            } else {
                Any::BigInt64(integer)
            }
        }
        Json::Array(values) => Any::Array(
            values
                .iter()
                .map(attribute_value)
                .collect::<Result<_, _>>()?,
        ),
        Json::Object(values) => Any::Object(Box::new(
            values
                .iter()
                .map(|(k, v)| Ok((k.clone(), attribute_value(v)?)))
                .collect::<Result<_, Error>>()?,
        )),
        Json::Bool(value) => Any::from(*value),
        Json::String(value) => Any::from(value.clone()),
        Json::Null => Any::Null,
    })
}

fn decode_attribute(value: &Any) -> Result<serde_json::Value, Error> {
    use serde_json::Value as Json;
    let number = match value {
        Any::Float32(n) => f64::from(n.0),
        Any::Float64(n) => n.0,
        Any::Object(entries) => {
            return Ok(Json::Object(
                entries
                    .iter()
                    .map(|(key, val)| Ok((key.clone(), decode_attribute(val)?)))
                    .collect::<Result<_, Error>>()?,
            ));
        }
        Any::Array(values) => {
            return Ok(Json::Array(
                values
                    .iter()
                    .map(decode_attribute)
                    .collect::<Result<_, _>>()?,
            ));
        }
        other => return Ok(serde_json::to_value(other)?),
    };
    // Yjs can encode an integer as a float. Keep it valid for Integer attributes.
    Ok(
        if number.fract() == 0.0 && (i64::MIN as f64..-(i64::MIN as f64)).contains(&number) {
            Json::from(number as i64)
        } else {
            Json::from(number)
        },
    )
}

fn decode_attrs(attrs: &Formats) -> Result<Attrs, Error> {
    attrs
        .iter()
        .map(|(k, v)| Ok((k.clone(), decode_attribute(v)?)))
        .collect()
}

pub(crate) fn encode(nodes: &[Node], depth: usize) -> Result<Vec<Child>, Error> {
    check_depth(depth)?;
    let mut result = Vec::new();
    for node in nodes {
        if let ContentRef::Text(run) = node.content() {
            let mut marks = Formats::new();
            for mark in run.marks {
                let attrs = encode_attrs(&mark.attrs)?.into_iter().collect();
                if marks
                    .insert(mark.kind.as_str().into(), Any::Object(Box::new(attrs)))
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
            ContentRef::Blocks(nodes) | ContentRef::Inline(nodes) => encode(nodes, depth + 1)?,
            _ => {
                check_depth(depth + 1)?;
                vec![]
            }
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
                        let Any::Object(values) = attrs else {
                            return Err(Error::Content("invalid Yjs mark".into()));
                        };
                        let attrs = values.into_iter().collect();
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

pub(crate) fn read(doc: &Doc, root: &XMLFragment) -> Result<Vec<Child>, Error> {
    doc.transact(|tx| read_children(tx, tx.xml_fragment(root)?.iter(), 0))?
}

fn check_depth(depth: usize) -> Result<(), Error> {
    if depth > 128 {
        return Err(Error::Content("Yjs XML nesting exceeds 128".into()));
    }
    Ok(())
}

fn read_children(
    tx: &Transaction,
    children: impl Iterator<Item = Value>,
    depth: usize,
) -> Result<Vec<Child>, Error> {
    check_depth(depth)?;
    children
        .map(|child| match child {
            Value::XMLText(text) => {
                // identity_runs also reports shared-type embeds that to_delta skips.
                let mut runs: Vec<Run> = Vec::new();
                for run in tx.xml_text(&text)?.identity_runs()? {
                    let TextInsert::Text(text) = run.insert else {
                        return Err(Error::Content("unsupported inline Yjs embed".into()));
                    };
                    if let Some(last) = runs.last_mut()
                        && last.marks == run.attributes
                    {
                        last.text.push_str(&text);
                    } else {
                        runs.push(Run {
                            text,
                            marks: run.attributes,
                        });
                    }
                }
                Ok(Child::Text(runs))
            }
            Value::XMLElement(el) => {
                let el = tx.xml_element(&el)?;
                let attrs = el
                    .attributes()
                    .into_iter()
                    .map(|(key, value)| match value {
                        Value::Any(value) => Ok((key, value)),
                        _ => Err(Error::Content(format!("invalid XML attribute {key}"))),
                    })
                    .collect::<Result<_, _>>()?;
                Ok(Child::Element {
                    kind: el.tag_name().to_owned(),
                    attrs,
                    children: read_children(tx, el.iter(), depth + 1)?,
                })
            }
            _ => Err(Error::Content("unexpected XML child".into())),
        })
        .collect()
}

fn xml_value(child: &Child) -> SessionValue {
    match child {
        Child::Element {
            kind,
            attrs,
            children,
        } => SessionValue::XmlElement {
            tag: kind.clone(),
            attrs: attrs.clone().into_iter().collect(),
            children: children.iter().map(xml_value).collect(),
        },
        Child::Text(runs) => SessionValue::XmlText(
            runs.iter()
                .map(|r| TextDeltaOp::Insert {
                    insert: TextInsert::Text(r.text.clone()),
                    format: Some(r.marks.clone()),
                })
                .collect(),
        ),
    }
}

pub(crate) fn seed(tx: &mut DocSession, parent: &TypeTarget, nodes: &[Child]) -> Result<(), Error> {
    tx.array_push(parent, nodes.iter().map(xml_value).collect())?;
    Ok(())
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
    tx: &mut DocSession,
    root: &TypeTarget,
    old: &[Child],
    new: &[Child],
    schema: &Schema,
    map: &PositionMap,
) -> Result<(), Error> {
    let mut edit = Reconcile { tx, schema, map };
    edit.children(root, old, 0, new, 0)
}

struct Reconcile<'a> {
    tx: &'a mut DocSession,
    schema: &'a Schema,
    map: &'a PositionMap,
}

impl Reconcile<'_> {
    fn children(
        &mut self,
        parent: &TypeTarget,
        old: &[Child],
        old_start: usize,
        new: &[Child],
        new_start: usize,
    ) -> Result<(), Error> {
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
                self.tx
                    .array_remove(parent, cursor, group.parts.len() as u64)?;
                continue;
            };
            for g in &new[next..next + skip] {
                for child in g.parts {
                    self.tx
                        .array_insert(parent, cursor, vec![xml_value(child)])?;
                    cursor += 1;
                }
            }
            next += skip;
            self.patch(parent, cursor, group, &new[next])?;
            cursor += group.parts.len() as u64;
            next += 1;
        }
        for g in &new[next..] {
            for child in g.parts {
                self.tx
                    .array_insert(parent, cursor, vec![xml_value(child)])?;
                cursor += 1;
            }
        }
        Ok(())
    }

    fn patch(
        &mut self,
        parent: &TypeTarget,
        cursor: u64,
        old: &Group,
        new: &Group,
    ) -> Result<(), Error> {
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
                let Some(SessionReadValue::XmlElement { identity, .. }) =
                    self.tx.read_array_at(parent, cursor)?
                else {
                    return Err(Error::Content(
                        "missing XML element during reconciliation".into(),
                    ));
                };
                let el = identity.into();
                for key in attrs.keys().filter(|k| !values.contains_key(*k)) {
                    self.tx.xml_remove_attr(&el, key)?;
                }
                for (key, value) in values {
                    if attrs.get(key) != Some(value) {
                        self.tx.xml_set_attr(&el, key.clone(), value.clone())?;
                    }
                }
                self.children(&el, children, old.start + 1, kids, new.start + 1)?;
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
                    let Some(SessionReadValue::XmlText(text)) =
                        self.tx.read_array_at(parent, cursor + i as u64)?
                    else {
                        return Err(Error::Content(
                            "missing XML text during reconciliation".into(),
                        ));
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
                        &text.into(),
                        &characters(prev),
                        old_pos,
                        &chars[target..limit],
                        new_pos,
                        self.map,
                    )?;
                    target = limit;
                    old_pos = end;
                    new_pos = next_pos;
                }
            }
            _ => unreachable!("matched XML type"),
        }
        Ok(())
    }
}

fn characters(runs: &[Run]) -> Vec<(char, &Formats)> {
    runs.iter()
        .flat_map(|run| run.text.chars().map(|ch| (ch, &run.marks)))
        .collect()
}

fn update_text(
    tx: &mut DocSession,
    text: &TypeTarget,
    old: &[(char, &Formats)],
    mut old_pos: usize,
    new: &[(char, &Formats)],
    new_pos: usize,
    map: &PositionMap,
) -> Result<(), Error> {
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
        replace_chars(tx, text, &mut offset, &old[prev..a], &new[next..b])?;
        prev = a + group.len();
        next = b + group.len();
        format_changed(tx, text, &mut offset, &old[a..prev], &new[b..next])?;
    }
    replace_chars(tx, text, &mut offset, &old[prev..], &new[next..])
}

fn replace_chars(
    tx: &mut DocSession,
    text: &TypeTarget,
    offset: &mut u64,
    old: &[(char, &Formats)],
    new: &[(char, &Formats)],
) -> Result<(), Error> {
    let removed = old.iter().map(|(ch, _)| ch.len_utf16() as u64).sum();
    if removed > 0 {
        tx.text_remove(text, *offset, removed)?;
    }
    for run in new.chunk_by(|a, b| a.1 == b.1) {
        let string: String = run.iter().map(|(ch, _)| *ch).collect();
        tx.text_apply_delta(
            text,
            &[
                TextDeltaOp::Retain {
                    retain: *offset,
                    format: None,
                },
                TextDeltaOp::Insert {
                    insert: TextInsert::Text(string.clone()),
                    format: Some(run[0].1.clone()),
                },
            ],
        )?;
        *offset += string.encode_utf16().count() as u64;
    }
    Ok(())
}

fn format_changed(
    tx: &mut DocSession,
    text: &TypeTarget,
    offset: &mut u64,
    old: &[(char, &Formats)],
    new: &[(char, &Formats)],
) -> Result<(), Error> {
    let mut i = 0;
    while i < old.len() {
        let len = (i + 1..old.len())
            .take_while(|j| old[*j].1 == old[i].1 && new[*j].1 == new[i].1)
            .count()
            + 1;
        let units = old[i..i + len]
            .iter()
            .map(|(ch, _)| ch.len_utf16() as u64)
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
            tx.text_format(text, *offset, units, changed)?;
        }
        *offset += units;
        i += len;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Any, attribute_value, decode_attribute};
    use serde_json::json;

    #[test]
    fn numeric_attributes_keep_their_yjs_number_type_and_round_trip_losslessly() {
        assert_eq!(attribute_value(&json!(-7)).unwrap(), Any::Integer(-7));
        let attrs = json!({"nested": [-4_294_967_297_i64, 8.25, i64::MAX]});
        assert_eq!(
            decode_attribute(&attribute_value(&attrs).unwrap()).unwrap(),
            attrs
        );
        assert!(attribute_value(&json!(u64::MAX)).is_err());
    }
}
