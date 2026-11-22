//! Floating menu anchored to the editor selection.
use gpdoc_plugin::core::Selection;
use gpdoc_plugin_bubble_menu::{SelectionMenu, bubble_menu};

/// Configure a menu at a caret in an empty top-level text block.
pub fn floating_menu() -> SelectionMenu {
    bubble_menu().named("floatingMenu").show_when(|view| {
        let editor = view.editor();
        let Selection::Text { anchor, head } = editor.selection() else {
            return false;
        };
        let doc = editor.document();
        anchor == head
            && doc.text_at(head).is_ok_and(|(before, node)| {
                node.kind == editor.config().default_text_block()
                    && node.size() == 2
                    && doc.resolve(before).is_ok_and(|pos| pos.depth() == 0)
            })
    })
}
