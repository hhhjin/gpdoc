//! A host application combining editing plugins through the public plugin API.
use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::core::Shortcut;
use gpdoc_plugin::prelude::*;
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_heading::heading;
use gpdoc_plugin_heading::{Level, Set as SetHeading};
use gpdoc_plugin_paragraph::paragraph;

unit_command!(WrapCallout, "wrapCallout");

fn callout() -> Plugin {
    Plugin::new("callout", declare_callout)
}

fn declare_callout(p: &mut PluginSpec) {
    p.node(
        NodeSpec::block("callout")
            .group("block")
            .content("paragraph+"),
    );
    p.command::<WrapCallout>(|editor, _| {
        let Selection::Text { anchor, head } = editor.selection() else {
            return Ok(None);
        };
        let (before, block) = editor.document().text_at(head)?;
        if block.kind != "paragraph" || editor.document().text_at(anchor)?.0 != before {
            return Ok(None);
        }
        let mut tx = editor.transaction();
        tx.push(Edit::Wrap {
            range: before..DocPos(before.0 + block.size()),
            kind: "callout".into(),
            attrs: Attrs::new(),
        });
        Ok(Some(tx))
    });
    p.input_rule(|ctx| {
        let (doc, selection, inserted) = (ctx.document, ctx.selection, ctx.inserted);
        let Selection::Text { head, .. } = selection else {
            return Ok(None);
        };
        let (before, block) = doc.text_at(head)?;
        if block.kind != "paragraph"
            || *block != Node::text("paragraph", "! ")
            || inserted != (DocPos(before.0 + 2)..DocPos(before.0 + 3))
        {
            return Ok(None);
        }
        Ok(Some(vec![
            Edit::ReplaceText {
                range: DocPos(before.0 + 1)..head,
                text: String::new(),
            },
            Edit::Wrap {
                range: before..DocPos(before.0 + 2),
                kind: "callout".into(),
                attrs: Attrs::new(),
            },
        ]))
    });
}

#[test]
fn command_and_typed_marker_build_the_same_custom_node() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(callout())
        .build()
        .unwrap();
    let mut command =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "")])
            .unwrap();
    assert!(matches!(
        command.run(WrapCallout).unwrap(),
        CommandOutcome::Applied(_)
    ));

    let mut typing =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "!")])
            .unwrap();
    let old_selection = Selection::caret(DocPos(2));
    typing.select(old_selection).unwrap();
    typing.insert_text(" ").unwrap();
    let callout = Node::container("callout", vec![Node::text("paragraph", "")]);
    assert_eq!(typing.document(), command.document());
    assert_eq!(typing.document().blocks(), std::slice::from_ref(&callout));
    assert_eq!(typing.selection(), Selection::caret(DocPos(2)));
    typing.undo().unwrap();
    assert_eq!(typing.document().blocks(), &[Node::text("paragraph", "!")]);
    assert_eq!(typing.selection(), old_selection);

    let mut plain = gpdoc_plugin::core::Editor::from_document(
        &EditorConfig::builder().plugin(paragraph()).build().unwrap(),
        vec![Node::text("paragraph", "")],
    )
    .unwrap();
    let mut invalid = plain.transaction();
    invalid.push(Edit::ReplaceNodes {
        range: DocPos(0)..DocPos(2),
        nodes: vec![callout],
    });
    assert!(plain.apply(invalid).is_err());
    assert_eq!(plain.document().blocks(), &[Node::text("paragraph", "")]);
    assert_eq!(plain.revision(), 0);
}

fn rule_plugin(name: &'static str) -> Plugin {
    Plugin::new(name, |p| {
        p.input_rule(move |ctx| {
            let inserted = ctx.inserted;
            Ok(Some(vec![Edit::ReplaceText {
                range: inserted,
                text: name.into(),
            }]))
        })
    })
}

