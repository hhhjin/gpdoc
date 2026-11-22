//! Counterpart to tests/tiptap/markdown.mjs.

use gpdoc_plugin_blockquote::blockquote;
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_code_block::code_block;
use gpdoc_plugin_heading::heading;
use gpdoc_plugin_horizontal_rule::horizontal_rule;
use gpdoc_plugin_inline_code::inline_code;
use gpdoc_plugin_italic::italic;
use gpdoc_plugin_link::link;
use gpdoc_plugin_list_item::list_item;
use gpdoc_plugin_markdown::{open, to_markdown};
use gpdoc_plugin_ordered_list::ordered_list;
use gpdoc_plugin_paragraph::paragraph;
use gpdoc_plugin_strike::strike;
use serde_json::json;
use std::io;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([
            paragraph(),
            heading(),
            blockquote(),
            ordered_list(),
            list_item(),
            bold(),
            italic(),
            link(Default::default()),
            inline_code(),
            code_block(),
            strike(),
            horizontal_rule(),
            gpdoc_plugin_task_list::task_list(),
            gpdoc_plugin_task_list::task_item(),
            gpdoc_plugin_table::table(),
            gpdoc_plugin_image::image(),
            gpdoc_plugin_mathematics::mathematics(),
        ])
        .build()?;
    let inputs: Vec<String> = serde_json::from_reader(io::stdin().lock())?;
    let mut results = Vec::new();
    for input in inputs {
        let imported = open(&config, &input)?;
        assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
        let doc = imported.value.document();
        let exported = to_markdown(&imported.value)?;
        assert!(exported.warnings.is_empty(), "{:?}", exported.warnings);
        results.push(json!({"doc":doc.root().to_json(),"markdown":exported.value}));
    }
    serde_json::to_writer(io::stdout().lock(), &results)?;
    Ok(())
}
