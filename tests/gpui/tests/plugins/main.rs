//! Plugin interactions exercised with GPUI's in-memory test platform.
#![cfg(not(target_family = "wasm"))]

mod collaboration;
mod content_view;
mod folds;
mod tables;
mod view_plugins;

use gpdoc::{DocumentSource, EditorConfig, EditorView, OpenError, PluginViewExt, ViewPlugin};
use gpdoc_core::{Attrs, DocPos, Node};
use gpdoc_plugin::Plugin;
use gpdoc_plugin_paragraph::paragraph;
use gpui::{
    Bounds, Entity, EntityInputHandler, TestAppContext, VisualTestContext, WindowBounds,
    WindowOptions,
};

fn open_editor(
    app: &mut TestAppContext,
    config: &EditorConfig,
    source: DocumentSource<'_>,
) -> (Entity<EditorView>, &'static mut VisualTestContext) {
    let window = app.update(|cx| {
        let bounds = Bounds::maximized(None, cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| config.open(source, cx).unwrap(),
        )
        .unwrap()
    });
    let view = window.root(app).unwrap();
    let cx = VisualTestContext::from_window(*window, app).into_mut();
    cx.run_until_parked();
    (view, cx)
}

fn hover_block(
    cx: &mut VisualTestContext,
    editor: &Entity<EditorView>,
    before: usize,
    gutter: bool,
) {
    let bounds = cx.update(|window, cx| {
        editor.update(cx, |view, _| {
            view.child_bounds(DocPos(before), window).unwrap()
        })
    });
    let x = if gutter {
        bounds.left() + gpui::px(1.)
    } else {
        bounds.center().x
    };
    cx.simulate_mouse_move(
        gpui::point(x, bounds.top() + gpui::px(2.)),
        None,
        Default::default(),
    );
}

#[test]
fn open_rejects_invalid_content_before_initializing_view_behaviors() {
    use std::{cell::Cell, rc::Rc};

    struct Behavior;
    impl gpdoc::ViewBehavior for Behavior {}

    let initialized = Rc::new(Cell::new(0));
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(ViewPlugin::new("countViews", {
            let initialized = initialized.clone();
            move || {
                initialized.set(initialized.get() + 1);
                Behavior
            }
        }))
        .build()
        .unwrap();
    TestAppContext::single().update(|cx| {
        assert!(matches!(
            config.open(DocumentSource::Json("{"), cx),
            Err(OpenError::Json(_))
        ));
        assert!(matches!(
            config.open(DocumentSource::Blocks(vec![Node::text("unknown", "x")]), cx),
            Err(OpenError::Editor(_))
        ));
        assert_eq!(initialized.get(), 0);
        let view = config.open(DocumentSource::Empty, cx).unwrap();
        assert_eq!(initialized.get(), 1);
        assert_eq!(
            view.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "")]
        );
    });
}

#[test]
fn disclosure_click_and_undo() {
    use gpdoc_plugin_details::{DetailsSummary, details_with_persistence, disclosure};
    use gpui::TestAppContext;
    let mut app = TestAppContext::single();
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(details_with_persistence(true).with_view(|| disclosure(DetailsSummary::render)))
        .plugin(gpdoc_plugin_drag_handle::drag_handle().with_view(|_, _, _| "⠿"))
        .build()
        .unwrap();
    let (entity, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![Node::container(
            "details",
            vec![
                Node::text("detailsSummary", "Title"),
                Node::container("detailsContent", vec![Node::text("paragraph", "Body")]),
            ],
        )]),
    );
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            assert_eq!(
                view.text_for_range(0..40, &mut None, window, cx).as_deref(),
                Some("Title")
            );
        })
    });
    let disclosure = cx
        .debug_bounds("gpdoc-disclosure-1")
        .expect("summary disclosure");
    hover_block(cx, &entity, 0, true);
    let drag = cx
        .debug_bounds("gpdoc-drag-handle")
        .expect("details drag handle");
    assert!(
        !drag.intersects(&disclosure),
        "disclosure and drag have separate hit targets"
    );
    cx.simulate_click(disclosure.center(), Default::default());
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            assert_eq!(
                view.text_for_range(0..40, &mut None, window, cx).as_deref(),
                Some("Title\nBody")
            );
            assert_eq!(
                view.editor().document().blocks()[0].attrs["open"],
                gpdoc_core::Value::Bool(true)
            );
        })
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            assert_eq!(
                view.text_for_range(0..40, &mut None, window, cx).as_deref(),
                Some("Title")
            );
        })
    });
}