#[test]
fn later_plugin_wins_between_equal_priority_input_rules() {
    for (first, second) in [("first", "second"), ("second", "first")] {
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(rule_plugin(first))
            .plugin(rule_plugin(second))
            .build()
            .unwrap();
        let mut editor =
            gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "")])
                .unwrap();
        editor.insert_text("x").unwrap();
        assert_eq!(
            editor.document().blocks(),
            &[Node::text("paragraph", second)]
        );
        editor.undo().unwrap();
        assert_eq!(editor.document().blocks(), &[Node::text("paragraph", "")]);
    }
}

fn use_command(name: &'static str) -> Plugin {
    unit_command!(SameCommand, "same-command");
    Plugin::new(name, |p| p.command::<SameCommand>(|_, _| Ok(None)))
}

#[test]
fn duplicate_command_identifies_both_registrations() {
    let result = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(use_command("left"))
        .plugin(use_command("right"))
        .build();
    assert!(
        matches!(result, Err(Error::Duplicate { name, first, second, .. })
        if name == "same-command" && first == "left" && second == "right")
    );
}

fn divider() -> Plugin {
    Plugin::new("divider", |p| {
        p.node(NodeSpec::block("divider").group("block"))
    })
}

#[test]
fn deleting_the_only_text_among_dividers_keeps_the_insertion_point_editable() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(divider())
        .build()
        .unwrap();
    let divider = Node::leaf("divider", Attrs::new());
    let original = vec![
        divider.clone(),
        Node::text("paragraph", "text"),
        divider.clone(),
    ];
    let mut editor = gpdoc_plugin::core::Editor::from_document(&config, original.clone()).unwrap();
    let selection = Selection::Node { before: DocPos(1) };
    editor.select(selection).unwrap();
    editor.delete_selection().unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[divider.clone(), Node::text("paragraph", ""), divider]
    );
    assert_eq!(editor.selection(), Selection::caret(DocPos(2)));
    editor.insert_text("x").unwrap();
    assert_eq!(editor.document().blocks()[1], Node::text("paragraph", "x"));
    editor.undo().unwrap();
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), original);
    assert_eq!(editor.selection(), selection);
}

fn writing_tools() -> Vec<Plugin> {
    vec![
        callout(),
        Plugin::new("writing-tools", |p| {
            p.shortcut(key!("Mod-2"), SetHeading(Level::H2));
        }),
    ]
}

#[test]
fn preset_can_bind_a_later_command_without_installing_unrelated_plugins() {
    let config = EditorConfig::builder()
        .plugins(writing_tools())
        .plugin(paragraph())
        .plugin(heading())
        .build()
        .unwrap();
    assert!(config.schema().node_spec("callout").is_some());
    assert!(config.schema().mark_spec("italic").is_none());
    assert!(config.shortcuts(&Shortcut::primary("i")).next().is_none());
    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "Title")])
            .unwrap();
    let command = config.shortcuts(&Shortcut::primary("2")).next().unwrap();
    editor.run_stored(command).unwrap();
    assert_eq!(editor.document().blocks()[0].kind, "heading");
    assert_eq!(editor.document().blocks()[0].attrs["level"], Value::from(2));
    editor.undo().unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[Node::text("paragraph", "Title")]
    );
}

#[test]
fn missing_shortcut_target_and_duplicate_preset_member_fail_at_build_time() {
    assert!(
        matches!(EditorConfig::builder().plugins(writing_tools()).plugin(paragraph()).build(),
        Err(Error::InvalidSchema(msg)) if msg.contains("Mod-2") && msg.contains("setHeading"))
    );
    assert!(
        matches!(EditorConfig::builder().plugins(writing_tools().into_iter().chain([callout()])).build(),
        Err(Error::Duplicate { kind: "plugin", name, .. }) if name == "callout")
    );
}

