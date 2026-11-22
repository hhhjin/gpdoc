use gpdoc_plugin::marks::{set_mark, toggle_mark, unset_mark};
use gpdoc_plugin::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct Set {
    pub rt: Option<String>,
}

impl Command for Set {
    const NAME: &'static str = "setRubyText";
}

#[derive(Clone, Debug, Default)]
pub struct Toggle {
    pub rt: Option<String>,
}

impl Command for Toggle {
    const NAME: &'static str = "toggleRubyText";
}

unit_command!(pub Unset, "unsetRubyText");

fn ruby_attrs(rt: Option<String>) -> Attrs {
    rt.map(|value| ("rt".into(), value.into()))
        .into_iter()
        .collect()
}

pub fn ruby_text() -> Plugin {
    let plugin = Plugin::new("rubyText", |p| {
        p.mark(
            MarkSpec::new("rubyText").inclusive_end(false).attrs(
                [(
                    "rt".into(),
                    AttrSpec::with_default(AttrType::NullableString, Value::Null),
                )]
                .into(),
            ),
        );
        p.command::<Set>(|ctx, args| set_mark("rubyText")(ctx, ruby_attrs(args.rt)));
        p.command::<Unset>(|ctx, _| change(ctx, Attrs::new(), false));
        p.command::<Toggle>(|ctx, args| change(ctx, ruby_attrs(args.rt), true));
    });
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, ruby_text_format);
    plugin
}

/// Apply the native text format for the `rubyText` mark.
#[cfg(feature = "gpui")]
pub fn ruby_text_format() -> gpdoc::ViewPlugin {
    gpdoc::ViewPlugin::named("rubyTextFormat").mark_format("rubyText", |attrs, style| {
        if let Some(Value::String(rt)) = attrs.get("rt") {
            style.ruby_text = Some(rt.as_str().into());
        }
    })
}

// At a caret inside a ruby annotation, unset removes the mark across its
// contiguous range, including adjacent runs with other formatting.
fn change(
    ctx: CommandContext<'_>,
    args: Attrs,
    toggle: bool,
) -> Result<Option<Transaction>, Error> {
    if let Selection::Text { anchor, head } = ctx.selection()
        && anchor == head
        && let Some(marks) = ctx.cursor_marks()?
        && let Some(mark) = marks.iter().find(|m| m.kind == "rubyText")
    {
        let doc = ctx.document().root();
        let pos = doc.resolve(head.0)?;
        let parent = pos.parent();
        let index = pos.index(pos.depth());
        let has_mark = |index: usize| {
            parent
                .content
                .maybe_child(index)
                .is_some_and(|n| n.marks.contains(mark))
        };
        let mut left = index;
        let mut right = index;
        while left > 0 && has_mark(left - 1) {
            left -= 1
        }
        while right < parent.child_count() && has_mark(right) {
            right += 1
        }
        let mut tx = ctx.transaction();
        if left < right {
            tx.push(Edit::RemoveMark {
                range: DocPos(pos.pos_at_index(left, pos.depth()))
                    ..DocPos(pos.pos_at_index(right, pos.depth())),
                kind: "rubyText".into(),
            });
        }
        tx.set_stored_marks(
            marks
                .iter()
                .filter(|m| m.kind != "rubyText")
                .cloned()
                .collect(),
        );
        return Ok(Some(tx));
    }
    if toggle {
        toggle_mark("rubyText")(ctx, args)
    } else {
        unset_mark("rubyText")(ctx, args)
    }
}
