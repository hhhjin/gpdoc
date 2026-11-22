use super::open_editor;
use gpdoc::{DocumentSource, EditorConfig, PluginViewExt};
use gpdoc_core::{DocPos, Node, Value};
use gpdoc_plugin_paragraph::paragraph;
use gpdoc_plugin_table::{default_resize_guide, table, table_grid};
use gpui::{Bounds, EntityInputHandler, MouseButton, TestAppContext, point, px};

#[test]
fn cell_selection_formatting_and_resize() {
    let mut app = TestAppContext::single();
    let row = |a, b| {
        Node::container(
            "tableRow",
            [a, b]
                .map(|text| Node::container("tableCell", vec![Node::text("paragraph", text)]))
                .to_vec(),
        )
    };
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_bold::bold())
        .plugin(table())
        .build()
        .unwrap();
    let blocks = vec![Node::container("table", vec![row("a", "b"), row("c", "d")])];
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(blocks));
    let [first, second, third, fourth] = cx.update(|window, cx| {
        entity.update(cx, |view, _| {
            [2, 7, 14, 19].map(|pos| view.child_bounds(DocPos(pos), window).unwrap())
        })
    });
    let [a, b, c, d] = [first, second, third, fourth].map(|bounds| bounds.center());
    assert_eq!(a.y, b.y);
    assert_eq!(c.y, d.y);
    assert_eq!(a.x, c.x);
    assert!(b.x > a.x + px(20.) && c.y > a.y);
    cx.simulate_mouse_down(a, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(d, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(d, MouseButton::Left, Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().selection(),
            gpdoc_plugin_table::cell_selection(DocPos(2), DocPos(19))
        )
    });
    cx.simulate_mouse_down(b, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(d, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(d, MouseButton::Left, Default::default());
    cx.simulate_keystrokes("secondary-b");
    cx.update(|_, cx| {
        let doc = entity.read(cx).editor().document();
        let bold: Vec<_> = doc
            .iter_nodes()
            .filter_map(|(_, node)| match node.content() {
                gpdoc_core::ContentRef::Text(run) if run.marks.iter().any(|m| m.kind == "bold") => {
                    Some(run.text)
                }
                _ => None,
            })
            .collect();
        assert_eq!(bold, ["b", "d"]);
        let mut editor = entity.read(cx).editor().fork();
        assert!(editor.mark_active("bold").unwrap());
        editor
            .select(gpdoc_plugin_table::cell_selection(DocPos(2), DocPos(14)))
            .unwrap();
        assert!(!editor.mark_active("bold").unwrap());
    });
    let edge = point(second.left(), b.y);
    let moved = point(edge.x + px(28.), edge.y);
    cx.simulate_mouse_down(edge, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(moved, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(moved, MouseButton::Left, Default::default());
    cx.update(|_, cx| {
        let doc = entity.read(cx).editor().document();
        let width = &doc.node_at(DocPos(2)).unwrap().attrs["colwidth"];
        assert!(width.get(0).and_then(Value::as_i64).is_some_and(|w| w > 28));
        assert_eq!(&doc.node_at(DocPos(14)).unwrap().attrs["colwidth"], width);
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| {
        assert_eq!(
            entity
                .read(cx)
                .editor()
                .document()
                .node_at(DocPos(2))
                .unwrap()
                .attrs["colwidth"],
            Value::Null
        )
    });
}

#[test]
fn merged_cell_layout_hit_testing_and_ime_share_document_positions() {
    use gpdoc_core::{Attrs, Content};
    use gpui::{Focusable, div, prelude::*};

    let cell = |text: &str| Node::container("tableCell", vec![Node::text("paragraph", text)]);
    let merged = Node::new(
        "tableCell",
        Attrs::from([
            ("colspan".into(), 2.into()),
            ("rowspan".into(), 3.into()),
            ("colwidth".into(), Value::from(vec![60, 80])),
        ]),
        Content::Blocks(vec![
            Node::text("paragraph", "A"),
            Node::text("paragraph", "second"),
        ]),
    );
    let blocks = vec![Node::container(
        "table",
        vec![
            Node::container("tableRow", vec![merged, cell("B")]),
            Node::container(
                "tableRow",
                vec![Node::new(
                    "tableCell",
                    Attrs::from([("rowspan".into(), 2.into())]),
                    Content::Blocks(vec![Node::text("paragraph", "C")]),
                )],
            ),
            // Both cells extend into the final row, which owns no new cells.
            Node::container("tableRow", vec![]),
        ],
    )];
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(table().with_view(|| {
            table_grid(
                |cell, _, _| cell.content(div().p(px(4.))),
                default_resize_guide,
            )
        }))
        .plugin(
            gpdoc::ViewPlugin::named("tall rows").wrap_node("tableRow", |_, content, _, _| {
                div().min_h(px(72.)).child(content)
            }),
        )
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(blocks));
    let [a, second, b, c] = cx.update(|window, cx| {
        window.focus(&entity.read(cx).focus_handle(cx));
        entity.update(cx, |view, cx| {
            [0, 2, 9, 11].map(|offset| {
                let rect = view
                    .bounds_for_range(offset..offset, Bounds::default(), window, cx)
                    .unwrap();
                let pt = point(rect.left(), rect.center().y);
                assert_eq!(view.character_index_for_point(pt, window, cx), Some(offset));
                pt
            })
        })
    });
    assert_eq!(b.x - a.x, px(140.));
    assert_eq!(a.y, b.y);
    assert_eq!(b.x, c.x);
    assert!(second.y > a.y && second.x == a.x);
    assert!(c.y >= b.y + px(72.));
    cx.update(|window, cx| {
        entity.update(cx, |view, _| {
            let bounds = view.child_bounds(DocPos(2), window).unwrap();
            assert!(
                bounds.size.height >= px(216.),
                "the cell spans all three wrapped rows: {bounds:?}"
            );
            let covered = point(bounds.left() + px(8.), bounds.bottom() - px(8.));
            assert_eq!(view.child_bounds_at(covered, window).unwrap().0, DocPos(2));
        });
    });

    // Dragging the right boundary of a spanning cell resizes its final column.
    let edge = point(b.x - px(4.), b.y);
    let moved = point(edge.x + px(16.), edge.y);
    cx.simulate_mouse_down(edge, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(moved, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(moved, MouseButton::Left, Default::default());
    cx.update(|_, cx| {
        let doc = entity.read(cx).editor().document();
        assert_eq!(
            doc.node_at(DocPos(2)).unwrap().attrs["colwidth"],
            Value::from(vec![60, 96])
        );
    });
    cx.simulate_keystrokes("secondary-z");
    // Click the second paragraph *inside* the merged cell and compose at its start.
    cx.simulate_click(second, Default::default());
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let committed = view.editor().document().clone();
            view.replace_and_mark_text_in_range(None, "ㅎ", Some(1..1), window, cx);
            view.replace_and_mark_text_in_range(None, "한", Some(1..1), window, cx);
            assert_eq!(view.marked_text_range(window, cx), Some(2..3));
            assert_eq!(view.editor().document(), &committed);
            let caret = view
                .bounds_for_range(3..3, Bounds::default(), window, cx)
                .unwrap();
            assert!(caret.left() > second.x);
            assert_eq!(caret.center().y, second.y);
            view.unmark_text(window, cx);
            let merged = view.editor().document().node_at(DocPos(2)).unwrap();
            assert_eq!(merged.child(1).text_content(), "한second");
        });
    });
}

#[test]
fn spanning_cell_uses_all_rows_beyond_native_grid_line_range() {
    use gpdoc_core::{Attrs, Content};

    const COUNT: usize = i16::MAX as usize + 1;
    let cell = Node::new(
        "tableCell",
        Attrs::from([("rowspan".into(), Value::from(COUNT))]),
        Content::Blocks(
            ["one", "two", "three"]
                .map(|t| Node::text("paragraph", t))
                .to_vec(),
        ),
    );
    let empty = Node::container("tableRow", Vec::new());
    let mut rows = vec![empty; COUNT];
    rows[0] = Node::container("tableRow", vec![cell]);
    let table_node = Node::container("table", rows);
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(table())
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(vec![table_node]));
    cx.update(|window, cx| {
        entity.update(cx, |view, _| {
            let bounds = view.child_bounds(DocPos(2), window).unwrap();
            let last_row = view.editor().document().size() - 3;
            let last = view.child_bounds(DocPos(last_row), window).unwrap();
            assert_eq!(bounds.bottom(), last.bottom());
            assert!(last.size.height > px(8.));
            assert_eq!(bounds.size.height, last.size.height * COUNT as f32);
        });
    });
}

#[test]
fn merged_cell_fits_in_existing_row_heights() {
    use gpdoc_core::{Attrs, Content};

    let cell = |text| Node::container("tableCell", vec![Node::text("paragraph", text)]);
    let merged = Node::new(
        "tableCell",
        Attrs::from([("rowspan".into(), 3.into())]),
        Content::Blocks(
            ["one", "two", "three"]
                .map(|t| Node::text("paragraph", t))
                .to_vec(),
        ),
    );
    let rows = [vec![merged, cell("A")], vec![cell("B")], vec![cell("C")]]
        .map(|cells| Node::container("tableRow", cells))
        .to_vec();
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(table())
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (entity, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![Node::container("table", rows)]),
    );
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let [a, b, c] = [14, 16, 18].map(|offset| {
                view.bounds_for_range(offset..offset, Bounds::default(), window, cx)
                    .unwrap()
                    .top()
            });
            assert!(b > a);
            assert_eq!(
                b - a,
                c - b,
                "three paragraphs fit across three ordinary rows"
            );
        });
    });
}
