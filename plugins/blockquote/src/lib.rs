use gpdoc_plugin::core::{Boundary, BoundaryEdit};
use gpdoc_plugin::prelude::{
    Attrs, DocPos, Edit, Error, Fragment, InputRuleContext, NodeSpec, Plugin, Selection,
    unit_command,
};
use gpdoc_plugin::selection::{Located, path_at, selected_blocks};

unit_command!(pub Set, "setBlockquote");
unit_command!(pub Unset, "unsetBlockquote");
unit_command!(pub Toggle, "toggleBlockquote");

gpdoc_plugin::plugin_config! {
    pub Config {
        shortcuts {
            toggle: Toggle => ["Mod-Shift-b"],
        }
        input_rules InputRules { quote_marker: input }
    }
}

pub fn blockquote() -> Plugin<Config> {
    Plugin::<Config>::new("blockquote", |p| {
        p.node(
            NodeSpec::block("blockquote")
                .group("block")
                .content("block+"),
        );
        p.drag_handle("blockquote", ["paragraph"]);
        p.input_rules().quote_marker().set(wrap_input);
        p.command::<Set>(|editor, _| {
            let blocks = selected_blocks(editor);
            let (Some(first), Some(last)) = (blocks.first(), blocks.last()) else {
                return Ok(None);
            };
            Ok(Some(editor.one_step(Edit::Wrap {
                range: DocPos(first.before)..DocPos(last.end()),
                kind: "blockquote".into(),
                attrs: Attrs::new(),
            })))
        });
        p.command::<Unset>(|editor, _| {
            let (from, to) = match editor.selection() {
                Selection::Node { before } => (before, before),
                Selection::Gap { at } => (at, at),
                Selection::Custom { anchor, head, .. }
                | Selection::NodeRange { anchor, head }
                | Selection::Text { anchor, head } => (anchor.min(head), anchor.max(head)),
            };
            let path = path_at(editor.document().blocks(), from);
            let quote = path
                .into_iter()
                .rev()
                .find(|block: &Located<'_>| block.node.kind == "blockquote" && to.0 < block.end());
            Ok(quote.map(|quote| {
                editor.one_step(Edit::Unwrap {
                    before: DocPos(quote.before),
                })
            }))
        });
        p.command::<Toggle>(|editor, _| {
            if let Some(tx) = editor.plan(Unset)? {
                return Ok(Some(tx));
            }
            editor.plan(Set)
        });
    })
}

fn wrap_input(ctx: InputRuleContext<'_>) -> Result<Option<Vec<Edit>>, Error> {
    let Selection::Text { anchor, head } = ctx.selection else {
        return Ok(None);
    };
    if anchor != head || head != ctx.inserted.end || ctx.inserted.is_empty() {
        return Ok(None);
    }
    let doc = ctx.document.root();
    let pos = doc.resolve(head.0)?;
    if pos.parent().kind == "codeBlock" || pos.depth() == 0 {
        return Ok(None);
    }
    let start = DocPos(pos.start(pos.depth()));
    let schema = ctx.schema;
    let prefix = pos
        .parent()
        .text_between(0, pos.parent_offset, "", "\u{fffc}")?;
    // Tiptap accepts optional whitespace, '>', and one final whitespace character.
    let whitespace = |c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}';
    if !prefix
        .strip_suffix(whitespace)
        .and_then(|s| s.strip_suffix('>'))
        .is_some_and(|indent| indent.chars().all(whitespace))
    {
        return Ok(None);
    }
    let input_start = doc.resolve(ctx.inserted.start.0)?;
    if input_start
        .node_before()?
        .or(input_start.node_after()?)
        .is_some_and(|n| n.marks.iter().any(|m| m.kind == "code"))
    {
        return Ok(None);
    }

    let parent_depth = pos.depth() - 1;
    let parent = pos.node(parent_depth);
    let index = pos.index(parent_depth);
    let Some(mut wrappers) = parent
        .content_match_at(schema, index)?
        .find_wrapping(schema, "blockquote")
    else {
        return Ok(None);
    };
    wrappers.push("blockquote".into());
    if !parent.can_replace_with(schema, index, index + 1, &wrappers[0], &[]) {
        return Ok(None);
    }
    let quote = schema.node_type("blockquote").expect("registered quote");
    let Some(inside) = quote
        .content_match
        .find_wrapping(schema, &pos.parent().kind)
    else {
        return Ok(None);
    };
    wrappers.extend(inside);
    let inner = schema.node_type(wrappers.last().unwrap()).unwrap();
    if !inner
        .content_match
        .match_type(&pos.parent().kind)
        .is_some_and(|m| m.valid_end())
    {
        return Ok(None);
    }

    // Wrap the existing node after deleting the marker so all suffix marks and atoms survive.
    let before = DocPos(start.0 - 1);
    let after = DocPos(pos.after(pos.depth())? - pos.parent_offset);
    let join_previous = wrappers[0] == "blockquote"
        && index > 0
        && parent.child(index - 1).kind == "blockquote"
        && parent.can_replace(schema, index, index + 1, &Fragment::default());
    let mut steps = vec![Edit::ReplaceText {
        range: start..head,
        text: String::new(),
    }];
    if wrappers.len() == 1 {
        steps.push(Edit::Wrap {
            range: before..after,
            kind: "blockquote".into(),
            attrs: Attrs::new(),
        });
    } else {
        let close = vec![Boundary::Close; wrappers.len()];
        let open = wrappers
            .into_iter()
            .map(|kind| Boundary::Open {
                kind,
                attrs: Attrs::new(),
            })
            .collect();
        steps.push(Edit::Boundaries(vec![
            BoundaryEdit {
                range: before..before,
                insert: open,
            },
            BoundaryEdit {
                range: after..after,
                insert: close,
            },
        ]));
    }
    if join_previous {
        steps.push(Edit::Join { at: before });
    }
    Ok(Some(steps))
}

/// Compose the editable content of each `blockquote` node.
#[cfg(feature = "gpui")]
pub fn quote_block(render: impl gpdoc::BlockRenderer) -> gpdoc::ViewPlugin {
    gpdoc::editable_block("blockquote", render)
}
