use super::{Accept, Mention, MentionCatalog};
use gpdoc::ViewPlugin;
use gpdoc_plugin::core::Editor;
use gpdoc_plugin_suggestion::{
    MenuRenderer, SuggestionItem, SuggestionMenu, Trigger, suggestion_picker,
};

pub type MentionMenu = SuggestionMenu<Mention>;

/// Create a picker using the application's mention search and row layout.
pub fn mention_picker(
    search: impl Fn(&str) -> Vec<Mention> + 'static,
    render: impl MenuRenderer<Mention>,
) -> ViewPlugin {
    picker(move |_, query| search(query), render)
}

/// Search the catalogs contributed by the editor's installed plugins.
pub fn mention_catalog_picker(render: impl MenuRenderer<Mention>) -> ViewPlugin {
    picker(
        |editor, query| {
            let query = query.to_lowercase();
            editor
                .config()
                .plugin_data::<MentionCatalog>()
                .flat_map(|(_, catalog)| catalog.entries())
                .filter(|entry| {
                    entry.trigger == '@'
                        && [entry.label.as_deref(), entry.id.as_deref()]
                            .into_iter()
                            .flatten()
                            .any(|text| text.to_lowercase().starts_with(&query))
                })
                .cloned()
                .collect()
        },
        render,
    )
}

pub(super) fn catalog_menu() -> ViewPlugin {
    mention_catalog_picker(|menu: MentionMenu, _, _| {
        menu.labeled_list(|entry| {
            entry
                .label
                .as_ref()
                .or(entry.id.as_ref())
                .cloned()
                .unwrap_or_default()
        })
    })
}

fn picker(
    search: impl Fn(&Editor, &str) -> Vec<Mention> + 'static,
    render: impl MenuRenderer<Mention>,
) -> ViewPlugin {
    let trigger = Trigger::new('@');
    suggestion_picker(
        "mentionPicker",
        move |editor| trigger.find(editor.document(), editor.selection()),
        move |editor, matched| {
            search(editor, &matched.query)
                .into_iter()
                .map(|mention| SuggestionItem::new(mention.clone(), Accept(mention)))
                .collect()
        },
        render,
    )
    .for_plugin("mention")
}
