use super::{hover_block, open_editor};
use gpdoc::{
    DecorationFrame, DocumentSource, EditorConfig, PaintLayer, PluginViewExt, ViewBehavior,
    ViewPlugin,
};
use gpdoc_core::{Attrs, DocPos, Node, ReplaceStep, Selection, Step, Value};
use gpdoc_plugin::Plugin;
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
fn extended_bold_uses_the_new_shortcut_in_the_focused_view() {
    let bold = gpdoc_plugin_bold::bold()
        .extend(|p| p.shortcuts().toggle().set(gpdoc::key!("F6")))
        .with_view(gpdoc_plugin_bold::bold_format);
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(bold)
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (view, cx) = open_editor(&mut app, &config, DocumentSource::Empty);
    cx.update(|window, cx| window.focus(&view.read(cx).focus_handle(cx)));
    cx.simulate_input("Draft");
    cx.simulate_keystrokes("secondary-a secondary-b");
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).editor().document().blocks(),
            [Node::text("paragraph", "Draft")]
        );
    });
    cx.simulate_keystrokes("f6");
    cx.update(|_, cx| {
        let block = &view.read(cx).editor().document().blocks()[0];
        assert_eq!(block.child(0).marks()[0].kind, "bold");
    });
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
        .plugin(
            ViewPlugin::named("lastDrop").on_block_drop(|_, _: &gpui::ExternalPaths, _, _| {
                panic!("a handled drop reached a later plugin");
            }),
        )
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
    hover_block(cx, &entity, 0, false);
    let first = cx
        .debug_bounds("gpdoc-drag-handle")
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

fn mention_picker() -> Plugin<gpdoc_plugin_mention::Shortcuts> {
    use gpdoc_plugin_mention::{Mention, MentionCatalog, mention};

    mention().with_metadata(MentionCatalog::new([
        Mention::new("ada", "Ada"),
        Mention::new("lin", "Lin"),
        Mention::new("sam", "Sam"),
    ]))
}

#[test]
fn slash_menu_creates_a_task_and_preserves_literal_slashes() {
    use gpdoc_plugin_slash_command::{SlashCommand, slash_commands};

    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_task_list::task_item())
        .plugin(gpdoc_plugin_task_list::task_list())
        .plugin(slash_commands([SlashCommand {
            id: "task-list",
            label: "To-do list",
            aliases: &["todo"],
            command: gpdoc::StoredCommand::new(gpdoc_plugin_task_list::Toggle),
        }]))
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

    let contributed = gpdoc_plugin::Plugin::<()>::new("articleCommands", |spec| {
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
    hover_block(cx, &entity, 0, false);
    let handle = cx
        .debug_bounds("gpdoc-drag-handle")
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

fn drag_to_block(
    cx: &mut gpui::VisualTestContext,
    editor: &gpui::Entity<gpdoc::EditorView>,
    source: usize,
    over: usize,
    after: bool,
) {
    let top = cx.update(|_, cx| {
        editor
            .read(cx)
            .editor()
            .document()
            .resolve(DocPos(source))
            .unwrap()
            .depth()
            == 0
    });
    hover_block(cx, editor, source, top);
    let start = cx
        .debug_bounds("gpdoc-drag-handle")
        .expect("source grip")
        .center();
    cx.simulate_mouse_move(start, None, Default::default());
    let end = cx.update(|window, cx| {
        editor.update(cx, |view, _| {
            let bounds = view
                .child_bounds(DocPos(over), window)
                .expect("target block");
            point(
                bounds.right() - px(4.),
                if after {
                    bounds.bottom() - px(1.)
                } else {
                    bounds.top() + px(1.)
                },
            )
        })
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(start.x + px(8.), start.y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
}

#[test]
fn named_root_blocks_can_be_reordered_without_a_block_group() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin::Plugin::<()>::new("sections", |p| {
            p.node(gpdoc_core::NodeSpec::block("section").content("text*"));
            p.document_content("section+");
        }))
        .plugin(drag_handle())
        .build()
        .unwrap();
    let [a, b] = ["A", "B"].map(|text| Node::text("section", text));
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![a.clone(), b.clone()]),
    );
    drag_to_block(cx, &editor, 0, a.size(), true);
    cx.update(|_, cx| assert_eq!(editor.read(cx).editor().document().blocks(), [b, a]));
}

#[test]
fn heading_can_leave_a_quote_via_the_adjacent_paragraph_edge() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_heading::heading())
        .plugin(gpdoc_plugin_blockquote::blockquote())
        .plugin(drag_handle())
        .build()
        .unwrap();
    let heading = Node::text("heading", "Title");
    let body = Node::text("paragraph", "Body");
    let body_before = 1 + heading.size();
    let quote = Node::container("blockquote", vec![heading, body.clone()]);
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(vec![quote]));
    let revision = cx.update(|_, cx| editor.read(cx).editor().revision());
    drag_to_block(cx, &editor, 1, 1, true);
    cx.update(|_, cx| assert_eq!(editor.read(cx).editor().revision(), revision));

    drag_to_block(cx, &editor, 1, body_before, false);
    cx.update(|_, cx| {
        let state = editor.read(cx).editor();
        let [quote, heading] = state.document().blocks() else {
            panic!("expected the heading after the quote")
        };
        assert_eq!(quote.child(0), &body);
        assert_eq!(heading.kind, "heading");
        assert_eq!(
            state.selection(),
            Selection::Node {
                before: DocPos(quote.size())
            }
        );
    });
}

