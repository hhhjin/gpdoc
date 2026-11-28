use super::*;
pub(super) fn facts(slug: &str, editor: &Editor) -> Vec<(String, String)> {
    let mut facts = Vec::new();
    let mut fact = |key: String, value: String| facts.push((key, value));
    let document = editor.document();
    match slug {
        "character-count" => {
            use gpdoc_plugin_character_count::{CountMode, characters, words};
            fact(
                "UTF-16 units".into(),
                format!(
                    "{} / {}",
                    characters(document, CountMode::TextSize),
                    setup::CHARACTER_LIMIT
                ),
            );
            fact("Words".into(), words(document).to_string());
        }
        "find-and-replace" => {
            let search = gpdoc_plugin_find_and_replace::results(editor);
            fact("Matches".into(), search.ranges.len().to_string());
        }
        "table-of-contents" => {
            for heading in gpdoc_plugin_table_of_contents::headings(document) {
                fact(
                    heading.text,
                    format!("H{} · #{}", heading.original_level, heading.id),
                );
            }
        }
        "unique-id" => {
            for (index, block) in document.blocks().iter().enumerate() {
                if let Some(id) = block.attrs.get("id").and_then(Value::as_str) {
                    fact(format!("Paragraph {} ID", index + 1), id.into());
                }
            }
        }
        _ => {}
    }
    facts
}
