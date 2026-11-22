use gpdoc_core::{
    AttrSpecs, CommandContext, DocPos, Edit, Error, Fragment, Node, NodeSpec, Placement, Selection,
    Slice, Step, Transaction,
};
use std::ops::Range;

pub fn atom(name: &str, placement: Placement, attrs: AttrSpecs) -> NodeSpec {
    NodeSpec::new(name, placement)
        .group(match placement {
            Placement::Block => "block",
            Placement::Inline => "inline",
        })
        .attrs(attrs)
}

/// Insert an atom with Tiptap's insertContentAt behavior. A block can replace an
/// empty paragraph or split a text block. A position after a block atom selects
/// the atom itself when there is no inline cursor there.
pub fn insert_atom(
    ctx: CommandContext<'_>,
    node: Node,
    range: Range<DocPos>,
) -> Result<Option<Transaction>, Error> {
    let schema = ctx.schema();
    let doc = ctx.document().root();
    let node = schema.bind_node(&node)?;
    let (mut from, mut to) = (range.start.0, range.end.0);
    if !node.is_inline() {
        let at = doc.resolve(from)?;
        let is_textblock = schema
            .node_type(&at.parent().kind)
            .is_some_and(|ty| ty.is_textblock());
        if is_textblock && at.parent_offset == 0 {
            if from == to && at.parent().child_count() == 0 && at.parent().kind != "codeBlock" {
                from = at.before(at.depth())?;
                to = at.after(at.depth())?;
            } else if at.parent().child_count() > 0 {
                from = at.before(at.depth())?;
            }
        }
    }
    let slice = Slice::new(Fragment::from(vec![node]), 0, 0);
    let Some(step) = Step::fit_replace(schema, doc, from, to, &slice)? else {
        return Ok(None);
    };
    let (next, map) = step.preview(ctx.document())?;
    let end = map.changes().next().map_or(to, |(_, range)| range.end.0);
    let pos = next.resolve(DocPos(end))?;
    let mut tx = ctx.transaction();
    tx.push(step);
    if schema
        .node_type(&pos.parent().kind)
        .is_some_and(|t| t.inline_content())
    {
        tx.select(Selection::caret(DocPos(end)));
    } else if let Some(prev) = pos.node_before()? {
        if prev.is_leaf() {
            tx.select(Selection::Node {
                before: DocPos(end - prev.size()),
            });
        }
    }
    Ok(Some(tx))
}

