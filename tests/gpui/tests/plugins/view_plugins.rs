use super::open_editor;
use gpdoc::{
    DecorationFrame, DocumentSource, EditorConfig, PaintLayer, PluginViewExt, ViewBehavior,
    ViewPlugin,
};
use gpdoc_core::{Attrs, DocPos, Node, Selection, Value};
use gpdoc_plugin_box_selection::{box_selection, selection_box};
use gpdoc_plugin_bubble_menu::bubble_menu;
use gpdoc_plugin_drag_handle::drag_handle;
use gpdoc_plugin_drop_cursor::drop_cursor;
use gpdoc_plugin_file_handler::{FileInput, file_handler};
use gpdoc_plugin_floating_menu::floating_menu;
use gpdoc_plugin_focus::{FocusMode, focus};
use gpdoc_plugin_gap_cursor::{gap_cursor, paint_gap_cursor};
use gpdoc_plugin_invisible_characters::{
    character_markers, invisible_characters, is_visible, paint_marker,
};
use gpdoc_plugin_node_range::node_range;
use gpdoc_plugin_paragraph::paragraph;
use gpdoc_plugin_placeholder::{paint_placeholder, placeholder};
use gpui::{
    Bounds, ClipboardItem, EntityInputHandler, FileDropEvent, Focusable, Image, ImageFormat,
    MouseButton, TestAppContext, div, point, px, rgba,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct PaintProbe {
    frames: Rc<RefCell<Vec<(PaintLayer, Bounds<gpui::Pixels>)>>>,
}

impl ViewBehavior for PaintProbe {
    fn paint(
        &self,
        layer: PaintLayer,
        frame: &DecorationFrame<'_>,
        _: &mut gpui::Window,
        _: &mut gpui::App,
    ) {
        if frame.focused
            && let Some(rect) = frame.rects_for_range(DocPos(3)..DocPos(4)).next()
        {
            self.frames.borrow_mut().push((layer, rect));
        }
    }
}

#[test]
fn editor_isolation() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_bold::bold())
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (first, cx) = open_editor(&mut app, &config, DocumentSource::Empty);
    cx.update(|window, cx| window.focus(&first.read(cx).focus_handle(cx)));
    cx.simulate_input("Draft");
    cx.simulate_keystrokes("secondary-a secondary-b");
    cx.update(|_, cx| {
        assert!(
            first
                .read(cx)
                .editor()
                .document()
                .iter_nodes()
                .any(|(_, node)| { node.marks().iter().any(|mark| mark.kind == "bold") })
        );
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| {
        assert_eq!(
            first.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "Draft")]
        );
    });

    let (second, cx) = open_editor(&mut app, &config, DocumentSource::Empty);
    cx.update(|_, cx| {
        let undo = gpui::Keystroke::parse("secondary-z").unwrap();
        assert_eq!(cx.all_bindings_for_input(&[undo]).len(), 1);
        assert_eq!(
            second.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "")]
        );
        assert_eq!(
            first.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "Draft")]
        );
    });
}

