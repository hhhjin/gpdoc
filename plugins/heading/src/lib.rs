use gpdoc_plugin::prelude::{
    AttrSpec, AttrSpecs, AttrType, Attrs, Command, NodeSpec, Plugin, Value,
};
use gpdoc_plugin::textblock::{set_node, textblock_type_input_rule, toggle_node};
use regex::Regex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Level {
    H1 = 1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

impl Level {
    pub const ALL: [Self; 6] = [Self::H1, Self::H2, Self::H3, Self::H4, Self::H5, Self::H6];

    fn attrs(self) -> Attrs {
        Attrs::from([("level".into(), Value::from(u8::from(self)))])
    }
}

impl From<Level> for u8 {
    fn from(level: Level) -> Self {
        level as u8
    }
}

impl TryFrom<u8> for Level {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        value
            .checked_sub(1)
            .and_then(|i| Self::ALL.get(usize::from(i)).copied())
            .ok_or("heading level must be 1 through 6")
    }
}

/// Set the current text block to the given heading level.
///
/// ```compile_fail,E0308
/// use gpdoc_plugin_heading::Set;
/// let command = Set(2);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Set(pub Level);

impl Command for Set {
    const NAME: &'static str = "setHeading";
}

#[derive(Clone, Copy, Debug)]
pub struct Toggle(pub Level);

impl Command for Toggle {
    const NAME: &'static str = "toggleHeading";
}

pub fn heading() -> Plugin {
    let spec = NodeSpec::block("heading")
        .group("block")
        .content("inline*")
        .attrs(AttrSpecs::from([(
            "level".into(),
            AttrSpec::with_default(AttrType::Integer, Value::from(1)),
        )]));
    Plugin::new("heading", |p| {
        p.node(spec);
        p.command::<Set>(|ctx, Set(level)| set_node("heading", Attrs::new())(ctx, level.attrs()));
        p.command::<Toggle>(|ctx, Toggle(level)| toggle_node("heading")(ctx, level.attrs()));
        for level in Level::ALL {
            p.shortcut(format!("Mod-Alt-{}", u8::from(level)), Toggle(level));
        }
        p.input_rule(textblock_type_input_rule(
            Regex::new(r"^(#{1,6}) $").expect("valid heading pattern"),
            "paragraph",
            "heading",
            |captures| Some(Level::ALL[captures[1].len() - 1].attrs()),
        ));
    })
}

/// Compose the editable content of each `heading` node.
#[cfg(feature = "gpui")]
pub fn heading_block(render: impl gpdoc::BlockRenderer) -> gpdoc::ViewPlugin {
    gpdoc::editable_block("heading", render)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin::EditorConfig;
    use gpdoc_plugin::prelude::{CommandOutcome, Node};
    use gpdoc_plugin_paragraph::paragraph;

    #[test]
    fn set_heading_applies_the_requested_level() {
        let config = EditorConfig::builder()
            .plugins([paragraph(), heading()])
            .build()
            .unwrap();
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "Title")],
        )
        .unwrap();

        assert!(matches!(
            editor.run(Set(Level::H2)).unwrap(),
            CommandOutcome::Applied(_)
        ));
        let node = &editor.document().blocks()[0];
        assert_eq!(node.kind, "heading");
        assert_eq!(node.attrs["level"], 2);
        assert_eq!(node.text_content(), "Title");
    }
}
