//! Task lists contain task items whose checked attribute survives JSON exchange.
use gpdoc_plugin::prelude::*;
use gpdoc_plugin_list_item::{lift_item, sink_item, split_item, toggle_list};

unit_command!(pub Toggle, "toggleTaskList");
unit_command!(pub Split, "splitTaskItem");
unit_command!(pub Lift, "liftTaskItem");
unit_command!(pub Sink, "sinkTaskItem");

pub fn task_list() -> Plugin {
    let spec = NodeSpec::block("taskList")
        .group("block")
        .group("list")
        .content("taskItem+");
    Plugin::new("taskList", |p| {
        p.node(spec);
        p.command::<Toggle>(|ctx, _| toggle_list(ctx, "taskList", "taskItem", &Attrs::new()));
        p.shortcut("Mod-Shift-9", Toggle);
    })
}

pub fn task_item() -> Plugin {
    task_item_with_nesting(false)
}

pub fn task_item_with_nesting(nested: bool) -> Plugin {
    let plugin = Plugin::new("taskItem", |p| {
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
        p.command::<Split>(|ctx, _| split_item(ctx, "taskItem"));
        p.on_break("taskItem", |ctx| ctx.plan(Split));
        p.command::<Lift>(|ctx, _| lift_item(ctx, "taskItem"));
        p.shortcut("Shift-Tab", Lift);
        p.input_rule(gpdoc_plugin_list_item::list_input_rule(
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
            p.command::<Sink>(|ctx, _| sink_item(ctx, "taskItem"));
            p.shortcut("Tab", Sink);
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
