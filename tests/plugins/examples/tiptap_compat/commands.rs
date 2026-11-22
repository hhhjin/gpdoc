//! Translate the reference corpus's Tiptap operations into Rust command values.
use gpdoc_plugin::prelude::{Attrs, Error, StoredCommand};
use gpdoc_plugin_audio as audio;
use gpdoc_plugin_bold as bold;
use gpdoc_plugin_bullet_list as bullet_list;
use gpdoc_plugin_code_block as code_block;
use gpdoc_plugin_details as details;
use gpdoc_plugin_find_and_replace as find_and_replace;
use gpdoc_plugin_hard_break as hard_break;
use gpdoc_plugin_heading as heading;
use gpdoc_plugin_horizontal_rule as horizontal_rule;
use gpdoc_plugin_image as image;
use gpdoc_plugin_italic as italic;
use gpdoc_plugin_link as link;
use gpdoc_plugin_mathematics as mathematics;
use gpdoc_plugin_ordered_list as ordered_list;
use gpdoc_plugin_ruby_text as ruby_text;
use gpdoc_plugin_strike as strike;
use gpdoc_plugin_subscript as subscript;
use gpdoc_plugin_superscript as superscript;
use gpdoc_plugin_table as table;
use gpdoc_plugin_table_of_contents as table_of_contents;
use gpdoc_plugin_task_list as task_list;
use gpdoc_plugin_text_align as text_align;
use gpdoc_plugin_text_style as text_style;
use gpdoc_plugin_underline as underline;
use serde::de::DeserializeOwned;

struct Fields<'a> {
    command: &'a str,
    values: Attrs,
}

impl Fields<'_> {
    fn invalid(&self, reason: impl Into<String>) -> Error {
        Error::InvalidCommandArguments {
            command: self.command.into(),
            reason: reason.into(),
        }
    }

    fn optional<T: DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>, Error> {
        self.values
            .remove(key)
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| self.invalid(format!("{key}: {err}")))
    }

    fn required<T: DeserializeOwned>(&mut self, key: &str) -> Result<T, Error> {
        self.optional(key)?
            .ok_or_else(|| self.invalid(format!("missing {key}")))
    }

    fn level(&mut self) -> Result<heading::Level, Error> {
        heading::Level::try_from(self.required::<u8>("level")?).map_err(|err| self.invalid(err))
    }

    fn link_attr(&mut self, key: &str) -> Result<link::LinkAttr, Error> {
        Ok(match self.optional::<Option<String>>(key)? {
            None => link::LinkAttr::Keep,
            Some(None) => link::LinkAttr::Clear,
            Some(Some(value)) => link::LinkAttr::Set(value),
        })
    }
}

