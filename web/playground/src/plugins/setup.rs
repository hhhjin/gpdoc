use super::*;
use gpdoc::Node;
use gpdoc_plugin_bullet_list::bullet_list;
use gpdoc_plugin_heading::heading;
use gpdoc_plugin_list_item::list_item;
use gpdoc_plugin_task_list::{task_item, task_list};

pub(super) const CHARACTER_LIMIT: usize = 24;

pub(super) fn open(slug: &str, source: &str) -> Result<Editor> {
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
    let plugins = match slug {
        "overview" => vec![
            gpdoc_plugin_bold::bold().into(),
            gpdoc_plugin_italic::italic().into(),
            gpdoc_plugin_underline::underline().into(),
            heading().into(),
            gpdoc_plugin_blockquote::blockquote().into(),
            list_item().into(),
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
        "link" => vec![gpdoc_plugin_link::link(Default::default()).into()],
        "ruby-text" => vec![gpdoc_plugin_ruby_text::ruby_text()],
        "highlight" => vec![gpdoc_plugin_highlight::multicolor_highlight().into()],
        "text-align" => vec![gpdoc_plugin_text_align::text_align(["paragraph"]).into()],
        "text-style" => gpdoc_plugin_text_style::text_style_kit(),
        "paragraph" | "heading" => vec![heading().into()],
        "blockquote" => vec![gpdoc_plugin_blockquote::blockquote().into()],
        "bullet-list" | "list-item" => vec![list_item().into(), bullet_list().into()],
        "ordered-list" => vec![
            list_item().into(),
            gpdoc_plugin_ordered_list::ordered_list().into(),
        ],
        "list-keymap" => vec![
            list_item().into(),
            bullet_list().into(),
            gpdoc_plugin_list_keymap::list_keymap().into(),
        ],
        "lists" => vec![
            bullet_list().into(),
            list_item().into(),
            gpdoc_plugin_list_keymap::list_keymap().into(),
            gpdoc_plugin_ordered_list::ordered_list().into(),
            task_item().into(),
            task_list().into(),
        ],
        "task-list" => vec![task_item().into(), task_list().into()],
        "hard-break" => vec![gpdoc_plugin_hard_break::hard_break().into()],
        "horizontal-rule" => vec![gpdoc_plugin_horizontal_rule::horizontal_rule().into()],
        "code-block" => vec![gpdoc_plugin_code_block::code_block().into()],
        "details" => vec![gpdoc_plugin_details::details()],
        "table" => vec![gpdoc_plugin_table::table().into()],
        "image" => vec![gpdoc_plugin_image::image().into()],
        "audio" => vec![gpdoc_plugin_audio::audio(Default::default()).into()],
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
        "find-and-replace" => vec![gpdoc_plugin_find_and_replace::find_and_replace()],
        "markdown" => vec![
            heading().into(),
            gpdoc_plugin_bold::bold().into(),
            gpdoc_plugin_italic::italic().into(),
            gpdoc_plugin_markdown::markdown(),
        ],
        "table-of-contents" => vec![
            heading().into(),
            gpdoc_plugin_table_of_contents::table_of_contents(),
        ],
        "trailing-node" => vec![
            heading().into(),
            gpdoc_plugin_trailing_node::trailing_node().into(),
        ],
        "typography" => vec![gpdoc_plugin_typography::typography(Default::default()).into()],
        "unique-id" => vec![gpdoc_plugin_unique_id::unique_id(["paragraph"])],
        "suggestion" | "collaboration" | "collaboration-caret" => vec![],
        _ => return Err(format!("Unknown plugin example: {slug}").into()),
    };
    let paragraph = gpdoc_plugin_paragraph::paragraph();
    let config = gpdoc::EditorConfig::builder()
        .plugin(paragraph)
        .plugins(plugins)
        .build()?;
    Ok(gpdoc::Editor::from_document(&config, content)?)
}
