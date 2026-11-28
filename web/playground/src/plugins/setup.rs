use super::*;
use crate::demo::block_views;
use gpdoc::Node;
use gpdoc::PluginViewExt;
use gpdoc_plugin_bullet_list::bullet_list;
use gpdoc_plugin_heading::heading;
use gpdoc_plugin_list_item::list_item;
use gpdoc_plugin_task_list::{task_item, task_list};

pub(super) const CHARACTER_LIMIT: usize = 24;

pub(super) fn example(slug: &str, source: &str) -> Result<(gpdoc::EditorConfigBuilder, Vec<Node>)> {
    let content = match slug {
        "paragraph" => vec![Node::text("heading", source)],
        "table-of-contents" => vec![
            Node::text("heading", source),
            Node::text(
                "paragraph",
                "An outline follows the first and second sections.",
            ),
            Node::text("heading", "Changes"),
        ],
        "list-item" | "list-keymap" => vec![Node::container(
            "bulletList",
            vec![Node::container(
                "listItem",
                vec![Node::text("paragraph", source)],
            )],
        )],
        _ => vec![Node::text("paragraph", source)],
    };
    let plugins: Vec<gpdoc::EditorPlugin> = match slug {
        "overview" => vec![
            gpdoc_plugin_bold::bold().into(),
            gpdoc_plugin_italic::italic().into(),
            gpdoc_plugin_underline::underline().into(),
            heading().with_view(block_views::heading),
            gpdoc_plugin_blockquote::blockquote().with_view(block_views::quote),
            list_item().with_view(block_views::list_item),
            bullet_list().into(),
            gpdoc_plugin_ordered_list::ordered_list().into(),
        ],
        "bold" => vec![gpdoc_plugin_bold::bold().into()],
        "italic" => vec![gpdoc_plugin_italic::italic().into()],
        "underline" => vec![gpdoc_plugin_underline::underline().into()],
        "strike" => vec![gpdoc_plugin_strike::strike().into()],
        "inline-code" => vec![gpdoc_plugin_inline_code::inline_code().into()],
        "superscript" => vec![gpdoc_plugin_superscript::superscript().into()],
        "subscript" => vec![gpdoc_plugin_subscript::subscript().into()],
        "link" => vec![gpdoc_plugin_link::link(Default::default()).with_view(|| {
            [
                gpdoc_plugin_link::link_format(),
                gpdoc_plugin_link::link_activation(),
            ]
        })],
        "ruby-text" => vec![gpdoc_plugin_ruby_text::ruby_text().into()],
        "highlight" => vec![gpdoc_plugin_highlight::multicolor_highlight().into()],
        "text-align" => vec![gpdoc_plugin_text_align::text_align(["paragraph"]).into()],
        "text-style" => gpdoc_plugin_text_style::text_style_kit()
            .into_iter()
            .chain([actions::style_commands()])
            .map(Into::into)
            .collect(),
        "paragraph" | "heading" => vec![heading().with_view(block_views::heading)],
        "blockquote" => vec![gpdoc_plugin_blockquote::blockquote().with_view(block_views::quote)],
        "bullet-list" | "list-item" => vec![
            list_item().with_view(block_views::list_item),
            bullet_list().into(),
        ],
        "ordered-list" => vec![
            list_item().with_view(block_views::list_item),
            gpdoc_plugin_ordered_list::ordered_list().into(),
        ],
        "list-keymap" => vec![
            list_item().with_view(block_views::list_item),
            bullet_list().into(),
            gpdoc_plugin_list_keymap::list_keymap().into(),
        ],
        "lists" => vec![
            bullet_list().into(),
            list_item().with_view(block_views::list_item),
            gpdoc_plugin_list_keymap::list_keymap().into(),
            gpdoc_plugin_ordered_list::ordered_list().into(),
            task_item().with_view(block_views::tasks),
            task_list().into(),
        ],
        "task-list" => vec![
            task_item().with_view(block_views::tasks),
            task_list().into(),
        ],
        "hard-break" => vec![gpdoc_plugin_hard_break::hard_break().into()],
        "horizontal-rule" => vec![gpdoc_plugin_horizontal_rule::horizontal_rule().into()],
        "code-block" => vec![gpdoc_plugin_code_block::code_block().with_view(block_views::code)],
        "details" => vec![gpdoc_plugin_details::details().into()],
        "table" => vec![gpdoc_plugin_table::table().with_view(|| {
            gpdoc_plugin_table::table_grid(
                block_views::table_cell,
                gpdoc_plugin_table::default_resize_guide,
            )
        })],
        "image" => vec![gpdoc_plugin_image::image().into()],
        "audio" => {
            vec![gpdoc_plugin_audio::audio(Default::default()).with_view(audio_player)]
        }
        "mention" => vec![gpdoc_plugin_mention::mention().into()],
        "mathematics" => vec![gpdoc_plugin_mathematics::mathematics().into()],
        "character-count" => vec![
            gpdoc_plugin_character_count::character_count(
                gpdoc_plugin_character_count::CountOptions {
                    limit: Some(CHARACTER_LIMIT),
                    ..Default::default()
                },
            )
            .into(),
        ],
        "find-and-replace" => vec![gpdoc_plugin_find_and_replace::find_and_replace().into()],
        "markdown" => vec![
            heading().with_view(block_views::heading),
            gpdoc_plugin_bold::bold().into(),
            gpdoc_plugin_italic::italic().into(),
            gpdoc_plugin_markdown::markdown().into(),
        ],
        "table-of-contents" => vec![
            heading().with_view(block_views::heading),
            gpdoc_plugin_table_of_contents::table_of_contents().into(),
        ],
        "trailing-node" => vec![
            heading().with_view(block_views::heading),
            gpdoc_plugin_trailing_node::trailing_node().into(),
        ],
        "typography" => vec![gpdoc_plugin_typography::typography(Default::default()).into()],
        "unique-id" => vec![gpdoc_plugin_unique_id::unique_id(["paragraph"]).into()],
        "suggestion" | "collaboration" | "collaboration-caret" => vec![],
        _ => return Err(format!("Unknown plugin example: {slug}").into()),
    };
    let paragraph = gpdoc_plugin_paragraph::paragraph();
    let config = gpdoc::EditorConfig::builder()
        .plugin(paragraph)
        .plugins(plugins);
    Ok((config, content))
}

fn audio_player() -> gpdoc::ViewPlugin {
    struct Player(web_sys::HtmlAudioElement);
    impl Drop for Player {
        fn drop(&mut self) {
            let _ = self.0.pause();
        }
    }
    gpdoc_plugin_audio::audio_controls(|clip, window, cx| {
        let url = clip
            .source()
            .and_then(|src| clip.slot.view.read(cx).resolve_media_url(src));
        let repeat = clip
            .slot
            .node
            .attrs
            .get("loop")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        div()
            .size_full()
            .bg(gpui::rgb(0xf1f2f3))
            .child("Play / Pause")
            .when_some(url, |button, url| {
                let player = window.use_keyed_state(("clip", clip.slot.before.0), cx, |_, _| {
                    Player(web_sys::HtmlAudioElement::new().expect("create audio player"))
                });
                let audio = &player.read(cx).0;
                if audio.src() != url.as_str() {
                    audio.set_src(url.as_str());
                }
                audio.set_loop(repeat);
                button
                    .cursor_pointer()
                    .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                        let audio = &player.read(cx).0;
                        if audio.paused() {
                            let _ = audio.play();
                        } else {
                            let _ = audio.pause();
                        }
                        cx.stop_propagation();
                    })
            })
    })
}
