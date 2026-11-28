use gpdoc_plugin::Plugin;
use gpdoc_plugin_details::details;

pub fn plugin(name: &str) -> Option<Plugin> {
    Some(match name {
        "Details" => details(),
        _ => return None,
    })
}
