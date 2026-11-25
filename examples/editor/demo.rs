#[path = "block_views.rs"]
pub(crate) mod block_views;
#[path = "completion.rs"]
mod completion;
#[path = "completion_views.rs"]
pub(crate) mod completion_views;

#[path = "format_menu.rs"]
mod format_menu;
#[path = "menu_icons.rs"]
mod menu_icons;

pub(crate) fn load_fonts(cx: &mut gpui::App) {
    use std::borrow::Cow;

    let faces = gpdoc_demo_fonts::FACES.iter().copied();
    #[cfg(target_family = "wasm")]
    let faces = faces.chain([guic_gpui_assets::LILEX_REGULAR]);
    cx.text_system()
        .add_fonts(faces.map(Cow::Borrowed).collect())
        .expect("register editor fonts");
}

struct DocumentSurface(gpui::Entity<gpdoc::EditorView>);

impl gpui::Render for DocumentSurface {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{div, prelude::*, px, rgb};
        gpdoc::EditorSurface::new(&self.0)
            .size_full()
            .font_family(if cfg!(target_os = "macos") {
                ".SystemUIFont"
            } else {
                gpdoc_demo_fonts::FAMILY
            })
            .bg(rgb(0xffffff))
            .text_color(rgb(0x2c2c2c))
            .text_size(px(16.))
            .line_height(px(24.))
            .layout(|document| {
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    // The block handle occupies the other 20 px of the left margin.
                    .pl(px(4.))
                    .pr(px(24.))
                    .pt(px(44.))
                    .pb(px(96.))
                    .child(document.max_w(px(724.)))
            })
    }
}

pub(crate) struct DemoAssets;

impl gpui::AssetSource for DemoAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        let bytes = match path {
            "session-plan.png" => Some(include_bytes!("session-plan.png").as_slice()),
            id => menu_icons::ICONS
                .iter()
                .find(|(key, _)| *key == id)
                .map(|(_, icon)| icon.svg_bytes()),
        };
        Ok(bytes.map(std::borrow::Cow::Borrowed))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<gpui::SharedString>> {
        Ok(if path.is_empty() {
            std::iter::once("session-plan.png")
                .chain(menu_icons::ICONS.iter().map(|(key, _)| *key))
                .map(Into::into)
                .collect()
        } else {
            vec![]
        })
    }
}

