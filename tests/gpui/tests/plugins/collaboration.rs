use super::open_editor;
use gpdoc::{DecorationFrame, DocumentSource, PaintLayer, PluginViewExt, ViewBehavior, ViewPlugin};
use gpdoc_core::{DocPos, Node, Selection};
use gpdoc_plugin_collaboration::{Collaboration, CollaborationView, StateVector};
use gpdoc_plugin_collaboration_caret::{
    CollaborationCaret, User, collaboration_carets, paint_remote_caret,
};
use gpdoc_plugin_paragraph::paragraph;
use gpui::{Bounds, EntityInputHandler, Focusable, Pixels, TestAppContext, div, prelude::*, px};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Default)]
struct PeerGeometry {
    selection: Rc<Cell<Option<Selection>>>,
    measured: Rc<Cell<Option<(Bounds<Pixels>, Bounds<Pixels>)>>>,
}

impl ViewBehavior for PeerGeometry {
    fn paint(
        &self,
        layer: PaintLayer,
        frame: &DecorationFrame<'_>,
        _: &mut gpui::Window,
        _: &mut gpui::App,
    ) {
        if layer != PaintLayer::Background {
            return;
        }
        if let Some(Selection::Text { anchor, head }) = self.selection.get()
            && let Some(caret) = frame.bounds_for_range(head..head)
            && let Some(span) = frame
                .rects_for_range(anchor.min(head)..anchor.max(head))
                .next()
        {
            self.measured.set(Some((caret, span)));
        }
    }
}

