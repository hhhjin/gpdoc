use gpdoc_plugin::Plugin;
use gpdoc_plugin_table::table;
use gpdoc_plugin_task_list::{task_item, task_list};

pub fn plugin(name: &str) -> Option<Plugin> {
    Some(match name {
        "Table" | "TableKit" => table().into(),
        "TaskList" => task_list().into(),
        "TaskItem" => task_item().into(),
        "ListKeymap" => gpdoc_plugin_list_keymap::list_keymap().into(),
        _ => return None,
    })
}
