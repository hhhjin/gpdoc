use gpdoc::ViewPlugin;
use gpdoc_plugin::core::{Node, Value};
use gpui::{AnyElement, App, Window, div, prelude::*};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;

/// One editable list item and its position among the immediate list's children.
pub struct ListItemRow<'a> {
    pub node: &'a Node,
    pub index: usize,
    /// Initial counter of an ordered list. Unordered lists have no counter.
    pub start: Option<i64>,
    /// Decimal ordinal or bullet. Custom renderers may use the parent list attributes instead.
    pub marker: String,
    pub list: &'a Node,
    pub content: AnyElement,
}

pub(super) fn default_items() -> ViewPlugin {
    list_item_body(|row, _, _| {
        div()
            .flex()
            .items_start()
            .child(div().flex_none().child(row.marker))
            .child(div().flex_1().min_w_0().child(row.content))
    })
}

/// Compose a marker with its item's editor-owned contents. Nested lists have their own counters.
pub fn list_item_body<E: IntoElement>(
    render: impl Fn(ListItemRow<'_>, &mut Window, &mut App) -> E + 'static,
) -> ViewPlugin {
    ViewPlugin::named("listItem")
        .for_plugin("listItem")
        .wrap_node("listItem", move |block, content, window, cx| {
            let at = block
                .displayed_position(cx)
                .expect("a rendered list item has a document position");
            let list = at.parent();
            let index = at.index(at.depth());
            let start = (list.kind == "orderedList")
                .then(|| list.attrs.get("start").and_then(Value::as_i64).unwrap_or(1));
            let marker = start.map_or_else(
                || "•".into(),
                |start| format!("{}.", i128::from(start) + index as i128),
            );
            render(
                ListItemRow {
                    node: block.node,
                    index,
                    start,
                    marker,
                    list,
                    content,
                },
                window,
                cx,
            )
        })
}
