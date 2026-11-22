//! Exercise Command shortcuts, text input, and native font shaping in hidden macOS windows.

#[cfg(target_os = "macos")]
fn main() {
    use gpdoc::{DocumentSource, EditorConfig, EditorView, PluginViewExt};
    use gpdoc_core::{CommandOutcome, DocPos, Node, Selection};
    use gpdoc_plugin::Plugin;
    use gpdoc_plugin_bold::bold;
    use gpdoc_plugin_highlight::highlight;
    use gpdoc_plugin_horizontal_rule::{horizontal_rule, rule_line};
    use gpdoc_plugin_italic::italic;
    use gpdoc_plugin_paragraph::paragraph;
    use gpui::{
        AnyWindowHandle, App, Application, Bounds, EntityInputHandler, Focusable, Keystroke,
        WindowBounds, WindowHandle, WindowOptions, px, size,
    };

    fn highlight_on_enter() -> Plugin {
        gpdoc_core::unit_command!(HighlightSelection, "highlightSelection");
        Plugin::new("highlight-on-enter", |p| {
            p.command::<HighlightSelection>(|ctx, _| {
                if matches!(ctx.selection(), Selection::Text { anchor, head } if anchor != head) {
                    ctx.plan(gpdoc_plugin_highlight::Set(None))
                } else {
                    Ok(None)
                }
            });
            p.shortcut("enter", HighlightSelection);
        })
    }

    fn open(cx: &mut App, config: &EditorConfig) -> WindowHandle<EditorView> {
        let handle = cx
            .open_window(
                WindowOptions {
                    focus: false,
                    show: false,
                    ..Default::default()
                },
                |window, cx| {
                    let view = config
                        .open(
                            DocumentSource::Blocks(vec![Node::text("paragraph", "Sample text")]),
                            cx,
                        )
                        .unwrap();
                    window.focus(&view.read(cx).focus_handle(cx));
                    view
                },
            )
            .unwrap();
        handle
            .update(cx, |view, window, cx| {
                view.select_document(
                    Selection::Text {
                        anchor: DocPos(12),
                        head: DocPos(1),
                    },
                    window,
                    cx,
                )
                .unwrap();
            })
            .unwrap();
        handle
    }

    fn press(cx: &mut App, handle: WindowHandle<EditorView>, key: &str) -> bool {
        AnyWindowHandle::from(handle)
            .update(cx, |_, window, cx| {
                window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx)
            })
            .unwrap()
    }

    fn end_x(cx: &mut App, handle: WindowHandle<EditorView>) -> gpui::Pixels {
        handle
            .update(cx, |view, window, cx| {
                view.bounds_for_range(11..11, Bounds::default(), window, cx)
                    .unwrap()
                    .left()
            })
            .unwrap()
    }

    fn check_navigation(cx: &mut App) {
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(horizontal_rule().with_view(rule_line))
            .build()
            .unwrap();
        let latin = "sample text ".repeat(9);
        let hebrew = "אבגד";
        let rule = Node::leaf("horizontalRule", Default::default());
        let first = Node::text("paragraph", &latin);
        let last_start = DocPos(rule.size() + first.size() + 1);
        let last_end = DocPos(last_start.0 + hebrew.encode_utf16().count());
        let bounds = Bounds::centered(None, size(px(280.), px(400.)), cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    show: false,
                    ..Default::default()
                },
                |window, cx| {
                    let editor = config
                        .open(
                            DocumentSource::Blocks(vec![
                                rule.clone(),
                                first,
                                Node::text("paragraph", hebrew),
                                rule.clone(),
                            ]),
                            cx,
                        )
                        .unwrap();
                    window.focus(&editor.read(cx).focus_handle(cx));
                    editor
                },
            )
            .unwrap();

        let selection = |cx: &App| handle.read(cx).unwrap().editor().selection();
        let caret = |pos| Selection::Text {
            anchor: pos,
            head: pos,
        };
        let first_start = DocPos(rule.size() + 1);
        assert!(press(cx, handle, "cmd-up"));
        assert_eq!(selection(cx), caret(first_start));
        assert!(press(cx, handle, "shift-right"));

        // The leading rule contributes one projected newline. Find the soft wrap
        // using the shaped coordinates so the fixture works across system fonts.
        let (wrap, first_top, next_top) = handle
            .update(cx, |view, window, cx| {
                let mut top = |offset| {
                    view.bounds_for_range(offset..offset, Bounds::default(), window, cx)
                        .unwrap()
                        .top()
                };
                let first_top = top(1);
                let wrap = (2..latin.len())
                    .find(|&offset| top(offset) > first_top)
                    .expect("paragraph should wrap");
                (wrap, first_top, top(wrap))
            })
            .unwrap();
        let current_top = |cx: &mut App| {
            handle
                .update(cx, |view, window, cx| {
                    let selection = view.selected_text_range(false, window, cx).unwrap();
                    let offset = if selection.reversed {
                        selection.range.start
                    } else {
                        selection.range.end
                    };
                    view.bounds_for_range(offset..offset, Bounds::default(), window, cx)
                        .unwrap()
                        .top()
                })
                .unwrap()
        };
        let first_line_end = DocPos(first_start.0 + wrap - 1);
        assert!(press(cx, handle, "cmd-right"));
        assert_eq!(selection(cx), caret(first_line_end));
        assert_eq!(current_top(cx), first_top);
        assert!(press(cx, handle, "cmd-left"));
        assert_eq!(selection(cx), caret(first_start));
        assert!(press(cx, handle, "cmd-right"));
        assert!(press(cx, handle, "down"));
        assert_eq!(current_top(cx), next_top);

        assert!(press(cx, handle, "cmd-down"));
        assert_eq!(selection(cx), caret(last_end));
        assert!(press(cx, handle, "cmd-right"));
        assert_eq!(selection(cx), caret(last_start));
        assert!(press(cx, handle, "cmd-left"));
        assert_eq!(selection(cx), caret(last_end));

        assert!(press(cx, handle, "cmd-up"));
        assert!(press(cx, handle, "right"));
        let anchor = DocPos(first_start.0 + 1);
        assert_eq!(selection(cx), caret(anchor));
        for (key, head) in [
            ("cmd-shift-right", first_line_end),
            ("cmd-shift-left", first_start),
            ("cmd-shift-down", last_end),
            ("cmd-shift-up", first_start),
        ] {
            assert!(press(cx, handle, key));
            assert_eq!(selection(cx), Selection::Text { anchor, head }, "{key}");
            if head == first_line_end {
                assert_eq!(current_top(cx), first_top, "{key}");
            }
        }
        assert_eq!(handle.read(cx).unwrap().editor().revision(), 0);
        handle
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    }

    Application::new().run(|cx| {
        check_navigation(cx);
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(bold().with_view(gpdoc_plugin_bold::bold_format))
            .plugin(italic().with_view(gpdoc_plugin_italic::italic_format))
            .plugin(highlight_on_enter())
            .plugin(highlight())
            .build()
            .unwrap();
        let rich = open(cx, &config);
        let plain_config = EditorConfig::builder().plugin(paragraph()).build().unwrap();
        let plain = cx
            .open_window(
                WindowOptions {
                    show: false,
                    ..Default::default()
                },
                |window, cx| {
                    let editor = plain_config.open(DocumentSource::Empty, cx).unwrap();
                    window.focus(&editor.read(cx).focus_handle(cx));
                    editor
                },
            )
            .unwrap();
        let initial_x = end_x(cx, rich);
        assert!(press(cx, rich, "cmd-b"));
        assert_ne!(end_x(cx, rich), initial_x, "shaping must use the bold font");
        assert!(press(cx, rich, "cmd-shift-h"));
        let read = rich.read(cx).unwrap().editor();
        let (_, run) = read
            .document()
            .text_runs_in(DocPos(1)..DocPos(12))
            .next()
            .unwrap();
        assert_eq!(
            run.marks
                .iter()
                .map(|m| m.kind.as_str())
                .collect::<Vec<_>>(),
            ["bold", "highlight"]
        );
        assert!(press(cx, rich, "cmd-z"));
        assert!(press(cx, rich, "cmd-b"));
        assert_eq!(end_x(cx, rich), initial_x);
        assert_eq!(
            rich.read(cx).unwrap().editor().document().blocks(),
            &[Node::text("paragraph", "Sample text")]
        );

        assert!(!press(cx, plain, "cmd-b"));
        assert!(!press(cx, plain, "cmd-shift-h"));
        assert_eq!(plain.read(cx).unwrap().editor().revision(), 0);
        plain
            .update(cx, |editor, window, cx| {
                editor.replace_and_mark_text_in_range(None, "ㅎ", Some(1..1), window, cx);
                editor.replace_text_in_range(None, "한", window, cx);
                assert_eq!(
                    editor.editor().document().blocks(),
                    [Node::text("paragraph", "한")]
                );
            })
            .unwrap();
        assert!(press(cx, plain, "cmd-z"));
        assert_eq!(
            plain.read(cx).unwrap().editor().document().blocks(),
            [Node::text("paragraph", "")]
        );
        rich.update(cx, |view, window, cx| {
            assert!(matches!(
                view.run(gpdoc_plugin_italic::Toggle, window, cx).unwrap(),
                CommandOutcome::Applied(_)
            ));
        })
        .unwrap();
        assert!(press(cx, rich, "enter"));
        let doc = rich.read(cx).unwrap().editor().document();
        assert_eq!(doc.blocks().len(), 1);
        assert!(
            doc.text_runs_in(DocPos(1)..DocPos(12))
                .all(|(_, r)| r.marks.iter().any(|m| m.kind == "highlight"))
        );
        assert!(press(cx, rich, "right"));
        assert!(press(cx, rich, "enter"));
        assert_eq!(rich.read(cx).unwrap().editor().document().blocks().len(), 2);
        rich.update(cx, |view, window, cx| {
            view.replace_and_mark_text_in_range(None, "한", Some(0..1), window, cx);
        })
        .unwrap();
        assert!(press(cx, rich, "enter"));
        rich.update(cx, |view, window, _| {
            assert_eq!(
                view.editor().document().blocks()[1],
                Node::text("paragraph", "한")
            );
            window.remove_window();
        })
        .unwrap();
        plain
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        println!("Plugin shortcut dispatch, font geometry, history and config isolation passed.");
        cx.quit();
    });
}

#[cfg(not(target_os = "macos"))]
fn main() {}