#[test]
fn decorations_menus_and_drop_handlers() {
    let floating = Rc::new(Cell::new(0));
    let bubble = Rc::new(Cell::new(0));
    let files = Rc::new(Cell::new(0));
    let drop_position = Rc::new(Cell::new(None));
    let drop_order = Rc::new(RefCell::new(Vec::new()));
    let custom = Rc::new(Cell::new(0));
    let frames = Rc::new(RefCell::new(Vec::new()));
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(ViewPlugin::new("paintProbe", {
            let frames = frames.clone();
            move || PaintProbe {
                frames: frames.clone(),
            }
        }))
        .plugin(placeholder("Write something …").with_view(paint_placeholder))
        .plugin(invisible_characters(true).with_view(|| character_markers(paint_marker)))
        .plugin(focus(FocusMode::Deepest).with_view(|bounds, window, _| {
            window.paint_quad(gpui::fill(bounds, rgba(0x88888822)));
        }))
        .plugin(node_range())
        .plugin(drag_handle().with_view(|_, _, _| "⠿"))
        .plugin(gap_cursor().with_view(paint_gap_cursor))
        .plugin(drop_cursor().with_view(|bounds, window, _| {
            window.paint_quad(gpui::fill(bounds, rgba(0x666666ff)));
        }))
        .plugin(floating_menu().with_view({
            let count = floating.clone();
            move |_, _, _| {
                count.set(count.get() + 1);
                div()
            }
        }))
        .plugin(bubble_menu().with_view({
            let count = bubble.clone();
            move |_, _, _| {
                count.set(count.get() + 1);
                div()
            }
        }))
        .plugin(
            bubble_menu()
                .named("caretMenu")
                .show_when(|view| view.editor().selection() == Selection::caret(DocPos(1)))
                .with_view({
                    let count = custom.clone();
                    move |_, _, _| {
                        count.set(count.get() + 1);
                        div()
                    }
                }),
        )
        .plugin(ViewPlugin::named("interceptDrop").on_text_drop({
            let drop_order = drop_order.clone();
            move |_, _: &gpui::ExternalPaths, _, _, _| {
                drop_order.borrow_mut().push("interceptor");
                false
            }
        }))
        .plugin(file_handler({
            let count = files.clone();
            let drop_position = drop_position.clone();
            let drop_order = drop_order.clone();
            move |_, files, _, _| {
                match files {
                    FileInput::Paste(images) => {
                        assert_eq!(images[0].bytes, b"png fixture");
                        count.set(count.get() + images.len());
                    }
                    FileInput::Drop { at, .. } => {
                        drop_order.borrow_mut().push("file handler");
                        assert_eq!(drop_position.replace(Some(at)), None);
                    }
                }
                true
            }
        }))
        .plugin(ViewPlugin::named("lastDrop").on_block_drop(
            |_, _: &gpui::ExternalPaths, _, _, _| panic!("a handled drop reached a later plugin"),
        ))
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let source = r#"{"type":"doc","content":[{"type":"paragraph"},{"type":"paragraph","content":[{"type":"text","text":"A B"}]}]}"#;
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Json(source));
    cx.update(|window, cx| {
        assert_eq!(
            entity.read(cx).editor().document().to_json().unwrap(),
            source
        );
        window.focus(&entity.read(cx).focus_handle(cx));
        window.refresh();
    });
    cx.run_until_parked();
    // The second paragraph's first character has document positions 3..4 and
    // projected UTF-16 offsets 1..2 due to the empty first paragraph.
    let rect = cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            view.bounds_for_range(1..2, Bounds::default(), window, cx)
                .unwrap()
        })
    });
    assert!(frames.borrow().ends_with(&[
        (PaintLayer::Background, rect),
        (PaintLayer::TextBackground, rect),
        (PaintLayer::Foreground, rect)
    ]));
    assert!(
        floating.get() > 0,
        "floating menu on the focused empty paragraph"
    );
    assert_eq!(bubble.get(), 0);
    assert!(
        custom.get() > 0,
        "custom bubble menu can open at a collapsed caret"
    );
    let caret_renders = custom.get();
    cx.simulate_keystrokes("secondary-a");
    assert!(bubble.get() > 0, "bubble menu on block selection");
    assert_eq!(custom.get(), caret_renders);
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            assert_eq!(
                view.editor().selection(),
                Selection::NodeRange {
                    anchor: DocPos(0),
                    head: DocPos(7)
                }
            );
            view.run(gpdoc_plugin_invisible_characters::Toggle, window, cx)
                .unwrap();
            assert!(!is_visible(view.editor()));
            let mut actual = None;
            assert_eq!(
                view.text_for_range(0..4, &mut actual, window, cx)
                    .as_deref(),
                Some("\nA B")
            );
            cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
                ImageFormat::Png,
                b"png fixture".to_vec(),
            )));
        })
    });
    cx.simulate_keystrokes("secondary-v");
    assert_eq!(files.get(), 1);
    cx.simulate_keystrokes("left");
    let first = cx
        .debug_bounds("gpdoc-drag-0")
        .expect("first paragraph handle")
        .center();
    let last = cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let last = view
                .bounds_for_range(1..1, Bounds::default(), window, cx)
                .unwrap();
            point(last.left() + px(4.), last.bottom() - px(1.))
        })
    });
    cx.simulate_mouse_down(first, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(first.x + px(8.), first.y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(last, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(last, MouseButton::Left, Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "A B"), Node::text("paragraph", "")]
        )
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks()[0],
            Node::text("paragraph", "")
        )
    });

    let position = cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            view.bounds_for_range(3..3, Bounds::default(), window, cx)
                .unwrap()
                .center()
        })
    });
    cx.simulate_event(FileDropEvent::Entered {
        position,
        paths: Default::default(),
    });
    cx.simulate_event(FileDropEvent::Pending { position });
    cx.simulate_event(FileDropEvent::Submit { position });
    assert_eq!(*drop_order.borrow(), ["interceptor", "file handler"]);
    assert_eq!(drop_position.get(), Some(DocPos(5)));

    cx.simulate_click(position, Default::default());
    cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
    cx.simulate_keystrokes("secondary-v");
    assert_eq!(files.get(), 1);
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks()[1],
            Node::text("paragraph", "A pastedB")
        );
    });
}

