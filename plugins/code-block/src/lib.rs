use gpdoc_plugin::prelude::{
    AllowedMarks, AttrSpec, AttrSpecs, AttrType, Attrs, Command, ContentSpec, DocPos, Edit,
    NodeSpec, Plugin, PluginSpec, Selection, Value,
};
use gpdoc_plugin::textblock::{set_node, textblock_type_input_rule, toggle_node};
use regex::Regex;

/// How a command treats the language on an existing code block.
#[derive(Clone, Debug, Default)]
pub enum Language {
    #[default]
    Keep,
    Clear,
    Set(String),
}

impl Language {
    fn attrs(self) -> Attrs {
        let value = match self {
            Self::Keep => return Attrs::new(),
            Self::Clear => Value::Null,
            Self::Set(language) => Value::String(language),
        };
        [("language".into(), value)].into()
    }
}

#[derive(Clone, Debug)]
pub struct Set(pub Language);

impl Command for Set {
    const NAME: &'static str = "setCodeBlock";
}

#[derive(Clone, Debug)]
pub struct Toggle(pub Language);

impl Command for Toggle {
    const NAME: &'static str = "toggleCodeBlock";
}

pub fn code_block() -> Plugin {
    Plugin::new("codeBlock", register_code_block)
}

pub fn register_code_block(p: &mut PluginSpec) {
    p.node(
        NodeSpec::block("codeBlock")
            .group("block")
            .content(ContentSpec::text(AllowedMarks::Only(vec![])))
            .attrs(AttrSpecs::from([(
                "language".into(),
                AttrSpec::with_default(AttrType::NullableString, Value::Null),
            )])),
    );
    p.on_break("codeBlock", |ctx| {
        let mut tx = ctx.transaction();
        let range = ctx.selection().bounds(ctx.document());
        let caret = DocPos(range.start.0 + 1);
        tx.push(Edit::ReplaceText {
            range,
            text: "\n".into(),
        });
        tx.select(Selection::caret(caret));
        Ok(Some(tx))
    });
    p.command::<Set>(|ctx, command| set_node("codeBlock", Attrs::new())(ctx, command.0.attrs()));
    p.command::<Toggle>(|ctx, command| toggle_node("codeBlock")(ctx, command.0.attrs()));
    p.shortcut("Mod-Alt-c", Toggle(Language::Keep));
    p.input_rule(textblock_type_input_rule(
        Regex::new(r"^(?:```|~~~)([a-z]+)?\s$").expect("valid code fence"),
        "paragraph",
        "codeBlock",
        |capture| {
            Some(
                capture
                    .get(1)
                    .map(|language| {
                        Attrs::from([("language".into(), Value::String(language.as_str().into()))])
                    })
                    .unwrap_or_default(),
            )
        },
    ));
}

/// Compose the editable content of each `codeBlock` node.
#[cfg(feature = "gpui")]
pub fn code_block_body(render: impl gpdoc::BlockRenderer) -> gpdoc::ViewPlugin {
    gpdoc::editable_block("codeBlock", render)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_updates_preserve_omitted_null_and_text() {
        assert_eq!(Language::Keep.attrs(), Attrs::new());
        assert_eq!(Language::Clear.attrs()["language"], Value::Null);
        assert_eq!(Language::Set("rust".into()).attrs()["language"], "rust");
    }
}
