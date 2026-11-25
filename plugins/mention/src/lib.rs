#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{MentionChip, mention_chip};

#[cfg(feature = "gpui")]
mod picker;
#[cfg(feature = "gpui")]
pub use picker::{MentionMenu, mention_catalog_picker, mention_picker};

#[cfg(feature = "gpui")]
use gpdoc::PluginViewExt;
use gpdoc_plugin::atom::atom;
use gpdoc_plugin::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Accept(pub Mention);

impl Command for Accept {
    const NAME: &'static str = "acceptMention";
}

unit_command!(pub DeleteBackward, "deleteMentionBackward");

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Mention {
    pub id: Option<String>,
    pub label: Option<String>,
    #[serde(default = "default_trigger", rename = "mentionSuggestionChar")]
    pub trigger: char,
    #[serde(flatten)]
    pub attrs: Attrs,
}

/// Mention records contributed by an installed plugin to the editor's default picker.
#[derive(Clone, Default)]
pub struct MentionCatalog {
    entries: Arc<[Mention]>,
}

impl MentionCatalog {
    pub fn new(entries: impl Into<Arc<[Mention]>>) -> Self {
        Self {
            entries: entries.into(),
        }
    }

    /// Records in the order they were registered.
    pub fn entries(&self) -> &[Mention] {
        &self.entries
    }
}

fn default_trigger() -> char {
    '@'
}

impl Mention {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            label: Some(label.into()),
            trigger: default_trigger(),
            attrs: Attrs::new(),
        }
    }
}

fn label_text(attrs: &Attrs) -> &str {
    attrs
        .get("label")
        .and_then(Value::as_str)
        .or_else(|| attrs.get("id").and_then(Value::as_str))
        .unwrap_or("")
}

fn trigger_text(attrs: &Attrs) -> &str {
    attrs
        .get("mentionSuggestionChar")
        .and_then(Value::as_str)
        .unwrap_or("@")
}

/// Resolve the stored trigger and human-readable label for export or an application view.
pub fn mention_label(attrs: &Attrs) -> String {
    format!("{}{}", trigger_text(attrs), label_text(attrs))
}

pub fn mention() -> Plugin<Shortcuts> {
    mention_with_backspace(false)
}

gpdoc_plugin::plugin_shortcuts! {
    pub Shortcuts {
        delete_backward: DeleteBackward => ["Backspace"],
    }
}

pub fn mention_with_backspace(delete_trigger: bool) -> Plugin<Shortcuts> {
    let attrs = AttrSpecs::from([
        (
            "id".into(),
            AttrSpec::with_default(AttrType::NullableString, Value::Null),
        ),
        (
            "label".into(),
            AttrSpec::with_default(AttrType::NullableString, Value::Null),
        ),
        (
            "mentionSuggestionChar".into(),
            AttrSpec::with_default(AttrType::String, Value::String("@".into())),
        ),
    ]);
    let plugin = Plugin::new("mention", |p| {
        p.node(atom("mention", Placement::Inline, attrs));
        p.atom_label("mention", mention_label);

        p.command::<Accept>(|ctx, Accept(mention)| {
            let mut attrs = mention.attrs;
            attrs.extend([
                ("id".into(), Value::from(mention.id)),
                ("label".into(), Value::from(mention.label)),
                (
                    "mentionSuggestionChar".into(),
                    Value::from(mention.trigger.to_string()),
                ),
            ]);
            gpdoc_plugin_suggestion::Trigger::new(mention.trigger)
                .replace_with_atom(ctx, Node::leaf("mention", attrs))
        });
        p.command::<DeleteBackward>(move |ctx, _| {
            let Selection::Text { anchor, head } = ctx.selection() else {
                return Ok(None);
            };
            if anchor != head || head.0 == 0 {
                return Ok(None);
            }
            let start = DocPos(head.0 - 1);
            let Some(node) = ctx
                .document()
                .node_at(start)
                .filter(|n| n.kind == "mention")
            else {
                return Ok(None);
            };
            let text = match node.attrs.get("mentionSuggestionChar") {
                Some(Value::String(trigger)) if !delete_trigger => trigger.clone(),
                _ => String::new(),
            };
            Ok(Some(ctx.one_step(Edit::ReplaceText {
                range: start..head,
                text,
            })))
        });
    })
    .priority(101);
    #[cfg(feature = "gpui")]
    let plugin = plugin.gpui_default_view(picker::catalog_menu);
    plugin
}