#[test]
fn extensions_compose_per_action_and_preserve_command_arguments() {
    use gpdoc_plugin_hard_break::hard_break;

    let base = heading();
    let custom = base
        .clone()
        .extend(|p| p.shortcuts().toggle(Level::H2).set(key!("F6")))
        .extend(|p| {
            let h2 = p.shortcuts().toggle(Level::H2);
            h2.add("Shift-Mod-.".parse().unwrap());
            h2.add(key!("Mod-Shift-."));
            p.shortcuts().toggle(Level::H3).disable();
        });
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(custom)
        .plugin(hard_break().extend(|p| p.shortcuts().insert().remove(key!("Mod-Enter"))))
        .build()
        .unwrap();

    for key in [key!("Mod-Alt-2"), key!("Mod-Alt-3"), key!("Mod-Enter")] {
        assert_eq!(config.shortcuts(&key).count(), 0);
    }
    for key in [key!("Mod-Alt-1"), key!("Shift-Enter"), key!("Mod-Shift-.")] {
        assert_eq!(config.shortcuts(&key).count(), 1);
    }
    let mut editor = Editor::from_config(&config).unwrap();
    let key = key!("F6");
    editor
        .run_stored(config.shortcuts(&key).next().unwrap())
        .unwrap();
    assert_eq!(editor.document().blocks()[0].attrs["level"], 2);
    let defaults = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(base)
        .build()
        .unwrap();
    assert_eq!(defaults.shortcuts(&key!("Mod-Alt-2")).count(), 1);
    assert_eq!(defaults.shortcuts(&key!("F6")).count(), 0);
}

#[test]
fn task_item_nesting_configures_the_shortcuts_seen_by_extensions() {
    use gpdoc_plugin_task_list::{Sink, task_item_with_nesting, task_list};

    for nested in [false, true] {
        let item = task_item_with_nesting(nested).extend(|p| {
            let sink = p.shortcuts().sink();
            assert_eq!(sink.is_some(), nested);
            if let Some(keys) = sink {
                keys.set(key!("F6"));
            }
        });
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(task_list())
            .plugin(item)
            .build()
            .unwrap();
        assert_eq!(config.shortcuts(&key!("Tab")).count(), 0);
        assert_eq!(
            config
                .shortcuts(&key!("F6"))
                .next()
                .map(StoredCommand::name),
            nested.then_some(Sink::NAME)
        );
        assert_eq!(config.shortcuts(&key!("Shift-Tab")).count(), 1);
    }
}

#[test]
fn disabling_a_shared_key_keeps_other_plugins_and_rejects_ambiguous_local_actions() {
    use gpdoc_plugin_table::table;

    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(table().extend(|p| p.shortcuts().advance_cell().disable()))
        .plugin(Plugin::<()>::new("app", |p| {
            p.shortcut(key!("Tab"), gpdoc_plugin_paragraph::Set)
        }))
        .build()
        .unwrap();
    assert_eq!(
        config
            .shortcuts(&key!("Tab"))
            .map(StoredCommand::name)
            .collect::<Vec<_>>(),
        ["setParagraph"]
    );
    let result = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(table().extend(|p| p.shortcuts().advance_cell().set(key!("Shift-Tab"))))
        .build();
    assert!(
        matches!(result, Err(Error::InvalidSchema(reason)) if reason.contains("table") && reason.contains("Shift-tab"))
    );
}

#[test]
fn duplicate_direct_shortcuts_fail_at_build_time() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(bold())
        .plugin(Plugin::<()>::new("app", |p| {
            p.shortcut(key!("F6"), gpdoc_plugin_paragraph::Set);
            p.shortcut(key!("f6"), gpdoc_plugin_bold::Toggle);
        }))
        .build();
    assert!(matches!(config, Err(Error::InvalidSchema(reason))
        if reason.contains("duplicate shortcut 'f6'") && reason.contains("app")));
}

#[test]
fn malformed_shortcut_keys_fail_at_build_time() {
    let key = Shortcut::new("Mod-b");
    for plugin in [
        Plugin::<()>::new("app", |p| {
            p.shortcut(key.clone(), gpdoc_plugin_paragraph::Set)
        }),
        bold().extend(|p| p.shortcuts().toggle().set(key)).into(),
    ] {
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(plugin)
            .build();
        assert!(matches!(config, Err(Error::InvalidSchema(reason))
            if reason.contains("invalid shortcut 'mod-b'")));
    }
}

