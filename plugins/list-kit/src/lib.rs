use gpdoc_plugin::prelude::Plugin;
use gpdoc_plugin_bullet_list::bullet_list;
use gpdoc_plugin_list_item::list_item;
use gpdoc_plugin_list_keymap::list_keymap;
use gpdoc_plugin_ordered_list::ordered_list;
use gpdoc_plugin_task_list::{task_item, task_list};

pub fn list_kit() -> Vec<Plugin> {
    vec![
        bullet_list().into(),
        list_item().into(),
        list_keymap().into(),
        ordered_list().into(),
        task_item().into(),
        task_list().into(),
    ]
}
