use gpdoc_plugin::prelude::*;

gpdoc_plugin::plugin_config! {
    pub Config {
        hooks Hooks { append_trailing_node: append }
    }
}

pub fn trailing_node() -> Plugin<Config> {
    trailing_node_with(None, Vec::new())
}

/// Append the configured node after edits that leave a different last block.
pub fn trailing_node_with(node: Option<String>, not_after: Vec<String>) -> Plugin<Config> {
    Plugin::<Config>::new("trailingNode", |p| {
        p.hooks().append_trailing_node().set(move |change| {
            if change.before.is_none() {
                return Ok(vec![]);
            }
            let schema = change.schema;
            let kind = node
                .as_deref()
                .or_else(|| schema.node_type("doc")?.content_match.default_type(schema))
                .unwrap_or(change.default_text_block);
            if change
                .document
                .blocks()
                .last()
                .is_some_and(|n| n.kind == kind || not_after.contains(&n.kind))
            {
                return Ok(vec![]);
            }
            let root = change.document.root();
            let at = root.content.size();
            let node = schema
                .create_and_fill(kind, Default::default(), Default::default(), vec![])?
                .ok_or_else(|| {
                    Error::InvalidSchema(format!("cannot fill trailing node '{kind}'"))
                })?;
            let slice = Slice::new(vec![node].into(), 0, 0);
            Ok(Step::fit_replace(schema, root, at, at, &slice)?
                .into_iter()
                .map(Into::into)
                .collect())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin_heading::{Level, Set, heading};
    use gpdoc_plugin_paragraph::paragraph;
    #[test]
    fn appended_block_shares_the_triggering_commands_history() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugin(paragraph())
            .plugin(heading())
            .plugin(trailing_node())
            .build()
            .unwrap();
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "hello")],
        )
        .unwrap();
        editor.run(Set(Level::H1)).unwrap();
        assert_eq!(editor.document().blocks().len(), 2);
        assert_eq!(editor.document().blocks()[1], Node::text("paragraph", ""));
        editor.undo().unwrap();
        assert_eq!(
            editor.document().blocks(),
            [Node::text("paragraph", "hello")]
        );
        editor.redo().unwrap();
        assert_eq!(editor.document().blocks().len(), 2);
    }
}
