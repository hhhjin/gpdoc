use gpdoc_plugin::Plugin;
use gpdoc_plugin_character_count::character_count;
use gpdoc_plugin_find_and_replace::find_and_replace_with;
use gpdoc_plugin_table_of_contents::table_of_contents_with;
use gpdoc_plugin_trailing_node::trailing_node;
use gpdoc_plugin_typography::typography;
use gpdoc_plugin_unique_id::unique_id_with;

pub fn plugin(name: &str) -> Option<Plugin> {
    Some(match name {
        "CharacterCount" => character_count(gpdoc_plugin_character_count::CountOptions {
            limit: Some(5),
            ..Default::default()
        })
        .into(),
        "Typography" => typography(Default::default()).into(),
        "FindAndReplace" => find_and_replace_with(|node| node.kind == "heading"),
        "UniqueID" => unique_id_with(["paragraph"], "id", |_, pos| format!("node-{}", pos.0)),
        "TableOfContents" => table_of_contents_with(["heading"], str::to_owned),
        "TrailingNode" => trailing_node().into(),
        _ => return None,
    })
}
