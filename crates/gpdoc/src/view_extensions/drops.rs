//! GPUI consumes each drag payload once. Install one native listener for each
//! payload type and let plugins pass the drop along until one handles it.
use super::{BlockDropTarget, EditorView, SurfaceHandler, ViewPlugin};
use gpdoc_core::{DocPos, Transaction};
use gpui::{Bounds, Context, Pixels, Window, prelude::*};
use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    rc::Rc,
};

#[derive(Clone, Copy)]
pub(crate) enum DropIndicator {
    Text,
    Block(Option<Bounds<Pixels>>),
}

type Handler =
    Box<dyn Fn(&mut EditorView, &dyn Any, DocPos, &mut Window, &mut Context<EditorView>) -> bool>;
type Planner = Box<
    dyn Fn(
        &EditorView,
        &dyn Any,
        &[BlockDropTarget],
        &Context<EditorView>,
    ) -> Option<(BlockDropTarget, Transaction)>,
>;
enum Rule {
    Text(Handler),
    Block {
        plan: Planner,
        snapshot: fn(&dyn Any) -> Box<dyn Any>,
    },
}
type Attach = fn(Vec<Rule>) -> SurfaceHandler;

pub(super) struct Registration {
    payload: TypeId,
    attach: Attach,
    rule: Option<Rule>,
}

impl Registration {
    pub(super) fn targets_blocks(&self) -> bool {
        matches!(self.rule, Some(Rule::Block { .. }))
    }

    fn for_payload<T: 'static>(rule: Option<Rule>) -> Self {
        Self {
            payload: TypeId::of::<T>(),
            attach: listener::<T>,
            rule,
        }
    }
}

pub(super) fn compile(registrations: Vec<Registration>) -> Vec<SurfaceHandler> {
    let mut groups = BTreeMap::new();
    for registration in registrations {
        let (_, rules) = groups
            .entry(registration.payload)
            .or_insert_with(|| (registration.attach, Vec::new()));
        rules.extend(registration.rule);
    }
    groups
        .into_values()
        .map(|(attach, rules)| attach(rules))
        .collect()
}

fn listener<T: 'static>(rules: Vec<Rule>) -> SurfaceHandler {
    let rules = Rc::new(rules);
    Rc::new(move |surface, cx| {
        let hover = rules.clone();
        let surface = surface.on_drag_move::<T>(cx.listener(
            move |view, event: &gpui::DragMoveEvent<T>, window, cx| {
                if let Some(state) = view.block_drag.as_ref().and_then(|state| state.upgrade()) {
                    state.update(cx, |active, _| {
                        *active = match hover.first() {
                            Some(Rule::Block { snapshot, .. }) => Some(BlockHover {
                                rules: hover.clone(),
                                payload: snapshot(event.dragged_item()),
                            }),
                            _ => None,
                        }
                    });
                }
                view.drop_indicator_kind = DropIndicator::Text;
                window.refresh();
            },
        ));
        if rules.is_empty() {
            return surface;
        }
        let rules = rules.clone();
        surface.on_drop(cx.listener(move |view, payload: &T, window, cx| {
            for rule in rules.iter() {
                let point = window.mouse_position();
                let handled = match rule {
                    Rule::Text(handle) => view
                        .position_at(point, window)
                        .is_some_and(|at| handle(view, payload, at, window, cx)),
                    Rule::Block { plan, .. } => {
                        let targets = view.block_drop_targets(point, window);
                        plan(view, payload, &targets, cx).is_some_and(|(_, tx)| {
                            matches!(view.apply_node_view_if_accepted(tx, window, cx), Ok(true))
                        })
                    }
                };
                if handled {
                    cx.stop_propagation();
                    break;
                }
            }
        }))
    })
}

pub(crate) struct BlockHover {
    rules: Rc<Vec<Rule>>,
    payload: Box<dyn Any>,
}

impl EditorView {
    pub(crate) fn refresh_drop_indicator(
        &mut self,
        layout: &crate::layout::TextLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.block_views.measure_all {
            return;
        }
        // A payload can hold the editor entity. Let the window own it for the
        // lifetime of this surface so closing the editor also releases the drag.
        let state = window.use_keyed_state(("gpdoc-block-hover", cx.entity_id()), cx, |_, _| None);
        self.block_drag = Some(state.downgrade());
        if !cx.has_active_drag() {
            if state.read(cx).is_some() {
                state.update(cx, |active, _| *active = None);
            }
            self.drop_indicator_kind = DropIndicator::Block(None);
        } else if let Some(hover) = state.read(cx) {
            let targets = self.block_drop_targets_in_layout(window.mouse_position(), layout);
            self.drop_indicator_kind = hover
                .rules
                .iter()
                .find_map(|rule| match rule {
                    Rule::Block { plan, .. } => plan(self, &*hover.payload, &targets, cx)
                        .map(|(target, _)| DropIndicator::Block(Some(target.indicator))),
                    Rule::Text(_) => Some(DropIndicator::Text),
                })
                .unwrap_or(DropIndicator::Block(None));
        }
    }
}

impl ViewPlugin {
    /// Show a text insertion position when GPUI drags this payload across the editor.
    pub fn with_text_drag<T: 'static>(mut self) -> Self {
        self.drops.push(Registration::for_payload::<T>(None));
        self
    }

    /// Receive a typed drop at the document position under the pointer.
    /// Return false to pass it to the next handler for this payload.
    pub fn on_text_drop<T: 'static>(
        mut self,
        handler: impl Fn(&mut EditorView, &T, DocPos, &mut Window, &mut Context<EditorView>) -> bool
        + 'static,
    ) -> Self {
        let rule = Rule::Text(Box::new(move |view, payload, at, window, cx| {
            handler(view, payload.downcast_ref::<T>().unwrap(), at, window, cx)
        }));
        self.drops.push(Registration::for_payload::<T>(Some(rule)));
        self
    }

    /// Choose a boundary and plan a transaction for a typed block drop.
    /// Candidates run from the innermost block under the pointer to its outermost ancestor.
    /// The view retains a payload snapshot to replan when layout changes during the drag.
    /// Hover paints the chosen boundary. Release replans against the current editor state.
    pub fn on_block_drop<T: Clone + 'static>(
        mut self,
        plan: impl Fn(
            &EditorView,
            &T,
            &[BlockDropTarget],
            &Context<EditorView>,
        ) -> Option<(BlockDropTarget, Transaction)>
        + 'static,
    ) -> Self {
        let rule = Rule::Block {
            plan: Box::new(move |view, payload, targets, cx| {
                plan(view, payload.downcast_ref::<T>().unwrap(), targets, cx)
            }),
            snapshot: |payload| Box::new(payload.downcast_ref::<T>().unwrap().clone()),
        };
        self.drops.push(Registration::for_payload::<T>(Some(rule)));
        self
    }
}
