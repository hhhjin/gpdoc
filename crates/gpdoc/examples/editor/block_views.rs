use gpdoc::ViewPlugin;
use gpui::{FontWeight, div, prelude::*, px, rgb};

pub fn block_spacing() -> ViewPlugin {
    ViewPlugin::named("blockSpacing").wrap_blocks(|slot, body, _, _| {
        let top = match slot.node.kind.as_str() {
            _ if slot.before.0 == 0 => 0.,
            "heading" => 22.,
            "table" | "image" | "audio" | "blockMath" | "codeBlock" => 10.,
            "horizontalRule" => 12.,
            _ => 4.,
        };
        div().group("demo-block").pt(px(top)).pb(px(3.)).child(body)
    })
}

pub fn drag_handle() -> ViewPlugin {
    gpdoc_plugin_drag_handle::drag_handle().with_view(|slot, _, _| {
        let dot = || div().size(px(3.)).rounded_full().bg(rgb(0x9b9a97));
        div()
            .w(px(16.))
            .h(px(24.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(2.))
            .when(
                slot.part == gpdoc_plugin_drag_handle::DragHandlePart::Grip,
                |el| {
                    el.opacity(0.)
                        .group_hover("demo-block", |style| style.opacity(1.))
                },
            )
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
            Some(1) => 40.,
            Some(2) => 28.,
            Some(3) => 22.,
            Some(4) => 18.,
            Some(5) => 16.,
            _ => 15.,
        };
        div()
            .text_size(px(size))
            .line_height(px(size * 1.25))
            .font_weight(if level == Some(1) {
                FontWeight::BOLD
            } else {
                FontWeight::SEMIBOLD
            })
            .child(block.content)
    })
}

pub fn quote() -> ViewPlugin {
    gpdoc_plugin_blockquote::quote_block(|block, _, _| {
        div()
            .pl(px(12.))
            .border_l_2()
            .border_color(rgb(0x37352f))
            .child(block.content)
    })
}

pub fn code() -> ViewPlugin {
    gpdoc_plugin_code_block::code_block_body(|block, _, _| {
        div()
            .px(px(18.))
            .py(px(14.))
            .rounded_sm()
            .bg(rgb(0xf7f6f3))
            .text_size(px(14.))
            .font_family("monospace")
            .child(block.content)
    })
}

pub fn list_item() -> ViewPlugin {
    gpdoc_plugin_list_item::list_item_body(|row, _, _| {
        div()
            .flex()
            .items_start()
            .gap(px(8.))
            .child(div().flex_none().child(row.marker))
            .child(div().flex_1().min_w_0().child(row.content))
    })
}

pub fn tasks() -> ViewPlugin {
    gpdoc_plugin_task_list::task_rows(|task, _, _| {
        let indicator = div()
            .size(px(16.))
            .rounded(px(2.))
            .border_2()
            .border_color(rgb(0x37352f))
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
            .h(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(3.))
            .hover(|el| el.bg(rgb(0xf1f1ef)))
            .child(indicator);
        div()
            .flex()
            .items_start()
            .gap(px(4.))
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
    cell.content(div().p(px(4.)))
}

pub fn audio() -> ViewPlugin {
    gpdoc_plugin_audio::audio_controls(|clip, _, cx| {
        let button = clip
            .play_button(cx)
            .size_full()
            .bg(rgb(0xf7f6f3))
            .rounded_sm();
        if clip.source() == Some("/examples/chime.wav") {
            button
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                    #[cfg(target_family = "wasm")]
                    cx.open_url("/examples/chime.wav");
                    #[cfg(not(target_family = "wasm"))]
                    cx.open_with_system(std::path::Path::new(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../../../web/public/examples/chime.wav"
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
            .gap(px(6.))
            .child(
                header
                    .trigger
                    .flex_none()
                    .w(px(16.))
                    .text_size(px(12.))
                    .child(if header.open { "−" } else { "+" }),
            )
            .child(div().flex_1().min_w_0().child(header.content))
    })
}
