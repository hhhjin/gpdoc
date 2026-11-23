//! Connect document plugins to GPUI input, rendering, and per-view behavior.
mod anchored;
mod config;
mod decorations;
mod drops;
mod formats;
mod interactions;

pub(crate) use anchored::prepaint_elements;
pub use anchored::{AnchoredElement, ViewFrame};
pub(crate) use config::ViewConfig;
pub use config::{DocumentSource, EditorPlugin, OpenError, PluginViewExt};
pub(crate) use decorations::paint_decorations;
pub use decorations::{DecorationFrame, PaintLayer};
pub(crate) use drops::{BlockHover, DropIndicator};
pub use interactions::BlockDropTarget;

use crate::{
    EditableChild, EditorView, NodeView,
    node_view::{ChildRenderer, ContentRenderer, Renderer},
};
use gpui::{AnyElement, App, Context, IntoElement, Window};
use std::rc::Rc;

#[derive(Clone, Default)]
pub(crate) struct ViewExtensions {
    pub(crate) behaviors: Vec<BehaviorFactory>,
    pub(crate) surface_handlers: Vec<SurfaceHandler>,
}

/// One instance per editor view. Override the input and rendering hooks
/// your plugin uses. Returning true from an input handler consumes the event.
pub trait ViewBehavior {
    /// Paint on the visible editor surface. A section can be clipped inside a custom block.
    fn paint(
        &self,
        _layer: PaintLayer,
        _frame: &DecorationFrame<'_>,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    /// Override text selection paint for a focused or unfocused window.
    fn selection_color(&self, _focused: bool) -> Option<gpui::Hsla> {
        None
    }

    /// Refresh transient state before the view renders or dispatches a key.
    fn update(&self, _view: &EditorView) {}

    /// Redirect ↑/↓ at a text section edge. By default the editor visits sections
    /// in document order. Return a boundary to resume from in the same direction.
    /// `neighbor` finds the next visible section beyond a document position.
    fn vertical_block_boundary(
        &self,
        _doc: &crate::Document,
        _head: crate::DocPos,
        _down: bool,
        _neighbor: &dyn Fn(crate::DocPos) -> Option<crate::DocPos>,
    ) -> Option<crate::DocPos> {
        None
    }

    fn key_down(
        &self,
        _view: &mut EditorView,
        _stroke: &gpui::Keystroke,
        _window: &mut Window,
        _cx: &mut Context<EditorView>,
    ) -> bool {
        false
    }

    /// Consume a press to receive subsequent drag events first. Returning false
    /// passes the event on to the next behavior and then to native text selection.
    fn mouse_down(
        &self,
        _view: &mut EditorView,
        _event: &gpui::MouseDownEvent,
        _window: &mut Window,
        _cx: &mut Context<EditorView>,
    ) -> bool {
        false
    }

    /// A behavior may claim a drag after seeing where the pointer moved.
    fn mouse_move(
        &self,
        _view: &mut EditorView,
        _event: &gpui::MouseMoveEvent,
        _window: &mut Window,
        _cx: &mut Context<EditorView>,
    ) -> bool {
        false
    }

    fn mouse_up(
        &self,
        _view: &mut EditorView,
        _event: &gpui::MouseUpEvent,
        _window: &mut Window,
        _cx: &mut Context<EditorView>,
    ) -> bool {
        false
    }

    /// Clear transient pointer state after focus loss or a cancelled gesture.
    fn pointer_cancel(&self) {}

    /// Return true to consume pasted content before the default text insertion.
    fn paste(
        &self,
        _view: &mut EditorView,
        _item: &gpui::ClipboardItem,
        _window: &mut Window,
        _cx: &mut Context<EditorView>,
    ) -> bool {
        false
    }

    /// Supply a GPUI element positioned against the current layout.
    fn render(
        self: Rc<Self>,
        _frame: &ViewFrame<'_>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<AnchoredElement> {
        None
    }
}

pub(crate) type BehaviorFactory = Rc<dyn Fn() -> Rc<dyn ViewBehavior>>;

type Surface = gpui::Stateful<gpui::Div>;
pub(crate) type SurfaceHandler = Rc<dyn Fn(Surface, &mut Context<EditorView>) -> Surface>;

/// Named view behavior or renderer installed into an editor configuration.
#[must_use]
pub struct ViewPlugin {
    name: String,
    owner: Option<&'static str>,
    formats: Vec<formats::Format>,
    drops: Vec<drops::Registration>,
    behavior: Option<BehaviorFactory>,
    node_view: Option<(String, Renderer)>,
    wrappers: Vec<(Option<String>, ContentRenderer)>,
    child_layouts: Vec<(String, ChildRenderer)>,
    fold: Option<(String, crate::FoldSpec)>,
}

impl ViewPlugin {
    /// Start a view plugin with a distinct registration name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            owner: None,
            formats: vec![],
            drops: vec![],
            behavior: None,
            node_view: None,
            wrappers: vec![],
            child_layouts: vec![],
            fold: None,
        }
    }

    /// Require this view to be attached to its document plugin using `with_view`.
    pub fn for_plugin(mut self, owner: &'static str) -> Self {
        self.owner = Some(owner);
        self
    }

    /// Create a fresh behavior for each editor instance.
    pub fn new<B: ViewBehavior + 'static>(
        name: impl Into<String>,
        create: impl Fn() -> B + 'static,
    ) -> Self {
        Self {
            behavior: Some(Rc::new(move || Rc::new(create()))),
            ..Self::named(name)
        }
    }

    /// Replace the projected label of an inline object or block leaf with a GPUI element.
    pub fn node_view<E: IntoElement>(
        node: impl Into<String>,
        render: impl Fn(NodeView<'_>, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        let node = node.into();
        let name = format!("node_view:{node}");
        Self {
            node_view: Some((
                node,
                Rc::new(move |node, window, cx| render(node, window, cx).into_any_element()),
            )),
            ..Self::named(name)
        }
    }

    /// Wrap the rendered body of each block of this kind. Insert the body once.
    pub fn wrap_node<E: IntoElement>(
        self,
        node: impl Into<String>,
        render: impl Fn(NodeView<'_>, AnyElement, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.wrap(Some(node.into()), render)
    }

    /// Wrap every top-level block, outside its node-specific wrappers.
    pub fn wrap_blocks<E: IntoElement>(
        self,
        render: impl Fn(NodeView<'_>, AnyElement, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.wrap(None, render)
    }

    fn wrap<E: IntoElement>(
        mut self,
        node: Option<String>,
        render: impl Fn(NodeView<'_>, AnyElement, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.wrappers.push((
            node,
            Rc::new(move |node, body, window, cx| {
                render(node, body, window, cx).into_any_element()
            }),
        ));
        self
    }

    /// Place the editable children of a block. Each node kind has one layout owner.
    pub fn child_layout<E: IntoElement>(
        mut self,
        node: impl Into<String>,
        render: impl Fn(NodeView<'_>, Vec<EditableChild>, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.child_layouts.push((
            node.into(),
            Rc::new(move |node, children, window, cx| {
                render(node, children, window, cx).into_any_element()
            }),
        ));
        self
    }

    pub fn fold(mut self, node: impl Into<String>, fold: crate::FoldSpec) -> Self {
        self.fold = Some((node.into(), fold));
        self
    }
}
