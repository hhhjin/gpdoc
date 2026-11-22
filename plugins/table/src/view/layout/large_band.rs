//! Position connected spans that exceed GPUI's grid track range. Each starting
//! row reserves only the intrinsic height not guaranteed by the covered rows.
//! Custom row wrappers taller than the minimum can leave extra room in that
//! starting row. Cell geometry always extends to the actual final row edge.
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Display, Element, ElementId, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Style, Window, canvas, div, prelude::*,
};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{cell::RefCell, ops::Range, rc::Rc};

struct Span {
    rows: Range<usize>,
    body: AnyElement,
    slot: Option<(LayoutId, Bounds<Pixels>)>,
    covered_minimum: Pixels,
}

struct IntrinsicCell(Rc<RefCell<Span>>);

#[cfg(not(target_family = "wasm"))]
impl IntoElement for IntrinsicCell {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for IntrinsicCell {
    type RequestLayoutState = LayoutId;
    type PrepaintState = ();

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
    ) -> (LayoutId, LayoutId) {
        let mut span = self.0.borrow_mut();
        let id = span.body.request_layout(window, cx);
        let mut style = Style::default();
        style.margin.bottom = (-span.covered_minimum).into();
        // Return a separate container; the body gets a full-height parent before painting.
        (window.request_layout(style, [id], cx), id)
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        body: &mut LayoutId,
        _: &mut Window,
        _: &mut App,
    ) {
        self.0.borrow_mut().slot = Some((*body, bounds));
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut LayoutId,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

#[derive(Default)]
pub(super) struct Spans(Vec<Rc<RefCell<Span>>>);

impl Spans {
    pub(super) fn measure(
        &mut self,
        rows: Range<usize>,
        body: AnyElement,
        covered_minimum: Pixels,
    ) -> impl IntoElement {
        let span = Rc::new(RefCell::new(Span {
            rows,
            body,
            slot: None,
            covered_minimum,
        }));
        self.0.push(span.clone());
        IntrinsicCell(span)
    }

    pub(super) fn overlay(self, rows: Vec<AnyElement>) -> AnyElement {
        let spans = Rc::new(self.0);
        let paint = spans.clone();
        div()
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .children(rows)
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, (), window, cx| {
                        for cell in paint.iter() {
                            cell.borrow_mut().body.paint(window, cx);
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
            .on_children_prepainted(move |row_bounds, window, cx| {
                for cell in spans.iter() {
                    let mut cell = cell.borrow_mut();
                    let (id, mut bounds) = cell
                        .slot
                        .expect("spanning cell measured with its starting row");
                    bounds.origin.y = row_bounds[cell.rows.start].top();
                    bounds.size.height = row_bounds[cell.rows.end - 1].bottom() - bounds.origin.y;
                    // Give the already measured subtree its final, full-height rectangle.
                    let style = Style {
                        display: Display::Flex,
                        size: bounds.size.map(Into::into),
                        ..Default::default()
                    };
                    let root = window.request_layout(style, [id], cx);
                    window.compute_layout(root, bounds.size.map(AvailableSpace::Definite), cx);
                    cell.body.prepaint_at(bounds.origin, window, cx);
                }
            })
            .into_any_element()
    }
}
