//! Block alignment for selected node types, such as paragraph and heading.
#[cfg(feature = "gpui")]
use gpdoc_plugin::core::TextAlignment;
use gpdoc_plugin::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Set(pub String);
impl Command for Set {
    const NAME: &'static str = "setTextAlign";
}
#[derive(Clone, Debug)]
pub struct Toggle(pub String);
impl Command for Toggle {
    const NAME: &'static str = "toggleTextAlign";
}
unit_command!(pub Unset, "unsetTextAlign");

/// Enable Tiptap's left, center, right and justify commands on the given block types.
pub fn text_align(types: impl IntoIterator<Item = impl Into<String>>) -> Plugin {
    text_align_with_options(types, TextAlignOptions::default())
}

#[derive(Clone, Debug)]
pub struct TextAlignOptions {
    pub alignments: Vec<String>,
    pub default_alignment: Option<String>,
}

impl Default for TextAlignOptions {
    fn default() -> Self {
        Self {
            alignments: ["left", "center", "right", "justify"]
                .map(String::from)
                .into(),
            default_alignment: None,
        }
    }
}

pub fn text_align_with_options(
    types: impl IntoIterator<Item = impl Into<String>>,
    options: TextAlignOptions,
) -> Plugin {
    let options = Arc::new(options);
    let default = options
        .default_alignment
        .clone()
        .map(Value::String)
        .unwrap_or(Value::Null);
    let types: Arc<[String]> = types.into_iter().map(Into::into).collect();
    #[cfg(feature = "gpui")]
    let view_types = types.clone();
    let plugin = Plugin::new("textAlign", |p| {
        p.attributes(
            AttributeTargets::names(types.iter().cloned()),
            AttrSpecs::from([(
                "textAlign".into(),
                AttrSpec::with_default(AttrType::NullableString, default),
            )]),
        );
        let change = |change| {
            let types = types.clone();
            let options = options.clone();
            move |ctx: CommandContext<'_>, alignment| {
                update(ctx, Some(alignment), &types, &options, change)
            }
        };
        let set = change(Change::Set);
        p.command::<Set>(move |ctx, Set(alignment)| set(ctx, alignment));
        let toggle = change(Change::Toggle);
        p.command::<Toggle>(move |ctx, Toggle(alignment)| toggle(ctx, alignment));
        p.command::<Unset>(move |ctx, _| update(ctx, None, &types, &options, Change::Unset));
        for (key, align) in [
            ("Mod-Shift-l", "left"),
            ("Mod-Shift-e", "center"),
            ("Mod-Shift-r", "right"),
            ("Mod-Shift-j", "justify"),
        ] {
            p.shortcut(key, Set(align.to_owned()));
        }
    });
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, move || {
        text_align_format(view_types.iter().cloned())
    });
    plugin
}

/// Render the alignment attribute on the supplied node types.
#[cfg(feature = "gpui")]
pub fn text_align_format(types: impl IntoIterator<Item = impl Into<String>>) -> gpdoc::ViewPlugin {
    let mut view = gpdoc::ViewPlugin::named("textAlignFormat");
    for kind in types {
        view = view.format_if_installed(kind, |attrs, format| {
            if let Some(Value::String(value)) = attrs.get("textAlign")
                && let Some(alignment) = alignment(value)
            {
                format.alignment = alignment;
            }
        });
    }
    view
}

#[cfg(feature = "gpui")]
fn alignment(value: &str) -> Option<TextAlignment> {
    Some(match value {
        "left" => TextAlignment::Left,
        "right" => TextAlignment::Right,
        "center" => TextAlignment::Center,
        "justify" => TextAlignment::Justify,
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    Set,
    Toggle,
    Unset,
}

fn update(
    ctx: CommandContext<'_>,
    alignment: Option<String>,
    types: &[String],
    options: &TextAlignOptions,
    change: Change,
) -> Result<Option<Transaction>, Error> {
    let default = options
        .default_alignment
        .clone()
        .map(Value::String)
        .unwrap_or(Value::Null);
    let value = if change == Change::Unset {
        default.clone()
    } else {
        let Some(value) = alignment else {
            return Ok(None);
        };
        if !options.alignments.contains(&value) {
            return Ok(None);
        }
        Value::String(value)
    };
    let bounds = ctx.selection().bounds(ctx.document());
    let (from, to) = (bounds.start.0, bounds.end.0);
    let blocks: Vec<_> = ctx
        .document()
        .iter_blocks()
        .take_while(|(before, _)| before.0 < to)
        .filter(|(before, node)| before.0 + node.size() > from && types.contains(&node.kind))
        .collect();
    if blocks.is_empty() {
        return Ok(None);
    }
    let active_len: usize = blocks
        .iter()
        .filter(|(_, node)| node.attrs.get("textAlign") == Some(&value))
        .map(|(pos, node)| to.min(pos.0 + node.size()) - from.max(pos.0))
        .sum();
    let reset = change == Change::Toggle
        && if from == to {
            blocks
                .iter()
                .any(|(_, node)| node.attrs.get("textAlign") == Some(&value))
        } else {
            active_len >= to - from
        };
    let mut tx = ctx.transaction();
    for (pos, block) in blocks {
        let mut attrs = block.attrs.clone();
        attrs.insert(
            "textAlign".into(),
            if reset {
                default.clone()
            } else {
                value.clone()
            },
        );
        tx.push(Edit::SetBlock {
            before: pos,
            kind: block.kind.clone(),
            attrs,
        });
    }
    Ok(Some(tx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_reset_restores_the_default_on_installed_types() {
        let align = text_align_with_options(
            ["paragraph", "heading"],
            TextAlignOptions {
                alignments: vec!["right".into(), "center".into()],
                default_alignment: Some("center".into()),
            },
        );
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &gpdoc_plugin::core::EditorConfig::builder()
                .plugins([align, gpdoc_plugin_paragraph::paragraph()])
                .build()
                .unwrap(),
            vec![Node::text("paragraph", "text")],
        )
        .unwrap();
        for (value, expected) in [("left", "center"), ("right", "right"), ("right", "center")] {
            let outcome = editor.run(Toggle(value.into())).unwrap();
            assert_eq!(
                matches!(outcome, CommandOutcome::Applied(_)),
                value == "right"
            );
            assert_eq!(
                editor.document().blocks()[0].attrs["textAlign"],
                Value::String(expected.into())
            );
        }
    }
}
