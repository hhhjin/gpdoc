//! Syntax coloring for code blocks. Tokens decorate text without adding document marks.
use gpdoc_plugin::core::SyntaxToken;
use gpdoc_plugin::prelude::{Plugin, Value};
use gpdoc_plugin_code_block::{Config, code_block};
use std::sync::LazyLock;
use syntect::{
    easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet, util::LinesWithEndings,
};

/// Use bundled syntax definitions and a light theme. Unknown languages render as plain text.
pub fn code_block_lowlight() -> Plugin<Config> {
    static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
    static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
    code_block_with_highlighter(|language, text| {
        let Some(syntax) = language
            .and_then(|name| SYNTAXES.find_syntax_by_token(name))
            .or_else(|| {
                text.lines()
                    .next()
                    .and_then(|first| SYNTAXES.find_syntax_by_first_line(first))
            })
        else {
            return Vec::new();
        };
        let mut highlighter = HighlightLines::new(syntax, &THEMES.themes["InspiredGitHub"]);
        let mut offset = 0;
        let mut tokens = Vec::new();
        for line in LinesWithEndings::from(text) {
            let Ok(spans) = highlighter.highlight_line(line, &SYNTAXES) else {
                break;
            };
            for (style, chunk) in spans {
                let end = offset + chunk.len();
                let c = style.foreground;
                tokens.push(SyntaxToken {
                    bytes: offset..end,
                    color: [c.r, c.g, c.b, c.a],
                });
                offset = end;
            }
        }
        tokens
    })
}

pub fn code_block_with_highlighter(
    highlight: impl Fn(Option<&str>, &str) -> Vec<SyntaxToken> + Send + Sync + 'static,
) -> Plugin<Config> {
    code_block().extend(|p| {
        p.syntax_highlighter("codeBlock", move |attrs, text| {
            let language = match attrs.get("language") {
                Some(Value::String(language)) => Some(language.as_str()),
                _ => None,
            };
            highlight(language, text)
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin::prelude::{Attrs, Node};

    #[test]
    fn colors_rust_code_without_document_marks() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugin(gpdoc_plugin_paragraph::paragraph())
            .plugin(code_block_lowlight())
            .build()
            .unwrap();
        let source = "fn main() { /* 한글 */ }";
        let content = Node::text("paragraph", source).content().to_owned();
        let editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::new(
                "codeBlock",
                Attrs::from([("language".into(), Value::String("rust".into()))]),
                content,
            )],
        )
        .unwrap();
        let block = &editor.document().blocks()[0];
        let tokens = config.presentation().syntax_tokens(block, source);
        assert!(tokens.windows(2).any(|pair| pair[0].color != pair[1].color));
        assert_eq!(tokens.last().unwrap().bytes.end, source.len());
        assert!(block.child_count() == 1 && block.child(0).marks().is_empty());
    }
}
