//! Search text across mark boundaries in UTF-16 document coordinates.
#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{MatchHighlight, paint_match, search_highlights};

use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::prelude::{
    Command, CommandContext, ContentRef, DocPos, Document, Edit, Editor, Error, Node, Plugin,
    Selection, Transaction, unit_command,
};
use regex::{Regex, RegexBuilder};
use std::{ops::Range, sync::LazyLock};

const NAME: &str = "findAndReplace";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchOptions {
    pub term: String,
    pub case_sensitive: bool,
    pub use_regex: bool,
    pub whole_word: bool,
}

#[derive(Clone, Debug)]
pub struct SetSearchTerm(pub String);
impl Command for SetSearchTerm {
    const NAME: &'static str = "setSearchTerm";
}
#[derive(Clone, Debug)]
pub struct SetReplaceTerm(pub String);
impl Command for SetReplaceTerm {
    const NAME: &'static str = "setReplaceTerm";
}
#[derive(Clone, Debug)]
pub struct SetUseRegex(pub bool);
impl Command for SetUseRegex {
    const NAME: &'static str = "setUseRegex";
}
#[derive(Clone, Debug)]
pub struct SetWholeWord(pub bool);
impl Command for SetWholeWord {
    const NAME: &'static str = "setWholeWord";
}
#[derive(Clone, Debug)]
pub struct SetCaseSensitive(pub bool);
impl Command for SetCaseSensitive {
    const NAME: &'static str = "setCaseSensitive";
}
unit_command!(pub ClearSearch, "clearSearch");
unit_command!(pub GoToNextResult, "goToNextResult");
unit_command!(pub GoToPreviousResult, "goToPreviousResult");
unit_command!(pub Replace, "replace");
unit_command!(pub ReplaceAll, "replaceAll");

pub fn search(document: &Document, options: &SearchOptions) -> Vec<Range<DocPos>> {
    search_with(document, options, |_| false)
}

/// Search connected textblocks as a single string, preserving the original document positions.
pub fn search_with(
    document: &Document,
    options: &SearchOptions,
    continues_node: impl Fn(&Node) -> bool,
) -> Vec<Range<DocPos>> {
    if options.term.is_empty() {
        return vec![];
    }
    let pattern = if options.use_regex {
        options.term.clone()
    } else {
        regex::escape(&options.term)
    };
    let Ok(regex) = RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .build()
    else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut group = TextGroup::default();
    for (pos, block) in document.iter_blocks() {
        match block.content() {
            ContentRef::Inline(children) => {
                if !continues_node(block) {
                    group.finish(&regex, options, &mut out);
                }
                let mut cursor = pos.0 + 1;
                group.origins.push((group.utf16_len, cursor));
                for child in children {
                    if let ContentRef::Text(run) = child.content() {
                        group.text.push_str(run.text);
                        group.utf16_len += child.size();
                    } else {
                        group.atoms.push(group.text.len());
                        group.text.push('\n');
                        group.utf16_len += 1;
                        if child.size() != 1 {
                            group.origins.push((group.utf16_len, cursor + child.size()));
                        }
                    }
                    cursor += child.size();
                }
            }
            // Container openings can separate adjacent textblocks; only a whole intervening node interrupts the chain.
            ContentRef::Blocks(children) if !children.is_empty() => {}
            _ => group.finish(&regex, options, &mut out),
        }
    }
    group.finish(&regex, options, &mut out);
    out
}

#[derive(Default)]
struct TextGroup {
    text: String,
    utf16_len: usize,
    atoms: Vec<usize>,
    // Start in the concatenated text (UTF-16), start in document coordinates.
    origins: Vec<(usize, usize)>,
}

impl TextGroup {
    fn finish(&mut self, regex: &Regex, options: &SearchOptions, out: &mut Vec<Range<DocPos>>) {
        let mut byte = 0;
        let mut offset = 0;
        for found in regex.find_iter(&self.text) {
            let start = found.start();
            let end = found.end();
            if found.is_empty()
                || self
                    .atoms
                    .get(self.atoms.partition_point(|&p| p < start))
                    .is_some_and(|&p| p < end)
            {
                continue;
            }
            if options.whole_word
                && !options.use_regex
                && (self.text[..start].chars().next_back().is_some_and(is_word)
                    || self.text[end..].chars().next().is_some_and(is_word))
            {
                continue;
            }
            offset += self.text[byte..start].encode_utf16().count();
            let next = offset + found.as_str().encode_utf16().count();
            // Resolve the last matched unit inside its own block, even when the next block starts at the same offset.
            let from = self.position(offset);
            let to = self.position(next - 1) + 1;
            out.push(DocPos(from)..DocPos(to));
            byte = end;
            offset = next;
        }
        self.text.clear();
        self.utf16_len = 0;
        self.atoms.clear();
        self.origins.clear();
    }

