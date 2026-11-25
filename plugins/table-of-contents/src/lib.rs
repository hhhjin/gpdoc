//! Stable heading anchors and an outline for native navigation.
use gpdoc_plugin::prelude::*;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Heading {
    pub id: String,
    pub text: String,
    pub pos: DocPos,
    pub original_level: i64,
    pub level: usize,
    pub item_index: usize,
    pub is_active: bool,
    pub is_scrolled_over: bool,
}

unit_command!(pub Update, "updateTableOfContents");

pub fn headings(document: &Document) -> Vec<Heading> {
    outline(document, &["heading".into()])
}

pub fn outline(document: &Document, types: &[String]) -> Vec<Heading> {
    outline_at(document, types, None, Indexing::Linear)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indexing {
    Linear,
    Hierarchical,
}

/// Pass the first visible document position to mark the active heading and the
/// headings scrolled past. Hierarchical indexes restart inside each section.
pub fn outline_at(
    document: &Document,
    types: &[String],
    first_visible: Option<DocPos>,
    indexing: Indexing,
) -> Vec<Heading> {
    let mut active = None;
    let mut items: Vec<Heading> = Vec::new();
    for (pos, node) in document
        .iter_blocks()
        .filter(|(_, node)| types.contains(&node.kind))
    {
        let Some(Value::String(id)) = node.attrs.get("data-toc-id") else {
            continue;
        };
        let text = text_at(document, pos);
        if text.is_empty() {
            continue;
        }
        let original_level = node.attrs.get("level").and_then(Value::as_i64).unwrap_or(1);
        let (previous, depth) = items
            .last()
            .map(|h| (h.original_level, h.level))
            .unwrap_or((1, 1));
        let level = match original_level.cmp(&previous) {
            std::cmp::Ordering::Equal => depth,
            std::cmp::Ordering::Greater => depth + 1,
            std::cmp::Ordering::Less => items
                .iter()
                .rev()
                .find(|h| h.original_level <= original_level)
                .map_or(1, |h| h.level),
        };
        let item_index = match indexing {
            Indexing::Linear => items.len() + 1,
            Indexing::Hierarchical => items
                .iter()
                .rev()
                .find(|h| h.level <= level)
                .filter(|h| h.level == level)
                .map_or(1, |h| h.item_index + 1),
        };
        let is_scrolled_over = first_visible.is_some_and(|visible| pos <= visible);
        if is_scrolled_over {
            active = Some(items.len());
        }
        items.push(Heading {
            id: id.clone(),
            text,
            pos,
            original_level,
            level,
            item_index,
            is_scrolled_over,
            is_active: false,
        });
    }
    if let Some(i) = active {
        items[i].is_active = true;
    }
    items
}

fn text_at(document: &Document, pos: DocPos) -> String {
    let node = document
        .root()
        .node_at(pos.0)
        .expect("valid position")
        .expect("visited node");
    node.text_between(0, node.content.size(), "", "")
        .expect("validated range")
}

pub fn table_of_contents() -> Plugin {
    table_of_contents_with(["heading"], |_| Uuid::new_v4().to_string())
}

pub fn table_of_contents_with(
    types: impl IntoIterator<Item = impl Into<String>>,
    get_id: impl Fn(&str) -> String + Send + Sync + 'static,
) -> Plugin {
    let types: Vec<String> = types.into_iter().map(Into::into).collect();
    let attrs: AttrSpecs = ["id", "data-toc-id"]
        .map(|name| {
            (
                name.into(),
                AttrSpec::with_default(AttrType::NullableString, Value::Null),
            )
        })
        .into();
    Plugin::new("tableOfContents", |p| {
        p.attributes(AttributeTargets::names(types.clone()), attrs);
        // The public outline is derived from the current snapshot, including in a chain.
        p.command::<Update>(|ctx, _| Ok(Some(ctx.transaction())));
        p.append_edits(move |change| {
            if change.before.is_some_and(|doc| doc == change.document) {
                return Ok(vec![]);
            }
            let mut seen = HashSet::new();
            let mut steps = Vec::new();
            for (pos, node) in change
                .document
                .iter_blocks()
                .filter(|(_, n)| types.contains(&n.kind))
            {
                let text = text_at(change.document, pos);
                if text.is_empty() {
                    continue;
                }
                if let Some(Value::String(id)) = node.attrs.get("data-toc-id")
                    && seen.insert(id.clone())
                {
                    continue;
                }
                let id = get_id(&text);
                if !seen.insert(id.clone()) {
                    return Err(Error::InvalidEdit(format!(
                        "duplicate heading anchor '{id}'"
                    )));
                }
                for attr in ["id", "data-toc-id"] {
                    steps.push(Step::set_node_attribute(pos, attr, id.clone().into()).into());
                }
            }
            Ok(steps)
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin_heading::heading;
    use gpdoc_plugin_paragraph::paragraph;
    #[test]
    fn outline_tracks_text_depth_and_stable_anchors() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugin(paragraph())
            .plugin(heading())
            .plugin(table_of_contents_with(["heading"], str::to_owned))
            .build()
            .unwrap();
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            [("first", 1), ("child", 3), ("sibling", 1)]
                .map(|(label, level)| {
                    Node::new(
                        "heading",
                        [("level".into(), Value::from(level))].into(),
                        Node::text("heading", label).content().to_owned(),
                    )
                })
                .into(),
        )
        .unwrap();
        let tree = outline_at(
            editor.document(),
            &["heading".into()],
            Some(DocPos(8)),
            Indexing::Hierarchical,
        );
        assert_eq!(
            tree.iter()
                .map(|h| (h.item_index, h.is_active, h.is_scrolled_over))
                .collect::<Vec<_>>(),
            [(1, false, true), (1, true, true), (2, false, false)]
        );
        let original = headings(editor.document());
        assert_eq!(
            original
                .iter()
                .map(|h| (h.level, h.item_index, h.pos.0))
                .collect::<Vec<_>>(),
            [(1, 1, 0), (2, 2, 7), (1, 3, 14)]
        );
        editor.insert_text("a ").unwrap();
        let after = headings(editor.document());
        assert_eq!(after[0].text, "a first");
        assert_eq!(after[0].id, original[0].id);
        assert_eq!(after[1].pos.0, original[1].pos.0 + 2);
    }
}