fn editor_config() -> gpdoc::EditorConfig {
    use gpdoc::PluginViewExt;
    use gpdoc_plugin_mention::mention;
    use gpdoc_plugin_placeholder::placeholder;
    use gpdoc_plugin_slash_command::slash_commands;

    gpdoc::EditorConfig::builder()
        .plugin(gpdoc_plugin_paragraph::paragraph())
        .plugin(gpdoc_plugin_bold::bold())
        .plugin(gpdoc_plugin_italic::italic())
        .plugin(gpdoc_plugin_underline::underline())
        .plugin(gpdoc_plugin_strike::strike())
        .plugin(gpdoc_plugin_hard_break::hard_break())
        .plugin(gpdoc_plugin_bullet_list::bullet_list())
        .plugin(gpdoc_plugin_ordered_list::ordered_list())
        .plugin(gpdoc_plugin_list_keymap::list_keymap())
        .plugin(gpdoc_plugin_horizontal_rule::horizontal_rule())
        .plugin(gpdoc_plugin_subscript::subscript())
        .plugin(gpdoc_plugin_superscript::superscript())
        .plugin(gpdoc_plugin_task_list::task_list())
        .plugin(gpdoc_plugin_image::image())
        .plugin(gpdoc_plugin_mathematics::mathematics().with_view(|| {
            gpdoc_plugin_mathematics::formula_content().node_format("blockMath", |_, format| {
                format.alignment = gpdoc_core::TextAlignment::Center;
            })
        }))
        .plugin(gpdoc_plugin_inline_code::inline_code().with_view(|| {
            gpdoc::ViewPlugin::named("inlineCode").mark_format("code", |_, style| {
                style.font_family = Some(block_views::MONOSPACE.into());
                style.font_size = Some(gpdoc_core::TextLength::Em(0.875));
                style.color = Some([199, 63, 58, 255]);
                style.background = Some([245, 244, 242, 255]);
            })
        }))
        .plugin(gpdoc_plugin_task_list::task_item().with_view(block_views::tasks))
        .plugin(
            gpdoc_plugin_details::details_with_persistence(true).with_view(block_views::details),
        )
        .plugin(gpdoc_plugin_audio::audio(Default::default()).with_view(block_views::audio))
        .plugin(gpdoc_plugin_table::table().with_view(|| {
            gpdoc_plugin_table::table_grid(
                block_views::table_cell,
                gpdoc_plugin_table::default_resize_guide,
            )
        }))
        .plugin(gpdoc_plugin_link::link(Default::default()).with_view(|| {
            [
                gpdoc_plugin_link::link_format(),
                gpdoc_plugin_link::link_activation(),
            ]
        }))
        .plugin(gpdoc_plugin_heading::heading().with_view(block_views::heading))
        .plugin(gpdoc_plugin_blockquote::blockquote().with_view(block_views::quote))
        .plugin(gpdoc_plugin_code_block::code_block().with_view(block_views::code))
        .plugin(gpdoc_plugin_list_item::list_item().with_view(block_views::list_item))
        .plugin(
            gpdoc_plugin_highlight::highlight()
                .with_view(|| gpdoc_plugin_highlight::highlight_format([250, 243, 227])),
        )
        .plugin(placeholder("Write a note, or type / for blocks…"))
        .plugin(gpdoc_plugin_box_selection::box_selection())
        .plugin(gpdoc_plugin_node_range::node_range())
        .plugin(gpdoc_plugin_gap_cursor::gap_cursor())
        .plugin(
            gpdoc_plugin_drop_cursor::drop_cursor().with_view(|bounds, window, _| {
                window.paint_quad(gpui::fill(bounds, gpui::rgb(0x2c2c2c)))
            }),
        )
        .plugin(mention().with_view(completion_views::mention_menu))
        .plugin(slash_commands(completion::commands()).with_view(completion_views::slash_menu))
        .plugin(format_menu::format_menu())
        .plugin(block_views::block_spacing())
        .plugin(block_views::drag_handle())
        .build()
        .expect("configure demo editor")
}