/// Replace a suffix of the input before the caret with an atom. The callback
/// returns the UTF-8 start of the suffix and the attributes for that match.
pub fn atom_input_rule(
    kind: &'static str,
    find: impl Fn(&str) -> Option<(usize, gpdoc_core::Attrs)>,
) -> impl Fn(gpdoc_core::InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error> {
    input_rule(kind, find, false)
}

/// Match the captured node syntax of Tiptap 3.34's nodeInputRule. This rule
/// inserts the final delimiter before replacing the captured range.
pub fn captured_atom_input_rule(
    kind: &'static str,
    find: impl Fn(&str) -> Option<(usize, gpdoc_core::Attrs)>,
) -> impl Fn(gpdoc_core::InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error> {
    input_rule(kind, find, true)
}

fn input_rule(
    kind: &'static str,
    find: impl Fn(&str) -> Option<(usize, gpdoc_core::Attrs)>,
    captured: bool,
) -> impl Fn(gpdoc_core::InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error> {
    move |ctx| {
        let Selection::Text { anchor, head } = ctx.selection else {
            return Ok(None);
        };
        if anchor != head || ctx.inserted.is_empty() || head != ctx.inserted.end {
            return Ok(None);
        }
        let doc = ctx.document.root();
        let schema = ctx.schema;
        let pos = doc.resolve(head.0)?;
        if pos.parent().kind == "codeBlock" || pos.marks(schema).iter().any(|m| m.kind == "code") {
            return Ok(None);
        }
        let text = pos
            .parent()
            .text_between(0, pos.parent_offset, "", "\u{fffc}")?;
        let Some((start, attrs)) = find(&text) else {
            return Ok(None);
        };
        let Some(matched) = text.get(start..) else {
            return Ok(None);
        };
        let mut from = head.0 - matched.encode_utf16().count();
        let mut to = head.0;
        let atom = ctx.schema.bind_node(&Node::leaf(kind, attrs))?;
        if captured {
            let Some(last) = matched.chars().last() else {
                return Ok(None);
            };
            let at = to - last.len_utf16();
            let delimiter = schema.text(&last.to_string(), pos.marks(schema))?;
            let slice = Slice::new(Fragment::from(vec![delimiter]), 0, 0);
            let Some(typed) = Step::fit_replace(schema, doc, at, at, &slice)? else {
                return Ok(None);
            };
            let (next, _) = typed.preview(ctx.document)?;
            let slice = Slice::new(Fragment::from(vec![atom]), 0, 0);
            let Some(replaced) = Step::fit_replace(schema, next.root(), from, to, &slice)? else {
                return Ok(None);
            };
            return Ok(Some(vec![typed.into(), replaced.into()]));
        }
        if !atom.is_inline()
            && from == pos.start(pos.depth())
            && to == pos.end(pos.depth())
            && pos.depth() > 0
        {
            let depth = pos.depth() - 1;
            let index = pos.index(depth);
            if pos
                .node(depth)
                .can_replace_with(schema, index, index + 1, kind, &[])
            {
                from = pos.before(pos.depth())?;
                to = pos.after(pos.depth())?;
            }
        }
        let slice = Slice::new(Fragment::from(vec![atom]), 0, 0);
        Ok(Step::fit_replace(schema, doc, from, to, &slice)?.map(|step| vec![step.into()]))
    }
}

/// Convert matches contained in newly pasted text into atomic nodes. Construct
/// the replacements from right to left so earlier ranges keep their positions.
pub fn atom_paste_rule(
    kind: &'static str,
    regex: regex::Regex,
    get_attributes: impl Fn(&regex::Captures<'_>) -> Option<(usize, gpdoc_core::Attrs)>,
) -> impl Fn(gpdoc_core::InputRuleContext<'_>) -> Result<Vec<Edit>, Error> {
    move |ctx| {
        let mut replacements = Vec::new();
        let schema = ctx.schema;
        let doc = ctx.document.root();
        for (span, run) in ctx.document.text_runs_in(ctx.inserted.clone()) {
            let pos = doc.resolve(span.start.0)?;
            if pos.parent().kind == "codeBlock" || run.marks.iter().any(|m| m.kind == "code") {
                continue;
            }
            for captures in regex.captures_iter(run.text) {
                let Some((group, attrs)) = get_attributes(&captures) else {
                    continue;
                };
                let Some(matched) = captures.get(group) else {
                    continue;
                };
                let from = span.start.0 + run.text[..matched.start()].encode_utf16().count();
                let end = from + matched.as_str().encode_utf16().count();
                if from < ctx.inserted.start.0 || end > ctx.inserted.end.0 {
                    continue;
                }
                let node = ctx.schema.bind_node(&Node::leaf(kind, attrs))?;
                replacements.push((from, end, node));
            }
        }
        let mut next = ctx.document.clone();
        let mut steps = Vec::new();
        for (mut from, mut end, node) in replacements.into_iter().rev() {
            let pos = next.resolve(DocPos(from))?;
            if !node.is_inline() && from == pos.start(pos.depth()) && end == pos.end(pos.depth()) {
                from = pos.before(pos.depth())?;
                end = pos.after(pos.depth())?;
            }
            let slice = Slice::new(Fragment::from(vec![node]), 0, 0);
            if let Some(step) = Step::fit_replace(schema, next.root(), from, end, &slice)? {
                (next, _) = step.preview(&next)?;
                steps.push(step.into());
            }
        }
        Ok(steps)
    }
}