    fn position(&self, offset: usize) -> usize {
        let index = self.origins.partition_point(|&(start, _)| start <= offset);
        let (start, pos) = self.origins[index - 1];
        pos + offset - start
    }
}

fn is_word(ch: char) -> bool {
    static WORD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[\p{L}\p{M}\p{N}\p{Pc}]$").unwrap());
    WORD.is_match(ch.encode_utf8(&mut [0; 4]))
}

#[derive(Clone, Debug, Default, PartialEq)]
struct SearchState {
    options: SearchOptions,
    replacement: String,
    current: Option<usize>,
}

fn update(
    ctx: CommandContext<'_>,
    change: impl FnOnce(&mut SearchState),
) -> Result<Option<Transaction>, Error> {
    let mut tx = ctx.transaction();
    tx.update_plugin_state(change);
    Ok(Some(tx))
}

impl SearchState {
    fn query(&mut self) -> &mut SearchOptions {
        self.current = None;
        &mut self.options
    }

    fn current_index(&self, count: usize) -> Option<usize> {
        count
            .checked_sub(1)
            .map(|last| self.current.unwrap_or(0).min(last))
    }
}

struct SearchScope(Box<dyn Fn(&Node) -> bool + Send + Sync>);

impl SearchScope {
    fn ranges(
        config: &EditorConfig,
        doc: &Document,
        options: &SearchOptions,
    ) -> Vec<Range<DocPos>> {
        config
            .plugin_data::<Self>()
            .next()
            .map_or_else(Vec::new, |(_, scope)| search_with(doc, options, &scope.0))
    }
}

/// Matches in document order. `current` indexes `ranges` and defaults to the first match.
/// Ranges use UTF-16 document positions with exclusive ends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub ranges: Vec<Range<DocPos>>,
    pub current: Option<usize>,
}

/// Read the matches and active index for this session's document and search settings.
pub fn results(editor: &Editor) -> SearchResults {
    let Some(state) = editor.plugin_state::<SearchState>() else {
        return SearchResults::default();
    };
    let ranges = SearchScope::ranges(editor.config(), editor.document(), &state.options);
    SearchResults {
        current: state.current_index(ranges.len()),
        ranges,
    }
}

#[derive(Clone, Copy)]
enum Action {
    Next,
    Previous,
    Replace,
    ReplaceAll,
}

fn edit(ctx: CommandContext<'_>, action: Action) -> Result<Option<Transaction>, Error> {
    let Some(state) = ctx.plugin_state::<SearchState>() else {
        return Ok(None);
    };
    let matches = SearchScope::ranges(ctx.config(), ctx.document(), &state.options);
    let Some(current) = state.current_index(matches.len()) else {
        return Ok(None);
    };
    let mut tx = ctx.transaction();
    let replacement = &state.replacement;
    let selection = match action {
        Action::Next | Action::Previous => {
            let index = match (state.current, action) {
                (None, _) => 0,
                (_, Action::Next) => (current + 1) % matches.len(),
                _ => (current + matches.len() - 1) % matches.len(),
            };
            Some((index, matches[index].clone()))
        }
        Action::Replace => {
            let range = matches[current].clone();
            let end = range.start.0 + replacement.encode_utf16().count();
            tx.push(Edit::ReplaceText {
                range,
                text: replacement.clone(),
            });
            let next = SearchScope::ranges(ctx.config(), tx.document()?, &state.options);
            let index = next.iter().position(|r| r.start.0 >= end).unwrap_or(0);
            next.get(index).cloned().map(|r| (index, r))
        }
        Action::ReplaceAll => {
            for range in matches.into_iter().rev() {
                tx.push(Edit::ReplaceText {
                    range,
                    text: replacement.clone(),
                });
            }
            None
        }
    };
    let current = selection.map(|(index, range)| {
        tx.select(Selection::Text {
            anchor: range.start,
            head: range.end,
        });
        index
    });
    tx.update_plugin_state(|state: &mut SearchState| state.current = current);
    Ok(Some(tx))
}

pub fn find_and_replace() -> Plugin {
    find_and_replace_with(|_| false)
}

