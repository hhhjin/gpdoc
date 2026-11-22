#[cfg(feature = "gpui")]
use crate::view_extensions::{EditorPlugin, ViewConfig};
use gpdoc_core::EditorConfig as DocumentConfig;
#[cfg(not(feature = "gpui"))]
use gpdoc_core::{Error, plugin::Plugin as EditorPlugin};

/// Validated document plugins and, when enabled, their GPUI presentation.
pub struct EditorConfig {
    pub(crate) document_config: DocumentConfig,
    #[cfg(feature = "gpui")]
    pub(crate) views: ViewConfig,
}

impl std::ops::Deref for EditorConfig {
    type Target = DocumentConfig;

    fn deref(&self) -> &Self::Target {
        &self.document_config
    }
}

impl EditorConfig {
    /// Register plugins in the order they should be installed.
    pub fn builder() -> EditorConfigBuilder {
        EditorConfigBuilder::default()
    }
}

#[derive(Default)]
#[must_use]
pub struct EditorConfigBuilder {
    pub(crate) entries: Vec<EditorPlugin>,
}

impl EditorConfigBuilder {
    pub fn plugin(self, plugin: impl Into<EditorPlugin>) -> Self {
        self.plugins([plugin])
    }

    pub fn plugins<P: Into<EditorPlugin>>(mut self, plugins: impl IntoIterator<Item = P>) -> Self {
        self.entries.extend(plugins.into_iter().map(Into::into));
        self
    }

    #[cfg(not(feature = "gpui"))]
    pub fn build(self) -> Result<EditorConfig, Error> {
        Ok(EditorConfig {
            document_config: DocumentConfig::builder().plugins(self.entries).build()?,
        })
    }
}