#[test]
fn keymaps_and_input_rules_respect_priority_before_installation_order() {
    let rebind: Plugin = Plugin::new("rebind", |p| {
        p.shortcut(key!("Mod-B"), SetHeading(Level::H1))
    })
    .priority(200);
    let config = EditorConfig::builder()
        .plugin(rebind)
        .plugin(paragraph())
        .plugin(bold())
        .plugin(heading())
        .plugin(rule_plugin("first").priority(200))
        .plugin(rule_plugin("second"))
        .build()
        .unwrap();
    let key = Shortcut::primary("b");
    assert_eq!(
        config
            .shortcuts(&key)
            .map(StoredCommand::name)
            .collect::<Vec<_>>(),
        ["setHeading", "toggleBold"]
    );
    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "")])
            .unwrap();
    editor.insert_text("x").unwrap();
    assert_eq!(
        editor.document().blocks(),
        &[Node::text("paragraph", "first")]
    );
}

fn article() -> Plugin {
    Plugin::new("article", |p| {
        p.document_content("heading (paragraph | callout)+")
    })
}

#[test]
fn document_expression_controls_open_and_replacing_a_required_root_child() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(heading())
        .plugin(callout())
        .plugin(article())
        .build()
        .unwrap();
    let paragraph = Node::text("paragraph", "body");
    assert!(gpdoc_plugin::core::Editor::from_document(&config, vec![paragraph.clone()]).is_err());
    let mut editor = gpdoc_plugin::core::Editor::from_document(
        &config,
        vec![Node::text("heading", "title"), paragraph.clone()],
    )
    .unwrap();
    let original = editor.document().clone();
    editor
        .select(Selection::Node { before: DocPos(0) })
        .unwrap();
    editor.delete_selection().unwrap();
    let empty_heading = Node::new(
        "heading",
        [("level".into(), Value::from(1))].into(),
        Content::Inline(vec![]),
    );
    assert_eq!(editor.document().blocks(), &[empty_heading, paragraph]);
    editor.undo().unwrap();
    assert_eq!(editor.document(), &original);
}

#[test]
fn chains_share_the_pending_selection_and_abort_without_losing_redo() {
    unit_command!(Missing, "missing");
    unit_command!(NeedsRange, "needsRange");
    unit_command!(SelectWord, "selectWord");
    let select_word: Plugin = Plugin::new("selectWord", |p| {
        p.command::<SelectWord>(|ctx, _| {
            let mut tx = ctx.transaction();
            tx.select(Selection::Text {
                anchor: DocPos(1),
                head: DocPos(4),
            });
            Ok(Some(tx))
        })
    });
    let mut editor = gpdoc_plugin::core::Editor::from_document(
        &EditorConfig::builder()
            .plugin(paragraph())
            .plugin(bold())
            .plugin(select_word)
            .plugin(Plugin::<()>::new("test", |p| {
                p.command::<NeedsRange>(|ctx, _| {
                    if ctx.selection() == Selection::caret(DocPos(1)) {
                        Ok(None)
                    } else {
                        Ok(Some(ctx.transaction()))
                    }
                })
            }))
            .build()
            .unwrap(),
        vec![Node::text("paragraph", "한😀")],
    )
    .unwrap();
    assert!(editor.can(gpdoc_plugin_bold::Set).unwrap());
    assert!(editor.can(SelectWord).unwrap());
    assert_eq!(editor.selection(), Selection::caret(DocPos(1)));
    editor
        .chain()
        .command(SelectWord)
        .command(gpdoc_plugin_bold::Set)
        .run()
        .unwrap();
    let formatted = editor.document().clone();
    assert_eq!(
        formatted
            .text_runs_in(DocPos(1)..DocPos(4))
            .next()
            .unwrap()
            .1
            .marks[0]
            .kind,
        "bold"
    );
    editor.undo().unwrap();
    let revision = editor.revision();
    assert!(matches!(
        editor.chain().command(SelectWord).command(Missing).run(),
        Err(Error::UnknownCommand(_))
    ));
    assert!(matches!(
        editor
            .chain()
            .command(gpdoc_plugin_bold::Set)
            .command(NeedsRange)
            .run(),
        Ok(CommandOutcome::NotApplicable)
    ));
    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.selection(), Selection::caret(DocPos(1)));
    editor.redo().unwrap();
    assert_eq!(editor.document(), &formatted);
}

