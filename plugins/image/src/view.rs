//! Optional GPUI presentation of inline and block images.
use gpdoc::{NodeView, ViewPlugin};
use gpdoc_plugin::core::{AtomImage, AtomImageSource, Attrs, TextFormat, Transaction, Value};
use gpui::{App, IntoElement, Window};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;

fn dimension(attrs: &Attrs, key: &str) -> Option<u32> {
    let value = attrs.get(key)?;
    match value {
        Value::String(s) => s.parse().ok(),
        other => other.as_u64().and_then(|n| u32::try_from(n).ok()),
    }
    .filter(|n| *n > 0)
}

fn format(attrs: &Attrs, style: &mut TextFormat) {
    if let Some(src) = attrs
        .get("src")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        style.atom_image = Some(AtomImage {
            source: AtomImageSource::Resource(src.into()),
            width: dimension(attrs, "width"),
            height: dimension(attrs, "height"),
        });
    }
}

/// Draw the resource in the document flow with the stored dimensions and alt-text fallback.
pub fn image_content() -> ViewPlugin {
    ViewPlugin::named("image")
        .for_plugin("image")
        .node_format("image", format)
}

/// The image and its revision-checked document operations.
pub struct ImageNode<'a> {
    pub slot: NodeView<'a>,
}

impl ImageNode<'_> {
    pub fn source(&self) -> Option<&str> {
        self.slot.node.attrs.get("src")?.as_str()
    }
    pub fn alt(&self) -> Option<&str> {
        self.slot.node.attrs.get("alt")?.as_str()
    }
    pub fn dimensions(&self) -> (Option<u32>, Option<u32>) {
        let attrs = &self.slot.node.attrs;
        (dimension(attrs, "width"), dimension(attrs, "height"))
    }

    /// Prepare one undoable resize. A missing dimension uses the resource's aspect ratio.
    pub fn resize(
        &self,
        width: Option<std::num::NonZeroU32>,
        height: Option<std::num::NonZeroU32>,
    ) -> Transaction {
        let mut attrs = self.slot.node.attrs.clone();
        for (key, value) in [("width", width), ("height", height)] {
            attrs.insert(
                key.into(),
                value.map_or(Value::Null, |n| Value::from(n.get())),
            );
        }
        self.slot.set_attrs(attrs)
    }
}

/// Compose controls over the projected picture. The slot is clipped to the measured image bounds.
pub fn image_controls<E: IntoElement>(
    render: impl Fn(ImageNode<'_>, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    ViewPlugin::node_view("image", move |slot, window, cx| {
        render(ImageNode { slot }, window, cx)
    })
    .for_plugin("image")
    .node_format("image", format)
}
