//! Character and word counts using the same separators and UTF-16 units as Tiptap.
use gpdoc_plugin::prelude::{ContentRef, Document, Node, Plugin, Slice, Step};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CountMode {
    #[default]
    TextSize,
    NodeSize,
}

gpdoc_plugin::plugin_config! {
    pub Config {
        hooks Hooks { limit: filter, trim: append }
    }
}

pub fn characters(document: &Document, mode: CountMode) -> usize {
    CountOptions {
        mode,
        ..Default::default()
    }
    .characters(document.root())
}

pub fn words(document: &Document) -> usize {
    CountOptions::default().words(document.root())
}

#[derive(Clone, Copy, Debug)]
pub struct CountOptions {
    pub mode: CountMode,
    pub limit: Option<usize>,
    pub auto_trim: bool,
    pub text_counter: fn(&str) -> usize,
    pub word_counter: fn(&str) -> usize,
}
impl Default for CountOptions {
    fn default() -> Self {
        Self {
            mode: CountMode::TextSize,
            limit: None,
            auto_trim: true,
            text_counter: |text| text.encode_utf16().count(),
            word_counter: |text| text.split(' ').filter(|word| !word.is_empty()).count(),
        }
    }
}

impl CountOptions {
    pub fn characters(self, node: &Node) -> usize {
        match self.mode {
            CountMode::NodeSize => node.size(),
            CountMode::TextSize => (self.text_counter)(&plain_text(node, "")),
        }
    }
    pub fn words(self, node: &Node) -> usize {
        (self.word_counter)(&plain_text(node, " "))
    }
}

fn plain_text(node: &Node, separator: &str) -> String {
    node.text_between(0, node.content.size(), separator, " ")
        .expect("validated document range")
}
pub fn character_count(options: CountOptions) -> Plugin<Config> {
    Plugin::<Config>::new("characterCount", |p| {
        let Some(limit) = options.limit.filter(|n| *n != 0) else {
            return;
        };
        p.hooks().limit().set(move |change| {
            let size = options.characters(change.document.root());
            Ok(size <= limit
                || change
                    .before
                    .is_some_and(|old| size <= options.characters(old.root())))
        });
        p.hooks().trim().set(move |change| {
            let root = change.document.root();
            let count = options.characters(change.document.root());
            if count <= limit {
                return Ok(vec![]);
            }
            // Preserve UTF-16 scalar boundaries even when the excess ends inside an emoji.
            let excess = count - limit;
            if let Some(range) = change.pasted {
                // Only shorten the inserted clipboard text. Reject the whole edit
                // if fitting the replacement still exceeds the limit.
                if change
                    .before
                    .is_some_and(|old| options.characters(old.root()) > limit)
                {
                    return Ok(vec![]);
                }
                let end = range.end.0.min(root.content.size());
                let mut from = end.saturating_sub(excess).max(range.start.0);
                if from >= end {
                    return Ok(vec![]);
                }
                // A UTF-16 differencing boundary may land in a surrogate pair.
                if root.text_between(from, end, "", "").is_err() && from > range.start.0 {
                    from -= 1
                }
                let step = Step::fit_replace(change.schema, root, from, end, &Slice::empty())?;
                let Some(step) = step else { return Ok(vec![]) };
                let Ok((projected, _)) = step.preview(change.document) else {
                    return Ok(vec![]);
                };
                let size = options.characters(projected.root());
                return Ok(if size <= limit {
                    vec![step.into()]
                } else {
                    vec![]
                });
            }
            if change.before.is_some() || !options.auto_trim {
                return Ok(vec![]);
            }
            let mut removed = 0;
            let mut end: usize = 0;
            for (pos, node) in change.document.iter_nodes() {
                match node.content() {
                    ContentRef::Text(run) => {
                        end = pos.0;
                        for ch in run.text.chars() {
                            end += ch.len_utf16();
                            removed += ch.len_utf16();
                            if removed >= excess {
                                break;
                            }
                        }
                    }
                    ContentRef::Leaf(_) => {
                        end = pos.0 + 1;
                        removed += 1;
                    }
                    _ => {}
                }
                if removed >= excess {
                    break;
                }
            }
            if end == 0 {
                return Ok(vec![]);
            }
            let step = Step::fit_replace(change.schema, root, 0, end, &Slice::empty())?;
            Ok(step.into_iter().map(Into::into).collect::<Vec<_>>())
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin_paragraph::paragraph;
    #[test]
    fn counts_blocks_and_surrogate_pairs_and_limits_edits() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugin(paragraph())
            .plugin(character_count(CountOptions {
                limit: Some(7),
                ..Default::default()
            }))
            .build()
            .unwrap();
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![
                Node::text("paragraph", "two 😀"),
                Node::text("paragraph", "x"),
            ],
        )
        .unwrap();
        assert_eq!(characters(editor.document(), CountMode::TextSize), 7);
        assert_eq!(characters(editor.document(), CountMode::NodeSize), 13);
        assert_eq!(words(editor.document()), 3);
        editor.insert_text("z").unwrap();
        assert_eq!(editor.revision(), 0);
        let truncated = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "😀123456")],
        )
        .unwrap();
        assert_eq!(characters(truncated.document(), CountMode::TextSize), 6);
        let mut pasted = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "abc")],
        )
        .unwrap();
        pasted.paste_text("xy😀!").unwrap();
        assert_eq!(
            pasted.document().blocks(),
            [Node::text("paragraph", "xy😀abc")]
        );
        pasted.undo().unwrap();
        assert_eq!(pasted.document().blocks(), [Node::text("paragraph", "abc")]);
    }
}