#[test]
fn quote_body_owns_the_drag_and_accepts_an_incoming_paragraph() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_blockquote::blockquote())
        .plugin(drag_handle())
        .build()
        .unwrap();
    let a = Node::text("paragraph", "A");
    let b = Node::text("paragraph", "B");
    let c = Node::text("paragraph", "C");
    let quote = |children| Node::container("blockquote", children);
    let original = vec![a.clone(), quote(vec![b.clone()]), c.clone()];
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(original.clone()));

    let revision = cx.update(|_, cx| editor.read(cx).editor().revision());
    drag_to_block(cx, &editor, 3, 4, true);
    cx.update(|_, cx| assert_eq!(editor.read(cx).editor().revision(), revision));

    // Drop A inside the quote, then use that same paragraph to move the whole quote.
    drag_to_block(cx, &editor, 0, 4, true);
    let nested = vec![quote(vec![b.clone(), a.clone()]), c.clone()];
    cx.update(|_, cx| {
        let state = editor.read(cx).editor();
        assert_eq!(state.document().blocks(), nested);
        assert_eq!(state.selection(), Selection::Node { before: DocPos(4) });
    });
    drag_to_block(cx, &editor, 4, 8, true);
    cx.update(|_, cx| {
        let state = editor.read(cx).editor();
        assert_eq!(state.document().blocks(), [c, quote(vec![b, a])]);
        assert_eq!(state.selection(), Selection::Node { before: DocPos(3) });
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| assert_eq!(editor.read(cx).editor().document().blocks(), nested));
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| assert_eq!(editor.read(cx).editor().document().blocks(), original));
}

