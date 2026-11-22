use gpdoc_plugin::marks::FormattingMark;
use gpdoc_plugin::prelude::{Plugin, unit_command};

unit_command!(pub Set, "setBold");
unit_command!(pub Unset, "unsetBold");
unit_command!(pub Toggle, "toggleBold");

pub fn bold() -> Plugin {
    let plugin: Plugin = FormattingMark {
        name: "bold",
        set: Set,
        unset: Unset,
        toggle: Toggle,
        shortcut: Some("Mod-b"),
        delimiters: &["**", "__"],
    }
    .into();
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, bold_format);
    plugin
}

/// Apply the native text format for the `bold` mark.
#[cfg(feature = "gpui")]
pub fn bold_format() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("boldFormat").mark_format("bold", |_, style| {
        style.bold = true;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin::core::Node;
    use gpdoc_plugin_paragraph::paragraph;
    #[test]
    fn paste_rules_run_across_paragraphs_and_remain_in_one_undo_entry() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugins([paragraph(), bold()])
            .build()
            .unwrap();
        let mut editor =
            gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "")])
                .unwrap();
        editor
            .paste_text("**first** and __second__\n**😀**")
            .unwrap();
        let bold_text = editor
            .document()
            .iter_nodes()
            .filter(|(_, node)| node.marks().iter().any(|m| m.kind == "bold"))
            .map(|(_, n)| n.projected_text())
            .collect::<Vec<_>>();
        assert_eq!(bold_text, ["first", "second", "😀"]);
        editor.undo().unwrap();
        assert_eq!(editor.document().blocks(), [Node::text("paragraph", "")]);
    }
}
