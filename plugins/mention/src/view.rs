//! Display an inserted mention independently from its input picker.
use super::{label_text, trigger_text};
use gpdoc::{NodeView, ViewPlugin};
use gpui::{App, IntoElement, Window};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;

pub struct MentionChip<'a> {
    pub slot: NodeView<'a>,
}

impl MentionChip<'_> {
    pub fn id(&self) -> Option<&str> {
        self.slot.node.attrs.get("id")?.as_str()
    }
    pub fn label(&self) -> &str {
        label_text(&self.slot.node.attrs)
    }
    pub fn trigger(&self) -> &str {
        trigger_text(&self.slot.node.attrs)
    }
}

/// Render interactive chip content inside the measured bounds of the mention label.
pub fn mention_chip<E: IntoElement>(
    render: impl Fn(MentionChip<'_>, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    ViewPlugin::node_view("mention", move |slot, window, cx| {
        render(MentionChip { slot }, window, cx)
    })
    .for_plugin("mention")
}
