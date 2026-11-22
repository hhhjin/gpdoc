use gpdoc::ViewPlugin;
use gpdoc_plugin::core::Value;
use gpui::{AnyElement, App, Div, MouseButton, Stateful, Window, div, prelude::*, px, rgb};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;

/// Parts of one editable task. Place `content` once in the row.
pub struct TaskRow {
    pub checked: bool,
    /// Click target wired to the document's checked attribute.
    pub checkbox: Stateful<Div>,
    pub content: AnyElement,
}

impl TaskRow {
    /// Render a square checkbox beside the task's editable body.
    pub fn render(self, _: &mut Window, _: &mut App) -> Div {
        let checkbox = self
            .checkbox
            .flex_none()
            .w(px(24.))
            .h(px(32.))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .size(px(14.))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(0x68737d))
                    .when(self.checked, |indicator| {
                        indicator.bg(rgb(0x346bb0)).border_color(rgb(0x346bb0))
                    }),
            );
        div()
            .flex()
            .items_start()
            .gap(px(4.))
            .child(checkbox)
            .child(div().flex_1().min_w_0().child(self.content))
    }
}

pub(super) fn default_tasks() -> ViewPlugin {
    task_rows(|task, _, _| {
        div()
            .flex()
            .items_start()
            .child(
                task.checkbox
                    .flex_none()
                    .child(if task.checked { "☑" } else { "☐" }),
            )
            .child(div().flex_1().min_w_0().child(task.content))
    })
}

/// Compose an interactive checkbox and the editor-owned body of each task.
pub fn task_rows<E: IntoElement>(
    render: impl Fn(TaskRow, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    ViewPlugin::named("taskRows")
        .wrap_node("taskItem", move |block, content, window, cx| {
            let checked = block.node.attrs.get("checked") == Some(&Value::Bool(true));
            let mut attrs = block.node.attrs.clone();
            attrs.insert("checked".into(), Value::Bool(!checked));
            let toggle = block.set_attrs(attrs);
            let before = block.before;
            let checkbox = div()
                .id(("gpdoc-task-checkbox", before.0))
                .debug_selector(move || format!("gpdoc-task-checkbox-{}", before.0))
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    block.view.update(cx, |editor, cx| {
                        if editor.is_composing() {
                            return;
                        }
                        editor.focus_in(window, cx);
                        if let Err(error) = editor.apply_node_view(toggle.clone(), window, cx) {
                            eprintln!("Could not toggle task: {error}");
                        }
                    });
                    cx.stop_propagation();
                });
            render(
                TaskRow {
                    checked,
                    checkbox,
                    content,
                },
                window,
                cx,
            )
        })
        .for_plugin("taskItem")
}
