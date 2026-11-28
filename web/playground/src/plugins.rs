mod actions;
mod behavior;
mod icons;
mod peers;
mod setup;
mod toolbar;

use crate::view_examples::ExampleSurface;
use gpdoc::{CommandOutcome, DocPos, DocumentSource, Edit, Editor, EditorView, Selection, Value};
use gpui::{AppContext, Context, Entity, Window, div, prelude::*, px};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Facts = Vec<(String, String)>;

struct ActionContext<'a, 'b> {
    view: &'a mut EditorView,
    window: &'a mut Window,
    cx: &'a mut Context<'b, EditorView>,
    facts: &'a mut Facts,
}

#[derive(Deserialize)]
struct Example {
    source: String,
    selection: Option<[usize; 2]>,
    actions: Vec<toolbar::Action>,
}

struct PluginSurface {
    slug: String,
    view: Entity<EditorView>,
    editor: Entity<ExampleSurface>,
    toolbar: toolbar::Toolbar,
    facts: Facts,
    error: Option<String>,
    peer: Option<Entity<ExampleSurface>>,
}

impl Render for PluginSurface {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let column = |label, surface| {
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(div().px(px(12.)).child(label))
                .child(div().flex_1().min_h_0().child(surface))
        };
        let document = if let Some(peer) = &self.peer {
            div()
                .size_full()
                .flex()
                .bg(gpui::white())
                .child(column("Alice", self.editor.clone()))
                .child(column("Bob", peer.clone()))
        } else {
            div().size_full().child(self.editor.clone())
        };
        let facts = behavior::facts(&self.slug, self.view.read(cx).editor());
        div()
            .size_full()
            .flex()
            .flex_col()
            .font_family(gpdoc_demo_fonts::FAMILY)
            .bg(gpui::white())
            .text_color(gpui::rgb(0x282b2e))
            .text_size(px(14.))
            .when(!self.toolbar.actions.is_empty(), |root| {
                root.child(self.render_toolbar(cx))
            })
            .child(div().flex_1().min_h_0().child(document))
            .when_some(self.error.clone(), |root, error| {
                root.child(
                    div()
                        .px(px(10.))
                        .text_color(gpui::rgb(0xaa241d))
                        .child(error),
                )
            })
            .when(!facts.is_empty() || !self.facts.is_empty(), |root| {
                root.child(
                    div()
                        .id("facts")
                        .max_h(px(96.))
                        .overflow_y_scroll()
                        .px(px(10.))
                        .children(
                            facts
                                .iter()
                                .chain(&self.facts)
                                .map(|(key, value)| div().child(format!("{key}: {value}"))),
                        ),
                )
            })
    }
}

#[wasm_bindgen]
pub fn start_plugin_example(
    slug: String,
    example: &str,
    base_url: &str,
) -> std::result::Result<(), JsError> {
    let Example {
        source,
        selection,
        actions,
    } = serde_json::from_str(example)?;
    let (builder, blocks) =
        setup::example(&slug, &source).map_err(|err| JsError::new(&err.to_string()))?;
    let carets = peers::Awareness::default();
    let config = carets.configure(builder, &slug).build()?;
    let base_url = base_url.parse()?;
    super::application()
        .with_assets(crate::demo::DemoAssets)
        .run(move |cx| {
            crate::demo::load_fonts(cx);
            let editor = config
                .open(DocumentSource::Blocks(blocks.clone()), cx)
                .expect("open example");
            let peer = slug.starts_with("collaboration").then(|| {
                config
                    .open(DocumentSource::Blocks(blocks), cx)
                    .expect("open peer")
            });
            cx.open_window(Default::default(), move |window, cx| {
                if let Some(peer) = &peer {
                    peers::connect(&editor, peer, carets, &slug, window, cx)
                        .expect("bind collaboration");
                }
                editor.update(cx, |view, cx| {
                    view.set_media_base_url(Some(base_url));
                    if let Some([from, to]) = selection {
                        view.select_document(
                            Selection::Text {
                                anchor: DocPos(from),
                                head: DocPos(to),
                            },
                            window,
                            cx,
                        )
                        .expect("valid example selection");
                    }
                });
                let toolbar = toolbar::Toolbar::new(actions, cx);
                let surface = cx.new(|_| ExampleSurface(editor.clone()));
                let peer = peer.map(|peer| cx.new(|_| ExampleSurface(peer)));
                cx.new(|cx| {
                    cx.observe(&editor, |_, _, cx| cx.notify()).detach();
                    PluginSurface {
                        slug,
                        view: editor,
                        editor: surface,
                        toolbar,
                        peer,
                        facts: Vec::new(),
                        error: None,
                    }
                })
            })
            .expect("open plugin example window");
        });
    Ok(())
}
