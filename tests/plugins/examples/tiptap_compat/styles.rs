use gpdoc_plugin::Plugin;
use gpdoc_plugin_text_align::text_align;
use gpdoc_plugin_text_style::{
    background_color, color, font_family, font_size, line_height, text_style,
};

pub fn plugin(name: &str) -> Option<Plugin> {
    Some(match name {
        "TextStyle" => text_style(),
        "Color" => color(),
        "BackgroundColor" => background_color(),
        "FontFamily" => font_family(),
        "FontSize" => font_size(),
        "LineHeight" => line_height(),
        "TextAlign" => text_align(["paragraph", "heading"]),
        _ => return None,
    })
}
