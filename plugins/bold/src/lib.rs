use gpdoc_plugin::marks::{delimited_mark_input, delimited_mark_paste, register_formatting_mark};
use gpdoc_plugin::prelude::{Plugin, unit_command};

unit_command!(pub Set, "setBold");
unit_command!(pub Unset, "unsetBold");
unit_command!(pub Toggle, "toggleBold");

gpdoc_plugin::plugin_config! {
    pub Config {
        shortcuts {
            toggle: Toggle => ["Mod-b"],
        }
        input_rules InputRules { double_asterisk: mark("bold"), double_underscore: mark("bold") }
        paste_rules PasteRules { double_asterisk: paste, double_underscore: paste }
    }
}

/// Install bold with configurable shortcuts, typing delimiters, and paste delimiters.
///
/// ```
/// use gpdoc_plugin::key;
/// let bold = gpdoc_plugin_bold::bold().extend(|p| p.shortcuts().toggle().set(key!("Mod-Alt-b")));
/// ```
///
/// ```compile_fail,E0599
/// gpdoc_plugin_bold::bold().extend(|p| p.shortcuts().next_cell().disable());
/// ```
///
/// ```compile_fail,E0599
/// gpdoc_plugin_bold::bold().extend(|p| p.input_rules().heading_marker().disable());
/// ```
pub fn bold() -> Plugin<Config> {
    let plugin = Plugin::<Config>::new("bold", |p| {
        register_formatting_mark::<Set, Unset, Toggle>(p, "bold");
        p.input_rules()
            .double_asterisk()
            .set(delimited_mark_input("bold", "**"));
        p.paste_rules()
            .double_asterisk()
            .set(delimited_mark_paste("bold", "**"));
        p.input_rules()
            .double_underscore()
            .set(delimited_mark_input("bold", "__"));
        p.paste_rules()
            .double_underscore()
            .set(delimited_mark_paste("bold", "__"));
    });
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
    fn disabling_underscore_typing_preserves_asterisk_typing_and_underscore_paste() {
        use gpdoc_plugin::core::{Editor, EditorConfig};
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(bold().extend(|p| p.input_rules().double_underscore().disable()))
            .build()
            .unwrap();
        let mut editor = Editor::from_config(&config).unwrap();
        for ch in "__plain__ **typed** ".chars() {
            editor.insert_text(&ch.to_string()).unwrap();
        }
        editor.paste_text("__pasted__").unwrap();
        assert_eq!(
            editor.document().root().text_content(),
            "__plain__ typed pasted"
        );
        let marked: Vec<_> = editor
            .document()
            .iter_nodes()
            .filter(|(_, node)| node.marks().iter().any(|mark| mark.kind == "bold"))
            .map(|(_, node)| node.projected_text())
            .collect();
        assert_eq!(marked, ["typed", "pasted"]);
    }

    #[test]
    fn replacing_an_underscore_paste_rule_removes_its_original_matcher() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugin(paragraph())
            .plugin(bold().extend(|p| {
                p.paste_rules()
                    .double_underscore()
                    .set(delimited_mark_paste("bold", "!!"));
            }))
            .build()
            .unwrap();
        let mut editor = gpdoc_plugin::core::Editor::from_config(&config).unwrap();
        editor.paste_text("__old__ !!new!! **other**").unwrap();
        assert_eq!(editor.document().root().text_content(), "__old__ new other");
        let marked: Vec<_> = editor
            .document()
            .iter_nodes()
            .filter(|(_, node)| node.marks().iter().any(|mark| mark.kind == "bold"))
            .map(|(_, node)| node.projected_text())
            .collect();
        assert_eq!(marked, ["new", "other"]);
    }

    #[test]
    fn paste_rules_run_across_paragraphs_and_remain_in_one_undo_entry() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugin(paragraph())
            .plugin(bold())
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
