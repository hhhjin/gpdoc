//! GPUI block wrappers and their shared document-coordinate geometry.
mod geometry;
use super::*;
pub(crate) use geometry::TextLayout;

use super::super::{
    buffer::InputBuffer,
    node_view::{BlockViews, ChildRenderer, ContentRenderer, NodeView},
};
use gpdoc_core::{DocPos, Node};
use gpui::{AnyElement, ContentMask, div};

struct Wrapper {
    slot: Slot,
    end: usize,
    blocks: Range<usize>,
    wrappers: Vec<ContentRenderer>,
    layout: Option<ChildRenderer>,
}

enum Part {
    Text(TextSpan),
    View(Wrapper, Vec<Part>),
}

impl Part {
    fn document(buffer: &InputBuffer, views: &BlockViews) -> Vec<Self> {
        let doc = buffer.state().0;
        let mut wrappers = Vec::new();
        if !views.top_level_wrappers.is_empty()
            || !views.wrappers.is_empty()
            || !views.layouts.is_empty()
        {
            let mut top_end = 0;
            let mut layout_end = 0;
            for (before, node) in doc.iter_blocks() {
                let top = before.0 >= top_end;
                let end = before.0 + node.size();
                if top {
                    top_end = end;
                }
                if buffer.hidden(before) {
                    continue;
                }
                let named = views.wrappers.get(&node.kind);
                let layout = views.layouts.get(&node.kind);
                if before.0 >= layout_end
                    && layout.is_none()
                    && named.is_none()
                    && (!top || views.top_level_wrappers.is_empty())
                {
                    continue;
                }
                if layout.is_some() {
                    layout_end = layout_end.max(end);
                }
                let mut renderers = named.cloned().unwrap_or_default();
                if top {
                    renderers.extend(views.top_level_wrappers.iter().cloned());
                }
                let range = before..DocPos(end);
                let blocks = buffer.projection.block_indices(range.clone());
                let offsets = buffer
                    .projection
                    .block_span(blocks.clone())
                    .map(|(_, offsets)| offsets);
                wrappers.push(Wrapper {
                    slot: Slot {
                        index: wrappers.len(),
                        node: node.clone(),
                        before,
                        offsets,
                    },
                    end,
                    blocks,
                    wrappers: renderers,
                    layout: layout.cloned(),
                });
            }
        }
        let blocks = buffer
            .projection
            .block_indices(DocPos(0)..DocPos(doc.size()));
        Self::within(
            buffer,
            blocks,
            doc.size(),
            &mut wrappers.into_iter().peekable(),
        )
    }

    fn within(
        buffer: &InputBuffer,
        blocks: Range<usize>,
        end: usize,
        wrappers: &mut std::iter::Peekable<impl Iterator<Item = Wrapper>>,
    ) -> Vec<Self> {
        let mut list = Vec::new();
        let text = |range| {
            buffer
                .projection
                .block_span(range)
                .map(|(bytes, units)| Part::Text(TextSpan { bytes, units }))
        };
        let mut start = blocks.start;
        while wrappers.peek().is_some_and(|w| w.slot.before.0 < end) {
            let wrapper = wrappers.next().unwrap();
            list.extend(text(start..wrapper.blocks.start));
            start = wrapper.blocks.end;
            let children = Self::within(buffer, wrapper.blocks.clone(), wrapper.end, wrappers);
            list.push(Part::View(wrapper, children));
        }
        list.extend(text(start..blocks.end));
        list
    }

    fn geometry(self, spans: &mut Vec<TextSpan>, slots: &mut Vec<Slot>) {
        match self {
            Self::Text(span) => spans.push(span),
            Self::View(wrapper, children) => {
                slots.push(wrapper.slot);
                for part in children {
                    part.geometry(spans, slots);
                }
            }
        }
    }
}

/// The document identity and projected text belonging to a measured child.
#[derive(Clone)]
pub(in super::super) struct Slot {
    index: usize,
    node: Node,
    before: DocPos,
    offsets: Option<Range<usize>>,
}

pub(in super::super) struct ChildBounds {
    slot: Slot,
    bounds: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    prepaint_order: usize,
}

#[derive(Default)]
struct Frame {
    measured: Vec<Rc<TextSection>>,
    children: Vec<ChildBounds>,
    document: Option<TextLayout>,
}

struct TextFragment {
    view: Entity<EditorView>,
    span: TextSpan,
    source: Rc<TextSource>,
    images: Rc<images::Images>,
    frame: Rc<RefCell<Frame>>,
}

