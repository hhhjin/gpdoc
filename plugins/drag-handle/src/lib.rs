//! Drag top-level blocks or a selected block range by its gutter handle.
use gpdoc::{EditorView, ViewPlugin};
use gpdoc_plugin::core::{DocPos, ReplaceAroundStep, Selection, Step};
use gpui::{
    AnyElement, App, Context, Entity, IntoElement, MouseButton, Window, div, prelude::*, px,
};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::rc::Rc;

/// Configure the native drag gesture for top-level blocks.
pub fn drag_handle() -> DragHandle {
    DragHandle
}

#[must_use]
pub struct DragHandle;

/// A grip in the editor gutter or its preview following the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragHandlePart {
    Grip,
    Preview,
}

pub struct DragHandleSlot {
    pub editor: Entity<EditorView>,
    pub before: DocPos,
    pub part: DragHandlePart,
}

type Renderer = Rc<dyn Fn(DragHandleSlot, &mut Window, &mut App) -> AnyElement>;

/// Display the same grip glyph in the gutter and in the native drag preview.
pub fn block_grip(_: DragHandleSlot, _: &mut Window, _: &mut App) -> gpui::Div {
    div().child("⠿")
}

impl From<DragHandle> for gpdoc::EditorPlugin {
    fn from(plugin: DragHandle) -> Self {
        plugin.with_view(block_grip).into()
    }
}

impl DragHandle {
    /// Compose the grip and preview. The plugin owns focus, hit testing, and the move transaction.
    pub fn with_view<E: IntoElement>(
        self,
        render: impl Fn(DragHandleSlot, &mut Window, &mut App) -> E + 'static,
    ) -> ViewPlugin {
        let render: Renderer =
            Rc::new(move |slot, window, cx| render(slot, window, cx).into_any_element());
        ViewPlugin::named("dragHandle")
            .wrap_blocks(move |block, body, window, cx| {
                let before = block.before;
                let view = block.view;
                let composing = view.read(cx).is_composing();
                let drag = BlockDrag::new(&view, before, render.clone(), cx);
                let handle = div()
                    .id(("gpdoc-drag", before.0))
                    .debug_selector(move || format!("gpdoc-drag-{}", before.0))
                    .min_w(px(16.))
                    .min_h(px(24.))
                    .cursor_move()
                    .child(drag.render_part(DragHandlePart::Grip, window, cx))
                    .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()));

                div()
                    .flex()
                    .items_start()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex_none()
                            .when(composing, |el| el.invisible())
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                view.update(cx, |editor, cx| editor.focus_in(window, cx));
                                cx.stop_propagation();
                            })
                            .child(handle),
                    )
                    .child(div().flex_1().min_w_0().child(body))
            })
            .on_block_drop(move_blocks)
    }
}

#[derive(Clone)]
struct BlockDrag {
    editor: Entity<EditorView>,
    before: DocPos,
    render: Renderer,
    revision: u64,
    selection: Selection,
}

impl BlockDrag {
    /// Capture the block or selected block range at the start of a native drag.
    fn new(view: &Entity<EditorView>, before: DocPos, render: Renderer, cx: &App) -> Self {
        let editor = view.read(cx).editor();
        let selection = match editor.selection() {
            range @ Selection::NodeRange { .. }
                if range.bounds(editor.document()).contains(&before) =>
            {
                range
            }
            _ => Selection::Node { before },
        };
        Self {
            editor: view.clone(),
            before,
            render,
            selection,
            revision: editor.revision(),
        }
    }

    fn render_part(&self, part: DragHandlePart, window: &mut Window, cx: &mut App) -> AnyElement {
        (self.render)(
            DragHandleSlot {
                editor: self.editor.clone(),
                before: self.before,
                part,
            },
            window,
            cx,
        )
    }
}

impl Render for BlockDrag {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_part(DragHandlePart::Preview, window, cx)
    }
}

fn move_blocks(
    view: &mut EditorView,
    drag: &BlockDrag,
    target: DocPos,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> bool {
    if drag.editor.entity_id() != cx.entity_id() || view.editor().revision() != drag.revision {
        return false;
    }
    let doc = view.editor().document();
    let range = drag.selection.bounds(doc);
    if (range.start..=range.end).contains(&target) {
        return false;
    }
    let width = range.end.0 - range.start.0;
    let Ok(slice) = doc.root().slice(range.start.0, range.end.0, false) else {
        return false;
    };
    // Carry the intervening siblings through the gap, preserving their mapped positions.
    let (gap_from, gap_to, insert, new_at) = if target > range.end {
        (range.end, target, 0, DocPos(target.0 - width))
    } else {
        (target, range.start, width, target)
    };
    let mut tx = view.editor().transaction();
    tx.push(Step::ReplaceAround(ReplaceAroundStep {
        from: range.start.min(target).0,
        to: range.end.max(target).0,
        gap_from: gap_from.0,
        gap_to: gap_to.0,
        insert,
        slice,
        structure: false,
    }));
    tx.select(match drag.selection {
        Selection::NodeRange { anchor, head } if anchor > head => Selection::NodeRange {
            anchor: DocPos(new_at.0 + width),
            head: new_at,
        },
        Selection::NodeRange { .. } => Selection::NodeRange {
            anchor: new_at,
            head: DocPos(new_at.0 + width),
        },
        _ => Selection::Node { before: new_at },
    });
    view.apply_node_view(tx, window, cx).is_ok()
}