#[test]
fn one_gutter_handle_follows_the_hovered_quote_row() {
    let painted = Rc::new(RefCell::new(Vec::new()));
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_blockquote::blockquote())
        .plugin(drag_handle().with_view({
            let painted = painted.clone();
            move |slot, _, _| {
                use gpui::Styled;
                let painted = painted.clone();
                gpui::canvas(
                    |_, _, _| (),
                    move |_, _, _, _| {
                        if slot.part == gpdoc_plugin_drag_handle::DragHandlePart::Grip {
                            painted.borrow_mut().push(slot.before.0);
                        }
                    },
                )
                .size(px(12.))
            }
        }))
        .build()
        .unwrap();
    let grips_in_frame = |cx: &mut gpui::VisualTestContext| {
        painted.borrow_mut().clear();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        painted.take()
    };
    let [a, b, c] = ["A", "B", "C"].map(|s| Node::text("paragraph", s));
    let multi = Node::container("blockquote", vec![a, b]);
    let single = Node::container("blockquote", vec![c]);
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![multi.clone(), single.clone()]),
    );

    for paragraph in [1, 4] {
        hover_block(cx, &editor, paragraph, false);
        assert_eq!(grips_in_frame(cx), [0]);
    }
    let grip = cx.debug_bounds("gpdoc-drag-handle").unwrap().center();
    cx.simulate_mouse_move(grip, None, Default::default());
    assert_eq!(grips_in_frame(cx), [0]);
    hover_block(cx, &editor, 9, false);
    assert_eq!(grips_in_frame(cx), [8]);
    // Approach the destination through its gutter to drop before the entire quote.
    let start = cx.debug_bounds("gpdoc-drag-handle").unwrap().center();
    let end = cx.update(|window, cx| {
        editor.update(cx, |view, _| {
            let bounds = view.child_bounds(DocPos(0), window).unwrap();
            point(bounds.left() + px(1.), bounds.top() + px(1.))
        })
    });
    cx.simulate_mouse_move(start, None, Default::default());
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(start.x + px(8.), start.y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).editor().document().blocks(),
            [single, multi]
        )
    });
    cx.simulate_mouse_move(point(end.x, end.y + px(300.)), None, Default::default());
    assert!(grips_in_frame(cx).is_empty());
}

#[test]
fn heading_grip_starts_at_the_first_line_after_all_wrappers() {
    use gpui::prelude::*;

    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_heading::heading())
        .plugin(
            ViewPlugin::named("heading typography").wrap_node("heading", |_, body, _, _| {
                div()
                    .pt(px(7.))
                    .text_size(px(40.))
                    .line_height(px(50.))
                    .child(body)
            }),
        )
        .plugin(
            ViewPlugin::named("block spacing")
                .wrap_blocks(|_, body, _, _| div().pt(px(22.)).child(body)),
        )
        .plugin(drag_handle())
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![
            Node::text("heading", "Title"),
            Node::text("paragraph", "After"),
        ]),
    );
    let original = cx.update(|_, cx| editor.read(cx).editor().document().clone());
    let (outer, line) = cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            (
                view.child_bounds(DocPos(0), window).unwrap(),
                view.bounds_for_range(0..1, Bounds::default(), window, cx)
                    .unwrap(),
            )
        })
    });
    assert_eq!(line.top() - outer.top(), px(29.));
    cx.simulate_mouse_move(line.center(), None, Default::default());
    let grip = cx.debug_bounds("gpdoc-drag-handle").unwrap();
    assert_eq!(grip.top(), line.top());
    cx.simulate_mouse_move(grip.center(), None, Default::default());
    assert_eq!(cx.debug_bounds("gpdoc-drag-handle"), Some(grip));
    drag_to_block(cx, &editor, 0, original.blocks()[0].size(), true);
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).editor().document().blocks()[1],
            original.blocks()[0]
        )
    });
}

#[test]
fn small_block_images_anchor_at_their_painted_top_inside_a_tall_line() {
    use gpui::{
        http_client::{FakeHttpClient, Response},
        prelude::*,
    };

    let mut app = TestAppContext::single();
    app.update(|cx| cx.set_http_client(FakeHttpClient::create(|_| async {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#;
        Ok(Response::new(svg.into()))
    })));
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_image::image())
        .plugin(
            ViewPlugin::named("tall lines")
                .wrap_blocks(|_, body, _, _| div().pt(px(10.)).line_height(px(60.)).child(body)),
        )
        .plugin(drag_handle())
        .build()
        .unwrap();
    let images = [4, 9].map(|height| {
        Node::leaf(
            "image",
            Attrs::from([
                ("src".into(), "https://example.test/square.svg".into()),
                ("height".into(), Value::from(height)),
            ]),
        )
    });
    let (editor, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(images.into()));
    let offsets = [0, 1].map(|before| {
        hover_block(cx, &editor, before, false);
        let grip = cx.debug_bounds("gpdoc-drag-handle").unwrap();
        let outer = cx.update(|window, cx| {
            editor.update(cx, |view, _| {
                view.child_bounds(DocPos(before), window).unwrap()
            })
        });
        grip.top() - outer.top()
    });
    assert!(offsets[1] > px(10.));
    assert_eq!(offsets[0] - offsets[1], px(5.));
}

