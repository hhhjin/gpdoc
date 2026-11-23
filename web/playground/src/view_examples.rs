use crate::demo::block_views;
use gpdoc::Node;
use gpdoc::PluginViewExt;
use gpdoc::{DocumentSource, EditorConfig, EditorConfigBuilder};
use gpui::{div, prelude::*, px, rgb, rgba};
use wasm_bindgen::prelude::*;

fn paragraphs(lines: &[&str]) -> Vec<Node> {
    lines
        .iter()
        .map(|text| Node::text("paragraph", *text))
        .collect()
}

fn menu(label: &'static str) -> impl IntoElement {
    div()
        .px(px(8.))
        .rounded_sm()
        .bg(rgb(0x283143))
        .text_color(rgb(0xffffff))
        .child(label)
}

struct ExampleSurface(gpui::Entity<gpdoc::EditorView>);

impl Render for ExampleSurface {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        gpdoc::EditorSurface::new(&self.0)
            .font_family(gpdoc_demo_fonts::FAMILY)
            .bg(rgb(0xfdfcf9))
            .text_color(rgb(0x282b2e))
            .text_size(px(19.))
            .line_height(px(32.))
            .layout(|document| div().px(px(40.)).py(px(32.)).child(document))
    }
}

struct Example {
    config: EditorConfigBuilder,
    blocks: Vec<Node>,
}

fn example(name: &str) -> Result<Example, JsError> {
    let mut config = EditorConfig::builder().plugin(gpdoc_plugin_paragraph::paragraph());
    let blocks = match name {
        "bubble-menu" => {
            config = config.plugin(
                gpdoc_plugin_bubble_menu::bubble_menu().with_view(|_, _, _| menu("Selected text")),
            );
            paragraphs(&["Edit the headline before publishing."])
        }
        "floating-menu" => {
            config = config.plugin(
                gpdoc_plugin_floating_menu::floating_menu()
                    .with_view(|_, _, _| menu("Insert a block")),
            );
            paragraphs(&["Meeting notes"])
        }
        "box-selection" => {
            config = config.plugin(gpdoc_plugin_box_selection::box_selection());
            paragraphs(&["Draft the outline", "Review the draft", "Publish the notes"])
        }
        "drag-handle" | "drop-cursor" => {
            config = config
                .plugin(gpdoc_plugin_drag_handle::drag_handle().with_view(|_, _, _| "≡"))
                .plugin(gpdoc_plugin_drop_cursor::drop_cursor());
            config =
                config.plugin(gpdoc_plugin_blockquote::blockquote().with_view(block_views::quote));
            vec![
                Node::text("paragraph", "Draft the outline"),
                Node::container(
                    "blockquote",
                    paragraphs(&["Review the draft", "Collect feedback"]),
                ),
                Node::text("paragraph", "Publish the notes"),
            ]
        }
        "file-handler" => {
            use gpdoc_plugin_file_handler::{FileInput, file_handler};
            config = config.plugin(file_handler(|_, input, _, _| {
                let FileInput::Paste(images) = input else {
                    return false;
                };
                let document = web_sys::window().unwrap().document().unwrap();
                if let Some(output) = document.get_element_by_id("paste-result") {
                    output.set_text_content(Some(&format!("Images pasted: {}", images.len())));
                }
                true
            }));
            paragraphs(&["Paste an image here."])
        }
        "focus" => {
            config =
                config.plugin(gpdoc_plugin_blockquote::blockquote().with_view(block_views::quote));
            config = config.plugin(
                gpdoc_plugin_focus::focus(gpdoc_plugin_focus::FocusMode::Deepest).with_view(
                    |bounds, window, _| window.paint_quad(gpui::fill(bounds, rgba(0x659ff344))),
                ),
            );
            vec![Node::container(
                "blockquote",
                paragraphs(&["First quoted paragraph", "Second quoted paragraph"]),
            )]
        }
        "gap-cursor" => {
            config = config.plugin(gpdoc_plugin_horizontal_rule::horizontal_rule());
            config = config.plugin(gpdoc_plugin_gap_cursor::gap_cursor());
            let rule = Node::leaf("horizontalRule", Default::default());
            vec![rule.clone(), rule]
        }
        "invisible-characters" => {
            use gpdoc_plugin_invisible_characters::invisible_characters;
            config = config.plugin(invisible_characters(true));
            paragraphs(&["Two words", "Another paragraph"])
        }
        "placeholder" => {
            use gpdoc_plugin_placeholder::placeholder;
            config = config.plugin(placeholder("Write an update…"));
            paragraphs(&[""])
        }
        "slash-command" => {
            use gpdoc_plugin_heading::{Level, Set, heading};
            use gpdoc_plugin_slash_command::{SlashCommand, slash_commands};
            let slash = slash_commands([SlashCommand {
                id: "heading.h1",
                label: "Heading 1",
                aliases: &["h1"],
                command: gpdoc::StoredCommand::new(Set(Level::H1)),
            }]);
            config = config
                .plugin(heading().with_view(block_views::heading))
                .plugin(slash.with_view(crate::demo::completion_views::slash_menu));
            paragraphs(&[""])
        }
        _ => return Err(JsError::new("Unknown view plugin example")),
    };
    Ok(Example { config, blocks })
}

#[wasm_bindgen]
pub fn start_view_example(name: &str) -> Result<(), JsError> {
    let Example { config, blocks } = example(name)?;
    let config = config.build()?;
    super::application()
        .with_assets(crate::demo::DemoAssets)
        .run(move |cx| {
            crate::demo::load_fonts(cx);
            let editor = config
                .open(DocumentSource::Blocks(blocks), cx)
                .expect("open example document");
            cx.open_window(Default::default(), |_, cx| {
                cx.new(|_| ExampleSurface(editor))
            })
            .expect("open view example");
        });
    Ok(())
}