#[path = "../../../../crates/gpdoc/examples/editor/completion.rs"]
mod completion;
#[path = "../../../../crates/gpdoc/examples/editor/completion_views.rs"]
mod completion_views;
#[path = "../../../../crates/gpdoc/examples/editor/menu_icons.rs"]
#[expect(
    dead_code,
    reason = "the test app uses the SVG elements without the demo asset catalog"
)]
mod menu_icons;

fn mention_picker() -> gpdoc::EditorPlugin {
    gpdoc_plugin_mention::mention().with_view(completion_views::mention_menu)
}

#[test]
fn slash_menu_creates_a_task_and_preserves_literal_slashes() {
    use gpdoc_plugin_slash_command::slash_commands;

    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_task_list::task_item())
        .plugin(gpdoc_plugin_task_list::task_list())
        .plugin(slash_commands(completion::commands()).with_view(completion_views::slash_menu))
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Empty);
    cx.update(|window, cx| window.focus(&entity.read(cx).focus_handle(cx)));
    cx.simulate_input("/tod");
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "o", Some(1..1), window, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "/todo")]
        );
    });
    assert!(cx.debug_bounds("gpdoc-suggestion-0").is_some());
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        let editor = entity.read(cx).editor();
        assert_eq!(editor.document().blocks()[0].kind, "taskList");
        assert_eq!(editor.document().blocks()[0].text_content(), "");
        let Selection::Text { head, .. } = editor.selection() else {
            panic!("expected text caret")
        };
        assert_eq!(
            editor.document().resolve(head).unwrap().parent().kind,
            "paragraph"
        );
    });
    cx.simulate_input("x");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks()[0].text_content(),
            "x"
        )
    });
    cx.simulate_keystrokes("secondary-z secondary-z");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "/todo")]
        )
    });

    cx.simulate_keystrokes("escape enter");
    cx.simulate_input("docs/todo");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks(),
            &[
                Node::text("paragraph", "/todo"),
                Node::text("paragraph", "docs/todo"),
                Node::text("paragraph", ""),
            ]
        )
    });
}

