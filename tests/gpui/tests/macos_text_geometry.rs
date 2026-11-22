//! Run on the main thread so GPUI can shape and draw with the macOS text system.
//! The windows stay hidden and the test needs no keyboard, clipboard, or input source changes.

#[cfg(target_os = "macos")]
fn main() {
    use gpdoc::{DocumentSource, EditorConfig, PluginViewExt};
    use gpdoc_core::{CommandOutcome, DocPos, Node, Selection, Value};
    use gpdoc_plugin_paragraph::paragraph;
    use gpui::{
        App, Application, Bounds, EntityInputHandler, WindowBounds, WindowOptions, point, px, size,
    };

    fn caret_points(
        cx: &mut App,
        config: &EditorConfig,
        text: &str,
        offsets: impl IntoIterator<Item = usize>,
    ) -> Vec<gpui::Point<gpui::Pixels>> {
        measure(
            cx,
            config,
            vec![Node::text("paragraph", text)],
            text,
            offsets,
        )
    }

    fn measure(
        cx: &mut App,
        config: &EditorConfig,
        blocks: Vec<Node>,
        text: &str,
        offsets: impl IntoIterator<Item = usize>,
    ) -> Vec<gpui::Point<gpui::Pixels>> {
        let bounds = Bounds::centered(None, size(px(280.), px(400.)), cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    focus: false,
                    show: false,
                    ..Default::default()
                },
                |_, cx| config.open(DocumentSource::Blocks(blocks), cx).unwrap(),
            )
            .unwrap();
        handle
            .update(cx, |view, window, cx| {
                let points = offsets
                    .into_iter()
                    .map(|offset| {
                        let caret = view
                            .bounds_for_range(offset..offset, Bounds::default(), window, cx)
                            .expect("each tested offset is a valid text boundary");
                        let pos = point(caret.left(), caret.center().y);
                        assert_eq!(
                            view.character_index_for_point(pos, window, cx),
                            Some(offset),
                            "native hit test for {text:?} at UTF-16 offset {offset}"
                        );
                        pos
                    })
                    .collect();
                window.remove_window();
                points
            })
            .unwrap()
    }

    struct Assets;
    impl gpui::AssetSource for Assets {
        fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
            let rect = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect width="100" height="50" fill="red"/></svg>"#;
            Ok((path == "image-test.svg").then(|| std::borrow::Cow::Borrowed(rect.as_slice())))
        }
        fn list(&self, _: &str) -> gpui::Result<Vec<gpui::SharedString>> {
            Ok(vec![])
        }
    }
    Application::new().with_assets(Assets).run(|cx| {
        let resource = gpui::Resource::Embedded("image-test.svg".into());
        let (loading, _) = cx.fetch_asset::<gpui::ImgResourceLoader>(&resource);
        cx.spawn(async move |cx| {
            loading.await.expect("decode embedded image");
            cx.update(check_geometry).unwrap();
        })
        .detach();
    });
    fn check_geometry(cx: &mut App) {
        let config = EditorConfig::builder().plugin(paragraph()).build().unwrap();
        for (text, offsets) in [
            ("אבגד", 0..=4),
            ("كتب", 0..=3),
            ("سلام", 0..=4),
            // The interior offsets of the Hebrew run have unambiguous visual positions.
            ("ID אבגד 42", 4..=6),
        ] {
            let points = caret_points(cx, &config, text, offsets);
            assert!(
                points
                    .windows(2)
                    .all(|p| p[0].y == p[1].y && p[0].x > p[1].x),
                "RTL caret positions for {text:?}: {points:?}"
            );
        }

        // Only grapheme boundaries are legal stops in these samples.
        for (text, offsets) in [
            ("A😀B", [0, 1, 3, 4]),
            ("Aक्षिB", [0, 1, 5, 6]),
            ("Aกิ้B", [0, 1, 4, 5]),
            ("Aa\u{302}\u{301}B", [0, 1, 4, 5]),
        ] {
            let points = caret_points(cx, &config, text, offsets);
            assert!(
                points.windows(2).all(|p| p[0].x < p[1].x),
                "LTR caret positions for {text:?}: {points:?}"
            );
        }

        let text = "אבגד ".repeat(30);
        let points = caret_points(cx, &config, &text, 0..=text.encode_utf16().count());
        assert!(
            points.first().unwrap().y < points.last().unwrap().y,
            "fixture must wrap"
        );
        assert!(points.windows(2).all(|p| p[0].y <= p[1].y));
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(
                gpdoc_plugin_text_align::text_align(["paragraph"])
                    .with_view(|| gpdoc_plugin_text_align::text_align_format(["paragraph"])),
            )
            .plugin(
                gpdoc_plugin_text_style::text_style()
                    .with_view(|| gpdoc_plugin_text_style::text_style_format(["textStyle"])),
            )
            .plugins([
                gpdoc_plugin_text_style::font_size(),
                gpdoc_plugin_text_style::line_height(),
            ])
            .build()
            .unwrap();
        let mut editor = gpdoc::Editor::from_document(
            &config,
            vec![
                Node::text("paragraph", "AA"),
                Node::text("paragraph", "next"),
            ],
        )
        .unwrap();
        editor
            .select(Selection::Text {
                anchor: DocPos(2),
                head: DocPos(3),
            })
            .unwrap();
        for outcome in [
            editor.run(gpdoc_plugin_text_style::SetFontSize(Some("2em".into()))),
            editor.run(gpdoc_plugin_text_style::SetLineHeight(Some("64px".into()))),
            editor.run(gpdoc_plugin_text_align::Set("center".into())),
        ] {
            assert!(matches!(outcome.unwrap(), CommandOutcome::Applied(_)));
        }
        let points = measure(
            cx,
            &config,
            editor.document().blocks().to_vec(),
            "mixed-size centered paragraph",
            0..=3,
        );
        assert!(points[1].x - points[0].x < (points[2].x - points[1].x) * 0.65);
        assert!(points[0].x > points[3].x + px(30.));
        assert!(points[3].y - points[0].y > px(30.));
        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(gpdoc_plugin_image::image().with_view(gpdoc_plugin_image::image_content))
            .build()
            .unwrap();
        let media = Node::leaf(
            "image",
            [
                ("src".into(), Value::String("image-test.svg".into())),
                ("alt".into(), Value::String("pic".into())),
                ("width".into(), Value::from(96)),
                ("height".into(), Value::from(48)),
            ]
            .into(),
        );
        let image_points = measure(
            cx,
            &config,
            vec![media, Node::text("paragraph", "tail")],
            "image with declared width and height",
            [0, 3, 4],
        );
        assert!((image_points[1].x - image_points[0].x - px(96.)).abs() < px(1.));
        assert!(image_points[2].y > image_points[0].y + px(24.));

        let config = EditorConfig::builder()
            .plugin(paragraph())
            .plugin(
                gpdoc_plugin_ruby_text::ruby_text()
                    .with_view(gpdoc_plugin_ruby_text::ruby_text_format),
            )
            .build()
            .unwrap();
        let mut editor = gpdoc::Editor::from_document(
            &config,
            vec![
                Node::text("paragraph", "東京"),
                Node::text("paragraph", "次"),
            ],
        )
        .unwrap();
        editor
            .select(Selection::Text {
                anchor: DocPos(1),
                head: DocPos(3),
            })
            .unwrap();
        editor
            .run(gpdoc_plugin_ruby_text::Set {
                rt: Some("とうきょう".into()),
            })
            .unwrap();
        let ruby_points = measure(
            cx,
            &config,
            editor.document().blocks().to_vec(),
            "ruby annotation reserves vertical space",
            [0, 2, 3],
        );
        let plain_points = measure(
            cx,
            &config,
            vec![
                Node::text("paragraph", "東京"),
                Node::text("paragraph", "次"),
            ],
            "unannotated base",
            [0, 2, 3],
        );
        assert!(ruby_points[2].y - ruby_points[0].y > plain_points[2].y - plain_points[0].y);
        println!("Native caret geometry and hit tests passed.");
        cx.quit();
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Skipping the macOS text geometry test on this platform.");
}
