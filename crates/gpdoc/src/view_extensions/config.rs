use super::{ViewExtensions, ViewPlugin, drops};
use crate::{
    Editor, EditorConfig, EditorConfigBuilder, EditorView, JsonError, Node, OpenJsonError,
    buffer::InputBuffer,
    fold::FoldSpecs,
    node_view::{BlockViews, Renderer, supports_node_view},
};
use gpdoc_core::plugin::Plugin;
use gpdoc_core::{EditorConfig as DocumentConfig, Error};
use gpui::{App, AppContext, Entity};
use std::{collections::BTreeMap, rc::Rc};

pub(crate) struct ViewConfig {
    fold_specs: Rc<FoldSpecs>,
    pub(crate) extensions: ViewExtensions,
    pub(crate) node_views: BTreeMap<String, Renderer>,
    pub(crate) block_views: Rc<BlockViews>,
}

/// Initial content for a new GPUI editor.
pub enum DocumentSource<'a> {
    /// Create an initial document using the configured schema.
    Empty,
    /// Read ProseMirror JSON using the configured schema.
    Json(&'a str),
    /// Open a document with these top-level blocks.
    Blocks(Vec<Node>),
}

/// An error preparing a document or its editing state.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Json(#[from] JsonError),
    #[error(transparent)]
    Editor(#[from] Error),
}

impl From<OpenJsonError> for OpenError {
    fn from(error: OpenJsonError) -> Self {
        match error {
            OpenJsonError::Document(error) => Self::Json(error),
            OpenJsonError::Editor(error) => Self::Editor(error),
        }
    }
}

impl EditorConfig {
    /// Open a document and create an editor entity with this config's views.
    pub fn open(
        &self,
        source: DocumentSource<'_>,
        cx: &mut App,
    ) -> Result<Entity<EditorView>, OpenError> {
        let editor = match source {
            DocumentSource::Empty => Editor::from_config(&self.document_config)?,
            DocumentSource::Json(json) => Editor::from_json(&self.document_config, json)?,
            DocumentSource::Blocks(blocks) => Editor::from_document(&self.document_config, blocks)?,
        };
        let buffer = InputBuffer::with_folds(editor, self.views.fold_specs.clone())?;
        Ok(cx.new(|cx| EditorView::from_buffer(buffer, &self.views, cx)))
    }
}

/// A registration can supply document behavior, a native view, or both.
#[must_use]
pub struct EditorPlugin {
    document: Option<Plugin>,
    views: Vec<ViewPlugin>,
}

struct DefaultView(Box<dyn Fn() -> Vec<ViewPlugin> + Send + Sync>);

/// Register application views or declare a plugin's default GPUI composition.
pub trait PluginViewExt {
    /// Set the default for GPUI hosts. Headless config construction never invokes this factory.
    fn gpui_default_view<V: Into<Vec<ViewPlugin>>>(
        self,
        view: impl Fn() -> V + Send + Sync + 'static,
    ) -> Plugin;

    /// Replace the entire default composition with an application view or collection of views.
    fn with_view<V: Into<Vec<ViewPlugin>>>(self, view: impl FnOnce() -> V) -> EditorPlugin;
}

impl PluginViewExt for Plugin {
    fn gpui_default_view<V: Into<Vec<ViewPlugin>>>(
        self,
        view: impl Fn() -> V + Send + Sync + 'static,
    ) -> Plugin {
        self.with_metadata(DefaultView(Box::new(move || view().into())))
    }

    fn with_view<V: Into<Vec<ViewPlugin>>>(self, view: impl FnOnce() -> V) -> EditorPlugin {
        EditorPlugin {
            document: Some(self),
            views: view().into(),
        }
    }
}

impl From<Plugin> for EditorPlugin {
    fn from(plugin: Plugin) -> Self {
        let views = plugin
            .metadata::<DefaultView>()
            .map(|factory| (factory.0)())
            .unwrap_or_default();
        Self {
            document: Some(plugin),
            views,
        }
    }
}

impl From<ViewPlugin> for Vec<ViewPlugin> {
    fn from(view: ViewPlugin) -> Self {
        vec![view]
    }
}

impl From<ViewPlugin> for EditorPlugin {
    fn from(view: ViewPlugin) -> Self {
        Self {
            document: None,
            views: vec![view],
        }
    }
}