#[test]
fn contributed_slash_item_uses_the_installed_view_and_command() {
    use gpdoc_plugin_heading::{Level, Set, heading};
    use gpdoc_plugin_slash_command::{
        SlashCatalog, SlashCommand, SlashMenu, slash_commands, slash_picker,
    };
    use gpui::prelude::*;

    let contributed = gpdoc_plugin::Plugin::new("articleCommands", |spec| {
        spec.metadata(SlashCatalog::new([SlashCommand {
            id: "article.title",
            label: "Article title",
            aliases: &["title"],
            command: gpdoc::StoredCommand::new(Set(Level::H1)),
        }]));
    });
    let slash = slash_commands([]).with_view(|| {
        slash_picker(|menu: SlashMenu, _, _| {
            menu.popover(menu.list(1, |row| {
                let id = row.value.id;
                row.element().child(
                    div()
                        .debug_selector(move || id.into())
                        .child(row.value.label),
                )
            }))
        })
    });
    let config = EditorConfig::builder()
        .plugin(slash)
        .plugin(mention_picker())
        .plugin(paragraph())
        .plugin(heading())
        .plugin(contributed)
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(&mut app, &config, DocumentSource::Empty);
    cx.update(|window, cx| window.focus(&editor.read(cx).focus_handle(cx)));
    cx.simulate_input("/title");
    let row = cx
        .debug_bounds("article.title")
        .expect("application-rendered contributed row");
    cx.simulate_click(row.center(), Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).editor().document().blocks()[0].kind,
            "heading"
        )
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "/title")]
        )
    });
}

#[test]
fn suggestion_picker() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(mention_picker())
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Empty);
    cx.update(|window, cx| window.focus(&entity.read(cx).focus_handle(cx)));
    cx.simulate_input("@");
    cx.simulate_keystrokes("down enter");
    cx.update(|_, cx| {
        let editor = entity.read(cx).editor();
        let mention = editor.document().node_at(DocPos(1)).unwrap();
        assert_eq!(mention.kind, "mention");
        assert_eq!(mention.attrs["label"], Value::String("Lin".into()));
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "@")]
        )
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_input("Ada");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks(),
            &[Node::text("paragraph", "@Ada"), Node::text("paragraph", "")]
        )
    });
    cx.simulate_input("👩🏽‍💻");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks()[1],
            Node::text("paragraph", "👩🏽‍💻")
        );
    });
    cx.simulate_keystrokes("backspace");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks()[1],
            Node::text("paragraph", "")
        );
    });
    cx.simulate_input("@S");
    let option = cx
        .debug_bounds("gpdoc-suggestion-0")
        .expect("visible suggestion");
    cx.simulate_click(option.center(), Default::default());
    cx.update(|_, cx| {
        assert!(
            entity
                .read(cx)
                .editor()
                .document()
                .iter_nodes()
                .any(|(_, node)| node.kind == "mention"
                    && node.attrs["label"] == Value::String("Sam".into()))
        );
    });

    // Reusing a config must not carry dismissal at position 1 into another view.
    let mut other_app = TestAppContext::single();
    let (other, second) = open_editor(&mut other_app, &config, DocumentSource::Empty);
    second.update(|window, cx| window.focus(&other.read(cx).focus_handle(cx)));
    second.simulate_input("@");
    second.simulate_keystrokes("escape");
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("@");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert_eq!(
            entity
                .read(cx)
                .editor()
                .document()
                .node_at(DocPos(1))
                .unwrap()
                .kind,
            "mention"
        );
    });
    second.simulate_input("Ada");
    second.simulate_keystrokes("enter");
    second.update(|_, cx| {
        assert_eq!(
            other.read(cx).editor().document().blocks()[0],
            Node::text("paragraph", "@Ada")
        );
    });

    second.simulate_input("@");
    second.update(|window, cx| {
        other.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "한", Some(1..1), window, cx);
        })
    });
    second.simulate_keystrokes("enter");
    second.update(|_, cx| {
        assert_eq!(
            other.read(cx).editor().document().blocks()[1],
            Node::text("paragraph", "@한")
        );
    });
}