#[test]
fn a_short_nested_line_keeps_its_handle_when_the_pointer_enters_the_grip() {
    use gpui::prelude::*;

    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_blockquote::blockquote())
        .plugin(
            ViewPlugin::named("compact lines").wrap_node("paragraph", |_, body, _, _| {
                div()
                    .pt(px(24.))
                    .text_size(px(8.))
                    .line_height(px(8.))
                    .child(body)
            }),
        )
        .plugin(drag_handle())
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![
            Node::container("blockquote", vec![Node::text("paragraph", "A")]),
            Node::text("paragraph", "B"),
        ]),
    );
    let line = cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.bounds_for_range(0..1, Bounds::default(), window, cx)
                .unwrap()
        })
    });
    cx.simulate_mouse_move(line.center(), None, Default::default());
    let grip = cx.debug_bounds("gpdoc-drag-handle").unwrap();
    assert_eq!(grip.top(), line.top());
    cx.simulate_mouse_move(
        point(grip.center().x, grip.bottom() - px(1.)),
        None,
        Default::default(),
    );
    assert_eq!(cx.debug_bounds("gpdoc-drag-handle"), Some(grip));
}

#[test]
fn gutter_handle_follows_the_block_scrolled_under_the_pointer() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(drag_handle())
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![Node::text("paragraph", "A"); 180]),
    );
    hover_block(cx, &editor, 15, true);
    let grip = cx.debug_bounds("gpdoc-drag-handle").unwrap();
    let position = grip.center();
    cx.simulate_mouse_move(position, None, Default::default());

    cx.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), -grip.size.height * 2.)),
        ..Default::default()
    });
    let (before, bounds) = cx.update(|window, cx| {
        editor.update(cx, |view, _| {
            view.child_bounds_at(position, window).unwrap()
        })
    });
    assert_ne!(before, DocPos(15));
    assert_eq!(
        cx.debug_bounds("gpdoc-drag-handle").unwrap().top(),
        bounds.top()
    );
}

#[test]
fn hovering_any_table_cell_keeps_the_grip_at_the_table_origin_and_moves_the_whole_table() {
    use gpui::prelude::*;
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_blockquote::blockquote())
        .plugin(gpdoc_plugin_table::table().with_view(|| {
            gpdoc_plugin_table::table_grid(
                gpdoc_plugin_table::default_cell,
                gpdoc_plugin_table::default_resize_guide,
            )
        }))
        .plugin(
            ViewPlugin::named("outer spacing")
                .wrap_blocks(|_, body, _, _| div().pt(px(17.)).child(body)),
        )
        .plugin(drag_handle())
        .build()
        .unwrap();
    let p = |s| Node::text("paragraph", s);
    let row = |cells| Node::container("tableRow", cells);
    let table = Node::container(
        "table",
        vec![
            row(vec![
                Node::container("tableHeader", vec![p("H1")]),
                Node::container("tableHeader", vec![p("H2")]),
            ]),
            row(vec![
                Node::container("tableCell", vec![p("A"), p("B")]),
                Node::container(
                    "tableCell",
                    vec![Node::container("blockquote", vec![p("C")])],
                ),
            ]),
        ],
    );
    let tail = p("After");
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![table, tail.clone()]),
    );
    let original = cx.update(|_, cx| editor.read(cx).editor().document().clone());
    let table = &original.blocks()[0];
    let table_origin = cx.update(|window, cx| {
        editor.update(cx, |view, _| {
            let outer = view.child_bounds(DocPos(0), window).unwrap();
            let cell = view.child_bounds(DocPos(2), window).unwrap();
            assert_eq!(cell.top() - outer.top(), px(17.));
            assert!(view.child_bounds(DocPos(3), window).unwrap().top() > cell.top());
            point(outer.left(), cell.top())
        })
    });
    let paragraphs: Vec<_> = original
        .iter_blocks()
        .filter(|(pos, node)| pos.0 < table.size() && node.kind == "paragraph")
        .map(|(pos, _)| pos.0)
        .collect();
    assert_eq!(paragraphs.len(), 5);
    for &paragraph in &paragraphs {
        hover_block(cx, &editor, paragraph, false);
        assert_eq!(
            cx.debug_bounds("gpdoc-drag-handle").unwrap().origin,
            table_origin
        );
    }
    drag_to_block(cx, &editor, *paragraphs.last().unwrap(), table.size(), true);
    cx.update(|_, cx| {
        let state = editor.read(cx).editor();
        assert_eq!(
            state.selection(),
            Selection::Node {
                before: DocPos(tail.size())
            }
        );
        assert_eq!(state.document().blocks(), [tail, table.clone()]);
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| assert_eq!(editor.read(cx).editor().document(), &original));
}