#[test]
fn nested_command_chains_replay_typing_marks_at_the_correct_step() {
    unit_command!(TypeBold, "typeBold");
    let typing: Plugin = Plugin::new("typing", |p| {
        unit_command!(TypeX, "typeX");
        p.command::<TypeX>(|ctx, _| {
            let Selection::Text { anchor, head } = ctx.selection() else {
                return Ok(None);
            };
            let mut tx = ctx.transaction();
            tx.push(Edit::ReplaceText {
                range: anchor.min(head)..anchor.max(head),
                text: "X".into(),
            });
            Ok(Some(tx))
        });
        p.command::<TypeBold>(|ctx, _| {
            ctx.plan_chain([
                StoredCommand::new(gpdoc_plugin_bold::Toggle),
                StoredCommand::new(TypeX),
            ])
        });
    });
    let mut editor = gpdoc_plugin::core::Editor::from_document(
        &EditorConfig::builder()
            .plugin(paragraph())
            .plugin(bold())
            .plugin(typing)
            .build()
            .unwrap(),
        vec![Node::text("paragraph", "")],
    )
    .unwrap();
    editor.run(TypeBold).unwrap();
    let (_, run) = editor
        .document()
        .text_runs_in(DocPos(1)..DocPos(2))
        .next()
        .unwrap();
    assert_eq!(run.marks[0].kind, "bold");
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks(), &[Node::text("paragraph", "")]);
}

#[test]
fn a_typing_mark_change_invalidates_older_input_without_adding_an_undo_entry() {
    let mut editor = gpdoc_plugin::core::Editor::from_document(
        &EditorConfig::builder()
            .plugin(paragraph())
            .plugin(bold())
            .build()
            .unwrap(),
        vec![Node::text("paragraph", "")],
    )
    .unwrap();
    let mut pending = editor.begin_text_input();
    pending.replace(editor.selection(), &["plain"]).unwrap();
    editor.run(gpdoc_plugin_bold::Toggle).unwrap();
    assert!(matches!(
        editor.commit_text_input(pending),
        Err(Error::StaleRevision)
    ));
    assert!(matches!(editor.undo(), Err(Error::NoHistory)));
    editor.insert_text("한").unwrap();
    assert_eq!(
        editor
            .document()
            .text_runs_in(DocPos(1)..DocPos(2))
            .next()
            .unwrap()
            .1
            .marks[0]
            .kind,
        "bold"
    );
}

#[test]
fn global_attributes_compose_across_independent_plugins() {
    let attrs = AttrSpecs::from([(
        "tags".into(),
        AttrSpec::with_default(AttrType::Json, Value::Null),
    )]);
    let by_name: Plugin = Plugin::new("byName", |p| {
        p.attributes(
            AttributeTargets::names(["paragraph", "bold"]),
            attrs.clone(),
        )
    });
    let by_placement: Plugin = Plugin::new("byPlacement", |p| {
        p.attributes(
            AttributeTargets::nodes(|node| node.placement == Placement::Block),
            attrs,
        )
    });
    let config = EditorConfig::builder()
        .plugin(by_name)
        .plugin(paragraph())
        .plugin(bold())
        .plugin(by_placement)
        .build()
        .unwrap();
    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "x")])
            .unwrap();
    assert!(editor.schema().node_spec("heading").is_none());
    assert_eq!(editor.document().blocks()[0].attrs["tags"], Value::Null);
    editor
        .select(Selection::Text {
            anchor: DocPos(1),
            head: DocPos(2),
        })
        .unwrap();
    editor.run(gpdoc_plugin_bold::Set).unwrap();
    let (_, text) = editor
        .document()
        .text_runs_in(DocPos(1)..DocPos(2))
        .next()
        .unwrap();
    assert_eq!(text.marks[0].attrs["tags"], Value::Null);
}

