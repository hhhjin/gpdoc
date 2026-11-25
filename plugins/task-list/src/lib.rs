//! Task lists contain task items whose checked attribute survives JSON exchange.
use gpdoc_plugin::prelude::*;
use gpdoc_plugin_list_item::{lift_item, sink_item, split_item, toggle_list};

unit_command!(pub Toggle, "toggleTaskList");
unit_command!(pub Split, "splitTaskItem");
unit_command!(pub Lift, "liftTaskItem");
unit_command!(pub Sink, "sinkTaskItem");

gpdoc_plugin::plugin_shortcuts! {
    pub Shortcuts {
        toggle: Toggle => ["Mod-Shift-9"],
    }
}

pub fn task_list() -> Plugin<Shortcuts> {
    let spec = NodeSpec::block("taskList")
        .group("block")
        .group("list")
        .content("taskItem+");
    Plugin::new("taskList", |p| {
        p.node(spec);
        p.command::<Toggle>(|ctx, _| toggle_list(ctx, "taskList", "taskItem", &Attrs::new()));
    })
}

#[derive(Clone)]
pub struct TaskItemConfig {
    lift: ShortcutBinding,
    sink: Option<ShortcutBinding>,
}

impl Default for TaskItemConfig {
    fn default() -> Self {
        Self {
            lift: ShortcutBinding::new(Lift, [key!("Shift-Tab")]),
            sink: None,
        }
    }
}

impl TaskItemConfig {
    pub fn lift(&mut self) -> &mut ShortcutKeys {
        self.lift.keys()
    }
    /// Nesting is available when the plugin was built with nested task items.
    pub fn sink(&mut self) -> Option<&mut ShortcutKeys> {
        self.sink.as_mut().map(ShortcutBinding::keys)
    }
}
impl PluginShortcuts for TaskItemConfig {
    fn into_bindings(self) -> impl IntoIterator<Item = ShortcutBinding> {
        std::iter::once(self.lift).chain(self.sink)
    }
}

gpdoc_plugin::plugin_config! {
    impl TaskItemConfig {
        input_rules InputRules { checkbox: input }
    }
}

pub fn task_item() -> Plugin<TaskItemConfig> {
    task_item_with_nesting(false)
}

pub fn task_item_with_nesting(nested: bool) -> Plugin<TaskItemConfig> {
    let plugin = Plugin::<TaskItemConfig>::new("taskItem", |p| {
        p.node(
            NodeSpec::block("taskItem")
                .content(if nested {
                    "paragraph block*"
                } else {
                    "paragraph+"
                })
                .attrs(AttrSpecs::from([(
                    "checked".into(),
                    AttrSpec::with_default(AttrType::Bool, Value::Bool(false)),
                )])),
        );
        p.drag_handle("taskItem", ["paragraph"]);
        p.command::<Split>(|ctx, _| split_item(ctx, "taskItem"));
        p.on_break("taskItem", |ctx| ctx.plan(Split));
        p.command::<Lift>(|ctx, _| lift_item(ctx, "taskItem"));

        p.input_rules()
            .checkbox()
            .set(gpdoc_plugin_list_item::list_input_rule(
                "taskList",
                "taskItem",
                r"^\s*\[([ x]?)\]\s$",
                |mark| {
                    Some((
                        Attrs::new(),
                        Attrs::from([("checked".into(), Value::Bool(mark == "x"))]),
                    ))
                },
            ));
        if nested {
            p.shortcuts().sink = Some(ShortcutBinding::new(Sink, [key!("Tab")]));
            p.command::<Sink>(|ctx, _| sink_item(ctx, "taskItem"));
        }
    });
    #[cfg(feature = "gpui")]
    let plugin = {
        use gpdoc::PluginViewExt;
        plugin.gpui_default_view(view::default_tasks)
    };
    plugin
}

#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{TaskRow, task_rows};
