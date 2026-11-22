//! Tiptap's textStyle mark and the plugins that add CSS attributes to it.
//! Install individual plugins alongside `text_style()`, or use `text_style_kit()`.
use gpdoc_plugin::core::StoredCommand;
#[cfg(feature = "gpui")]
use gpdoc_plugin::core::TextLength;
use gpdoc_plugin::prelude::*;
use std::ops::Range;

const STYLE: &str = "textStyle";

#[derive(Clone, Debug)]
pub struct SetColor(pub Option<String>);
impl Command for SetColor {
    const NAME: &'static str = "setColor";
}
unit_command!(pub UnsetColor, "unsetColor");
#[derive(Clone, Debug)]
pub struct SetBackgroundColor(pub Option<String>);
impl Command for SetBackgroundColor {
    const NAME: &'static str = "setBackgroundColor";
}
unit_command!(pub UnsetBackgroundColor, "unsetBackgroundColor");
#[derive(Clone, Debug)]
pub struct SetFontFamily(pub Option<String>);
impl Command for SetFontFamily {
    const NAME: &'static str = "setFontFamily";
}
unit_command!(pub UnsetFontFamily, "unsetFontFamily");
#[derive(Clone, Debug)]
pub struct SetFontSize(pub Option<String>);
impl Command for SetFontSize {
    const NAME: &'static str = "setFontSize";
}
unit_command!(pub UnsetFontSize, "unsetFontSize");
#[derive(Clone, Debug)]
pub struct SetLineHeight(pub Option<String>);
impl Command for SetLineHeight {
    const NAME: &'static str = "setLineHeight";
}
unit_command!(pub UnsetLineHeight, "unsetLineHeight");

#[derive(Clone, Debug, Default)]
pub struct Toggle {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<String>,
    pub line_height: Option<String>,
    /// Additional properties declared by plugins extending the textStyle mark.
    pub additional: Attrs,
}
impl Toggle {
    fn into_attrs(self) -> Attrs {
        let mut attrs = self.additional;
        attrs.extend(
            [
                ("color", self.color),
                ("backgroundColor", self.background_color),
                ("fontFamily", self.font_family),
                ("fontSize", self.font_size),
                ("lineHeight", self.line_height),
            ]
            .into_iter()
            .filter_map(|(key, value)| Some((key.into(), Value::String(value?)))),
        );
        attrs
    }
}
impl Command for Toggle {
    const NAME: &'static str = "toggleTextStyle";
}
unit_command!(pub RemoveEmpty, "removeEmptyTextStyle");

pub fn text_style() -> Plugin {
    let plugin = Plugin::new(STYLE, |p| {
        p.mark(MarkSpec::new(STYLE));
        p.command::<Toggle>(|ctx, cmd| edit(ctx, &cmd.into_attrs(), Change::Toggle));
        p.command::<RemoveEmpty>(|ctx, _| {
            let mut tx = ctx.transaction();
            let range = selection_range(ctx);
            ctx.document()
                .root()
                .nodes_between(range.start, range.end, |node, pos, _, _| {
                    if let Some(style) = existing(&node.marks)
                        && empty(style)
                    {
                        tx.push(remove(pos..pos + node.size(), style));
                    }
                    true
                })?;
            Ok(Some(tx))
        });
    })
    .priority(101);
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, || text_style_format([STYLE]));
    plugin
}

/// A configurable global style attribute, corresponding to Tiptap's `types` option.
#[derive(Clone, Copy, Debug)]
pub enum StyleProperty {
    Color,
    BackgroundColor,
    FontFamily,
    FontSize,
    LineHeight,
}

impl StyleProperty {
    pub fn on(self, types: impl IntoIterator<Item = impl Into<String>>) -> Plugin {
        let types = types.into_iter().map(Into::into).collect();
        match self {
            Self::Color => property::<SetColor, UnsetColor>("color", types, |v| v.0, SetColor),
            Self::BackgroundColor => property::<SetBackgroundColor, UnsetBackgroundColor>(
                "backgroundColor",
                types,
                |v| v.0,
                SetBackgroundColor,
            ),
            Self::FontFamily => property::<SetFontFamily, UnsetFontFamily>(
                "fontFamily",
                types,
                |v| v.0,
                SetFontFamily,
            ),
            Self::FontSize => {
                property::<SetFontSize, UnsetFontSize>("fontSize", types, |v| v.0, SetFontSize)
            }
            Self::LineHeight => property::<SetLineHeight, UnsetLineHeight>(
                "lineHeight",
                types,
                |v| v.0,
                SetLineHeight,
            ),
        }
    }
}

pub fn color() -> Plugin {
    StyleProperty::Color.on([STYLE])
}
pub fn background_color() -> Plugin {
    StyleProperty::BackgroundColor.on([STYLE])
}
pub fn font_family() -> Plugin {
    StyleProperty::FontFamily.on([STYLE])
}
pub fn font_size() -> Plugin {
    StyleProperty::FontSize.on([STYLE])
}
pub fn line_height() -> Plugin {
    StyleProperty::LineHeight.on([STYLE])
}