#[test]
fn reject_invalid_or_conflicting_global_defaults_when_building_the_config() {
    let global = |name, kind, default| {
        Plugin::<()>::new(name, |p| {
            p.attributes(
                AttributeTargets::names(["paragraph"]),
                [("alignment".into(), AttrSpec::with_default(kind, default))].into(),
            )
        })
    };
    assert!(matches!(
        EditorConfig::builder()
            .plugin(paragraph())
            .plugin(global("first", AttrType::Bool, Value::from(1)))
            .build(),
        Err(Error::InvalidSchema(_))
    ));
    assert!(matches!(
        EditorConfig::builder()
            .plugin(paragraph())
            .plugin(global(
                "first",
                AttrType::String,
                Value::String("left".into())
            ))
            .plugin(global(
                "second",
                AttrType::String,
                Value::String("center".into())
            ))
            .build(),
        Err(Error::InvalidSchema(_))
    ));
}

#[test]
fn a_host_provides_its_own_mark_commands_and_keymap() {
    use gpdoc_plugin::marks::toggle_mark;

    unit_command!(ToggleEmphasis, "toggleEmphasis");
    let custom: Plugin = Plugin::new("emphasis", |p| {
        p.mark(MarkSpec::new("emphasis"));
        p.command::<ToggleEmphasis>(|ctx, _| {
            if ctx.selection().bounds(ctx.document()).is_empty() {
                return Ok(None);
            }
            toggle_mark("emphasis")(ctx, Attrs::new())
        });
        p.shortcut(key!("F7"), ToggleEmphasis);
    });
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(custom)
        .build()
        .unwrap();
    let mut editor = gpdoc_plugin::core::Editor::from_document(
        &config,
        vec![Node::text("paragraph", "**hello**")],
    )
    .unwrap();
    assert!(!editor.can(ToggleEmphasis).unwrap());
    editor
        .select(Selection::Text {
            anchor: DocPos(3),
            head: DocPos(8),
        })
        .unwrap();
    let command = config.shortcuts(&Shortcut::new("F7")).next().unwrap();
    editor.run_stored(command).unwrap();
    assert!(editor.mark_active("emphasis").unwrap());
    editor.undo().unwrap();
    assert_eq!(
        editor.document().blocks(),
        [Node::text("paragraph", "**hello**")]
    );
}

#[test]
fn independently_configured_plugins_use_their_own_options() {
    use gpdoc_plugin_link::{LinkOptions, link};

    for autolink in [false, true] {
        let options = LinkOptions {
            autolink,
            ..Default::default()
        };
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(link(options))
            .build()
            .unwrap();
        assert_eq!(
            config.schema().mark_spec("link").unwrap().inclusive,
            autolink
        );
    }
}

#[test]
fn a_third_party_mark_owns_its_markdown_syntax_in_every_entry_point() {
    use gpdoc_plugin_markdown::{MarkdownSyntax, markdown, open, to_markdown};
    let important: Plugin = Plugin::new("thirdPartyFormatting", |p| {
        p.mark(MarkSpec::new("important"));
        p.metadata(MarkdownSyntax::default().mark("important", "=="));
    });
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(bold())
        .plugin(gpdoc_plugin_inline_code::inline_code())
        .plugin(important)
        .plugin(markdown())
        .build()
        .unwrap();
    let source = r"before ==중요 **한** 😀== after \==literal\== and `==code==`";
    let imported = open(&config, source).unwrap();
    assert!(imported.warnings.is_empty());
    let doc = imported.value.document();
    let marked = doc
        .iter_nodes()
        .filter(|(_, n)| n.marks().iter().any(|m| m.kind == "important"))
        .map(|(_, n)| n.projected_text())
        .collect::<String>();
    assert_eq!(marked, "중요 한 😀");
    assert_eq!(
        doc.root().text_content(),
        "before 중요 한 😀 after ==literal== and ==code=="
    );
    let exported = to_markdown(&imported.value).unwrap();
    assert!(exported.warnings.is_empty());
    assert_eq!(
        open(&config, &exported.value).unwrap().value.document(),
        doc,
        "{}",
        exported.value
    );
    let simple = open(&config, "==중요==").unwrap();
    assert_eq!(to_markdown(&simple.value).unwrap().value.trim(), "==중요==");

    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "old")])
            .unwrap();
    editor
        .run(gpdoc_plugin_markdown::Set {
            markdown: source.into(),
        })
        .unwrap();
    assert_eq!(editor.document(), doc);
    editor.undo().unwrap();
    assert_eq!(editor.document().root().text_content(), "old");
}

