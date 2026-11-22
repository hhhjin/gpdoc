use gpdoc_core::{DocPos, Error};

/// Visit only the inserted portion of each text block whose schema allows this mark.
pub fn pasted_textblocks(
    ctx: &gpdoc_core::InputRuleContext<'_>,
    mark: &str,
    mut visit: impl FnMut(DocPos, &str),
) -> Result<(), Error> {
    let schema = ctx.schema;
    for (pos, node) in ctx.document.iter_blocks() {
        if pos >= ctx.inserted.end {
            break;
        }
        if !node.inline_content()
            || !schema
                .node_type(&node.kind)
                .is_some_and(|t| t.allows_mark(mark))
        {
            continue;
        }
        let from = ctx.inserted.start.0.max(pos.0 + 1);
        let to = ctx.inserted.end.0.min(pos.0 + node.size() - 1);
        if from >= to {
            continue;
        }
        let root = ctx.document.root();
        let text = root.text_between(from, to, "", "\u{fffc}")?;
        visit(DocPos(from), &text);
    }
    Ok(())
}