#[test]
fn task_checkbox_click_and_undo() {
    use gpui::{TestAppContext, div, prelude::*};
    let mut app = TestAppContext::single();
    use gpdoc_plugin_task_list::{task_item, task_list};
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(task_item().with_view(|| {
            gpdoc_plugin_task_list::task_rows(|row, _, _| {
                div()
                    .flex()
                    .child(
                        row.checkbox.child(
                            div()
                                .debug_selector(|| "painted-checkbox".into())
                                .child("☐"),
                        ),
                    )
                    .child(row.content)
            })
        }))
        .plugin(task_list())
        .plugin(gpdoc_plugin_drag_handle::drag_handle().with_view(|_, _, _| "⠿"))
        .build()
        .unwrap();
    let (entity, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![
            Node::text("paragraph", "A"),
            Node::container(
                "taskList",
                vec![Node::container(
                    "taskItem",
                    vec![Node::text("paragraph", "Task")],
                )],
            ),
        ]),
    );
    let checkbox = cx
        .debug_bounds("gpdoc-task-checkbox-4")
        .expect("task checkbox");
    hover_block(cx, &entity, 3, true);
    let handle = cx
        .debug_bounds("gpdoc-drag-handle")
        .expect("task list handle");
    assert!(
        !handle.intersects(&checkbox),
        "checkbox and drag must accept clicks independently"
    );
    hover_block(cx, &entity, 5, false);
    assert!(
        !cx.debug_bounds("gpdoc-drag-handle")
            .expect("task item handle")
            .intersects(&checkbox),
        "the nested handle leaves the checkbox clickable"
    );
    cx.simulate_click(checkbox.center(), Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            entity
                .read(cx)
                .editor()
                .document()
                .node_at(DocPos(4))
                .unwrap()
                .attrs
                .get("checked"),
            Some(&gpdoc_core::Value::Bool(true))
        )
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| {
        assert_eq!(
            entity
                .read(cx)
                .editor()
                .document()
                .node_at(DocPos(4))
                .unwrap()
                .attrs
                .get("checked"),
            Some(&gpdoc_core::Value::Bool(false))
        )
    });
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            // The second paragraph starts at offset 2 in the shared native text input.
            view.replace_and_mark_text_in_range(Some(2..2), "한", Some(1..1), window, cx);
            assert_eq!(view.marked_text_range(window, cx), Some(2..3));
            assert_eq!(view.editor().document().blocks()[1].text_content(), "Task");
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("painted-checkbox").is_some());
    cx.simulate_click(checkbox.center(), Default::default());
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            assert_eq!(view.marked_text_range(window, cx), Some(2..3));
            assert_eq!(
                view.editor()
                    .document()
                    .node_at(DocPos(4))
                    .unwrap()
                    .attrs
                    .get("checked"),
                Some(&gpdoc_core::Value::Bool(false))
            );
            view.unmark_text(window, cx);
            assert_eq!(
                view.editor().document().blocks()[1].text_content(),
                "한Task"
            );
        })
    });
}

#[test]
fn inline_node_view_receives_clicks() {
    use gpdoc_core::{Content, NodeSpec, TextRun};
    use gpui::{MouseButton, TestAppContext, div, prelude::*};
    use std::{cell::Cell, rc::Rc};
    fn mention() -> Plugin {
        Plugin::new("mention", |p| p.node(NodeSpec::inline("mention")))
    }
    let mut app = TestAppContext::single();
    let blocks = vec![Node::new(
        "paragraph",
        Attrs::new(),
        Content::Inline(vec![
            TextRun {
                text: "😀".into(),
                marks: vec![],
            }
            .into(),
            Node::leaf("mention", Attrs::new()),
        ]),
    )];
    let clicked = Rc::new(Cell::new(false));
    let flag = clicked.clone();
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(mention().with_view(|| {
            ViewPlugin::node_view("mention", move |slot, _, _| {
                assert_eq!(slot.before, DocPos(3));
                let flag = flag.clone();
                div()
                    .size_full()
                    .child("@")
                    .on_mouse_down(MouseButton::Left, move |_, _, _| flag.set(true))
            })
        }))
        .build()
        .unwrap();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(blocks));
    let rect = cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            view.bounds_for_range(2..3, Bounds::default(), window, cx)
                .unwrap()
        })
    });
    assert!(rect.size.width > gpui::px(0.));
    cx.simulate_click(rect.center(), Default::default());
    assert!(clicked.get());
}
