use gpdoc_plugin::core::{SyntaxKind, SyntaxToken};
use std::sync::LazyLock;
use syntect::{
    easy::ScopeRangeIterator,
    parsing::{ParseState, Scope, ScopeStack, SyntaxSet},
    util::LinesWithEndings,
};

pub(super) fn highlight(language: Option<&str>, text: &str) -> Vec<SyntaxToken> {
    static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
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
    let mut parser = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut offset = 0;
    let mut tokens: Vec<SyntaxToken> = Vec::new();
    for line in LinesWithEndings::from(text) {
        let Ok(ops) = parser.parse_line(line, &SYNTAXES) else {
            return Vec::new();
        };
        for (range, op) in ScopeRangeIterator::new(&ops, line) {
            if stack.apply(op).is_err() {
                return Vec::new();
            }
            if range.is_empty() {
                continue;
            }
            let kind = classify(stack.as_slice());
            let end = offset + range.end;
            if let Some(last) = tokens.last_mut()
                && last.kind == kind
            {
                last.bytes.end = end;
            } else {
                tokens.push(SyntaxToken {
                    bytes: offset + range.start..end,
                    kind,
                });
            }
        }
        offset += line.len();
    }
    tokens
}

fn classify(scopes: &[Scope]) -> SyntaxKind {
    use SyntaxKind::*;
    static PREFIXES: LazyLock<Vec<(Scope, SyntaxKind)>> = LazyLock::new(|| {
        [
            ("comment", Comment),
            ("string", String),
            ("keyword", Keyword),
            ("storage", Keyword),
            ("constant", Constant),
            ("entity.name.function", Function),
            ("support.function", Function),
            ("variable.function", Function),
            ("entity.name.type", Type),
            ("support.type", Type),
            ("support.class", Type),
        ]
        .into_iter()
        .map(|(name, kind)| (Scope::new(name).expect("valid syntax scope"), kind))
        .collect()
    });
    scopes
        .iter()
        .rev()
        .find_map(|scope| {
            PREFIXES
                .iter()
                .find(|(prefix, _)| prefix.is_prefix_of(*scope))
                .map(|(_, kind)| *kind)
        })
        .unwrap_or(Plain)
}