#[test]
fn selection_menu_yields_to_completions_and_preserves_editor_focus() {
    use gpui::prelude::*;

    let painted = Rc::new(Cell::new(None));
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(
            bubble_menu()
                .named("persistentMenu")
                .show_when(|_| true)
                .with_view({
                    let painted = painted.clone();
                    move |_, _, _| {
                        let painted = painted.clone();
                        gpui::canvas(
                            |_, _, _| (),
                            move |bounds, (), _, _| painted.set(Some(bounds)),
                        )
                        .w(px(40.))
                        .h(px(20.))
                    }
                }),
        )
        .plugin(mention_picker())
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(&mut app, &config, DocumentSource::Empty);
    let menu_in_next_frame = |cx: &mut gpui::VisualTestContext| {
        painted.set(None);
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        painted.get()
    };

    cx.update(|window, cx| window.focus(&editor.read(cx).focus_handle(cx)));
    let menu = menu_in_next_frame(cx).unwrap();
    cx.simulate_click(menu.center(), Default::default());
    assert!(cx.update(|window, cx| editor.read(cx).focus_handle(cx).is_focused(window)));

    cx.simulate_input("@A");
    assert!(cx.debug_bounds("gpdoc-suggestion-0").is_some());
    assert!(menu_in_next_frame(cx).is_none());

    cx.simulate_keystrokes("escape");
    assert!(menu_in_next_frame(cx).is_some());
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "한", Some(1..1), window, cx);
        });
    });
    assert!(menu_in_next_frame(cx).is_none());

    cx.update(|window, cx| {
        editor.update(cx, |view, cx| view.unmark_text(window, cx));
    });
    assert!(menu_in_next_frame(cx).is_some());
    cx.update(|window, _| window.blur());
    assert!(menu_in_next_frame(cx).is_none());
}

#[test]
fn gap_cursor_navigation() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(
            gpdoc_plugin_horizontal_rule::horizontal_rule()
                .with_view(gpdoc_plugin_horizontal_rule::rule_line),
        )
        .plugin(gap_cursor().with_view(paint_gap_cursor))
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let rule = Node::leaf("horizontalRule", Attrs::new());
    let (entity, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![rule.clone(), rule.clone()]),
    );
    cx.update(|window, cx| window.focus(&entity.read(cx).focus_handle(cx)));
    cx.simulate_keystrokes("right");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().selection(),
            Selection::Gap { at: DocPos(1) }
        )
    });
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let native = view.selected_text_range(false, window, cx).unwrap().range;
            view.replace_text_in_range(Some(native), "😀", window, cx);
        })
    });
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().document().blocks(),
            &[rule.clone(), Node::text("paragraph", "😀"), rule.clone()]
        )
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| {
        assert_eq!(
            entity.read(cx).editor().selection(),
            Selection::Gap { at: DocPos(1) }
        )
    });
}

