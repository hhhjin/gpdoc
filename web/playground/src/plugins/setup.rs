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
            gpdoc_plugin_bold::bold(),
            gpdoc_plugin_italic::italic(),
            gpdoc_plugin_underline::underline(),
            heading(),
            gpdoc_plugin_blockquote::blockquote(),
            list_item(),
            bullet_list(),
            gpdoc_plugin_ordered_list::ordered_list(),
        ],
        "bold" => vec![gpdoc_plugin_bold::bold()],
        "italic" => vec![gpdoc_plugin_italic::italic()],
        "underline" => vec![gpdoc_plugin_underline::underline()],
        "strike" => vec![gpdoc_plugin_strike::strike()],
        "inline-code" => vec![gpdoc_plugin_inline_code::inline_code()],
        "superscript" => vec![gpdoc_plugin_superscript::superscript()],
        "subscript" => vec![gpdoc_plugin_subscript::subscript()],
        "link" => vec![gpdoc_plugin_link::link(Default::default())],
        "ruby-text" => vec![gpdoc_plugin_ruby_text::ruby_text()],
        "highlight" => vec![gpdoc_plugin_highlight::multicolor_highlight()],
        "text-align" => vec![gpdoc_plugin_text_align::text_align(["paragraph"])],
        "text-style" => gpdoc_plugin_text_style::text_style_kit(),
        "paragraph" | "heading" => vec![heading()],
        "blockquote" => vec![gpdoc_plugin_blockquote::blockquote()],
        "bullet-list" | "list-item" => vec![list_item(), bullet_list()],
        "ordered-list" => vec![list_item(), gpdoc_plugin_ordered_list::ordered_list()],
        "list-keymap" => vec![
            list_item(),
            bullet_list(),
            gpdoc_plugin_list_keymap::list_keymap(),
        ],
        "list-kit" => gpdoc_plugin_list_kit::list_kit(),
        "task-list" => vec![task_item(), task_list()],
        "hard-break" => vec![gpdoc_plugin_hard_break::hard_break()],
        "horizontal-rule" => vec![gpdoc_plugin_horizontal_rule::horizontal_rule()],
        "code-block" => vec![gpdoc_plugin_code_block::code_block()],
        "code-block-lowlight" => vec![gpdoc_plugin_code_block_lowlight::code_block_lowlight()],
        "details" => vec![gpdoc_plugin_details::details()],
        "table" => vec![gpdoc_plugin_table::table()],
        "image" => vec![gpdoc_plugin_image::image()],
        "audio" => vec![gpdoc_plugin_audio::audio(Default::default())],
        "mention" => vec![gpdoc_plugin_mention::mention()],
        "mathematics" => vec![gpdoc_plugin_mathematics::mathematics()],
        "character-count" => vec![gpdoc_plugin_character_count::character_count(
            gpdoc_plugin_character_count::CountOptions {
                limit: Some(CHARACTER_LIMIT),
                ..Default::default()
            },
        )],
        "find-and-replace" => vec![gpdoc_plugin_find_and_replace::find_and_replace()],
        "markdown" => vec![
            heading(),
            gpdoc_plugin_bold::bold(),
            gpdoc_plugin_italic::italic(),
            gpdoc_plugin_markdown::markdown(),
        ],
        "table-of-contents" => vec![
            heading(),
            gpdoc_plugin_table_of_contents::table_of_contents(),
        ],
        "trailing-node" => vec![heading(), gpdoc_plugin_trailing_node::trailing_node()],
        "typography" => vec![gpdoc_plugin_typography::typography(Default::default())],
        "unique-id" => vec![gpdoc_plugin_unique_id::unique_id(["paragraph"])],
        "suggestion" | "collaboration" | "collaboration-caret" => vec![],
        _ => return Err(format!("Unknown plugin example: {slug}").into()),
    };
    let paragraph = gpdoc_plugin_paragraph::paragraph();
    let config = gpdoc::EditorConfig::builder()
        .plugins(std::iter::once(paragraph).chain(plugins))
        .build()?;
    Ok(gpdoc::Editor::from_document(&config, content)?)
}
