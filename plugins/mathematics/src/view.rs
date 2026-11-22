//! Document-flow typesetting and optional controls for LaTeX atoms.
use gpdoc::{NodeView, ViewPlugin};
use gpdoc_plugin::core::{AtomImage, AtomImageSource, Attrs, TextFormat, Transaction, Value};
use gpui::{App, IntoElement, Window};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{rc::Rc, sync::Arc};

fn project(view: ViewPlugin, kind: &'static str) -> ViewPlugin {
    view.node_format(kind, move |attrs: &Attrs, style: &mut TextFormat| {
        if let Some(source) = attrs.get("latex").and_then(Value::as_str) {
            let latex: Arc<str> = source.into();
            style.atom_image = Some(AtomImage {
                source: AtomImageSource::Formula {
                    latex,
                    display: kind == "blockMath",
                },
                width: None,
                height: None,
            });
        }
    })
}

/// Typeset both formula kinds installed by `mathematics()` with a LaTeX text fallback.
pub fn formula_content() -> ViewPlugin {
    project(
        project(
            ViewPlugin::named("formulas").for_plugin("mathematics"),
            "inlineMath",
        ),
        "blockMath",
    )
}

/// Typeset the inline formula installed by `inline_math()`.
pub fn inline_formula_content() -> ViewPlugin {
    project(
        ViewPlugin::named("inlineFormula").for_plugin("inlineMath"),
        "inlineMath",
    )
}

/// Typeset the display formula installed by `block_math()`.
pub fn block_formula_content() -> ViewPlugin {
    project(
        ViewPlugin::named("blockFormula").for_plugin("blockMath"),
        "blockMath",
    )
}

pub struct FormulaNode<'a> {
    pub slot: NodeView<'a>,
}

impl FormulaNode<'_> {
    pub fn latex(&self) -> &str {
        self.slot
            .node
            .attrs
            .get("latex")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
    pub fn is_display(&self) -> bool {
        self.slot.node.kind == "blockMath"
    }

    /// Prepare a revision-checked edit to the stored source.
    pub fn set_latex(&self, latex: impl Into<String>) -> Transaction {
        let mut attrs = self.slot.node.attrs.clone();
        attrs.insert("latex".into(), Value::String(latex.into()));
        self.slot.set_attrs(attrs)
    }
}

fn controls<E: IntoElement>(
    kind: &'static str,
    owner: &'static str,
    render: impl Fn(FormulaNode<'_>, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    project(
        ViewPlugin::node_view(kind, move |slot, window, cx| {
            render(FormulaNode { slot }, window, cx)
        })
        .for_plugin(owner),
        kind,
    )
}

/// Place controls over the typeset inline and display formulas in a mathematics plugin.
pub fn formula_controls<E: IntoElement>(
    render: impl Fn(FormulaNode<'_>, &mut Window, &mut App) -> E + 'static,
) -> [ViewPlugin; 2] {
    let render = Rc::new(render);
    ["inlineMath", "blockMath"].map(move |kind| {
        let render = render.clone();
        controls(kind, "mathematics", move |node, window, cx| {
            render(node, window, cx)
        })
    })
}

/// Place controls over formulas registered by the standalone inline plugin.
pub fn inline_formula_controls<E: IntoElement>(
    render: impl Fn(FormulaNode<'_>, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    controls("inlineMath", "inlineMath", render)
}

/// Place controls over formulas registered by the standalone display plugin.
pub fn block_formula_controls<E: IntoElement>(
    render: impl Fn(FormulaNode<'_>, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    controls("blockMath", "blockMath", render)
}