/// A textblock accepted by the predicate continues the preceding textblock for search and replacement.
pub fn find_and_replace_with(
    continues_node: impl Fn(&Node) -> bool + Send + Sync + 'static,
) -> Plugin {
    let plugin = Plugin::new(NAME, |p| {
        p.metadata(SearchScope(Box::new(continues_node)));
        p.command::<SetSearchTerm>(|ctx, SetSearchTerm(term)| {
            update(ctx, |s| s.query().term = term)
        });
        p.command::<SetReplaceTerm>(|ctx, SetReplaceTerm(term)| {
            update(ctx, |s| s.replacement = term)
        });
        p.command::<SetUseRegex>(|ctx, SetUseRegex(value)| {
            update(ctx, |s| s.query().use_regex = value)
        });
        p.command::<SetWholeWord>(|ctx, SetWholeWord(value)| {
            update(ctx, |s| s.query().whole_word = value)
        });
        p.command::<SetCaseSensitive>(|ctx, SetCaseSensitive(value)| {
            update(ctx, |s| s.query().case_sensitive = value)
        });
        p.command::<ClearSearch>(|ctx, _| update(ctx, |s| s.query().term.clear()));
        p.command::<GoToNextResult>(|ctx, _| edit(ctx, Action::Next));
        p.command::<GoToPreviousResult>(|ctx, _| edit(ctx, Action::Previous));
        p.command::<Replace>(|ctx, _| edit(ctx, Action::Replace));
        p.command::<ReplaceAll>(|ctx, _| edit(ctx, Action::ReplaceAll));
    });
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, || search_highlights(paint_match));
    plugin
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin::prelude::{Error, Node};
    use gpdoc_plugin_paragraph::paragraph;
    #[test]
    fn continued_blocks_map_utf16_offsets_and_replace_across_paragraphs() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugins([paragraph(), find_and_replace_with(|_| true)])
            .build()
            .unwrap();
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![
                Node::text("paragraph", "😀 Wor"),
                Node::text("paragraph", "ld!"),
            ],
        )
        .unwrap();
        let query = SetSearchTerm("world".into());
        editor.run(query).unwrap();
        assert_eq!(results(&editor).ranges, [DocPos(4)..DocPos(11)]);
        editor.run(SetReplaceTerm("Moon".into())).unwrap();
        editor.run(Replace).unwrap();
        assert_eq!(
            editor.document().blocks(),
            [Node::text("paragraph", "😀 Moon!")]
        );
        editor.undo().unwrap();
        assert_eq!(results(&editor).ranges, [DocPos(4)..DocPos(11)]);
    }

    #[test]
    fn searching_and_clearing_preserve_document_and_edit_history() -> Result<(), Error> {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugins([paragraph(), find_and_replace()])
            .build()?;
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "apple apple")],
        )?;
        let before = editor.document().to_json().unwrap();
        editor.insert_text("🍏 ")?;
        let edited = editor.document().to_json().unwrap();
        editor.undo()?;

        editor.run(SetSearchTerm("apple".into()))?;
        editor.run(GoToNextResult)?;
        editor.run(GoToNextResult)?;
        assert_eq!(results(&editor).current, Some(1));
        assert_eq!(editor.document().to_json().unwrap(), before);

        editor.run(ClearSearch)?;
        assert_eq!(results(&editor), SearchResults::default());
        assert_eq!(editor.document().to_json().unwrap(), before);
        assert_eq!(editor.undo().unwrap_err(), Error::NoHistory);
        editor.redo()?;
        assert_eq!(editor.document().to_json().unwrap(), edited);
        Ok(())
    }

    #[test]
    fn search_state_is_per_editor_and_transactional() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugins([paragraph(), find_and_replace()])
            .build()
            .unwrap();
        let mut left = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "😀a A")],
        )
        .unwrap();
        let right =
            gpdoc_plugin::core::Editor::from_document(&config, left.document().blocks().to_vec())
                .unwrap();
        unit_command!(Unknown, "unknown");
        let query = SetSearchTerm("a".into());
        assert!(left.can(query.clone()).unwrap());
        assert!(results(&left).ranges.is_empty());
        assert!(matches!(
            left.chain().command(query.clone()).command(Unknown).run(),
            Err(Error::UnknownCommand(_))
        ));
        assert!(results(&left).ranges.is_empty());
        left.run(query).unwrap();
        assert_eq!(
            results(&left).ranges,
            [DocPos(3)..DocPos(4), DocPos(5)..DocPos(6)]
        );
        assert_eq!(results(&left).current, Some(0));
        left.run(GoToNextResult).unwrap();
        left.run(GoToNextResult).unwrap();
        assert_eq!(results(&left).current, Some(1));
        assert!(results(&right).ranges.is_empty());
        left.run(ReplaceAll).unwrap();
        assert_eq!(left.document().blocks(), [Node::text("paragraph", "😀 ")]);
        left.undo().unwrap();
        assert_eq!(results(&left).ranges.len(), 2);
    }
}