impl EditorConfigBuilder {
    pub fn build(self) -> Result<EditorConfig, Error> {
        let mut document = DocumentConfig::builder();
        let mut views = Vec::new();
        for entry in self.entries {
            for view in entry.views {
                if let Some(owner) = view.owner
                    && entry.document.as_ref().map(Plugin::name) != Some(owner)
                {
                    return Err(Error::InvalidSchema(format!(
                        "attach view '{}' to plugin '{owner}' using with_view",
                        view.name
                    )));
                }
                views.push(view);
            }
            if let Some(plugin) = entry.document {
                document = document.plugin(plugin);
            }
        }
        let config = document.build_with_presentation(|schema, presentation| {
            for view in &views {
                for format in &view.formats {
                    format.install(schema, presentation);
                }
            }
        })?;
        let mut extensions = ViewExtensions::default();
        let mut fold_specs = FoldSpecs::new();
        let mut node_views = BTreeMap::new();
        let mut block_views = BlockViews::default();
        let mut drops = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        for view in views {
            if view.name.is_empty() || !names.insert(view.name.clone()) {
                return Err(Error::InvalidSchema(format!(
                    "empty or duplicate view name '{}'",
                    view.name
                )));
            }
            if let Some((node, render)) = view.node_view {
                supports_node_view(config.schema(), &node)?;
                if node_views.insert(node.clone(), render).is_some() {
                    return Err(Error::InvalidSchema(format!(
                        "duplicate node view for '{node}'"
                    )));
                }
            }
            for (node, render) in view.wrappers {
                if let Some(node) = node {
                    if !config
                        .schema()
                        .node_type(&node)
                        .is_some_and(|ty| ty.is_block())
                    {
                        return Err(Error::InvalidSchema(format!(
                            "wrapper requires a block node '{node}'"
                        )));
                    }
                    block_views.wrappers.entry(node).or_default().push(render);
                } else {
                    block_views.top_level_wrappers.push(render);
                }
            }
            if let Some((node, fold)) = view.fold {
                if !config
                    .schema()
                    .node_type(&node)
                    .is_some_and(|ty| ty.is_block() && !ty.is_leaf())
                {
                    return Err(Error::InvalidSchema(format!(
                        "fold requires a non-leaf block node '{node}'"
                    )));
                }
                if fold_specs.insert(node.clone(), fold).is_some() {
                    return Err(Error::InvalidSchema(format!("duplicate fold for '{node}'")));
                }
            }
            for (node, render) in view.child_layouts {
                if !config
                    .schema()
                    .node_type(&node)
                    .is_some_and(|ty| ty.is_block() && !ty.is_leaf() && !ty.inline_content())
                {
                    return Err(Error::InvalidSchema(format!(
                        "child layout requires a container of blocks '{node}'"
                    )));
                }
                if block_views.layouts.insert(node.clone(), render).is_some() {
                    return Err(Error::InvalidSchema(format!(
                        "duplicate child layout for '{node}'"
                    )));
                }
            }
            extensions.behaviors.extend(view.behavior);
            block_views.measure_all |= view.drops.iter().any(drops::Registration::targets_blocks);
            drops.extend(view.drops);
        }
        extensions.surface_handlers = drops::compile(drops);
        Ok(EditorConfig {
            document_config: config,
            views: ViewConfig {
                fold_specs: Rc::new(fold_specs),
                extensions,
                node_views,
                block_views: Rc::new(block_views),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ViewBehavior;

    fn blocks() -> EditorConfigBuilder {
        use gpdoc_core::NodeSpec;
        EditorConfig::builder().plugin(Plugin::new("blocks", |p| {
            p.node(
                NodeSpec::block("paragraph")
                    .group("block")
                    .content("inline*"),
            );
            p.node(NodeSpec::block("group").group("block").content("block+"));
            p.default_text_block("paragraph");
        }))
    }

    #[test]
    fn behavior_dispatch_uses_explicit_view_order() {
        struct Tint(u32);
        impl ViewBehavior for Tint {
            fn selection_color(&self, _: bool) -> Option<gpui::Hsla> {
                Some(gpui::rgb(self.0).into())
            }
        }
        let config = blocks()
            .plugins([
                ViewPlugin::new("first", || Tint(2)),
                ViewPlugin::new("second", || Tint(1)),
            ])
            .build()
            .unwrap();
        let colors: Vec<_> = config
            .views
            .extensions
            .behaviors
            .iter()
            .map(|create| create().selection_color(true).unwrap())
            .collect();
        assert_eq!(colors, [2, 1].map(|v| gpui::Hsla::from(gpui::rgb(v))));
    }

    #[test]
    fn build_validates_render_targets_and_view_names() {
        let invalid = ViewPlugin::named("test").wrap_node("missing", |_, content, _, _| content);
        assert!(blocks().plugin(invalid).build().is_err());
        assert!(
            blocks()
                .plugins([ViewPlugin::named("picker"), ViewPlugin::named("picker")])
                .build()
                .is_err()
        );
    }

    #[test]
    fn child_layouts_own_containers_and_wrappers_compose() {
        let layout =
            |name, kind| ViewPlugin::named(name).child_layout(kind, |_, _, _, _| gpui::div());
        let wrapper = |name| ViewPlugin::named(name).wrap_node("group", |_, body, _, _| body);
        let config = blocks()
            .plugins([
                layout("owner", "group"),
                wrapper("comments"),
                wrapper("drag"),
            ])
            .build()
            .unwrap();
        assert_eq!(config.views.block_views.wrappers["group"].len(), 2);
        assert!(
            blocks()
                .plugins([layout("first", "group"), layout("second", "group")])
                .build()
                .is_err()
        );
        assert!(blocks().plugin(layout("bad", "paragraph")).build().is_err());
    }
}
