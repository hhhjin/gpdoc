use gpdoc::{Selection, StoredCommand, ViewPlugin};
use gpdoc_plugin_bubble_menu::bubble_menu;
use gpui::{div, prelude::*, px, rgb};

pub(super) const DRAG_DELAY: std::time::Duration = std::time::Duration::from_millis(180);

pub fn format_menu() -> ViewPlugin {
    bubble_menu()
        .drag_delay(DRAG_DELAY)
        .show_when(|view| matches!(view.editor().selection(), Selection::Text { anchor, head } if anchor != head))
        .with_view(|view, _, cx| {
            let controls = [
                ("bold", StoredCommand::new(gpdoc_plugin_bold::Toggle)),
                ("italic", StoredCommand::new(gpdoc_plugin_italic::Toggle)),
                ("underline", StoredCommand::new(gpdoc_plugin_underline::Toggle)),
                ("strike", StoredCommand::new(gpdoc_plugin_strike::Toggle)),
                ("code", StoredCommand::new(gpdoc_plugin_inline_code::Toggle)),
            ];
            div()
                .flex()
                .items_center()
                .text_size(px(14.))
                .text_color(rgb(0x37352f))
                .p(px(3.))
                .rounded_md()
                .bg(rgb(0xffffff))
                .border_1()
                .border_color(rgb(0xe9e9e7))
                .shadow_md()
                .children(controls.into_iter().map(|(mark, command)| {
                    let active = view.read(cx).editor().mark_active(mark).unwrap_or(false);
                    let view = view.clone();
                    div()
                        .id(mark)
                        .debug_selector(move || format!("format-{mark}"))
                        .size(px(28.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .cursor_pointer()
                        .when(active, |el| el.bg(rgb(0xeaf2fb)).text_color(rgb(0x236eae)))
                        .when(!active, |el| el.hover(|style| style.bg(rgb(0xf1f1ef))))
                        .on_click(move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                let _ = view.run_stored(&command, window, cx);
                            });
                        })
                        .child(super::menu_icons::icon(mark, 16.).text_color(rgb(if active { 0x236eae } else { 0x37352f })))
                }))
        })
}
