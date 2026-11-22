use gpdoc_plugin::prelude::{
    Command, CommandOutcome, ContentRef, DocPos, Editor, Error, Node, Selection, Value,
};
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_bullet_list::bullet_list;
use gpdoc_plugin_heading::heading;
use gpdoc_plugin_list_item::list_item;
use gpdoc_plugin_paragraph::paragraph as paragraph_plugin;

fn paragraph(text: &str) -> Node {
    Node::text("paragraph", text)
}
fn item(text: &str) -> Node {
    Node::container("listItem", vec![paragraph(text)])
}
fn list(items: Vec<Node>) -> Node {
    Node::container("bulletList", items)
}
fn editor(blocks: Vec<Node>) -> Editor {
    gpdoc_plugin::core::Editor::from_document(
        &gpdoc_plugin::core::EditorConfig::builder()
            .plugins([
                paragraph_plugin(),
                heading(),
                bold(),
                list_item(),
                bullet_list(),
            ])
            .build()
            .unwrap(),
        blocks,
    )
    .unwrap()
}
fn run(editor: &mut Editor, command: impl Command) {
    assert!(matches!(
        editor.run(command).unwrap(),
        CommandOutcome::Applied(_)
    ));
}
fn select_text_start(editor: &mut Editor, text: &str) {
    let before = editor
        .document()
        .nodes()
        .into_iter()
        .find(|(_, node)| **node == paragraph(text))
        .unwrap()
        .0;
    editor
        .select(Selection::caret(DocPos(before.0 + 1)))
        .unwrap();
}

#[test]
fn lists_wrap_and_lift_an_item_between_two_siblings() {
    let mut ed = editor(vec![paragraph("one"), paragraph("two"), paragraph("three")]);
    ed.select(Selection::Text {
        anchor: DocPos(1),
        head: DocPos(12),
    })
    .unwrap();
    run(&mut ed, gpdoc_plugin_bullet_list::Wrap);
    let original = list(vec![item("one"), item("two"), item("three")]);
    assert_eq!(ed.document().blocks(), std::slice::from_ref(&original));
    select_text_start(&mut ed, "two");
    run(&mut ed, gpdoc_plugin_list_item::Lift);
    assert_eq!(
        ed.document().blocks(),
        &[
            list(vec![item("one")]),
            paragraph("two"),
            list(vec![item("three")])
        ]
    );
    ed.undo().unwrap();
    assert_eq!(ed.document().blocks(), &[original]);
}

#[test]
fn indent_appends_to_existing_sublist_and_lifting_keeps_following_siblings() {
    let mut ed = editor(vec![list(vec![item("a"), item("b"), item("c")])]);
    select_text_start(&mut ed, "b");
    run(&mut ed, gpdoc_plugin_list_item::Sink);
    select_text_start(&mut ed, "c");
    run(&mut ed, gpdoc_plugin_list_item::Sink);
    let a = Node::container(
        "listItem",
        vec![paragraph("a"), list(vec![item("b"), item("c")])],
    );
    assert_eq!(ed.document().blocks(), &[list(vec![a])]);
    select_text_start(&mut ed, "b");
    run(&mut ed, gpdoc_plugin_list_item::Lift);
    let b = Node::container("listItem", vec![paragraph("b"), list(vec![item("c")])]);
    assert_eq!(ed.document().blocks(), &[list(vec![item("a"), b])]);
}

#[test]
fn installing_individual_features_limits_schema_and_commands() {
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([bold(), paragraph_plugin()])
        .build()
        .unwrap();
    assert!(
        gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("heading", "title")])
            .is_err()
    );
    let mut ed =
        gpdoc_plugin::core::Editor::from_document(&config, vec![paragraph("text")]).unwrap();
    assert!(matches!(
        ed.run(gpdoc_plugin_link::Unset),
        Err(Error::UnknownCommand(_))
    ));
    ed.select(Selection::Text {
        anchor: DocPos(5),
        head: DocPos(1),
    })
    .unwrap();
    run(&mut ed, gpdoc_plugin_bold::Set);
    let ContentRef::Inline(runs) = ed.document().blocks()[0].content() else {
        panic!()
    };
    assert_eq!(runs[0].marks()[0].kind, "bold");
}

