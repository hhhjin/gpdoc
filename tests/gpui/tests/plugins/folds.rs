use super::open_editor;
use gpdoc::{DocumentSource, EditorConfig, PluginViewExt};
use gpdoc_core::{DocPos, Node, Selection};
use gpdoc_plugin_details::{DetailsSummary, details, disclosure};
use gpdoc_plugin_paragraph::paragraph;
use gpui::{Bounds, EntityInputHandler, Focusable, TestAppContext};

#[test]
fn keyboard_edits_folded_content() {
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(details().with_view(|| disclosure(DetailsSummary::render)))
        .build()
        .unwrap();
    let summary = Node::text("detailsSummary", "Summary");
    let summary_end = DocPos(summary.size());
    let blocks = vec![Node::container(
        "details",
        vec![
            summary,
            Node::container("detailsContent", vec![Node::text("paragraph", "Body")]),
        ],
    )];
    let mut app = TestAppContext::single();
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(blocks));
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            view.select_document(Selection::caret(DocPos(summary_end.0 + 3)), window, cx)
                .unwrap();
        });
        assert_eq!(
            entity.read(cx).editor().selection(),
            Selection::caret(summary_end)
        )
    });
    cx.update(|window, cx| window.focus(&entity.read(cx).focus_handle(cx)));
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            assert_eq!(
                view.text_for_range(0..80, &mut None, window, cx).as_deref(),
                Some("Summary\n")
            );
        })
    });
    cx.simulate_keystrokes("secondary-z");
    let handle = cx
        .debug_bounds("gpdoc-disclosure-1")
        .expect("summary disclosure")
        .center();
    cx.simulate_click(handle, Default::default());
    cx.simulate_keystrokes("enter");
    let start = cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            assert_eq!(
                view.text_for_range(0..80, &mut None, window, cx).as_deref(),
                Some("Summary\n\nBody")
            );
            view.bounds_for_range(0..0, Bounds::default(), window, cx)
                .unwrap()
                .center()
        })
    });
    cx.simulate_click(start, Default::default());
    cx.simulate_keystrokes("backspace");
    cx.update(|_, cx| assert_eq!(entity.read(cx).editor().document().blocks().len(), 3));
}

#[test]
fn nested_disclosures_align_and_keep_the_body_editable() {
    let mut app = TestAppContext::single();
    let config = EditorConfig::builder()
        .plugin(paragraph())
        .plugin(details().with_view(|| disclosure(DetailsSummary::render)))
        .build()
        .unwrap();
    let section = |title, content| {
        Node::container(
            "details",
            vec![
                Node::text("detailsSummary", title),
                Node::container("detailsContent", vec![content]),
            ],
        )
    };
    let blocks = vec![section(
        "Outer",
        section("Inner", Node::text("paragraph", "Value")),
    )];
    let (entity, cx) = open_editor(&mut app, &config, DocumentSource::Blocks(blocks));
    let outer = cx.debug_bounds("gpdoc-disclosure-1").unwrap();
    assert!(cx.debug_bounds("gpdoc-disclosure-10").is_none());
    cx.simulate_click(outer.center(), Default::default());
    let inner = cx
        .debug_bounds("gpdoc-disclosure-10")
        .expect("inner summary");
    assert_eq!(
        outer.left(),
        inner.left(),
        "the outer button only indents its summary"
    );
    cx.simulate_click(inner.center(), Default::default());
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| {
            let body = view
                .bounds_for_range(12..12, Bounds::default(), window, cx)
                .unwrap();
            assert_eq!(body.left(), inner.left());
            view.replace_and_mark_text_in_range(Some(12..12), "한", Some(1..1), window, cx);
            assert_eq!(view.marked_text_range(window, cx), Some(12..13));
            view.unmark_text(window, cx);
            assert_eq!(
                view.text_for_range(0..100, &mut None, window, cx)
                    .as_deref(),
                Some("Outer\nInner\n한Value")
            );
        })
    });
}
