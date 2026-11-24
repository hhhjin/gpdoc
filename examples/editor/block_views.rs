use gpdoc::ViewPlugin;
use gpui::{FontWeight, div, prelude::*, px, rgb};

pub const MONOSPACE: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "monospace"
};

pub fn block_spacing() -> ViewPlugin {
    ViewPlugin::named("blockSpacing").wrap_blocks(|slot, body, _, _| {
        let kind = slot.node.kind.as_str();
        let (top, bottom) = match kind {
            "heading" => (16.5, 3.),
            "paragraph" => (13.5, 3.),
            "blockquote" => (14., 16.),
            "bulletList" | "orderedList" => (7.5, 16.),
            "taskList" => (10.5, 16.5),
            "table" => (22., 3.),
            "horizontalRule" => (2., 3.),
            _ => (10., 3.),
        };
        let block = div()
            .pb(px(bottom))
            .when(slot.before.0 != 0, |el| el.pt(px(top)));
        match kind {
            "horizontalRule" => block.opacity(0.26).child(body),
            "orderedList" | "bulletList" => block.ml(-px(2.)).child(body),
            "table" => block.pl(px(6.)).child(body),
            "image" => {
                let width = slot.node.attrs.get("width").and_then(|v| v.as_u64());
                block.child(
                    div()
                        .mx_auto()
                        .w_full()
                        .when_some(width, |el, w| el.max_w(px(w as f32)))
                        .child(body),
                )
            }
            _ => block.child(body),
        }
    })
}

pub fn drag_handle() -> ViewPlugin {
    gpdoc_plugin_drag_handle::drag_handle().with_view(|_, _, _| {
        let dot = || div().size(px(3.)).rounded_full().bg(rgb(0x9b9a97));
        div()
            .w(px(16.))
            .h(px(24.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(2.))
            .children((0..3).map(|_| div().flex().gap(px(2.)).child(dot()).child(dot())))
    })
}

pub fn heading() -> ViewPlugin {
    gpdoc_plugin_heading::heading_block(|block, _, _| {
        let level = block
            .node
            .attrs
            .get("level")
            .and_then(|value| value.as_u64());
        let size = match level {
            Some(1) => 30.,
            Some(2) => 24.,
            _ => 20.,
        };
        div()
            .text_size(px(size))
            .line_height(px(size * 1.25))
            .font_weight(FontWeight::SEMIBOLD)
            .child(block.content)
    })
}

pub fn quote() -> ViewPlugin {
    gpdoc_plugin_blockquote::quote_block(|block, _, _| {
        div()
            .pl(px(22.))
            .pr(px(24.))
            .border_l(px(3.))
            .border_color(rgb(0x2c2c2c))
            .child(block.content)
    })
}

pub fn code() -> ViewPlugin {
    gpdoc_plugin_code_block::code_block_body(|block, _, _| {
        div()
            .px(px(24.))
            .py(px(36.))
            .rounded(px(8.))
            .bg(rgb(0xf9f8f6))
            .text_size(px(14.))
            .line_height(px(20.))
            .font_family(MONOSPACE)
            .child(block.content)
    })
}

pub fn list_item() -> ViewPlugin {
    gpdoc_plugin_list_item::list_item_body(|row, _, _| {
        div()
            .flex()
            .items_start()
            .child(
                div()
                    .min_w(px(32.))
                    .flex_none()
                    .pl(px(9.))
                    .child(row.marker),
            )
            .child(div().flex_1().min_w_0().child(row.content))
            .pt(px(6.))
    })
}

pub fn tasks() -> ViewPlugin {
    gpdoc_plugin_task_list::task_rows(|task, _, _| {
        let indicator = div()
            .size(px(16.))
            .rounded(px(2.))
            .border_2()
            .border_color(rgb(0x2c2c2c))
            .when(task.checked, |el| {
                el.flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x2383e2))
                    .border_color(rgb(0x2383e2))
                    .text_color(rgb(0xffffff))
                    .text_size(px(13.))
                    .line_height(px(14.))
                    .font_weight(FontWeight::BOLD)
                    .child("✓")
            });
        let checkbox = task
            .checkbox
            .flex_none()
            .w(px(24.))
            .h(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(3.))
            .hover(|el| el.bg(rgb(0xf1f1ef)))
            .child(indicator);
        div()
            .flex()
            .items_start()
            .py(px(3.))
            .gap(px(6.))
            .child(checkbox)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when(task.checked, |el| {
                        el.text_color(rgb(0x787774)).line_through()
                    })
                    .child(task.content),
            )
    })
}

pub fn table_cell(
    mut cell: gpdoc_plugin_table::TableCell,
    _: &mut gpui::Window,
    _: &mut gpui::App,
) -> gpdoc_plugin_table::TableCell {
    let background = div()
        .border_1()
        .border_color(rgb(0xe3e2df))
        .when(cell.is_header(), |el| el.bg(rgb(0xf7f6f3)));
    cell = cell.layer(background);
    if cell.selected {
        cell = cell.layer(div().bg(gpui::rgba(0x337ea933)));
    }
    let header = cell.is_header();
    cell.content(
        div()
            .pl(px(10.))
            .pr(px(8.))
            .py(px(6.))
            .text_size(px(14.))
            .line_height(px(23.))
            .when(header, |el| el.font_weight(FontWeight::SEMIBOLD)),
    )
}

pub fn audio() -> ViewPlugin {
    gpdoc_plugin_audio::audio_controls(|clip, _, cx| {
        let button = clip
            .play_button(cx)
            .size_full()
            .bg(rgb(0xf1f2f3))
            .rounded_full()
            .text_size(px(14.))
            .text_center()
            .whitespace_nowrap();
        if clip.source() == Some("/examples/chime.wav") {
            button
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                    #[cfg(target_family = "wasm")]
                    cx.open_url("/examples/chime.wav");
                    #[cfg(not(target_family = "wasm"))]
                    cx.open_with_system(std::path::Path::new(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/public/examples/chime.wav"
                    )));
                    cx.stop_propagation();
                })
                .child("▶  Start chime · WAV")
        } else {
            button.child(clip.source().unwrap_or("Audio").to_owned())
        }
    })
}

pub fn details() -> ViewPlugin {
    gpdoc_plugin_details::disclosure(|header, _, _| {
        div()
            .flex()
            .items_start()
            .gap(px(8.))
            .child(
                header
                    .trigger
                    .flex_none()
                    .w(px(24.))
                    .text_center()
                    .text_size(px(12.))
                    .child(if header.open { "▾" } else { "▸" }),
            )
            .child(div().flex_1().min_w_0().child(header.content))
    })
    .wrap_node("detailsContent", |_, content, _, _| {
        div().pl(px(32.)).child(content)
    })
}