#[test]
fn a_selected_paragraph_range_can_drop_into_a_table_cell() {
    let marker = Rc::new(Cell::new(None));
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_table::table())
        .plugin(drag_handle())
        .plugin(drop_cursor().with_view({
            let marker = marker.clone();
            move |bounds, _, _| marker.set(Some(bounds))
        }))
        .build()
        .unwrap();
    let [x, a, b] = ["X", "A", "B"].map(|text| Node::text("paragraph", text));
    let row = Node::container(
        "tableRow",
        vec![Node::container("tableCell", vec![x.clone()])],
    );
    let table = Node::container("table", vec![row]);
    let start = table.size();
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![table, a.clone(), b.clone()]),
    );
    let original = cx.update(|_, cx| editor.read(cx).editor().document().clone());
    let selection = Selection::NodeRange {
        anchor: DocPos(start + a.size() + b.size()),
        head: DocPos(start),
    };
    let target_bounds = cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.select_document(selection, window, cx).unwrap();
            view.child_bounds(DocPos(3), window).unwrap()
        })
    });
    cx.run_until_parked();
    drag_to_block(cx, &editor, start + a.size(), 3, false);
    let line = marker.get().expect("nested insertion line");
    assert_eq!(line.origin, target_bounds.origin);
    assert_eq!(line.size.width, target_bounds.size.width);
    cx.update(|_, cx| {
        let state = editor.read(cx).editor();
        let row = state.document().blocks()[0].child(0);
        assert_eq!(row.child(0).content.children(), [a, b, x]);
        assert_eq!(state.document().blocks().len(), 1);
        assert_eq!(
            state.selection(),
            Selection::NodeRange {
                anchor: DocPos(9),
                head: DocPos(3)
            }
        );
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| {
        let state = editor.read(cx).editor();
        assert_eq!(state.document(), &original);
        assert_eq!(state.selection(), selection);
    });
}

#[test]
fn dragging_the_last_task_to_another_list_preserves_the_item_and_removes_the_empty_list() {
    use gpdoc_plugin_task_list::{task_item, task_list};
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(task_item())
        .plugin(task_list())
        .plugin(drag_handle())
        .build()
        .unwrap();
    let item = |labels: &[&str], checked| {
        Node::new(
            "taskItem",
            Attrs::from([("checked".into(), Value::Bool(checked))]),
            gpdoc_core::Content::Blocks(
                labels.iter().map(|s| Node::text("paragraph", *s)).collect(),
            ),
        )
    };
    let list = |items| Node::container("taskList", items);
    let (a, b) = (item(&["A", "C"], true), item(&["B"], false));
    let tail = Node::text("paragraph", "After the lists");
    let original = vec![list(vec![a.clone()]), list(vec![b.clone()]), tail.clone()];
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(original.clone()));
    let revision = cx.update(|_, cx| editor.read(cx).editor().revision());
    drag_to_block(cx, &editor, 5, 17, true);
    cx.update(|_, cx| assert_eq!(editor.read(cx).editor().revision(), revision));

    // Over B's paragraph, the list item boundary is the first compatible destination.
    drag_to_block(cx, &editor, 5, 12, true);
    cx.update(|_, cx| {
        let state = editor.read(cx).editor();
        assert_eq!(state.document().blocks(), [list(vec![b, a]), tail]);
        assert_eq!(state.selection(), Selection::Node { before: DocPos(6) });
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| assert_eq!(editor.read(cx).editor().document().blocks(), original));
}

