use super::open_editor;
use gpdoc::{DocumentSource, EditorConfig, EditorView, PluginViewExt, ViewPlugin};
use gpdoc_core::{Node, NodeSpec};
use gpdoc_plugin::Plugin;
use gpdoc_plugin_paragraph::paragraph;
use gpui::{
    Bounds, Entity, EntityInputHandler, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, div, point, prelude::*, px, size,
};

fn caret(entity: &Entity<EditorView>, cx: &mut VisualTestContext, offset: usize) -> Point<Pixels> {
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let r = view
                .bounds_for_range(offset..offset, Bounds::default(), window, cx)
                .unwrap();
            let p = point(r.left(), r.center().y);
            assert_eq!(view.character_index_for_point(p, window, cx), Some(offset));
            p
        })
    })
}

fn callout() -> Plugin {
    Plugin::new("callout", |p| {
        p.node(NodeSpec::block("callout").group("block").content("block+"))
    })
}

fn callout_view() -> ViewPlugin {
    ViewPlugin::named("calloutView").wrap_node("callout", |node, content, _, _| {
        assert_eq!(node.node.kind, "callout");
        div()
            .p(px(16.))
            .border_1()
            .flex()
            .flex_col()
            .child(div().h(px(36.)).child("Note"))
            .child(content)
    })
}

#[test]
fn wrapped_content_supports_selection_editing_and_undo() {
    let mut app = TestAppContext::single();

    let body = "change the width to wrap these words inside the card";
    let initial = vec![
        Node::text("paragraph", "top"),
        Node::container("callout", vec![Node::text("paragraph", body)]),
        Node::text("paragraph", "end"),
    ];
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(callout().with_view(callout_view))
        .plugin(
            ViewPlugin::named("testTypography")
                .wrap_blocks(|_, body, _, _| div().line_height(px(32.)).child(body)),
        )
        .build()
        .unwrap();
    let blocks = initial.clone();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(blocks));
    let start = 4;
    let tail = start + body.len() + 1;
    cx.simulate_resize(size(px(980.), px(640.)));
    cx.run_until_parked();
    let first = caret(&entity, cx, 0);
    let inside = caret(&entity, cx, start);
    let wide_end = caret(&entity, cx, tail - 1);
    let after = caret(&entity, cx, tail);
    assert!(inside.x >= first.x + px(16.) && inside.y >= first.y + px(68.));
    assert_eq!(inside.y, wide_end.y);
    assert_eq!(after.x, first.x);
    assert!(after.y >= wide_end.y + px(48.));
    cx.simulate_resize(size(px(275.), px(640.)));
    cx.run_until_parked();
    let narrow_end = caret(&entity, cx, tail - 1);
    assert!(narrow_end.y > wide_end.y);
    assert!(caret(&entity, cx, tail).y >= narrow_end.y + px(48.));

    let from = caret(&entity, cx, start + 1);
    let to = caret(&entity, cx, tail + 1);
    cx.simulate_mouse_down(from, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(to, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Default::default());
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let sel = view.selected_text_range(false, window, cx).unwrap();
            assert_eq!(sel.range, start + 1..tail + 1);
        })
    });

    cx.simulate_click(from, Default::default());
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "ㅎ", Some(1..1), window, cx);
            view.replace_and_mark_text_in_range(None, "한", Some(1..1), window, cx);
            assert_eq!(
                view.marked_text_range(window, cx),
                Some(start + 1..start + 2)
            );
            let p = view
                .bounds_for_range(start + 2..start + 2, Bounds::default(), window, cx)
                .unwrap()
                .center();
            assert!(p.x > inside.x && p.y == from.y);
            assert_eq!(
                view.character_index_for_point(p, window, cx),
                Some(start + 2)
            );
            assert_eq!(view.editor().document().blocks(), &initial);
            view.unmark_text(window, cx);
            assert_eq!(
                view.editor().document().blocks()[1].text_content(),
                format!("c한{}", &body[1..])
            );
        })
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| assert_eq!(entity.read(cx).editor().document().blocks(), &initial));
}