#[test]
fn a_custom_block_can_parse_and_render_its_own_fence() {
    use gpdoc_plugin_markdown::{MarkdownSyntax, open, to_markdown};
    let callout = Plugin::new("callout", |p| {
        declare_callout(p);
        p.metadata(MarkdownSyntax::default().fence(
            "callout",
            "note",
            |body| {
                Ok(Node::container(
                    "callout",
                    vec![Node::text("paragraph", body)],
                ))
            },
            |node| node.text_content(),
        ));
    });
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(callout)
        .build()
        .unwrap();
    let imported = open(&config, "```note\n중요\n```").unwrap();
    assert!(imported.warnings.is_empty());
    assert_eq!(imported.value.document().blocks()[0].kind, "callout");
    let exported = to_markdown(&imported.value).unwrap();
    assert!(exported.warnings.is_empty());
    assert_eq!(
        open(&config, &exported.value).unwrap().value.document(),
        imported.value.document()
    );
}

#[test]
fn default_text_block_must_support_empty_insertion() {
    let paragraph = NodeSpec::block("p").group("block").content("text*");
    let quote = NodeSpec::block("quote").group("block").content("p+");
    let titled = NodeSpec::block("titled")
        .group("block")
        .content("text*")
        .attrs([("title".into(), AttrSpec::required(AttrType::String))].into());
    let nodes = [paragraph, quote, titled];

    for default in ["unknown", "quote", "titled"] {
        let plugin: Plugin = Plugin::new("editor", |p| {
            for node in &nodes {
                p.node(node.clone());
            }
            p.default_text_block(default);
        });
        assert!(matches!(
            EditorConfig::builder().plugins([plugin]).build(),
            Err(Error::InvalidSchema(message)) if message.contains(default)
        ));
    }
}

#[test]
fn insert_table_limits_dimensions_before_editing() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_table::table())
        .build()
        .unwrap();
    let mut editor =
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "")])
            .unwrap();
    let too_large = gpdoc_plugin_table::Insert {
        rows: std::num::NonZeroUsize::new(100_001).unwrap(),
        ..Default::default()
    };
    assert!(matches!(editor.run(too_large),
        Err(Error::InvalidCommandArguments { command, .. }) if command == "insertTable"));
    assert_eq!(editor.revision(), 0);
    editor.run(gpdoc_plugin_table::Insert::default()).unwrap();
    let table = &editor.document().blocks()[0];
    assert_eq!(table.kind, "table");
    assert_eq!(table.child_count(), 3);
    assert_eq!(table.child(0).child_count(), 3);
    assert_eq!(table.child(0).child(0).kind, "tableHeader");
}

