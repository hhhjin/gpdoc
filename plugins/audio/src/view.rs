//! GPUI media presentation with editor-aware URI handling.
use gpdoc::{NodeView, ViewPlugin};
use gpdoc_plugin::core::{Attrs, TextFormat, Value};
use gpui::{App, Div, IntoElement, MouseButton, Stateful, Window, div, prelude::*};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;

fn format(attrs: &Attrs, style: &mut TextFormat) {
    if let Some(src) = attrs.get("src").and_then(Value::as_str) {
        style.media_source = Some(src.into());
    }
}

/// Show the audio label and enable native media activation on the projected atom.
pub fn audio_content() -> ViewPlugin {
    ViewPlugin::named("audio")
        .for_plugin("audio")
        .node_format("audio", format)
}

pub struct AudioNode<'a> {
    pub slot: NodeView<'a>,
}

impl AudioNode<'_> {
    pub fn source(&self) -> Option<&str> {
        self.slot.node.attrs.get("src")?.as_str()
    }

    /// A click target that opens HTTP(S) audio using the editor's media base URL.
    /// Apps can use the record's source directly when supplying an in-app player.
    pub fn play_button(&self, cx: &App) -> Stateful<Div> {
        let editor = self.slot.view.clone();
        let before = self.slot.before;
        let revision = editor.read(cx).editor().revision();
        let url = self
            .source()
            .and_then(|src| editor.read(cx).resolve_media_url(src));
        div()
            .id(("audio-play", before.0))
            .when_some(url, |button, url| {
                button
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        editor.update(cx, |view, cx| {
                            if view.editor().revision() == revision && !view.is_composing() {
                                view.focus_in(window, cx);
                                cx.open_url(url.as_str());
                            }
                        });
                        cx.stop_propagation();
                    })
            })
    }
}

/// Compose controls inside the space reserved by the native audio label.
pub fn audio_controls<E: IntoElement>(
    render: impl Fn(AudioNode<'_>, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    ViewPlugin::node_view("audio", move |slot, window, cx| {
        render(AudioNode { slot }, window, cx)
    })
    .for_plugin("audio")
    .node_format("audio", format)
}