pub(super) fn decode(name: &str, args: Attrs) -> Result<StoredCommand, Error> {
    let mut fields = Fields {
        command: name,
        values: args,
    };
    let command = match name {
        "addColumnAfter" => StoredCommand::new(table::AddColumnAfter),
        "addRowAfter" => StoredCommand::new(table::AddRowAfter),
        "clearSearch" => StoredCommand::new(find_and_replace::ClearSearch),
        "deleteColumn" => StoredCommand::new(table::DeleteColumn),
        "deleteInlineMath" => StoredCommand::new(mathematics::DeleteInline {
            pos: fields.optional("pos")?,
        }),
        "deleteRow" => StoredCommand::new(table::DeleteRow),
        "deleteTable" => StoredCommand::new(table::Delete),
        "fixTables" => StoredCommand::new(table::Fix),
        "goToNextCell" => StoredCommand::new(table::GoToNextCell),
        "goToNextResult" => StoredCommand::new(find_and_replace::GoToNextResult),
        "goToPreviousCell" => StoredCommand::new(table::PreviousCell),
        "insertBlockMath" => StoredCommand::new(mathematics::InsertBlock {
            latex: fields.required("latex")?,
            pos: fields.optional("pos")?,
        }),
        "insertInlineMath" => StoredCommand::new(mathematics::InsertInline {
            latex: fields.required("latex")?,
            pos: fields.optional("pos")?,
        }),
        "insertTable" => StoredCommand::new(table::Insert {
            rows: fields.required("rows")?,
            columns: fields.required("cols")?,
            with_header_row: fields.optional("withHeaderRow")?.unwrap_or(true),
        }),
        "mergeCells" => StoredCommand::new(table::MergeCells),
        "removeEmptyTextStyle" => StoredCommand::new(text_style::RemoveEmpty),
        "replace" => StoredCommand::new(find_and_replace::Replace),
        "replaceAll" => StoredCommand::new(find_and_replace::ReplaceAll),
        "setAudio" => StoredCommand::new(audio::Set {
            src: fields.required("src")?,
            muted: fields.optional("muted")?,
            ..Default::default()
        }),
        "setBackgroundColor" => StoredCommand::new(text_style::SetBackgroundColor(
            fields.optional("backgroundColor")?,
        )),
        "setBold" => StoredCommand::new(bold::Set),
        "setCellSelection" => StoredCommand::new(table::SetSelection {
            anchor_cell: fields.required("anchorCell")?,
            head_cell: fields.optional("headCell")?,
        }),
        "setColor" => StoredCommand::new(text_style::SetColor(fields.optional("color")?)),
        "setDetails" => StoredCommand::new(details::Set),
        "setFontFamily" => {
            StoredCommand::new(text_style::SetFontFamily(fields.optional("fontFamily")?))
        }
        "setFontSize" => StoredCommand::new(text_style::SetFontSize(fields.optional("fontSize")?)),
        "setHardBreak" => StoredCommand::new(hard_break::Set),
        "setHeading" => StoredCommand::new(heading::Set(fields.level()?)),
        "setHorizontalRule" => StoredCommand::new(horizontal_rule::Set),
        "setImage" => StoredCommand::new(image::Set {
            src: fields.required("src")?,
            width: fields.optional("width")?,
            alt: fields.optional("alt")?,
            ..Default::default()
        }),
        "setLineHeight" => {
            StoredCommand::new(text_style::SetLineHeight(fields.optional("lineHeight")?))
        }
        "setLink" => StoredCommand::new(link::Set(link::LinkAttrs {
            href: fields.link_attr("href")?,
            title: fields.link_attr("title")?,
            target: fields.link_attr("target")?,
            class: fields.link_attr("class")?,
            rel: fields.link_attr("rel")?,
        })),
        "setReplaceTerm" => {
            StoredCommand::new(find_and_replace::SetReplaceTerm(fields.required("term")?))
        }
        "setRubyText" => StoredCommand::new(ruby_text::Set {
            rt: fields.optional("rt")?,
        }),
        "setSearchTerm" => {
            StoredCommand::new(find_and_replace::SetSearchTerm(fields.required("term")?))
        }
        "setSectionTitle" => StoredCommand::new(super::SetSectionTitle),
        "setSubscript" => StoredCommand::new(subscript::Set),
        "setSuperscript" => StoredCommand::new(superscript::Set),
        "setTextAlign" => StoredCommand::new(text_align::Set(fields.required("textAlign")?)),
        "splitCell" => StoredCommand::new(table::SplitCell),
        "toggleBold" => StoredCommand::new(bold::Toggle),
        "toggleBulletList" => StoredCommand::new(bullet_list::Toggle),
        "toggleCodeBlock" => StoredCommand::new(code_block::Toggle(
            match fields.optional::<Option<String>>("language")? {
                None => code_block::Language::Keep,
                Some(None) => code_block::Language::Clear,
                Some(Some(value)) => code_block::Language::Set(value),
            },
        )),
        "toggleHeaderColumn" => StoredCommand::new(table::ToggleHeaderColumn),
        "toggleHeaderRow" => StoredCommand::new(table::ToggleHeaderRow),
        "toggleHeading" => StoredCommand::new(heading::Toggle(fields.level()?)),
        "toggleItalic" => StoredCommand::new(italic::Toggle),
        "toggleOrderedList" => StoredCommand::new(ordered_list::Toggle {
            start: fields.optional("start")?,
            list_type: fields.optional("type")?,
        }),
        "toggleStrike" => StoredCommand::new(strike::Toggle),
        "toggleSubscript" => StoredCommand::new(subscript::Toggle),
        "toggleSuperscript" => StoredCommand::new(superscript::Toggle),
        "toggleTaskList" => StoredCommand::new(task_list::Toggle),
        "toggleTextAlign" => StoredCommand::new(text_align::Toggle(fields.required("textAlign")?)),
        "toggleTextStyle" => StoredCommand::new(text_style::Toggle {
            color: fields.optional("color")?,
            background_color: fields.optional("backgroundColor")?,
            font_family: fields.optional("fontFamily")?,
            font_size: fields.optional("fontSize")?,
            line_height: fields.optional("lineHeight")?,
            ..Default::default()
        }),
        "toggleUnderline" => StoredCommand::new(underline::Toggle),
        "unsetBackgroundColor" => StoredCommand::new(text_style::UnsetBackgroundColor),
        "unsetColor" => StoredCommand::new(text_style::UnsetColor),
        "unsetDetails" => StoredCommand::new(details::Unset),
        "unsetFontFamily" => StoredCommand::new(text_style::UnsetFontFamily),
        "unsetFontSize" => StoredCommand::new(text_style::UnsetFontSize),
        "unsetLineHeight" => StoredCommand::new(text_style::UnsetLineHeight),
        "unsetLink" => StoredCommand::new(link::Unset),
        "unsetRubyText" => StoredCommand::new(ruby_text::Unset),
        "unsetSubscript" => StoredCommand::new(subscript::Unset),
        "unsetSuperscript" => StoredCommand::new(superscript::Unset),
        "unsetTextAlign" => StoredCommand::new(text_align::Unset),
        "unsetUnderline" => StoredCommand::new(underline::Unset),
        "updateInlineMath" => StoredCommand::new(mathematics::UpdateInline {
            latex: fields.optional("latex")?,
            pos: fields.optional("pos")?,
        }),
        "updateTableOfContents" => StoredCommand::new(table_of_contents::Update),
        _ => return Err(Error::UnknownCommand(name.into())),
    };
    if let Some(key) = fields.values.keys().next() {
        return Err(fields.invalid(format!("unexpected field {key}")));
    }
    Ok(command)
}
