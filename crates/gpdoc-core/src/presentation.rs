use crate::{Attrs, Error, Mark, Node, Schema};
use std::collections::BTreeMap;

/// Vertical position of a text run relative to the surrounding text.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Baseline {
    #[default]
    Normal,
    Subscript,
    Superscript,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextAlignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// An absolute length in CSS pixels or a multiple of the parent or root font size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextLength {
    Pixels(f32),
    Em(f32),
    Rem(f32),
}

/// Content painted in place of an atomic node's selectable text label.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AtomImageSource {
    Resource(std::sync::Arc<str>),
    Formula {
        latex: std::sync::Arc<str>,
        display: bool,
    },
}

/// Image source and optional display dimensions in CSS pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomImage {
    pub source: AtomImageSource,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Text properties contributed by enclosing nodes and inline marks. Colors are sRGB with alpha.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextFormat {
    pub bold: bool,
    pub baseline: Baseline,
    pub underline: bool,
    pub strike: bool,
    /// Draw a separator across the row for a block leaf.
    pub horizontal_rule: bool,
    /// Reading drawn above the marked base text.
    pub ruby_text: Option<std::sync::Arc<str>>,
    pub atom_image: Option<AtomImage>,
    /// URL activated by a user double-click on a media atom.
    pub media_source: Option<std::sync::Arc<str>>,
    /// Destination of an interactive text link.
    pub link_href: Option<std::sync::Arc<str>>,
    /// Native display text for an atomic node. The editor selects the label as a single object.
    pub atom_label: Option<std::sync::Arc<str>>,
    pub italic: bool,
    pub background: Option<[u8; 4]>,
    pub color: Option<[u8; 4]>,
    pub syntax_kind: Option<SyntaxKind>,
    /// Fixed background palette for this text. The view chooses when unset.
    pub syntax_palette: Option<SyntaxPalette>,
    pub font_family: Option<std::sync::Arc<str>>,
    pub font_fallbacks: Option<std::sync::Arc<Vec<String>>>,
    pub alignment: TextAlignment,
    pub font_size: Option<TextLength>,
    pub line_height: Option<TextLength>,
}

/// A theme-independent classification of source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SyntaxKind {
    Plain,
    Comment,
    Keyword,
    String,
    Constant,
    Function,
    Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxPalette {
    Light,
    Dark,
}

/// A syntax token in a UTF-8 text run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxToken {
    pub bytes: std::ops::Range<usize>,
    pub kind: SyntaxKind,
}

pub type Highlighter = std::sync::Arc<dyn Fn(&Attrs, &str) -> Vec<SyntaxToken> + Send + Sync>;

pub type Formatter = std::sync::Arc<dyn Fn(&Attrs, &mut TextFormat) + Send + Sync>;
type Formatters = BTreeMap<String, Vec<Formatter>>;

/// Formatting and syntax highlighting shared by hosts displaying this config.
#[derive(Default)]
pub struct Presentation {
    pub highlighters: BTreeMap<String, Highlighter>,
    pub node_styles: Formatters,
    pub mark_styles: Formatters,
}

impl Presentation {
    pub(crate) fn validate(&self, schema: &Schema) -> Result<(), Error> {
        for name in self.node_styles.keys().chain(self.highlighters.keys()) {
            if schema.node_type(name).is_none() {
                return Err(Error::InvalidSchema(format!(
                    "style references unknown node '{name}'"
                )));
            }
        }
        for name in self.mark_styles.keys() {
            if schema.mark_type(name).is_none() {
                return Err(Error::InvalidSchema(format!(
                    "style references unknown mark '{name}'"
                )));
            }
        }
        Ok(())
    }

    pub fn style_node(&self, node: &Node, format: &mut TextFormat) {
        for style in self.node_styles.get(&node.kind).into_iter().flatten() {
            style(&node.attrs, format);
        }
    }
    /// Get ordered, disjoint syntax tokens for the given block and text.
    pub fn syntax_tokens(&self, block: &Node, text: &str) -> Vec<SyntaxToken> {
        let Some(highlight) = self.highlighters.get(&block.kind) else {
            return Vec::new();
        };
        let mut end = 0;
        highlight(&block.attrs, text)
            .into_iter()
            .filter(|token| {
                let valid = token.bytes.start >= end
                    && !token.bytes.is_empty()
                    && text.get(token.bytes.clone()).is_some();
                if valid {
                    end = token.bytes.end;
                }
                valid
            })
            .collect()
    }

    pub fn style_mark(&self, mark: &Mark, format: &mut TextFormat) {
        for style in self.mark_styles.get(&mark.kind).into_iter().flatten() {
            style(&mark.attrs, format);
        }
    }
}
