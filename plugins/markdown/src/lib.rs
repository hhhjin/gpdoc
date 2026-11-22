//! Import CommonMark into an installed gpdoc schema and export it with source-positioned warnings.
use gpdoc_plugin::EditorConfig;
use gpdoc_plugin::prelude::*;
mod export;
mod import;
mod rules;
pub use rules::MarkdownSyntax;

use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
pub struct Warning<P> {
    pub position: P,
    pub message: String,
}

pub struct Conversion<T, P> {
    pub value: T,
    pub warnings: Vec<Warning<P>>,
}

/// Import warnings refer to UTF-8 byte ranges in the input.
pub fn open(
    config: &EditorConfig,
    markdown: &str,
) -> Result<Conversion<Editor, Range<usize>>, Error> {
    let parsed = import::parse(config, markdown)?;
    Ok(Conversion {
        value: gpdoc_plugin::core::Editor::from_document(config, parsed.value)?,
        warnings: parsed.warnings,
    })
}

/// Export warnings refer to positions in the gpdoc document.
pub fn to_markdown(editor: &Editor) -> Result<Conversion<String, DocPos>, Error> {
    export::render(
        editor.document(),
        rules::Rules::from_config(editor.config())?,
    )
    .map_err(|error| Error::InvalidDocument(error.to_string()))
}

#[derive(Clone, Debug)]
pub struct Set {
    pub markdown: String,
}

impl Command for Set {
    const NAME: &'static str = "setMarkdown";
}

/// Register a command to replace the whole document in one transaction.
/// Pass the source in the string argument `markdown`.
pub fn markdown() -> Plugin {
    Plugin::new("markdown", |p| {
        p.command::<Set>(|ctx, command| {
            let parsed = import::parse(ctx.config(), &command.markdown)?;
            let next = ctx.schema().document(parsed.value.clone())?;
            let mut tx = ctx.transaction();
            tx.push(Edit::ReplaceNodes {
                range: DocPos(0)..DocPos(ctx.document().size()),
                nodes: parsed.value,
            });
            if let Some(pos) = next.nearest_text(DocPos(0)) {
                tx.select(Selection::caret(pos));
            }
            Ok(Some(tx))
        })
    })
}