#[test]
fn dragging_a_leaf_handle_moves_the_selected_block_range() {
    let first = Node::leaf("horizontalRule", Attrs::new());
    let second = Node::text("paragraph", "A");
    let third = Node::text("paragraph", "B");
    let original = vec![first.clone(), second.clone(), third.clone()];
    let cursor_visible = Rc::new(Cell::new(false));
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(
            gpdoc_plugin_horizontal_rule::horizontal_rule()
                .with_view(gpdoc_plugin_horizontal_rule::rule_line),
        )
        .plugin(drag_handle().with_view(|_, _, _| "⠿"))
        .plugin(drop_cursor().with_view({
            let visible = cursor_visible.clone();
            move |bounds, window, _| {
                let clipped = bounds.intersect(&window.content_mask().bounds);
                visible.set(clipped == bounds);
                window.paint_quad(gpui::fill(bounds, rgba(0x666666ff)));
            }
        }))
        .build()
        .unwrap();
    let before_third = first.size() + second.size();
    let mut app = TestAppContext::single();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(original.clone()));
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            view.select_document(
                Selection::NodeRange {
                    anchor: DocPos(before_third),
                    head: DocPos(0),
                },
                window,
                cx,
            )
            .unwrap();
        });
    });
    cx.run_until_parked();
    let handle = cx
        .debug_bounds("gpdoc-drag-0")
        .expect("leaf drag handle")
        .center();
    let end = cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let length = view
                .text_for_range(0..100, &mut None, window, cx)
                .unwrap()
                .encode_utf16()
                .count();
            let rect = view
                .bounds_for_range(length..length, Bounds::default(), window, cx)
                .unwrap();
            point(rect.left(), rect.bottom() - px(1.))
        })
    });
    cx.simulate_mouse_down(handle, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(handle.x + px(8.), handle.y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    assert!(
        cursor_visible.get(),
        "the full block drop cursor must be visible"
    );
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    cx.update(|_, cx| {
        let editor = entity.read(cx).editor();
        let after_third = third.size();
        assert_eq!(editor.document().blocks(), [third, first, second]);
        assert_eq!(
            editor.selection(),
            Selection::NodeRange {
                anchor: DocPos(after_third + before_third),
                head: DocPos(after_third)
            }
        );
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| assert_eq!(entity.read(cx).editor().document().blocks(), original));
}

#[test]
fn box_selection_starts_only_outside_block_bounds() {
    use gpui::prelude::*;

    struct Page(gpui::Entity<gpdoc::EditorView>);

    impl Render for Page {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .pl(px(96.))
                .child(gpdoc::EditorSurface::new(&self.0).layout(|document| {
                    div()
                        .flex()
                        .justify_center()
                        .px(px(40.))
                        .py(px(24.))
                        .child(document.max_w(px(480.)))
                }))
        }
    }

    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(drag_handle().with_view(|_, _, _| "⠿"))
        .plugin(box_selection().with_view(selection_box))
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let source = DocumentSource::Blocks(
        ["First", "Second", "Third"]
            .map(|s| Node::text("paragraph", s))
            .to_vec(),
    );
    // Editing the document must also invalidate cached host views.
    struct Host(gpui::Entity<Page>);
    impl Render for Host {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            gpui::AnyView::from(self.0.clone()).cached(gpui::StyleRefinement::default().size_full())
        }
    }
    let entity = app.update(|cx| config.open(source, cx).unwrap());
    let (_, cx) = app.add_window_view(|_, cx| Host(cx.new(|_| Page(entity.clone()))));
    let (first, second) = cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let mut glyph = |offset| {
                view.bounds_for_range(offset..offset + 1, Bounds::default(), window, cx)
                    .unwrap()
                    .center()
            };
            (glyph(1), glyph(7))
        })
    });
    let first = point(first.x.round(), first.y);
    let origin = point(px(100.), second.y);
    let drag = MouseButton::Left;
    // A drag from the host margin never belongs to the editor.
    cx.simulate_mouse_down(point(px(4.), second.y), drag, Default::default());
    cx.simulate_mouse_move(first, drag, Default::default());
    assert!(cx.debug_bounds("gpdoc-box-selection").is_none());
    cx.simulate_mouse_up(first, drag, Default::default());

    cx.simulate_mouse_down(origin, drag, Default::default());
    cx.simulate_mouse_move(first, drag, Default::default());
    assert_eq!(
        cx.debug_bounds("gpdoc-box-selection"),
        Some(Bounds::from_corners(
            point(origin.x, first.y),
            point(first.x, origin.y)
        ))
    );
    let expected = Selection::NodeRange {
        anchor: DocPos(15),
        head: DocPos(0),
    };
    cx.update(|_, cx| assert_eq!(entity.read(cx).editor().selection(), expected));
    cx.simulate_mouse_up(first, drag, Default::default());
    assert!(cx.debug_bounds("gpdoc-box-selection").is_none());
    cx.update(|_, cx| assert_eq!(entity.read(cx).editor().selection(), expected));

    for inside in [second, point(second.x + px(150.), second.y)] {
        cx.simulate_mouse_down(inside, drag, Default::default());
        cx.simulate_mouse_move(first, drag, Default::default());
        cx.update(|_, cx| assert!(matches!(entity.read(cx).editor().selection(), Selection::Text { anchor, head } if anchor != head)));
        cx.simulate_mouse_up(first, drag, Default::default());
        cx.update(|_, cx| assert!(matches!(entity.read(cx).editor().selection(), Selection::Text { anchor, head } if anchor != head)));
    }
}
