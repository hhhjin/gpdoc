//! Move blocks and sibling ranges using one editor-wide gutter handle.
mod movement;
use gpdoc::{AnchoredElement, BlockDropTarget, EditorView, ViewBehavior, ViewFrame, ViewPlugin};
use gpdoc_plugin::core::{DocPos, Editor, Selection, Transaction};
use gpui::{
    AnyElement, App, Bounds, Context, Entity, IntoElement, MouseButton, MouseMoveEvent, Pixels,
    Point, Window, div, point, prelude::*, px,
};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{cell::Cell, rc::Rc};

/// Configure the native drag gesture for document blocks.
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
        ViewPlugin::new("dragHandle", move || GutterHandle {
            render: render.clone(),
            active: Cell::new(None),
        })
        .wrap_blocks(|_, body, _, _| div().pl(px(GUTTER)).min_h(px(GRIP_HEIGHT)).child(body))
        .on_block_drop(move_blocks)
    }
}

// Reserve the gutter once at the document edge, independent of nesting depth.
const GUTTER: f32 = 20.;
const GRIP_HEIGHT: f32 = 24.;

struct GutterHandle {
    render: Renderer,
    active: Cell<Option<ActiveGrip>>,
}

#[derive(Clone, Copy)]
struct ActiveGrip {
    root: DocPos,
    owner: DocPos,
    revision: u64,
    // Includes the space to cross from the block's left edge to the handle.
    approach: Bounds<Pixels>,
}

impl GutterHandle {
    fn target(
        editor: &Editor,
        hit: Option<DocPos>,
        mouse: Point<Pixels>,
        active: Option<ActiveGrip>,
    ) -> Option<(DocPos, DocPos)> {
        let doc = editor.document();
        let active = active
            .filter(|grip| grip.revision == editor.revision() && grip.approach.contains(&mouse));
        // A 24px grip can extend below a short line into the next block's gutter.
        // Its visible hitbox still belongs to the block that placed it.
        let hit = match active {
            Some(grip) if mouse.x < grip.approach.left() + px(GUTTER) => grip.owner,
            _ => hit?,
        };
        let path = doc.resolve(hit).ok()?;
        let root = DocPos(path.before(1).ok()?);
        if let Some(grip) = active
            && (hit.0..hit.0 + doc.node_at(hit)?.size()).contains(&grip.owner.0)
        {
            return Some((root, grip.owner));
        }
        Some((root, editor.config().drag_handle_owner(&path)?))
    }

    fn grip(&self, frame: &ViewFrame<'_>, mouse: Point<Pixels>, cx: &App) -> Option<ActiveGrip> {
        let view = frame.view.read(cx);
        if view.is_composing() || cx.has_active_drag() || !frame.clip.contains(&mouse) {
            return None;
        }
        let active = self.active.get().and_then(|mut grip| {
            grip.approach = Self::approach(frame, grip.root, grip.owner)?;
            Some(grip)
        });
        let (root, owner) = Self::target(view.editor(), frame.block_at(mouse), mouse, active)?;
        Some(ActiveGrip {
            root,
            owner,
            revision: view.editor().revision(),
            approach: Self::approach(frame, root, owner)?,
        })
    }

    fn approach(frame: &ViewFrame<'_>, root: DocPos, owner: DocPos) -> Option<Bounds<Pixels>> {
        let gutter = frame.block_bounds(root)?.left();
        let block = frame.block_bounds(owner)?;
        let top = frame.block_content_top(owner)?;
        Some(Bounds::from_corners(
            point(gutter, top),
            point(block.left().max(gutter + px(GUTTER)), top + px(GRIP_HEIGHT)),
        ))
    }
}

impl ViewBehavior for GutterHandle {
    fn mouse_move(
        &self,
        view: &mut EditorView,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        if event.pressed_button.is_none() && !view.is_composing() {
            let hit = view
                .child_bounds_at(event.position, window)
                .map(|(hit, _)| hit);
            let hovered = Self::target(view.editor(), hit, event.position, self.active.get())
                .map(|(_, owner)| owner);
            if hovered != self.active.get().map(|grip| grip.owner) {
                cx.notify();
            }
        }
        false
    }

    fn render(
        self: Rc<Self>,
        frame: &ViewFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnchoredElement> {
        let grip = self.grip(frame, window.mouse_position(), cx);
        self.active.set(grip);
        let grip = grip?;
        let view = frame.view.clone();
        let drag = BlockDrag::new(&view, grip.owner, self.render.clone(), cx);
        let handle = div()
            .id(("gpdoc-drag-handle", view.entity_id()))
            .debug_selector(|| "gpdoc-drag-handle".into())
            .w(px(GUTTER))
            .h(px(GRIP_HEIGHT))
            .cursor_move()
            .child(drag.render_part(DragHandlePart::Grip, window, cx))
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()));
        let handle = div()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                view.update(cx, |editor, cx| editor.focus_in(window, cx));
                cx.stop_propagation();
            })
            .child(handle);
        Some(AnchoredElement::new(handle, move |_, _| {
            grip.approach.origin
        }))
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
    view: &EditorView,
    drag: &BlockDrag,
    targets: &[BlockDropTarget],
    cx: &Context<EditorView>,
) -> Option<(BlockDropTarget, Transaction)> {
    if drag.editor.entity_id() != cx.entity_id() || view.editor().revision() != drag.revision {
        return None;
    }
    movement::plan(view.editor(), drag.selection, targets)
}
