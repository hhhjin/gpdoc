//! Match completion triggers before the caret and replace their text in one edit.
#[cfg(feature = "gpui")]
mod picker;
#[cfg(feature = "gpui")]
pub use picker::{MenuRenderer, SuggestionItem, SuggestionMenu, SuggestionRow, suggestion_picker};

use gpdoc_plugin::core::{
    CommandContext, DocPos, Document, Edit, Error, Node, Selection, TextRun, Transaction,
};
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct Trigger {
    pub character: char,
    pub allow_spaces: bool,
    pub allow_to_include_char: bool,
    pub allowed_prefixes: Option<Vec<char>>,
    pub start_of_line: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuggestionMatch {
    pub range: Range<DocPos>,
    pub query: String,
    pub text: String,
}

impl Trigger {
    pub fn new(character: char) -> Self {
        Self {
            character,
            allow_spaces: false,
            allow_to_include_char: false,
            allowed_prefixes: Some(vec![' ']),
            start_of_line: false,
        }
    }

    pub fn find(&self, document: &Document, selection: Selection) -> Option<SuggestionMatch> {
        let Selection::Text { anchor, head } = selection else {
            return None;
        };
        if anchor != head {
            return None;
        }
        let before = document.root().resolve(head.0).ok()?.node_before().ok()??;
        let text = before.text.as_deref()?;
        let line = text.rsplit('\n').next()?;
        let allow_spaces = self.allow_spaces && !self.allow_to_include_char;
        let start = if self.start_of_line {
            line.starts_with(self.character).then_some(0)?
        } else if allow_spaces {
            line.match_indices(self.character)
                .rfind(|(offset, _)| *offset == 0 || line[..*offset].ends_with(char::is_whitespace))
                .or_else(|| line.match_indices(self.character).next())?
                .0
        } else if self.allow_to_include_char {
            let word = line.rsplit(char::is_whitespace).next()?;
            line.len() - word.len() + word.find(self.character)?
        } else {
            line.rfind(self.character)?
        };
        let prefix = line[..start].chars().next_back();
        if let (Some(ch), Some(allowed)) = (prefix, &self.allowed_prefixes)
            && !allowed.contains(&ch)
        {
            return None;
        }
        let matched = &line[start..];
        let query = &matched[self.character.len_utf8()..];
        if (!allow_spaces && query.contains(char::is_whitespace))
            || (!allow_spaces && !self.allow_to_include_char && query.contains(self.character))
        {
            return None;
        }
        Some(SuggestionMatch {
            range: DocPos(head.0 - matched.encode_utf16().count())..head,
            query: query.into(),
            text: matched.into(),
        })
    }

    /// Accept a mention-style suggestion. The cursor ends after a single space.
    pub fn replace_with_atom(
        &self,
        ctx: CommandContext<'_>,
        node: Node,
    ) -> Result<Option<Transaction>, Error> {
        let Some(found) = self.find(ctx.document(), ctx.selection()) else {
            return Ok(None);
        };
        let from = ctx.document().resolve(found.range.start)?;
        let to = ctx.document().resolve(found.range.end)?;
        let schema = ctx.schema();
        let parent = schema
            .node_type(&from.parent().kind)
            .expect("registered parent");
        if !schema
            .node_type(&node.kind)
            .is_some_and(|ty| ty.is_inline() && ty.is_leaf())
            || parent.content_match.match_type(&node.kind).is_none()
        {
            return Ok(None);
        }
        let skip = to
            .node_after()?
            .and_then(|n| n.text.clone())
            .is_some_and(|text| text.starts_with(' '));
        let start = found.range.start;
        let end = DocPos(found.range.end.0 + usize::from(skip));
        let mut tx = ctx.transaction();
        tx.push(Edit::ReplaceInline {
            range: start..end,
            nodes: vec![
                node,
                TextRun {
                    text: " ".into(),
                    marks: vec![],
                }
                .into(),
            ],
        });
        tx.select(Selection::caret(DocPos(start.0 + 2)));
        Ok(Some(tx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_queries_at_utf16_offsets_and_honors_prefixes() {
        let editor = gpdoc_plugin::core::Editor::from_document(
            &gpdoc_plugin::core::EditorConfig::builder()
                .plugins([gpdoc_plugin_paragraph::paragraph()])
                .build()
                .unwrap(),
            vec![Node::text("paragraph", "😀 @한글 x@y")],
        )
        .unwrap();
        let trigger = Trigger::new('@');
        let found = trigger
            .find(editor.document(), Selection::caret(DocPos(7)))
            .unwrap();
        assert_eq!(found.range, DocPos(4)..DocPos(7));
        assert_eq!(found.query, "한글");
        assert!(
            trigger
                .find(editor.document(), Selection::caret(DocPos(11)))
                .is_none()
        );
    }
}