#[test]
fn nested_cells() {
    let mut app = TestAppContext::single();
    let para = |text| Node::text("paragraph", text);
    let card = |body| Node::container("callout", vec![body]);
    let cell = |body| Node::container("tableCell", vec![body]);
    let row = |cells| Node::container("tableRow", cells);
    let table = Node::container(
        "table",
        vec![
            row(vec![cell(card(card(para("first")))), cell(para("side"))]),
            row(vec![cell(para("next")), cell(para("last"))]),
        ],
    );
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(callout().with_view(callout_view))
        .plugin(gpdoc_plugin_table::table())
        .build()
        .unwrap();
    let (entity, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![table, para("tail")]),
    );
    let first = caret(&entity, cx, 0);
    let side = caret(&entity, cx, 6);
    let next = caret(&entity, cx, 11);
    let last = caret(&entity, cx, 16);
    let tail = caret(&entity, cx, 21);
    assert!(
        side.x > first.x && side.y + px(100.) < first.y,
        "{first:?} {side:?}"
    );
    assert!(first.x >= next.x + px(32.) && next.y > first.y);
    assert_eq!(next.y, last.y);
    assert_eq!(last.x, side.x);
    assert!(tail.y > next.y);
    cx.simulate_mouse_down(first, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(last, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(last, MouseButton::Left, Default::default());
    cx.update(|_, cx| {
        assert!(
            gpdoc_plugin_table::selected_cell_endpoints(entity.read(cx).editor().selection())
                .is_some()
        )
    });
}

#[test]
fn styled_empty_block() {
    let mut app = TestAppContext::single();
    let quote = || {
        ViewPlugin::named("quoteView").wrap_node("blockquote", |_, content, _, _| {
            div().pl(px(24.)).line_height(px(50.)).child(content)
        })
    };
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_blockquote::blockquote().with_view(quote))
        .build()
        .unwrap();
    let (entity, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![
            Node::container("blockquote", vec![Node::text("paragraph", "")]),
            Node::container("blockquote", vec![Node::text("paragraph", "😀")]),
        ]),
    );
    let empty = caret(&entity, cx, 0);
    let next = caret(&entity, cx, 1);
    assert_eq!(next.y - empty.y, px(50.));
    assert!(caret(&entity, cx, 3).x > next.x);
    cx.simulate_click(empty, Default::default());
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            view.replace_text_in_range(None, "한", window, cx);
            assert_eq!(view.editor().document().blocks()[0].text_content(), "한");
        })
    });
    assert_eq!(caret(&entity, cx, 2), next);
}