#[test]
fn box_selection_drags_from_the_margin_and_tracks_scrolling() {
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
            .into_iter()
            .chain(vec![Node::text("paragraph", "More"); 100])
            .collect(),
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

    let row_height = second.y - first.y;
    let scroll = |position, rows| gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), row_height * rows)),
        ..Default::default()
    };
    // Scrolling a pending press must not count toward the four-pixel drag threshold.
    cx.simulate_mouse_down(origin, drag, Default::default());
    let caret = cx.update(|_, cx| entity.read(cx).editor().selection());
    cx.simulate_event(scroll(origin, -3.));
    assert!(cx.debug_bounds("gpdoc-box-selection").is_none());
    let near = point(origin.x + px(3.), origin.y);
    cx.simulate_mouse_move(near, drag, Default::default());
    assert!(cx.debug_bounds("gpdoc-box-selection").is_none());
    cx.update(|_, cx| assert_eq!(entity.read(cx).editor().selection(), caret));
    let threshold = point(origin.x + px(4.), origin.y);
    cx.simulate_mouse_move(threshold, drag, Default::default());
    assert_eq!(
        cx.debug_bounds("gpdoc-box-selection"),
        Some(Bounds::from_corners(
            point(origin.x, origin.y - row_height * 3.),
            threshold,
        ))
    );
    cx.simulate_mouse_up(threshold, drag, Default::default());
    cx.simulate_event(scroll(origin, 3.));

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

    // Keep the pointer still while the document moves past the drag origin.
    cx.simulate_mouse_down(origin, drag, Default::default());
    cx.simulate_mouse_move(first, drag, Default::default());
    let observed = Rc::new(Cell::new(None));
    let _subscription = cx.update(|_, cx| {
        let observed = observed.clone();
        cx.observe(&entity, move |editor, cx| {
            observed.set(Some(editor.read(cx).editor().selection()));
        })
    });
    let forward = Selection::NodeRange {
        anchor: DocPos(7),
        head: DocPos(28),
    };
    for (rows, anchor_y, selection) in [
        (-3., origin.y - row_height * 3., forward),
        (3., origin.y, expected),
    ] {
        cx.simulate_event(scroll(first, rows));
        assert_eq!(
            cx.debug_bounds("gpdoc-box-selection"),
            Some(Bounds::from_corners(
                point(origin.x, anchor_y.min(first.y)),
                point(first.x, anchor_y.max(first.y))
            ))
        );
        cx.update(|_, cx| assert_eq!(entity.read(cx).editor().selection(), selection));
        assert_eq!(observed.get(), Some(selection));
    }
    cx.simulate_mouse_up(first, drag, Default::default());
    cx.update(|_, cx| assert_eq!(entity.read(cx).editor().selection(), expected));

    for inside in [second, point(second.x + px(150.), second.y)] {
        cx.simulate_mouse_down(inside, drag, Default::default());
        cx.simulate_mouse_move(first, drag, Default::default());
        cx.update(|_, cx| assert!(matches!(entity.read(cx).editor().selection(), Selection::Text { anchor, head } if anchor != head)));
        cx.simulate_mouse_up(first, drag, Default::default());
        cx.update(|_, cx| assert!(matches!(entity.read(cx).editor().selection(), Selection::Text { anchor, head } if anchor != head)));
    }
}

