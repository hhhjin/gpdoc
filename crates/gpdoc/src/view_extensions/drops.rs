//! GPUI consumes each drag payload once. Install one native listener for each
//! payload type and let plugins pass the drop along until one handles it.
use super::{EditorView, SurfaceHandler, ViewPlugin};
use gpdoc_core::DocPos;
use gpui::{Context, Window, prelude::*};
use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    rc::Rc,
};

#[derive(Clone, Copy)]
enum Target {
    Text,
    Block,
}

type Handler =
    Box<dyn Fn(&mut EditorView, &dyn Any, DocPos, &mut Window, &mut Context<EditorView>) -> bool>;
type Rule = (Target, Handler);
type Attach = fn(Vec<Rule>) -> SurfaceHandler;

pub(super) struct Registration {
    payload: TypeId,
    attach: Attach,
    rule: Option<Rule>,
}

impl Registration {
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
    // The first configured drop handler chooses the insertion indicator.
    // A payload registered only for hover shows a text insertion point.
    let inline = !matches!(rules.first(), Some((Target::Block, _)));
    Rc::new(move |surface, cx| {
        let surface = surface.on_drag_move::<T>(cx.listener(move |view, _, window, _| {
            view.inline_drop_target = inline;
            window.refresh();
        }));
        if rules.is_empty() {
            return surface;
        }
        let rules = rules.clone();
        surface.on_drop(cx.listener(move |view, payload: &T, window, cx| {
            for (target, handle) in rules.iter() {
                let point = window.mouse_position();
                let position = match target {
                    Target::Text => view.position_at(point, window),
                    Target::Block if !view.is_composing() => {
                        view.block_boundary_at(point, window).map(|(at, _)| at)
                    }
                    Target::Block => None,
                };
                if let Some(at) = position
                    && handle(view, payload, at, window, cx)
                {
                    cx.stop_propagation();
                    break;
                }
            }
        }))
    })
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
        self,
        handler: impl Fn(&mut EditorView, &T, DocPos, &mut Window, &mut Context<EditorView>) -> bool
        + 'static,
    ) -> Self {
        self.drop_at(Target::Text, handler)
    }

    /// Receive a typed drop at the closest top-level block boundary.
    /// Return false to pass it to the next handler for this payload.
    pub fn on_block_drop<T: 'static>(
        self,
        handler: impl Fn(&mut EditorView, &T, DocPos, &mut Window, &mut Context<EditorView>) -> bool
        + 'static,
    ) -> Self {
        self.drop_at(Target::Block, handler)
    }

    fn drop_at<T: 'static>(
        mut self,
        target: Target,
        handler: impl Fn(&mut EditorView, &T, DocPos, &mut Window, &mut Context<EditorView>) -> bool
        + 'static,
    ) -> Self {
        let handle: Handler = Box::new(move |view, payload, at, window, cx| {
            handler(
                view,
                payload
                    .downcast_ref::<T>()
                    .expect("registered GPUI drop payload type"),
                at,
                window,
                cx,
            )
        });
        self.drops
            .push(Registration::for_payload::<T>(Some((target, handle))));
        self
    }
}
