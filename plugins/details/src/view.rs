use gpdoc::{EditorView, Fold, FoldControl, FoldRegion, ViewBehavior, ViewPlugin};
use gpdoc_plugin::core::{CommandOutcome, ContentRef, DocPos, Edit, Error, Node, Selection};
use gpui::{
    AnyElement, App, Context, Div, Keystroke, MouseButton, Stateful, Window, div, prelude::*, px,
};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;

/// Parts of one disclosure header. Place the editable summary `content` once.
pub struct DetailsSummary {
    pub open: bool,
    /// Click target wired to the fold's current state.
    pub trigger: Stateful<Div>,
    pub content: AnyElement,
}

impl DetailsSummary {
    /// Render the disclosure arrow beside its editable summary.
    pub fn render(self, _: &mut Window, _: &mut App) -> Div {
        div()
            .flex()
            .items_start()
            .gap(px(4.))
            .child(
                self.trigger
                    .flex_none()
                    .w(px(18.))
                    .child(if self.open { "▾" } else { "▸" }),
            )
            .child(div().flex_1().min_w_0().child(self.content))
    }
}

pub(super) fn default_disclosure() -> ViewPlugin {
    disclosure(|header, _, _| {
        div()
            .flex()
            .items_start()
            .child(
                header
                    .trigger
                    .flex_none()
                    .child(if header.open { "▾" } else { "▸" }),
            )
            .child(div().flex_1().min_w_0().child(header.content))
    })
}

/// Compose a disclosure header and install folding and keyboard editing for its body.
pub fn disclosure<E: IntoElement>(
    render: impl Fn(DetailsSummary, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    ViewPlugin::new("details", || DetailsKeys)
        .fold("details", region)
        .wrap_node("detailsSummary", move |node, content, window, cx| {
            // The summary is the first child of its details node.
            let before = node.before;
            let control = before
                .0
                .checked_sub(1)
                .and_then(|pos| FoldControl::at(&node.view, DocPos(pos), cx));
            let open = control.as_ref().is_some_and(FoldControl::is_open);
            let control = control.filter(|_| !node.view.read(cx).is_composing());
            let trigger = div()
                .id(("details-toggle", before.0))
                .when(control.is_none(), |trigger| trigger.invisible())
                .when_some(control, |trigger, control| {
                    trigger
                        .debug_selector(move || format!("gpdoc-disclosure-{}", before.0))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            control.toggle(window, cx);
                            cx.stop_propagation();
                        })
                });
            render(
                DetailsSummary {
                    open,
                    trigger,
                    content,
                },
                window,
                cx,
            )
        })
        .for_plugin("details")
}

fn region(node: &Node, before: DocPos) -> Option<FoldRegion> {
    let ContentRef::Blocks(parts) = node.content() else {
        return None;
    };
    let [summary, body] = parts else { return None };
    if summary.kind != "detailsSummary" || body.kind != "detailsContent" {
        return None;
    }
    let end = DocPos(before.0 + summary.size());
    let start = DocPos(end.0 + 1);
    Some(FoldRegion {
        fallback: end,
        body: start..DocPos(start.0 + body.size()),
        open_attribute: node.attrs.contains_key("open").then_some("open"),
    })
}

struct DetailsKeys;
impl ViewBehavior for DetailsKeys {
    fn key_down(
        &self,
        view: &mut EditorView,
        stroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        if stroke.modifiers != Default::default() {
            return false;
        }
        let result = match stroke.key.as_str() {
            "backspace" => backspace(view, window, cx),
            "enter" => enter(view, window, cx),
            _ => return false,
        };
        result.unwrap_or_else(|err| {
            eprintln!("Could not edit details: {err}");
            true
        })
    }
}

fn details_folds(view: &EditorView) -> impl DoubleEndedIterator<Item = &Fold> {
    view.folds().iter().filter(|fold| {
        view.editor()
            .document()
            .node_at(fold.before)
            .is_some_and(|node| node.kind == "details")
    })
}

fn backspace(
    view: &mut EditorView,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> Result<bool, Error> {
    let Selection::Text { anchor, head } = view.editor().selection() else {
        return Ok(false);
    };
    if view.is_composing() || anchor != head {
        return Ok(false);
    }
    if details_folds(view).any(|d| head == DocPos(d.before.0 + 2)) {
        return Ok(matches!(
            view.run(crate::Unset, window, cx)?,
            CommandOutcome::Applied(_)
        ));
    }
    Ok(false)
}

fn enter(
    view: &mut EditorView,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> Result<bool, Error> {
    let Selection::Text { anchor, head } = view.editor().selection() else {
        return Ok(false);
    };
    if view.is_composing() {
        return Ok(false);
    }
    let Some(details) = details_folds(view)
        .rev()
        .find(|d| d.before < head && head < d.region.body.end)
    else {
        return Ok(false);
    };
    let doc = view.editor().document();
    let mut remove = None;
    let mut removed = 0;
    let at = if head <= details.region.fallback {
        if details.open {
            DocPos(details.region.body.start.0 + 1)
        } else {
            DocPos(details.region.body.end.0 + 1)
        }
    } else {
        // A second Enter after the final empty body paragraph moves the caret outside.
        let (before, block) = doc.text_at(head)?;
        if anchor != head
            || before.0 + block.size() + 1 != details.region.body.end.0
            || block.kind != view.editor().config().default_text_block()
            || block.size() != 2
        {
            return Ok(false);
        }
        if before.0 > details.region.body.start.0 + 1 {
            remove = Some(before..DocPos(before.0 + block.size()));
            removed = block.size();
        }
        DocPos(details.region.body.end.0 + 1)
    };
    let pos = doc.resolve(at)?;
    let index = pos.index(pos.depth());
    let schema = view.editor().schema();
    let default = view.editor().config().default_text_block();
    if !pos
        .parent()
        .can_replace_with(schema, index, index, default, &[])
    {
        return Ok(false);
    }
    let mut tx = view.editor().transaction();
    tx.push(Edit::ReplaceNodes {
        range: at..at,
        nodes: vec![Node::text(default, "")],
    });
    if let Some(range) = remove {
        tx.push(Edit::ReplaceNodes {
            range,
            nodes: vec![],
        });
    }
    tx.select(Selection::caret(DocPos(at.0 + 1 - removed)));
    view.apply_node_view(tx, window, cx)?;
    Ok(true)
}