#[test]
fn heading_moves_out_of_a_list_when_the_inner_reorder_is_invalid() {
    use gpdoc_plugin_task_list::{task_item_with_nesting, task_list};
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_heading::heading())
        .plugin(task_item_with_nesting(true))
        .plugin(task_list())
        .plugin(drag_handle())
        .build()
        .unwrap();
    let body = Node::text("paragraph", "A");
    let heading = Node::text("heading", "B");
    let item = Node::container("taskItem", vec![body.clone(), heading]);
    let list = Node::container("taskList", vec![item]);
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(vec![list]));
    drag_to_block(cx, &editor, 2 + body.size(), 2, false);
    cx.update(|_, cx| {
        let state = editor.read(cx).editor();
        let [heading, list] = state.document().blocks() else {
            panic!("expected heading before list")
        };
        assert_eq!(heading.kind, "heading");
        assert_eq!(list.child(0).content.children(), [body]);
        assert_eq!(state.selection(), Selection::Node { before: DocPos(0) });
    });
}

#[test]
fn fallback_drop_indicator_tracks_scroll_with_a_stationary_pointer() {
    let marker = Rc::new(Cell::new(None));
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(ViewPlugin::named("decline").on_block_drop(|_, _: &gpui::ExternalPaths, _, _| None))
        .plugin(ViewPlugin::named("select").on_block_drop(
            |view, _: &gpui::ExternalPaths, targets, _| {
                let target = *targets.first()?;
                let mut tx = view.editor().transaction();
                tx.select(Selection::Node { before: target.at });
                Some((target, tx))
            },
        ))
        .plugin(drop_cursor().with_view({
            let marker = marker.clone();
            move |bounds, _, _| marker.set(Some(bounds))
        }))
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![Node::text("paragraph", "A"); 180]),
    );
    let row = cx.update(|window, cx| {
        editor.update(cx, |view, _| view.child_bounds(DocPos(6), window).unwrap())
    });
    let position = point(row.center().x, row.top() + px(2.));
    cx.simulate_event(FileDropEvent::Entered {
        position,
        paths: Default::default(),
    });
    cx.simulate_event(FileDropEvent::Pending { position });
    assert_eq!(
        marker.take().expect("second planner's line").top(),
        row.top()
    );

    cx.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), -row.size.height - px(3.))),
        ..Default::default()
    });
    let (before, bounds) = cx.update(|window, cx| {
        editor.update(cx, |view, _| {
            view.child_bounds_at(position, window).unwrap()
        })
    });
    assert!(before > DocPos(6));
    assert_ne!(bounds.top(), row.top());
    assert_eq!(
        marker.get().expect("line after scrolling").top(),
        bounds.top()
    );
    cx.simulate_event(FileDropEvent::Submit { position });
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).editor().selection(),
            Selection::Node { before }
        )
    });
}

#[test]
fn filtered_block_drop_passes_to_the_next_handler() {
    let fallback_called = Rc::new(Cell::new(false));
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(Plugin::<()>::new("rejectDocumentChanges", |p| {
            p.filter_transaction(|change| {
                Ok(change.before.is_none_or(|old| old == change.document))
            });
        }))
        .plugin(ViewPlugin::named("firstDrop").on_block_drop(
            |view, _: &gpui::ExternalPaths, targets, _| {
                let target = *targets.first()?;
                let doc = view.editor().document();
                let mut tx = view.editor().transaction();
                tx.push(Step::Replace(ReplaceStep {
                    from: target.at.0,
                    to: target.at.0,
                    slice: doc.root().slice(0, doc.size(), false).unwrap(),
                    structure: false,
                }));
                assert_eq!(tx.document().unwrap().blocks().len(), 2);
                Some((target, tx))
            },
        ))
        .plugin(ViewPlugin::named("fallbackDrop").on_text_drop({
            let fallback_called = fallback_called.clone();
            move |_, _: &gpui::ExternalPaths, _, _, _| {
                fallback_called.set(true);
                true
            }
        }))
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (editor, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![Node::text("paragraph", "A")]),
    );
    let position = cx.update(|window, cx| {
        editor.update(cx, |view, _| {
            view.child_bounds(DocPos(0), window).unwrap().center()
        })
    });
    cx.simulate_event(FileDropEvent::Entered {
        position,
        paths: Default::default(),
    });
    cx.simulate_event(FileDropEvent::Submit { position });
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).editor().document().blocks().len(), 1);
    });
    assert!(fallback_called.get());
}