pub fn run(app: gpui::Application, setup: impl FnOnce(&mut gpui::App) + 'static) {
    use gpdoc::DocumentSource;
    use gpui::{
        AppContext, Bounds, Focusable, TitlebarOptions, WindowBounds, WindowOptions, px, size,
    };

    app.with_assets(DemoAssets).run(move |cx| {
        load_fonts(cx);
        setup(cx);
        let config = editor_config();
        let bounds = Bounds::centered(None, size(px(960.), px(760.)), cx);
        let editor = config
            .open(DocumentSource::Json(include_str!("kickoff.json")), cx)
            .expect("open demo document");
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("gpdoc".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| DocumentSurface(editor.clone())),
            )
            .expect("open editor window");
        window
            .update(cx, |_, window, cx| {
                #[cfg(target_family = "wasm")]
                window.focus(&editor.read(cx).focus_handle(cx), cx);
                #[cfg(not(target_family = "wasm"))]
                window.focus(&editor.read(cx).focus_handle(cx));
                cx.activate(true);
            })
            .expect("focus editor");
    });
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn every_slash_command_has_an_icon() {
        use gpui::AssetSource;

        for command in super::completion::commands() {
            assert!(
                super::DemoAssets.load(command.id).unwrap().is_some(),
                "missing {} icon",
                command.id
            );
        }
    }

    #[test]
    fn kickoff_contains_every_registered_node() {
        let config = super::editor_config();
        let editor = gpdoc::Editor::from_json(&config, include_str!("kickoff.json")).unwrap();
        let doc = editor.document();
        let kinds: std::collections::HashSet<_> = std::iter::once(doc.root().kind.as_str())
            .chain(doc.iter_nodes().map(|(_, node)| node.kind.as_str()))
            .collect();
        for node in config.schema().node_types() {
            assert!(
                kinds.contains(node.name.as_str()),
                "missing {} example",
                node.name
            );
        }
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn task_checkbox_padding_toggles_completion() {
        use gpdoc::{DocPos, DocumentSource, Node};
        use gpui::{AppContext, TestAppContext, VisualTestContext, point, px};

        let app = TestAppContext::single();
        let editor = app.update(|cx| {
            super::editor_config()
                .open(
                    DocumentSource::Blocks(vec![Node::container(
                        "taskList",
                        vec![Node::container(
                            "taskItem",
                            vec![Node::text("paragraph", "Review")],
                        )],
                    )]),
                    cx,
                )
                .unwrap()
        });
        let window = app.update(|cx| {
            cx.open_window(Default::default(), |_, cx| {
                cx.new(|_| super::DocumentSurface(editor.clone()))
            })
            .unwrap()
        });
        let cx = VisualTestContext::from_window(*window, &app).into_mut();
        cx.run_until_parked();
        let bounds = cx.debug_bounds("gpdoc-task-checkbox-1").unwrap();
        for checked in [true, false] {
            cx.simulate_click(
                point(bounds.left() + px(1.), bounds.center().y),
                Default::default(),
            );
            cx.run_until_parked();
            cx.update(|_, cx| {
                let node = editor
                    .read(cx)
                    .editor()
                    .document()
                    .node_at(DocPos(1))
                    .unwrap();
                assert_eq!(node.attrs["checked"].as_bool(), Some(checked));
            });
        }
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn drag_selection_and_toggle_bold() {
        use gpdoc::{DocumentSource, Node};
        use gpui::{
            AppContext, Bounds, EntityInputHandler, MouseButton, TestAppContext, VisualTestContext,
            point,
        };

        let app = TestAppContext::single();
        let config = super::editor_config();
        let editor = app.update(|cx| {
            config
                .open(
                    DocumentSource::Blocks(vec![Node::text("paragraph", "Meeting notes")]),
                    cx,
                )
                .unwrap()
        });
        let window = app.update(|cx| {
            cx.open_window(Default::default(), |_, cx| {
                cx.new(|_| super::DocumentSurface(editor.clone()))
            })
            .unwrap()
        });
        let cx = VisualTestContext::from_window(*window, &app).into_mut();
        cx.run_until_parked();
        let [start, end] = [0, 7].map(|offset| {
            cx.update(|window, cx| {
                editor.update(cx, |view, cx| {
                    let bounds = view
                        .bounds_for_range(offset..offset, Bounds::default(), window, cx)
                        .unwrap();
                    point(bounds.left(), bounds.center().y)
                })
            })
        });
        cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
        app.background_executor
            .advance_clock(super::format_menu::DRAG_DELAY);
        cx.run_until_parked();
        assert!(cx.debug_bounds("format-bold").is_none());
        cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
        cx.run_until_parked();
        let tick = std::time::Duration::from_millis(1);
        app.background_executor
            .advance_clock(super::format_menu::DRAG_DELAY - tick);
        cx.run_until_parked();
        assert!(cx.debug_bounds("format-bold").is_none());
        app.background_executor.advance_clock(tick);
        cx.run_until_parked();
        let bold = cx.debug_bounds("format-bold").unwrap();
        assert!(bold.bottom() <= start.y);
        cx.simulate_click(bold.center(), Default::default());
        cx.update(|window, cx| {
            editor.update(cx, |view, cx| {
                assert_eq!(
                    view.selected_text_range(false, window, cx).unwrap().range,
                    0..7
                );
                assert!(view.editor().mark_active("bold").unwrap());
            })
        });
        cx.simulate_click(bold.center(), Default::default());
        cx.update(|_, cx| assert!(!editor.read(cx).editor().mark_active("bold").unwrap()));
        cx.simulate_click(end, Default::default());
        cx.run_until_parked();
        cx.update(|_, cx| assert!(matches!(editor.read(cx).editor().selection(), gpdoc::Selection::Text {anchor, head} if anchor == head)));
    }
}
