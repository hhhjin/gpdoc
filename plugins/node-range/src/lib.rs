//! Select sibling blocks with Shift+Up/Down or a primary-modifier drag.
use gpdoc::{EditorView, ViewBehavior, ViewPlugin, primary_pressed};
use gpdoc_plugin::core::{DocPos, Selection};
use gpui::{Context, Keystroke, Modifiers, MouseUpEvent, Window};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;

pub fn node_range() -> ViewPlugin {
    ViewPlugin::new("nodeRange", || NodeRange)
}

struct NodeRange;
impl ViewBehavior for NodeRange {
    fn key_down(
        &self,
        view: &mut EditorView,
        stroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        if stroke.modifiers
            == (Modifiers {
                shift: true,
                ..Default::default()
            })
        {
            return match stroke.key.as_str() {
                "up" => extend_blocks(view, false, window, cx),
                "down" => extend_blocks(view, true, window, cx),
                _ => false,
            };
        }
        if stroke.key == "a"
            && primary_pressed(stroke.modifiers, cx)
            && !stroke.modifiers.alt
            && !stroke.modifiers.shift
            && !stroke.modifiers.function
            && !(stroke.modifiers.platform && stroke.modifiers.control)
        {
            let head = DocPos(view.editor().document().size());
            return view
                .select_document(
                    Selection::NodeRange {
                        anchor: DocPos(0),
                        head,
                    },
                    window,
                    cx,
                )
                .is_ok();
        }
        false
    }

    fn mouse_up(
        &self,
        view: &mut EditorView,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        if view.is_dragging_selection()
            && primary_pressed(event.modifiers, cx)
            && let Selection::Text { anchor, head } = view.editor().selection()
        {
            let doc = view.editor().document();
            if doc.text_at(anchor).ok().map(|(pos, _)| pos)
                != doc.text_at(head).ok().map(|(pos, _)| pos)
            {
                return extend_blocks(view, anchor < head, window, cx);
            }
        }
        false
    }
}
fn extend_blocks(
    view: &mut EditorView,
    forward: bool,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> bool {
    if view.is_composing() {
        return false;
    }
    let doc = view.editor().document();
    let selection = view.editor().selection();
    let span = selection.bounds(doc);
    let root = doc.root();
    let Ok(from) = root.resolve(span.start.0) else {
        return false;
    };
    let Ok(to) = root.resolve(span.end.0) else {
        return false;
    };
    let (anchor, head) = match selection {
        Selection::NodeRange { anchor, head } => {
            let Ok(pos) = root.resolve(head.0) else {
                return false;
            };
            let mut index = pos.index(pos.depth());
            let end = pos.parent().child_count();
            if forward {
                index = (index + 1).min(end);
            } else {
                index = index.saturating_sub(1);
            }
            let mut target = DocPos(pos.pos_at_index(index, pos.depth()));
            if target == anchor {
                // Reversing a one-block selection steps across the anchor to the previous sibling.
                if forward {
                    index = (index + 1).min(end);
                } else {
                    index = index.saturating_sub(1);
                }
                target = DocPos(pos.pos_at_index(index, pos.depth()));
                if target == anchor {
                    return true;
                }
            }
            (anchor, target)
        }
        _ => {
            let Some(range) = from.block_range(&to, view.editor().schema(), |_| true) else {
                return false;
            };
            let start = DocPos(range.start());
            let end = DocPos(range.end());
            if forward { (start, end) } else { (end, start) }
        }
    };
    view.select_document(Selection::NodeRange { anchor, head }, window, cx)
        .is_ok()
}
