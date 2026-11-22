use gpdoc_plugin::Plugin;

pub fn plugin(name: &str) -> Option<Plugin> {
    Some(match name {
        "Image" => gpdoc_plugin_image::image(),
        "Audio" => gpdoc_plugin_audio::audio(Default::default()),
        "Mention" => gpdoc_plugin_mention::mention(),
        "Mathematics" => gpdoc_plugin_mathematics::mathematics(),
        "RubyText" => gpdoc_plugin_ruby_text::ruby_text(),
        _ => return None,
    })
}