#[cfg(not(target_family = "wasm"))]
impl IntoElement for TextFragment {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for TextFragment {
    type RequestLayoutState = Rc<RefCell<Measure>>;
    type PrepaintState = Rc<TextSection>;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let text_style = window.text_style();
        let measure = Rc::new(RefCell::new(Measure {
            span: self.span.clone(),
            source: self.source.clone(),
            typography: Typography {
                images: self.images.clone(),
                font_size: text_style.font_size.to_pixels(window.rem_size()),
                style: text_style,
                line_height: window.line_height(),
            },
            wrapped: None,
        }));
        let measuring = measure.clone();
        let mut style = Style {
            flex_shrink: 0.,
            ..Default::default()
        };
        style.size.width = relative(1.).into();
        let id = window.request_measured_layout(style, move |known, available, window, _| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            });
            let mut m = measuring.borrow_mut();
            m.wrap(width, window);
            let lines = &m.wrapped.as_ref().unwrap().1;
            let base = m.typography.line_height;
            let mut dimensions = Size::default();
            for line in lines {
                let s = line.size(base);
                dimensions.width = s.width.max(dimensions.width);
                dimensions.height += s.height;
            }
            dimensions.width = width.unwrap_or(dimensions.width);
            dimensions
        });
        (id, measure)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        measure: &mut Self::RequestLayoutState,
        window: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
        let section = Rc::new(measure.borrow_mut().layout(bounds, window));
        self.frame.borrow_mut().measured.push(section.clone());
        section
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        section: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !section.bounds.intersects(&window.content_mask().bounds) {
            return;
        }
        let frame = self.frame.borrow();
        let document = frame.document.as_ref().unwrap();
        if document.sections.len() == 1 && section.bounds == document.bounds {
            paint_section(&self.view, section, document, window, cx);
            return;
        }
        window.with_content_mask(
            Some(ContentMask {
                bounds: section.bounds,
            }),
            |window| {
                paint_section(&self.view, section, document, window, cx);
            },
        );
    }
}

pub(in super::super) struct Surface {
    root: AnyElement,
    frame: Rc<RefCell<Frame>>,
}

struct Builder {
    view: Entity<EditorView>,
    source: Rc<TextSource>,
    images: Rc<images::Images>,
    frame: Rc<RefCell<Frame>>,
}

impl Builder {
    fn children(
        self: &Rc<Self>,
        parts: Vec<Part>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let mut children: Vec<_> = parts
            .into_iter()
            .map(|p| self.element(p, window, cx))
            .collect();
        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            div()
                .flex()
                .flex_col()
                .children(children)
                .into_any_element()
        }
    }
    fn element(self: &Rc<Self>, part: Part, window: &mut Window, cx: &mut App) -> AnyElement {
        match part {
            Part::Text(span) => TextFragment {
                view: self.view.clone(),
                frame: self.frame.clone(),
                source: self.source.clone(),
                span,
                images: self.images.clone(),
            }
            .into_any_element(),
            Part::View(wrapper, parts) => self.child(wrapper, parts).render(window, cx),
        }
    }

    fn child(self: &Rc<Self>, wrapper: Wrapper, parts: Vec<Part>) -> EditableChild {
        EditableChild {
            slot: wrapper.slot,
            builder: self.clone(),
            wrappers: wrapper.wrappers,
            layout: wrapper.layout,
            parts,
        }
    }
}

/// A block's editable subtree. Put its rendered element into the layout once.
/// The editor continues to own its text, selection, history, and IME.
pub struct EditableChild {
    slot: Slot,
    builder: Rc<Builder>,
    wrappers: Vec<ContentRenderer>,
    layout: Option<ChildRenderer>,
    parts: Vec<Part>,
}

impl EditableChild {
    /// Position immediately before this block in the displayed document.
    pub fn before(&self) -> DocPos {
        self.slot.before
    }

    pub fn node(&self) -> &Node {
        &self.slot.node
    }

    fn children(builder: &Rc<Builder>, parts: Vec<Part>) -> Vec<Self> {
        parts
            .into_iter()
            .map(|part| match part {
                Part::View(wrapper, parts) => builder.child(wrapper, parts),
                Part::Text(_) => {
                    unreachable!("a structural slot's direct blocks each own their projected text")
                }
            })
            .collect()
    }

    fn into_parts(self) -> (ChildWrapper, Option<ChildRenderer>, Vec<Part>) {
        let wrap = ChildWrapper {
            slot: self.slot,
            builder: self.builder,
            wrappers: self.wrappers,
        };
        (wrap, self.layout, self.parts)
    }

    /// Open a structural container so a parent layout can place its descendants.
    /// Text, leaf nodes and children with their own layout must render as complete subtrees.
    pub fn split(self) -> Result<(ChildWrapper, Vec<Self>), Self> {
        if !matches!(self.node().content(), gpdoc_core::ContentRef::Blocks(_))
            || self.layout.is_some()
        {
            return Err(self);
        }
        let (wrap, _, parts) = self.into_parts();
        let children = Self::children(&wrap.builder, parts);
        Ok((wrap, children))
    }