#[test]
fn toggle_covers_a_partially_marked_reversed_selection_across_blocks() {
    let original = vec![paragraph("A😀"), paragraph("한B")];
    let mut ed = editor(original.clone());
    assert!(matches!(
        ed.run(gpdoc_plugin_bold::Toggle),
        Ok(CommandOutcome::Applied(_))
    ));
    ed.select(Selection::Text {
        anchor: DocPos(2),
        head: DocPos(4),
    })
    .unwrap();
    run(&mut ed, gpdoc_plugin_bold::Set);
    let partial = ed.document().clone();
    let selected = Selection::Text {
        anchor: DocPos(7),
        head: DocPos(2),
    };
    ed.select(selected).unwrap();
    run(&mut ed, gpdoc_plugin_bold::Toggle);
    let marked: Vec<_> = ed
        .document()
        .text_runs_in(DocPos(2)..DocPos(7))
        .map(|(_, run)| {
            assert!(run.marks.iter().any(|m| m.kind == "bold"));
            run.text
        })
        .collect();
    assert_eq!(marked, ["😀", "한"]);
    run(&mut ed, gpdoc_plugin_bold::Toggle);
    assert_eq!(ed.document().blocks(), original);
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.document(), &partial);
    assert_eq!(ed.selection(), selected);
}

#[test]
fn heading_rule_matches_at_caret_even_when_text_follows_and_undo_restores_prefix() {
    let mut ed = editor(vec![paragraph("##title")]);
    ed.select(Selection::caret(DocPos(3))).unwrap();
    ed.insert_text(" ").unwrap();
    let heading = &ed.document().blocks()[0];
    assert_eq!(heading.kind, "heading");
    assert_eq!(heading.attrs["level"], Value::from(2));
    let ContentRef::Inline(runs) = heading.content() else {
        panic!()
    };
    assert_eq!(runs[0].projected_text(), "title");
    assert_eq!(ed.selection(), Selection::caret(DocPos(1)));
    ed.undo().unwrap();
    assert_eq!(ed.document().blocks(), &[paragraph("##title")]);
    assert_eq!(ed.selection(), Selection::caret(DocPos(3)));
}

#[test]
fn custom_list_containers_use_the_registered_list_item() {
    gpdoc_plugin::unit_command!(WrapOutline, "wrapInOutline");

    fn outline() -> gpdoc_plugin::Plugin {
        gpdoc_plugin::Plugin::new("outline", |p| {
            gpdoc_plugin_list_item::list_container(p, "outline", Default::default());
            p.command::<WrapOutline>(|ctx, _| {
                gpdoc_plugin_list_item::wrap_list(ctx, "outline", "listItem", &Default::default())
            });
        })
    }

    assert!(matches!(
        gpdoc_plugin::core::EditorConfig::builder().plugins([paragraph_plugin(), outline()]).build(),
        Err(Error::InvalidSchema(message)) if message.contains("listItem")
    ));
    let config = gpdoc_plugin::core::EditorConfig::builder()
        .plugins([paragraph_plugin(), list_item(), outline()])
        .build()
        .unwrap();
    let outline = Node::container("outline", vec![item("first"), item("second")]);
    let mut ed = gpdoc_plugin::core::Editor::from_document(
        &config,
        vec![paragraph("first"), paragraph("second")],
    )
    .unwrap();
    ed.select(Selection::Text {
        anchor: DocPos(1),
        head: DocPos(14),
    })
    .unwrap();
    run(&mut ed, WrapOutline);
    assert_eq!(ed.document().blocks(), std::slice::from_ref(&outline));
    select_text_start(&mut ed, "second");
    run(&mut ed, gpdoc_plugin_list_item::Sink);
    let nested = Node::container("outline", vec![item("second")]);
    let first = Node::container("listItem", vec![paragraph("first"), nested]);
    assert_eq!(
        ed.document().blocks(),
        &[Node::container("outline", vec![first])]
    );
    run(&mut ed, gpdoc_plugin_list_item::Lift);
    assert_eq!(ed.document().blocks(), &[outline]);
}