pub fn text_style_kit() -> Vec<Plugin> {
    vec![
        background_color(),
        color(),
        font_family(),
        font_size(),
        line_height(),
        text_style(),
    ]
}

fn property<S: Command, U: Command>(
    key: &'static str,
    types: Vec<String>,
    value: fn(S) -> Option<String>,
    set: fn(Option<String>) -> S,
) -> Plugin {
    #[cfg(feature = "gpui")]
    let view_types: Vec<_> = types
        .iter()
        .filter(|kind| kind.as_str() != STYLE)
        .cloned()
        .collect();
    let plugin = Plugin::new(key, |p| {
        p.attributes(
            AttributeTargets::names(types),
            AttrSpecs::from([(
                key.into(),
                AttrSpec::with_default(AttrType::NullableString, Value::Null),
            )]),
        );
        p.command::<S>(move |ctx, command| {
            edit(
                ctx,
                &Attrs::from([(key.into(), Value::from(value(command)))]),
                Change::Set,
            )
        });
        p.command::<U>(move |ctx, _| {
            ctx.plan_chain([
                StoredCommand::new(set(None)),
                StoredCommand::new(RemoveEmpty),
            ])
        });
    });
    #[cfg(feature = "gpui")]
    if !view_types.is_empty() {
        return gpdoc::PluginViewExt::gpui_default_view(plugin, move || {
            let mut view = gpdoc::ViewPlugin::named(format!("{key}Format"));
            for kind in &view_types {
                view = view.format_if_installed(kind.clone(), move |attrs, format| {
                    if let Some(Value::String(value)) = attrs.get(key) {
                        format_property(key, value, format);
                    }
                });
            }
            view
        });
    }
    plugin
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    Set,
    Toggle,
}

fn selection_range(ctx: CommandContext<'_>) -> Range<usize> {
    let bounds = ctx.selection().bounds(ctx.document());
    bounds.start.0..bounds.end.0
}

fn existing(marks: &[Mark]) -> Option<&Mark> {
    marks.iter().find(|mark| mark.kind == STYLE)
}

fn matches(mark: &Mark, args: &Attrs) -> bool {
    args.iter()
        .all(|(key, value)| mark.attrs.get(key) == Some(value))
}

fn empty(mark: &Mark) -> bool {
    mark.attrs
        .values()
        .all(|value| value.is_null() || value.as_str() == Some(""))
}

fn remove(range: Range<usize>, mark: &Mark) -> Edit {
    Edit::RemoveExactMark {
        range: DocPos(range.start)..DocPos(range.end),
        mark: mark.clone(),
    }
}

fn merged(schema: &Schema, old: Option<&Mark>, args: &Attrs) -> Result<Mark, Error> {
    let mut attrs = old.map(|mark| mark.attrs.clone()).unwrap_or_default();
    attrs.extend(args.clone());
    Ok(schema.mark(STYLE, attrs)?)
}

fn edit(
    ctx: CommandContext<'_>,
    args: &Attrs,
    change: Change,
) -> Result<Option<Transaction>, Error> {
    let range = selection_range(ctx);
    let doc = ctx.document().root();
    let schema = ctx.schema();
    let Some(spec) = ctx.schema().mark_spec(STYLE) else {
        return Ok(None);
    };
    if !args.keys().all(|key| spec.attrs.contains_key(key)) {
        return Ok(None);
    }
    let mut tx = ctx.transaction();
    if let Some(marks) = ctx.cursor_marks()? {
        let parent = doc.resolve(range.start)?;
        if !schema
            .node_type(&parent.parent().kind)
            .is_some_and(|ty| ty.allows_mark(STYLE))
        {
            return Ok(None);
        }
        let old = existing(&marks);
        let next = if change == Change::Toggle && old.is_some_and(|mark| matches(mark, args)) {
            old.unwrap().remove_from_set(&marks)
        } else {
            let style = merged(schema, old, args)?;
            let next = style.add_to_set(schema, &marks);
            if !next.contains(&style) {
                return Ok(None);
            }
            next
        };
        tx.set_stored_marks(next);
        return Ok(Some(tx));
    }

    let mut runs = Vec::new();
    doc.nodes_between(range.start, range.end, |node, pos, parent, _| {
        if node.is_inline()
            && schema
                .node_type(&parent.kind)
                .is_some_and(|ty| ty.allows_mark(STYLE))
            && !node
                .marks
                .iter()
                .any(|mark| mark.kind != STYLE && schema.excludes(&mark.kind, STYLE))
        {
            runs.push((
                range.start.max(pos)..range.end.min(pos + node.size()),
                existing(&node.marks).cloned(),
            ));
        }
        true
    })?;
    if runs.is_empty() {
        return Ok(None);
    }
    let toggle_off = change == Change::Toggle
        && runs
            .iter()
            .all(|(_, old)| old.as_ref().is_some_and(|mark| matches(mark, args)));
    for (span, old) in runs {
        if toggle_off {
            if let Some(mark) = old {
                tx.push(remove(span, &mark));
            }
        } else {
            let style = merged(schema, old.as_ref(), args)?;
            tx.push(Edit::SetMark {
                range: DocPos(span.start)..DocPos(span.end),
                mark: style,
            });
        }
    }
    Ok(Some(tx))
}

