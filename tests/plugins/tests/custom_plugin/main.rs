use gpdoc_plugin::prelude::*;

#[derive(Clone)]
pub struct InsertSnippet {
    pub text: String,
}

impl Command for InsertSnippet {
    const NAME: &'static str = "insertSnippet";
}
unit_command!(pub RepeatSnippet, "repeatSnippet");

#[derive(Clone, Debug, Default, PartialEq)]
struct LastSnippet(String);

pub fn snippets() -> Plugin {
    Plugin::new("snippets", |p| {
        p.command::<InsertSnippet>(|ctx, command| insert_snippet(ctx, command.text));
        p.command::<RepeatSnippet>(|ctx, _| {
            let Some(last) = ctx.plugin_state::<LastSnippet>() else {
                return Ok(None);
            };
            insert_snippet(ctx, last.0.clone())
        });

        p.shortcut(
            gpdoc_plugin::key!("Mod-Alt-j"),
            InsertSnippet { text: "Hi!".into() },
        );
        p.shortcut(gpdoc_plugin::key!("Mod-Shift-j"), RepeatSnippet);
    })
}

fn insert_snippet(ctx: CommandContext<'_>, text: String) -> CommandResult {
    let Selection::Text { anchor, head } = ctx.selection() else {
        return Ok(None);
    };
    let mut tx = ctx.transaction();
    tx.update_plugin_state(|last: &mut LastSnippet| last.0.clone_from(&text));
    tx.push(Edit::ReplaceText {
        range: anchor.min(head)..anchor.max(head),
        text,
    });
    Ok(Some(tx))
}

#[cfg(test)]
mod tests;