#[test]
fn commands_keep_native_arguments_in_shortcuts_and_chains() {
    #[derive(Clone)]
    struct Insert {
        text: String,
        enabled: bool,
    }

    impl Command for Insert {
        const NAME: &'static str = "insert";
    }
    let enabled = |text: &str| Insert {
        text: text.into(),
        enabled: true,
    };
    let insert: Plugin = Plugin::new("insertion", |p| {
        p.command::<Insert>(|ctx, arg| {
            if !arg.enabled {
                return Ok(None);
            }
            Ok(Some(ctx.one_step(Edit::ReplaceText {
                range: ctx.selection().bounds(ctx.document()),
                text: arg.text,
            })))
        });
        p.shortcut(key!("F6"), enabled("A"));
    });
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(insert.clone())
        .build()
        .unwrap();
    let mut editor = gpdoc_plugin::core::Editor::from_config(&config).unwrap();
    let shortcut = config.shortcuts(&Shortcut::new("F6")).next().unwrap();
    assert!(editor.can_stored(shortcut).unwrap());
    assert!(matches!(
        editor
            .run(Insert {
                text: "A".into(),
                enabled: false
            })
            .unwrap(),
        CommandOutcome::NotApplicable
    ));
    editor
        .chain()
        .stored(shortcut.clone())
        .command(enabled("B"))
        .run()
        .unwrap();
    assert_eq!(editor.document().root().text_content(), "AB");
    editor.undo().unwrap();
    assert_eq!(editor.document().root().text_content(), "");

    unit_command!(Wrong, "insert");
    assert!(matches!(editor.run(Wrong), Err(Error::UnknownCommand(_))));
    let invalid: Plugin = Plugin::new("wrong-shortcut", |p| p.shortcut(key!("F5"), Wrong));
    assert!(matches!(EditorConfig::builder().plugin(paragraph())
        .plugin(insert)
        .plugin(invalid).build(), Err(Error::InvalidSchema(reason)) if reason.contains("f5") && reason.contains("insert")));
}

#[test]
fn toggling_a_heading_uses_the_kits_default_text_block() {
    let body: Plugin = Plugin::new("article-body", |p| {
        p.node(NodeSpec::block("body").group("block").content("inline*"));
        p.default_text_block("body");
    });
    let mut editor = gpdoc_plugin::core::Editor::from_config(
        &EditorConfig::builder()
            .plugin(body)
            .plugin(heading())
            .build()
            .unwrap(),
    )
    .unwrap();
    let toggle = gpdoc_plugin_heading::Toggle(Level::H2);
    editor.run(toggle).unwrap();
    assert_eq!(editor.document().blocks()[0].kind, "heading");
    editor.run(toggle).unwrap();
    assert_eq!(editor.document().blocks()[0].kind, "body");
}

plugin_config! {
    HookConfig { hooks TestHooks { review: filter, normalize: append } }
}

#[test]
fn replacing_and_reenabling_hooks_preserves_their_position_among_private_hooks() {
    use std::sync::{Arc, Mutex};

    let events = Arc::new(Mutex::new(Vec::new()));
    let record = |name| {
        let events = Arc::clone(&events);
        move || events.lock().unwrap().push(name)
    };
    let plugin = Plugin::<HookConfig>::new("ordered", |p| {
        let first = record("first append");
        p.append_edits(move |_| {
            first();
            Ok(vec![])
        });
        p.hooks().normalize().set(|_| unreachable!());
        let last = record("last append");
        p.append_edits(move |_| {
            last();
            Ok(vec![])
        });

        let first = record("first filter");
        p.filter_transaction(move |_| {
            first();
            Ok(true)
        });
        p.hooks().review().set(|_| unreachable!());
        let last = record("last filter");
        p.filter_transaction(move |_| {
            last();
            Ok(true)
        });
    })
    .extend(|p| {
        p.hooks().normalize().disable();
        p.hooks().review().disable();
    });
    let enabled = plugin.clone().extend(|p| {
        let append = record("new append");
        p.hooks().normalize().set(move |_| {
            append();
            Ok(vec![])
        });
        let filter = record("new filter");
        p.hooks().review().set(move |_| {
            filter();
            Ok(true)
        });
    });

    for (plugin, expected) in [
        (
            enabled,
            vec![
                "first append",
                "new append",
                "last append",
                "first filter",
                "new filter",
                "last filter",
            ],
        ),
        (
            plugin,
            vec!["first append", "last append", "first filter", "last filter"],
        ),
    ] {
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(plugin)
            .build()
            .unwrap();
        let mut editor = Editor::from_config(&config).unwrap();
        events.lock().unwrap().clear();
        editor.insert_text("x").unwrap();
        assert_eq!(*events.lock().unwrap(), expected);
    }
}