#[test]
fn edits_and_carets_follow_the_shared_session() {
    let geometry = PeerGeometry::default();
    let awareness = Rc::new(RefCell::new(None::<CollaborationCaret>));
    let config = gpdoc::EditorConfig::builder()
        .plugin(paragraph())
        .plugin(gpdoc_plugin_blockquote::blockquote().with_view(|| {
            ViewPlugin::named("indentedQuote").wrap_node("blockquote", |_, content, _, _| {
                div()
                    .debug_selector(|| "peer-quote".into())
                    .pl(px(48.))
                    .pt(px(20.))
                    .child(content)
            })
        }))
        .plugin(
            collaboration_carets({
                let selection = geometry.selection.clone();
                let awareness = awareness.clone();
                move |session| {
                    let peers = awareness.borrow().as_ref().unwrap().peers(session);
                    selection.set(peers.first().and_then(|peer| peer.selection));
                    peers
                }
            })
            .with_view(paint_remote_caret),
        )
        .plugin(ViewPlugin::new("peerGeometryProbe", {
            let geometry = geometry.clone();
            move || geometry.clone()
        }))
        .build()
        .unwrap();
    let mut app = TestAppContext::single();
    let (view, cx) = open_editor(
        &mut app,
        &config,
        DocumentSource::Blocks(vec![Node::container(
            "blockquote",
            vec![Node::text("paragraph", "abc")],
        )]),
    );
    let sent = Rc::new(RefCell::new(vec![]));
    let mut peer = cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            let local = Collaboration::new(v.editor().fork(), "default").unwrap();
            let mut peer = Collaboration::from_update(
                local.editor().config(),
                "default",
                &local.update_since(&StateVector::default()),
            )
            .unwrap();
            peer.select(Selection::Text {
                anchor: DocPos(3),
                head: DocPos(5),
            })
            .unwrap();
            let mut other = CollaborationCaret::new(&peer);
            let user = User::from_iter([
                ("name".into(), "Bob".into()),
                ("color".into(), "#468acf".into()),
            ]);
            let mut provider = CollaborationCaret::new(&local);
            provider
                .receive(&other.publish(&peer, user, true).unwrap())
                .unwrap();
            *awareness.borrow_mut() = Some(provider);
            let outbound = sent.clone();
            v.bind_collaboration(
                local,
                move |update| outbound.borrow_mut().push(update),
                window,
                cx,
            )
            .unwrap();
            window.focus(&v.focus_handle(cx));
            window.refresh();
            peer
        })
    });
    cx.run_until_parked();
    let (caret, span) = geometry
        .measured
        .get()
        .expect("resolved peer geometry during paint");
    let wrapper = cx.debug_bounds("peer-quote").unwrap();
    assert!(span.left() >= wrapper.left() + px(48.) && span.top() >= wrapper.top() + px(20.));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let native_caret = view
                .bounds_for_range(3..3, Bounds::default(), window, cx)
                .unwrap();
            let native_span = view
                .bounds_for_range(1..3, Bounds::default(), window, cx)
                .unwrap();
            assert_eq!(caret, native_caret);
            assert_eq!(span, native_span);
        });
    });
    cx.simulate_input("X");
    for update in sent.borrow_mut().drain(..) {
        peer.apply_update(&update).unwrap();
    }
    assert_eq!(peer.editor().document().root().text_content(), "Xabc");
    peer.select(Selection::caret(DocPos(6))).unwrap();
    let (_, remote) = peer.edit(|ed| ed.insert_text("!")).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.receive_collaboration_update(remote, window, cx).unwrap()
        })
    });
    cx.simulate_keystrokes("secondary-z");
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).editor().document().root().text_content(),
            "abc!"
        )
    });
    for update in sent.borrow_mut().drain(..) {
        peer.apply_update(&update).unwrap();
    }
    assert_eq!(peer.editor().document().root().text_content(), "abc!");
    cx.simulate_keystrokes("secondary-shift-z");
    for update in sent.borrow_mut().drain(..) {
        peer.apply_update(&update).unwrap();
    }
    assert_eq!(peer.editor().document().root().text_content(), "Xabc!");
    cx.simulate_keystrokes("secondary-z");
    for update in sent.borrow_mut().drain(..) {
        peer.apply_update(&update).unwrap();
    }
    assert_eq!(peer.editor().document().root().text_content(), "abc!");
    peer.select(Selection::caret(DocPos(6))).unwrap();
    let (_, concurrent) = peer.edit(|ed| ed.insert_text("?")).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.replace_and_mark_text_in_range(Some(0..0), "한", Some(1..1), window, cx);
            v.receive_collaboration_update(concurrent, window, cx)
                .unwrap();
            assert_eq!(v.editor().document().root().text_content(), "abc!");
            assert!(sent.borrow().is_empty());
            v.unmark_text(window, cx);
            assert_eq!(v.editor().document().root().text_content(), "한abc!?");
        })
    });
    for update in sent.borrow_mut().drain(..) {
        peer.apply_update(&update).unwrap();
    }
    assert_eq!(peer.editor().document().root().text_content(), "한abc!?");

    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.replace_and_mark_text_in_range(Some(0..0), "ㅎ", Some(1..1), window, cx);
            let other_config = gpdoc_core::EditorConfig::builder()
                .plugins([
                    gpdoc_plugin_paragraph::paragraph(),
                    gpdoc_plugin_bold::bold(),
                ])
                .build()
                .unwrap();
            let other =
                gpdoc::Editor::from_document(&other_config, vec![Node::text("paragraph", "other")])
                    .unwrap();
            assert!(
                v.bind_collaboration(
                    Collaboration::new(other, "default").unwrap(),
                    |_| {},
                    window,
                    cx,
                )
                .is_err()
            );
            assert!(v.is_composing());
            assert_eq!(
                v.collaboration()
                    .unwrap()
                    .editor()
                    .document()
                    .root()
                    .text_content(),
                "한abc!?"
            );

            let next = gpdoc::Editor::from_document(
                v.editor().config(),
                vec![Node::text("paragraph", "next")],
            )
            .unwrap();
            v.bind_collaboration(
                Collaboration::new(next, "default").unwrap(),
                |_| {},
                window,
                cx,
            )
            .unwrap();
            assert!(!v.is_composing());
            v.unmark_text(window, cx);
            assert_eq!(v.editor().document().root().text_content(), "next");
            assert!(
                sent.borrow().is_empty(),
                "the old IME preview was never sent"
            );
        })
    });
}
