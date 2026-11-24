use gpui::{Svg, prelude::*, px, svg};

use gpui_icons::LucideIcon;

// Keys match the demo's formatting marks and slash command IDs.
pub(super) const ICONS: &[(&str, LucideIcon)] = &[
    ("bold", LucideIcon::Bold),
    ("italic", LucideIcon::Italic),
    ("underline", LucideIcon::Underline),
    ("strike", LucideIcon::Strikethrough),
    ("code", LucideIcon::Code),
    ("paragraph", LucideIcon::Pilcrow),
    ("heading.h1", LucideIcon::Heading1),
    ("heading.h2", LucideIcon::Heading2),
    ("heading.h3", LucideIcon::Heading3),
    ("heading.h4", LucideIcon::Heading4),
    ("heading.h5", LucideIcon::Heading5),
    ("heading.h6", LucideIcon::Heading6),
    ("task-list", LucideIcon::ListChecks),
    ("bullet-list", LucideIcon::List),
    ("blockquote", LucideIcon::TextQuote),
    ("code-block", LucideIcon::SquareCode),
    ("ordered-list", LucideIcon::ListOrdered),
    ("details", LucideIcon::ListCollapse),
    ("horizontal-rule", LucideIcon::Minus),
    ("table", LucideIcon::Table),
    ("block-math", LucideIcon::Sigma),
    ("image", LucideIcon::Image),
    ("audio", LucideIcon::AudioLines),
];

pub(super) fn icon(id: &'static str, size: f32) -> Svg {
    svg().path(id).size(px(size)).flex_none()
}
