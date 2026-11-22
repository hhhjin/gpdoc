//! Editable block wrappers and interactive renderers for projected objects.
use super::{layout::TextLayout, view::EditorView};
use gpdoc_core::{Attrs, DocPos, Edit, Error, Node, Transaction};
use gpui::{AnyElement, App, AvailableSpace, Entity, Window, div, prelude::*, size};
use std::{collections::BTreeMap, rc::Rc};

/// A node as it was rendered. Transactions created here are revision-checked on click.
pub struct NodeView<'a> {
    pub before: DocPos,
    pub node: &'a Node,
    transaction: Transaction,
    pub view: Entity<EditorView>,
}

impl<'a> NodeView<'a> {
    pub(crate) fn new(
        before: DocPos,
        node: &'a Node,
        transaction: Transaction,
        view: Entity<EditorView>,
    ) -> Self {
        Self {
            before,
            node,
            transaction,
            view,
        }
    }

    /// Resolve this node's position in the displayed document, including active IME text.
    pub fn displayed_position(&self, cx: &App) -> Result<gpdoc_core::ResolvedPos, Error> {
        Ok(self.view.read(cx).buffer.state().0.resolve(self.before)?)
    }

    pub fn set_attrs(&self, attrs: Attrs) -> Transaction {
        let mut tx = self.transaction.clone();
        tx.push(Edit::SetBlock {
            before: self.before,
            kind: self.node.kind.clone(),
            attrs,
        });
        tx
    }
}

/// A document block and its editable GPUI subtree. Insert `content` once.
pub struct EditableBlock<'a> {
    pub before: DocPos,
    pub node: &'a Node,
    pub content: AnyElement,
}

/// A renderer for editable block wrappers.
pub trait BlockRenderer:
    Fn(EditableBlock<'_>, &mut Window, &mut App) -> Self::Element + 'static
{
    type Element: IntoElement;
}

impl<E: IntoElement, F> BlockRenderer for F
where
    F: Fn(EditableBlock<'_>, &mut Window, &mut App) -> E + 'static,
{
    type Element = E;
}

/// Attach a renderer to an editable node registered under the plugin's own name.
pub fn editable_block(kind: &'static str, render: impl BlockRenderer) -> crate::ViewPlugin {
    crate::ViewPlugin::named(kind).for_plugin(kind).wrap_node(
        kind,
        move |block, content, window, cx| {
            render(
                EditableBlock {
                    before: block.before,
                    node: block.node,
                    content,
                },
                window,
                cx,
            )
        },
    )
}

pub(crate) type ContentRenderer =
    Rc<dyn Fn(NodeView<'_>, AnyElement, &mut Window, &mut App) -> AnyElement>;

pub(crate) type ChildRenderer = Rc<
    dyn Fn(NodeView<'_>, Vec<crate::layout::EditableChild>, &mut Window, &mut App) -> AnyElement,
>;

#[derive(Default)]
pub(crate) struct BlockViews {
    pub(crate) layouts: BTreeMap<String, ChildRenderer>,
    pub(crate) wrappers: BTreeMap<String, Vec<ContentRenderer>>,
    pub(crate) top_level_wrappers: Vec<ContentRenderer>,
}

pub(crate) type Renderer = Rc<dyn Fn(NodeView<'_>, &mut Window, &mut App) -> AnyElement>;

pub(crate) fn supports_node_view(schema: &gpdoc_core::Schema, kind: &str) -> Result<(), Error> {
    if !schema
        .node_type(kind)
        .is_some_and(|ty| !ty.is_text() && (ty.is_inline() || ty.is_leaf()))
    {
        return Err(Error::InvalidSchema(format!(
            "node view requires an inline object or a block leaf '{kind}'"
        )));
    }
    Ok(())
}

pub(crate) fn prepaint_controls(
    view: &Entity<EditorView>,
    layout: &TextLayout,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let editor_view = view.read(cx);
    if editor_view.node_views.is_empty() || editor_view.buffer.has_preedit() {
        return Vec::new();
    }
    let editor = editor_view.editor();
    let clip = window.content_mask().bounds;
    let visible: Vec<_> = editor
        .document()
        .iter_nodes()
        .filter_map(|(before, node)| {
            if editor_view.buffer.hidden(before) {
                return None;
            }
            let renderer = editor_view.node_views.get(&node.kind)?;
            let rect = layout.bounds_for_range(editor_view.buffer.node_range(before))?;
            clip.intersects(&rect).then(|| {
                (
                    before,
                    node.clone(),
                    renderer.clone(),
                    rect,
                    editor.transaction(),
                )
            })
        })
        .collect();
    visible
        .into_iter()
        .map(|(before, node, renderer, rect, transaction)| {
            let child = renderer(
                NodeView::new(before, &node, transaction, view.clone()),
                window,
                cx,
            );
            let mut control = div()
                .id(("node-view", before.0))
                .w(rect.size.width)
                .h(rect.size.height)
                .overflow_hidden()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(child)
                .into_any_element();
            control.prepaint_as_root(
                rect.origin,
                size(
                    AvailableSpace::Definite(rect.size.width),
                    AvailableSpace::Definite(rect.size.height),
                ),
                window,
                cx,
            );
            control
        })
        .collect()
}
