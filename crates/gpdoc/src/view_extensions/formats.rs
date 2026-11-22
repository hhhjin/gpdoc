//! Bind application formatting to document types after the schema is compiled.
use super::ViewPlugin;
use gpdoc_core::{Attrs, Formatter, Presentation, Schema, TextFormat};
use std::sync::Arc;

pub(super) enum Format {
    Node(String, Formatter),
    Mark(String, Formatter),
    Optional(String, Formatter),
}

impl Format {
    pub(super) fn install(&self, schema: &Schema, presentation: &mut Presentation) {
        let (name, format, styles) = match self {
            Self::Node(name, format) => (name, format, &mut presentation.node_styles),
            Self::Mark(name, format) => (name, format, &mut presentation.mark_styles),
            Self::Optional(name, format) if schema.node_type(name).is_some() => {
                (name, format, &mut presentation.node_styles)
            }
            Self::Optional(name, format) if schema.mark_type(name).is_some() => {
                (name, format, &mut presentation.mark_styles)
            }
            Self::Optional(_, _) => return,
        };
        styles.entry(name.clone()).or_default().push(format.clone());
    }
}

impl ViewPlugin {
    /// Set text or atom properties for a displayed node.
    pub fn node_format(
        mut self,
        kind: impl Into<String>,
        format: impl Fn(&Attrs, &mut TextFormat) + Send + Sync + 'static,
    ) -> Self {
        self.formats
            .push(Format::Node(kind.into(), Arc::new(format)));
        self
    }

    /// Style marked text in place, preserving its shaping and editable positions.
    pub fn mark_format(
        mut self,
        kind: impl Into<String>,
        format: impl Fn(&Attrs, &mut TextFormat) + Send + Sync + 'static,
    ) -> Self {
        self.formats
            .push(Format::Mark(kind.into(), Arc::new(format)));
        self
    }

    /// Apply a shared attribute formatter when its node or mark is installed.
    pub fn format_if_installed(
        mut self,
        kind: impl Into<String>,
        format: impl Fn(&Attrs, &mut TextFormat) + Send + Sync + 'static,
    ) -> Self {
        self.formats
            .push(Format::Optional(kind.into(), Arc::new(format)));
        self
    }
}