    /// Render the ordinary editable subtree, including nested layouts and wrappers.
    pub fn render(self, window: &mut Window, cx: &mut App) -> AnyElement {
        self.render_with(window, cx, |body| body)
    }

    /// Place this child's editable content in a GPUI element. The editor records the
    /// outer element's bounds, including its padding, for pointer and selection geometry.
    pub fn render_with<E: IntoElement>(
        self,
        window: &mut Window,
        cx: &mut App,
        decorate: impl FnOnce(AnyElement) -> E,
    ) -> AnyElement {
        let (wrap, layout, parts) = self.into_parts();
        let body = match layout {
            Some(render) => render(
                wrap.node_view(cx),
                Self::children(&wrap.builder, parts),
                window,
                cx,
            ),
            None => wrap.builder.children(parts, window, cx),
        };
        let body = wrap.apply_wrappers(body, window, cx);
        wrap.track(decorate(body).into_any_element())
    }
}

/// Wrappers installed on a structural block opened by an enclosing layout.
pub struct ChildWrapper {
    slot: Slot,
    builder: Rc<Builder>,
    wrappers: Vec<ContentRenderer>,
}

impl ChildWrapper {
    pub fn has_wrapper(&self) -> bool {
        !self.wrappers.is_empty()
    }

    fn node_view(&self, cx: &App) -> NodeView<'_> {
        NodeView::new(
            self.slot.before,
            &self.slot.node,
            self.builder.view.read(cx).editor().transaction(),
            self.builder.view.clone(),
        )
    }

    /// Apply the block's wrappers to a body assembled from its editable children.
    pub fn wrap(self, content: AnyElement, window: &mut Window, cx: &mut App) -> AnyElement {
        let body = self.apply_wrappers(content, window, cx);
        self.track(body)
    }

    fn apply_wrappers(
        &self,
        mut content: AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        for render in &self.wrappers {
            content = render(self.node_view(cx), content, window, cx);
        }
        content
    }

    fn track(self, element: AnyElement) -> AnyElement {
        MeasuredChild {
            frame: self.builder.frame.clone(),
            slot: self.slot,
            element,
        }
        .into_any_element()
    }
}

// Forward the child's own layout ID so flex and grid styles still participate directly
// in its parent's layout. Recording bounds adds no layout node or paint layer.
struct MeasuredChild {
    frame: Rc<RefCell<Frame>>,
    slot: Slot,
    element: AnyElement,
}

#[cfg(not(target_family = "wasm"))]
impl IntoElement for MeasuredChild {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for MeasuredChild {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(("gpdoc-editable-child", self.slot.before.0).into())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.element.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        {
            let mut frame = self.frame.borrow_mut();
            let prepaint_order = frame.children.len();
            frame.children.push(ChildBounds {
                slot: self.slot.clone(),
                bounds,
                clip: window.content_mask().bounds,
                prepaint_order,
            });
        }
        self.element.prepaint(window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.element.paint(window, cx);
    }
}

#[cfg(not(target_family = "wasm"))]
impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = Surface;
    type PrepaintState = Vec<AnyElement>;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Surface) {
        let images = images::load(&self.view, window, cx);
        let view = self.view.read(cx);
        let source = TextSource::read(&view.buffer);
        let parts = Part::document(&view.buffer, &view.block_views);
        let builder = Rc::new(Builder {
            view: self.view.clone(),
            source: Rc::new(source),
            images: Rc::new(images),
            frame: Default::default(),
        });
        let mut root = builder.children(parts, window, cx);
        let id = root.request_layout(window, cx);
        (
            id,
            Surface {
                root,
                frame: builder.frame.clone(),
            },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        surface: &mut Surface,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        // An editor embedded in a cached host must invalidate that host when it changes.
        window.set_view_id(self.view.entity_id());
        surface.root.prepaint(window, cx);
        let mut sections = std::mem::take(&mut surface.frame.borrow_mut().measured);
        sections.sort_by_key(|s| s.rows[0].start);
        let mut children = std::mem::take(&mut surface.frame.borrow_mut().children);
        children.sort_unstable_by_key(|child| child.slot.before);
        let layout = TextLayout {
            bounds,
            sections,
            children,
        };
        let mut controls =
            super::super::node_view::prepaint_controls(&self.view, &layout, window, cx);
        controls.extend(crate::view_extensions::prepaint_elements(
            &self.view, &layout, window, cx,
        ));
        surface.frame.borrow_mut().document = Some(layout);
        controls
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        surface: &mut Surface,
        controls: &mut Vec<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = self.view.read(cx);
        window.handle_input(
            &view.focus,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        surface.root.paint(window, cx);
        for control in controls {
            control.paint(window, cx);
        }
        let layout = surface.frame.borrow_mut().document.take().unwrap();
        self.view
            .update(cx, |view, cx| view.set_layout(layout, window, cx));
    }
}
