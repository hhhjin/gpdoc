#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{
    FormulaNode, block_formula_content, block_formula_controls, formula_content, formula_controls,
    inline_formula_content, inline_formula_controls,
};

#[cfg(feature = "gpui")]
use gpdoc::PluginViewExt;
use gpdoc_plugin::atom::{atom, atom_input_rule, insert_atom};
use gpdoc_plugin::prelude::*;
use regex::Regex;

#[derive(Clone, Debug)]
pub struct InsertInline {
    pub latex: String,
    pub pos: Option<usize>,
}

impl Command for InsertInline {
    const NAME: &'static str = "insertInlineMath";
}

#[derive(Default, Clone, Debug)]
pub struct UpdateInline {
    pub latex: Option<String>,
    pub pos: Option<usize>,
}

impl Command for UpdateInline {
    const NAME: &'static str = "updateInlineMath";
}

#[derive(Default, Clone, Debug)]
pub struct DeleteInline {
    pub pos: Option<usize>,
}

impl Command for DeleteInline {
    const NAME: &'static str = "deleteInlineMath";
}

#[derive(Clone, Debug)]
pub struct InsertBlock {
    pub latex: String,
    pub pos: Option<usize>,
}

impl Command for InsertBlock {
    const NAME: &'static str = "insertBlockMath";
}

#[derive(Default, Clone, Debug)]
pub struct UpdateBlock {
    pub latex: Option<String>,
    pub pos: Option<usize>,
}

impl Command for UpdateBlock {
    const NAME: &'static str = "updateBlockMath";
}

#[derive(Default, Clone, Debug)]
pub struct DeleteBlock {
    pub pos: Option<usize>,
}

impl Command for DeleteBlock {
    const NAME: &'static str = "deleteBlockMath";
}

pub fn mathematics() -> Plugin {
    let plugin = Plugin::new("mathematics", |p| {
        block_formula(p);
        inline_formula(p);
    });
    #[cfg(feature = "gpui")]
    let plugin = plugin.gpui_default_view(formula_content);
    plugin
}

pub fn inline_math() -> Plugin {
    let plugin = Plugin::new("inlineMath", inline_formula);
    #[cfg(feature = "gpui")]
    let plugin = plugin.gpui_default_view(inline_formula_content);
    plugin
}
pub fn block_math() -> Plugin {
    let plugin = Plugin::new("blockMath", block_formula);
    #[cfg(feature = "gpui")]
    let plugin = plugin.gpui_default_view(block_formula_content);
    plugin
}

fn block_formula(p: &mut PluginSpec) {
    let kind = "blockMath";
    formula(p, kind, Placement::Block);
    p.command::<InsertBlock>(move |ctx, command| {
        insert(ctx, kind, Placement::Block, command.latex, command.pos)
    });
    p.command::<UpdateBlock>(move |ctx, command| update(ctx, kind, command.latex, command.pos));
    p.command::<DeleteBlock>(move |ctx, command| delete(ctx, kind, command.pos));
}

fn inline_formula(p: &mut PluginSpec) {
    let kind = "inlineMath";
    formula(p, kind, Placement::Inline);
    p.command::<InsertInline>(move |ctx, command| {
        insert(ctx, kind, Placement::Inline, command.latex, command.pos)
    });
    p.command::<UpdateInline>(move |ctx, command| update(ctx, kind, command.latex, command.pos));
    p.command::<DeleteInline>(move |ctx, command| delete(ctx, kind, command.pos));
}

fn formula(p: &mut PluginSpec, kind: &'static str, placement: Placement) {
    let attrs = [(
        "latex".into(),
        AttrSpec::with_default(AttrType::String, Value::String(String::new())),
    )]
    .into();
    let pattern = Regex::new(match placement {
        Placement::Block => r"^\$\$\$([^$]+)\$\$\$$",
        Placement::Inline => r"\$\$([^$\n]+)\$\$$",
    })
    .unwrap();
    p.node(atom(kind, placement, attrs));
    p.atom_label(kind, |attrs| {
        attrs
            .get("latex")
            .and_then(Value::as_str)
            .filter(|source| !source.is_empty())
            .unwrap_or("formula")
            .into()
    });

    p.input_rule(atom_input_rule(kind, move |text| {
        let capture = pattern.captures(text)?;
        let start = capture.get(0)?.start();
        if text[..start].ends_with('$') {
            return None;
        }
        Some((
            start,
            [("latex".into(), Value::String(capture[1].into()))].into(),
        ))
    }));
}

fn insert(
    ctx: CommandContext<'_>,
    kind: &'static str,
    placement: Placement,
    latex: String,
    pos: Option<usize>,
) -> CommandResult {
    let pos = pos
        .map(DocPos)
        .unwrap_or_else(|| ctx.selection().bounds(ctx.document()).start);
    if latex.is_empty() || pos.0 > ctx.document().size() {
        return Ok(None);
    }
    let node = Node::leaf(kind, [("latex".into(), Value::String(latex))].into());
    if placement == Placement::Block {
        insert_atom(ctx, node, pos..pos)
    } else {
        let slice = Slice::new(Fragment::from(vec![ctx.schema().bind_node(&node)?]), 0, 0);
        Ok(
            Step::fit_replace(ctx.schema(), ctx.document().root(), pos.0, pos.0, &slice)?
                .map(|step| ctx.one_step(step)),
        )
    }
}

fn update(
    ctx: CommandContext<'_>,
    kind: &'static str,
    latex: Option<String>,
    pos: Option<usize>,
) -> CommandResult {
    let pos = pos
        .map(DocPos)
        .unwrap_or_else(|| ctx.selection().bounds(ctx.document()).start);
    let Some(node) = ctx.document().node_at(pos).filter(|n| n.kind == kind) else {
        return Ok(None);
    };
    let Some(latex) = latex.or_else(|| node.attrs.get("latex")?.as_str().map(str::to_owned)) else {
        return Ok(None);
    };
    Ok(Some(ctx.one_step(Step::set_node_attribute(
        pos,
        "latex",
        Value::from(latex),
    ))))
}

fn delete(ctx: CommandContext<'_>, kind: &'static str, pos: Option<usize>) -> CommandResult {
    let pos = pos
        .map(DocPos)
        .unwrap_or_else(|| ctx.selection().bounds(ctx.document()).start);
    let Some(node) = ctx.document().node_at(pos).filter(|n| n.kind == kind) else {
        return Ok(None);
    };
    Ok(Step::fit_replace(
        ctx.schema(),
        ctx.document().root(),
        pos.0,
        pos.0 + node.size(),
        &Slice::empty(),
    )?
    .map(|step| ctx.one_step(step)))
}
