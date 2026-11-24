use gpdoc_plugin_mention::Mention;
use gpdoc_plugin_slash_command::SlashCommand;

pub fn people(query: &str) -> Vec<Mention> {
    let query = query.to_lowercase();
    ["Ada", "Lin", "Sam"]
        .into_iter()
        .filter(|name| name.to_lowercase().starts_with(&query))
        .map(|name| Mention::new(name.to_lowercase(), name))
        .collect()
}

pub fn commands() -> Vec<SlashCommand> {
    use gpdoc::StoredCommand;
    use gpdoc_plugin_code_block::{self as code_block, Language};
    use gpdoc_plugin_heading::{Level, Set};

    vec![
        SlashCommand {
            id: "paragraph",
            label: "Text",
            aliases: &["paragraph"],
            command: StoredCommand::new(gpdoc_plugin_paragraph::Set),
        },
        SlashCommand {
            id: "heading.h1",
            label: "Heading 1",
            aliases: &["h1"],
            command: StoredCommand::new(Set(Level::H1)),
        },
        SlashCommand {
            id: "heading.h2",
            label: "Heading 2",
            aliases: &["h2"],
            command: StoredCommand::new(Set(Level::H2)),
        },
        SlashCommand {
            id: "heading.h3",
            label: "Heading 3",
            aliases: &["h3"],
            command: StoredCommand::new(Set(Level::H3)),
        },
        SlashCommand {
            id: "task-list",
            label: "To-do list",
            aliases: &["todo", "task"],
            command: StoredCommand::new(gpdoc_plugin_task_list::Toggle),
        },
        SlashCommand {
            id: "bullet-list",
            label: "Bulleted list",
            aliases: &["ul"],
            command: StoredCommand::new(gpdoc_plugin_bullet_list::Wrap),
        },
        SlashCommand {
            id: "blockquote",
            label: "Quote",
            aliases: &["blockquote"],
            command: StoredCommand::new(gpdoc_plugin_blockquote::Set),
        },
        SlashCommand {
            id: "code-block",
            label: "Code block",
            aliases: &[],
            command: StoredCommand::new(code_block::Set(Language::Keep)),
        },
        SlashCommand {
            id: "ordered-list",
            label: "Numbered list",
            aliases: &["ol"],
            command: StoredCommand::new(gpdoc_plugin_ordered_list::Wrap::default()),
        },
        SlashCommand {
            id: "details",
            label: "Toggle list",
            aliases: &["details"],
            command: StoredCommand::new(gpdoc_plugin_details::Set),
        },
        SlashCommand {
            id: "horizontal-rule",
            label: "Divider",
            aliases: &["rule"],
            command: StoredCommand::new(gpdoc_plugin_horizontal_rule::Set),
        },
        SlashCommand {
            id: "table",
            label: "Table",
            aliases: &["grid"],
            command: StoredCommand::new(gpdoc_plugin_table::Insert::default()),
        },
        SlashCommand {
            id: "block-math",
            label: "Equation",
            aliases: &["math"],
            command: StoredCommand::new(gpdoc_plugin_mathematics::InsertBlock {
                latex: "x = 1".into(),
                pos: None,
            }),
        },
        SlashCommand {
            id: "image",
            label: "Image · session plan",
            aliases: &["picture"],
            command: StoredCommand::new(gpdoc_plugin_image::Set {
                src: "session-plan.png".into(),
                alt: Some("Session plan".into()),
                width: std::num::NonZeroU32::new(560),
                height: std::num::NonZeroU32::new(82),
                ..Default::default()
            }),
        },
        SlashCommand {
            id: "audio",
            label: "Audio · start chime",
            aliases: &["sound"],
            command: StoredCommand::new(gpdoc_plugin_audio::Set {
                src: "/examples/chime.wav".into(),
                ..Default::default()
            }),
        },
        SlashCommand {
            id: "heading.h4",
            label: "Heading 4",
            aliases: &["h4"],
            command: StoredCommand::new(Set(Level::H4)),
        },
        SlashCommand {
            id: "heading.h5",
            label: "Heading 5",
            aliases: &["h5"],
            command: StoredCommand::new(Set(Level::H5)),
        },
        SlashCommand {
            id: "heading.h6",
            label: "Heading 6",
            aliases: &["h6"],
            command: StoredCommand::new(Set(Level::H6)),
        },
    ]
}