#[test]
fn optional_search_view_projects_wrapped_matches_through_composition() {
    use gpdoc::{DecorationFrame, PaintLayer, ViewBehavior};
    use gpdoc_core::DocPos;
    use gpdoc_plugin_find_and_replace::{SetSearchTerm, find_and_replace_with};
    use std::{
        cell::RefCell,
        ops::Range,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    struct Probe {
        range: Range<DocPos>,
        rects: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    }
    impl ViewBehavior for Probe {
        fn paint(
            &self,
            layer: PaintLayer,
            frame: &DecorationFrame<'_>,
            _: &mut gpui::Window,
            _: &mut gpui::App,
        ) {
            if layer == PaintLayer::TextBackground {
                for rect in frame.rects_for_committed_range(self.range.clone()) {
                    assert!(frame.clip.intersects(&rect), "rect belongs to this section");
                    self.rects.borrow_mut().push(rect);
                }
            }
        }
    }

    let scans = Arc::new(AtomicUsize::new(0));
    let count = scans.clone();
    let plugin = find_and_replace_with(move |_| {
        count.fetch_add(1, Ordering::Relaxed);
        false
    });
    let body = "a match long enough to span several lines inside this narrow wrapper";
    let blocks = vec![
        Node::text("paragraph", "top"),
        Node::container("callout", vec![Node::text("paragraph", body)]),
        Node::text("paragraph", "end"),
    ];
    let set_term = |entity: &Entity<EditorView>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            entity.update(cx, |view, cx| {
                view.run(SetSearchTerm(body.to_owned()), window, cx)
                    .unwrap();
            });
        });
    };

    let mut app = TestAppContext::single();
    let without = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(callout().with_view(callout_view))
        .plugin(plugin.clone().with_view(|| []))
        .build()
        .unwrap();
    let (other, cx) = open_editor(&mut app, &without, DocumentSource::Blocks(blocks.clone()));
    set_term(&other, cx);
    let baseline = scans.load(Ordering::Relaxed);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(scans.load(Ordering::Relaxed), baseline);
    let range = cx.update(|_, cx| {
        gpdoc_plugin_find_and_replace::results(other.read(cx).editor())
            .ranges
            .remove(0)
    });

    let rects = Rc::new(RefCell::new(vec![]));
    let paints = rects.clone();
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(callout().with_view(callout_view))
        .plugin(plugin)
        .plugin(ViewPlugin::new("search-geometry-probe", move || Probe {
            range: range.clone(),
            rects: paints.clone(),
        }))
        .build()
        .unwrap();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(blocks));
    let committed = cx.update(|_, cx| entity.read(cx).editor().document().clone());
    let baseline = scans.load(Ordering::Relaxed);
    set_term(&entity, cx);
    cx.simulate_resize(size(px(275.), px(640.)));
    cx.run_until_parked();
    assert!(scans.load(Ordering::Relaxed) > baseline);
    assert!(
        rects
            .borrow()
            .windows(2)
            .any(|rows| rows[0].top() < rows[1].top())
    );

    // Projected offset 4 follows "top\n". Inserting a provisional prefix moves
    // the decoration without changing the committed match or rerunning search.
    let before_ime = scans.load(Ordering::Relaxed);
    rects.borrow_mut().clear();
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(Some(4..4), "한 ", Some(2..2), window, cx);
        });
    });
    cx.run_until_parked();
    let expected = caret(&entity, cx, 6);
    let painted = rects.borrow();
    assert!(
        painted
            .iter()
            .any(|rect| rect.left() == expected.x && rect.center().y == expected.y)
    );
    assert_eq!(scans.load(Ordering::Relaxed), before_ime);
    cx.update(|_, cx| {
        assert_eq!(entity.read(cx).editor().document(), &committed);
    });
}

#[test]
fn custom_child_layout_uses_allocated_bounds_for_hit_testing_and_composition() {
    use gpdoc_core::DocPos;
    let columns = || {
        ViewPlugin::named("columns").child_layout("callout", |_, children, window, cx| {
            div()
                .w_full()
                .flex()
                .children(children.into_iter().map(|child| {
                    child.render_with(window, cx, |body| div().w(px(96.)).p(px(10.)).child(body))
                }))
        })
    };
    let spacer = Plugin::new("spacer", |p| {
        p.node(NodeSpec::block("spacer").group("block").content("block*"))
    });
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(callout().with_view(columns))
        .plugin(spacer)
        .build()
        .unwrap();
    let doc = vec![Node::container(
        "callout",
        vec![
            Node::text("paragraph", "left"),
            Node::text("paragraph", "right"),
            Node::container("spacer", vec![]),
        ],
    )];
    let mut app = TestAppContext::single();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(doc));
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let left = view.child_bounds(DocPos(1), window).unwrap();
            let right = view.child_bounds(DocPos(7), window).unwrap();
            assert_eq!(left.top(), right.top());
            assert!(left.right() <= right.left());
            let padding = right.origin + point(px(2.), px(2.));
            assert_eq!(
                view.child_bounds_at(padding, window),
                Some((DocPos(7), right))
            );
            assert_eq!(view.character_index_for_point(padding, window, cx), Some(5));
            let empty = view.child_bounds(DocPos(14), window).unwrap();
            let blank = point(empty.left() + px(2.), right.center().y);
            assert_eq!(view.child_bounds_at(blank, window).unwrap().0, DocPos(14));
            assert_eq!(view.character_index_for_point(blank, window, cx), Some(10));

            view.replace_and_mark_text_in_range(Some(0..0), "한", Some(1..1), window, cx);
            assert_eq!(
                view.child_bounds_at(padding, window),
                Some((DocPos(8), right))
            );
            assert_eq!(view.character_index_for_point(padding, window, cx), Some(6));
        })
    });
}