/// Render the stored CSS properties on the supplied node and mark types.
#[cfg(feature = "gpui")]
pub fn text_style_format(types: impl IntoIterator<Item = impl Into<String>>) -> gpdoc::ViewPlugin {
    let mut view = gpdoc::ViewPlugin::named("textStyleFormat");
    for kind in types {
        view = view.format_if_installed(kind, |attrs, format| {
            for (key, value) in attrs {
                if let Some(value) = value.as_str() {
                    format_property(key, value, format);
                }
            }
        });
    }
    view
}

#[cfg(feature = "gpui")]
fn format_property(key: &str, value: &str, format: &mut TextFormat) {
    match key {
        "color" | "backgroundColor" => {
            if let Ok(c) = value.parse::<svgtypes::Color>() {
                let rgba = Some([c.red, c.green, c.blue, c.alpha]);
                if key == "color" {
                    format.color = rgba
                } else {
                    format.background = rgba
                }
            }
        }
        "fontFamily" => {
            if let Ok(names) = svgtypes::parse_font_families(value) {
                let mut names = names.into_iter().map(|name| match name {
                    svgtypes::FontFamily::Named(name) => name,
                    generic => generic.to_string(),
                });
                if let Some(first) = names.next() {
                    format.font_family = Some(first.into());
                    let fallbacks: Vec<_> = names.collect();
                    format.font_fallbacks = (!fallbacks.is_empty()).then(|| fallbacks.into());
                }
            }
        }
        "fontSize" => {
            if let Some(len) = css_length(value, false) {
                format.font_size = Some(len);
            }
        }
        "lineHeight" => {
            if let Some(len) = css_length(value, true) {
                format.line_height = Some(len);
            }
        }
        _ => {}
    }
}

#[cfg(feature = "gpui")]
fn css_length(value: &str, allow_multiplier: bool) -> Option<TextLength> {
    use svgtypes::{Length, LengthUnit};
    let value = value.trim();
    if let Some(rem) = value.strip_suffix("rem") {
        let n = rem.parse::<f32>().ok()?;
        return (n.is_finite() && n > 0.).then_some(TextLength::Rem(n));
    }
    let Length { number, unit } = value.parse().ok()?;
    let n = number as f32;
    if !n.is_finite() || n <= 0. {
        return None;
    }
    Some(match unit {
        LengthUnit::Px => TextLength::Pixels(n),
        LengthUnit::Pt => TextLength::Pixels(n * 96. / 72.),
        LengthUnit::Pc => TextLength::Pixels(n * 16.),
        LengthUnit::In => TextLength::Pixels(n * 96.),
        LengthUnit::Cm => TextLength::Pixels(n * 96. / 2.54),
        LengthUnit::Mm => TextLength::Pixels(n * 96. / 25.4),
        LengthUnit::Percent => TextLength::Em(n / 100.),
        LengthUnit::Em => TextLength::Em(n),
        LengthUnit::None if allow_multiplier => TextLength::Em(n),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_can_target_a_custom_set_of_nodes_and_marks() {
        let types = ["paragraph", "textStyle", "heading"];
        let plugins = [
            gpdoc_plugin_paragraph::paragraph(),
            text_style(),
            StyleProperty::Color.on(types),
        ];
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugins(plugins.clone())
            .build()
            .unwrap();
        let attrs = Attrs::from([("color".into(), Value::String("#00448880".into()))]);
        let editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::new("paragraph", attrs, Content::Inline(vec![]))],
        )
        .unwrap();
        assert_eq!(
            editor.document().blocks()[0].attrs["color"].as_str(),
            Some("#00448880")
        );
        let block = &editor.document().blocks()[0];
        let mut bare = TextFormat::default();
        config.presentation().style_node(block, &mut bare);
        assert_eq!(bare.color, None);

        #[cfg(feature = "gpui")]
        {
            let config = gpdoc::EditorConfig::builder()
                .plugins(plugins)
                .build()
                .unwrap();
            let mut format = TextFormat::default();
            config.presentation().style_node(block, &mut format);
            assert_eq!(format.color, Some([0, 68, 136, 128]));
        }
        assert!(
            config
                .schema()
                .mark_spec("textStyle")
                .unwrap()
                .attrs
                .contains_key("color")
        );
    }
}
