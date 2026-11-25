use gpdoc_plugin::Plugin;

pub fn plugin(name: &str) -> Option<Plugin> {
    Some(match name {
        "Image" => gpdoc_plugin_image::image().into(),
        "Audio" => gpdoc_plugin_audio::audio(Default::default()).into(),
        "Mention" => gpdoc_plugin_mention::mention().into(),
        "Mathematics" => gpdoc_plugin_mathematics::mathematics().into(),
        "RubyText" => gpdoc_plugin_ruby_text::ruby_text(),
        _ => return None,
    })
}
