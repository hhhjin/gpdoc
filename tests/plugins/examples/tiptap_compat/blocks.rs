use gpdoc_plugin::Plugin;
use gpdoc_plugin_details::details;

pub fn plugin(name: &str) -> Option<Plugin> {
    Some(match name {
        "Details" => details(),
        "CodeBlockLowlight" => gpdoc_plugin_code_block_lowlight::code_block_lowlight().into(),
        _ => return None,
    })
}
