//! Markdown syntax supplied by installed plugins.
use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::prelude::{Attrs, Error, Mark, Node};
use std::collections::BTreeMap;

/// Markdown codecs contributed by a plugin. Schema targets are explicit so one plugin can
/// declare syntax for several marks and block nodes.
#[derive(Default)]
pub struct MarkdownSyntax {
    marks: Vec<(String, String)>,
    fences: Vec<(String, MarkdownFence)>,
}

impl MarkdownSyntax {
    pub fn mark(mut self, kind: impl Into<String>, delimiter: impl Into<String>) -> Self {
        self.marks.push((kind.into(), delimiter.into()));
        self
    }

    pub fn fence(
        mut self,
        kind: impl Into<String>,
        info: impl Into<String>,
        parse: impl Fn(&str) -> Result<Node, Error> + Send + Sync + 'static,
        render: impl Fn(&Node) -> String + Send + Sync + 'static,
    ) -> Self {
        self.fences.push((
            kind.into(),
            MarkdownFence {
                info: info.into(),
                parse: Box::new(parse),
                render: Box::new(render),
            },
        ));
        self
    }
}

type ParseFence = dyn Fn(&str) -> Result<Node, Error> + Send + Sync;

pub(super) struct MarkdownFence {
    info: String,
    pub(super) parse: Box<ParseFence>,
    pub(super) render: Box<dyn Fn(&Node) -> String + Send + Sync>,
}

impl MarkdownFence {
    pub(super) fn info(&self) -> &str {
        &self.info
    }
}

#[derive(Default)]
pub(super) struct Rules<'a> {
    pub mark_by_delimiter: BTreeMap<&'a str, &'a str>,
    pub delimiter_by_mark: BTreeMap<&'a str, (&'a str, Attrs)>,
    pub node_by_info: BTreeMap<&'a str, (&'a str, &'a MarkdownFence)>,
    pub fence_by_node: BTreeMap<&'a str, &'a MarkdownFence>,
}

impl<'a> Rules<'a> {
    pub fn from_config(config: &'a EditorConfig) -> Result<Self, Error> {
        let mut rules = Self::default();
        for (_, syntax) in config.plugin_data::<MarkdownSyntax>() {
            for (name, delimiter) in &syntax.marks {
                let name = name.as_str();
                let delimiter = delimiter.as_str();
                if !is_literal_delimiter(delimiter) {
                    return Err(Error::InvalidSchema(format!(
                        "Markdown delimiter '{delimiter}' on '{name}' must be punctuation parsed as literal inline text"
                    )));
                }
                let default_mark = config
                    .schema()
                    .bind_mark(&Mark::new(name, Attrs::new()))
                    .map_err(|error| {
                        Error::InvalidSchema(format!(
                            "cannot use '{delimiter}' with mark '{name}': {error}"
                        ))
                    })?;
                if rules.mark_by_delimiter.keys().any(|other: &&str| {
                    other.starts_with(delimiter) || delimiter.starts_with(other)
                }) {
                    return Err(Error::InvalidSchema(format!(
                        "ambiguous Markdown delimiter '{delimiter}' on '{name}'"
                    )));
                }
                rules.mark_by_delimiter.insert(delimiter, name);
                if rules
                    .delimiter_by_mark
                    .insert(name, (delimiter, default_mark.attrs))
                    .is_some()
                {
                    return Err(Error::InvalidSchema(format!(
                        "duplicate Markdown mark '{name}'"
                    )));
                }
            }
            for (name, fence) in &syntax.fences {
                let name = name.as_str();
                if !config.schema().node_spec(name).is_some_and(|s| !s.inline)
                    || fence.info.is_empty()
                    || !fence
                        .info
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                {
                    return Err(Error::InvalidSchema(format!(
                        "invalid Markdown fence '{}' on '{name}'",
                        fence.info
                    )));
                }
                if rules
                    .node_by_info
                    .insert(fence.info(), (name, fence))
                    .is_some()
                {
                    return Err(Error::InvalidSchema(format!(
                        "duplicate Markdown fence '{}'",
                        fence.info
                    )));
                }
                if rules.fence_by_node.insert(name, fence).is_some() {
                    return Err(Error::InvalidSchema(format!(
                        "duplicate Markdown fence for '{name}'"
                    )));
                }
            }
        }
        Ok(rules)
    }
}

fn is_literal_delimiter(delimiter: &str) -> bool {
    use pulldown_cmark::{Event, Options, Parser};
    if delimiter.is_empty()
        || delimiter
            .chars()
            .any(|ch| ch.is_alphanumeric() || ch.is_whitespace() || ch.is_control())
    {
        return false;
    }
    // Only claim syntax the CommonMark parser delivers to our inline text hook.
    let probe = format!("a {delimiter}word{delimiter} z");
    Parser::new_ext(&probe, Options::all())
        .any(|event| matches!(event, Event::Text(raw) if raw.as_ref() == probe))
}
