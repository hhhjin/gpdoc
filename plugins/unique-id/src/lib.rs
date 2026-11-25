//! Assign IDs to configured node types when a document opens or changes.
use gpdoc_plugin::prelude::*;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum IdTargets {
    All,
    Named(Vec<String>),
}
impl IdTargets {
    fn matches(&self, name: &str) -> bool {
        match self {
            Self::All => name != "doc" && name != "text",
            Self::Named(types) => types.iter().any(|t| t == name),
        }
    }
}

pub fn unique_id_all() -> Plugin {
    configured_ids(IdTargets::All, "id", true, |_, _| {
        Uuid::new_v4().to_string()
    })
}

pub fn unique_id(types: impl IntoIterator<Item = impl Into<String>>) -> Plugin {
    unique_id_with(types, "id", |_, _| Uuid::new_v4().to_string())
}

pub fn unique_id_with(
    types: impl IntoIterator<Item = impl Into<String>>,
    attribute: impl Into<String>,
    generate: impl Fn(&Node, DocPos) -> String + Send + Sync + 'static,
) -> Plugin {
    configured_ids(
        IdTargets::Named(types.into_iter().map(Into::into).collect()),
        attribute,
        true,
        generate,
    )
}

/// Set update_document to false to retain the ID attribute on a read-only schema.
pub fn configured_ids(
    types: IdTargets,
    attribute: impl Into<String>,
    update_document: bool,
    generate: impl Fn(&Node, DocPos) -> String + Send + Sync + 'static,
) -> Plugin {
    let attribute = attribute.into();
    let attrs = AttrSpecs::from([(
        attribute.clone(),
        AttrSpec::with_default(AttrType::NullableString, Value::Null),
    )]);
    let schema_targets = types.clone();
    Plugin::new("uniqueID", |p| {
        p.attributes(
            AttributeTargets::nodes(move |spec| schema_targets.matches(&spec.name)),
            attrs,
        );
        if !update_document {
            return;
        }
        p.append_edits(move |change| {
            if change.before.is_some_and(|doc| doc == change.document) {
                return Ok(vec![]);
            }
            let targets: Vec<_> = change
                .document
                .iter_nodes()
                .filter(|(_, node)| types.matches(&node.kind))
                .collect();
            // Preserve an unchanged original if a duplicate was pasted before it.
            let mut existing = HashSet::new();
            if let Some(before) = change.before {
                for (_, node) in before.iter_nodes() {
                    if types.matches(&node.kind) {
                        existing.insert(std::ptr::from_ref(&**node));
                    }
                }
            }
            let mut keep = HashMap::new();
            for &(pos, node) in &targets {
                if let Some(Value::String(id)) = node.attrs.get(&attribute) {
                    if existing.contains(&std::ptr::from_ref(&**node)) {
                        keep.insert(id.clone(), pos);
                    } else {
                        keep.entry(id.clone()).or_insert(pos);
                    }
                }
            }
            let mut reserved: HashSet<_> = keep.keys().cloned().collect();
            let mut steps = Vec::new();
            for (pos, node) in targets {
                if matches!(node.attrs.get(&attribute), Some(Value::String(id)) if keep[id]==pos) {
                    continue;
                }
                let id = generate(node, pos);
                if !reserved.insert(id.clone()) {
                    return Err(Error::InvalidEdit(format!(
                        "ID generator returned duplicate '{id}'"
                    )));
                }
                steps.push(
                    Step::set_node_attribute(DocPos(pos.0), attribute.clone(), id.into()).into(),
                );
            }
            Ok(steps)
        });
    })
    .priority(10000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin_paragraph::paragraph;
    #[test]
    fn pasted_duplicate_before_original_gets_new_id_and_undo_restores_document() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugin(paragraph())
            .plugin(unique_id(["paragraph"]))
            .build()
            .unwrap();
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "hello")],
        )
        .unwrap();
        let first = editor.document().blocks()[0].clone();
        let id = first.attrs["id"].clone();
        let mut tx = editor.transaction();
        let copy = Node::new(
            "paragraph",
            Attrs::from([("id".into(), id.clone())]),
            first.content().to_owned(),
        );
        tx.push(Edit::ReplaceNodes {
            range: DocPos(0)..DocPos(0),
            nodes: vec![copy],
        });
        tx.select(Selection::caret(DocPos(1)));
        editor.apply(tx).unwrap();
        assert_eq!(editor.document().blocks()[1].attrs["id"], id);
        assert_ne!(editor.document().blocks()[0].attrs["id"], id);
        editor.undo().unwrap();
        assert_eq!(editor.document().blocks(), [first]);
    }
}
