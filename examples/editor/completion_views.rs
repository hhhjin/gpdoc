//! Application-owned views for the demo's two completion plugins.
use gpdoc::ViewPlugin;
use gpdoc_plugin_mention::{MentionMenu, mention_picker};
use gpdoc_plugin_slash_command::{SlashMenu, slash_picker};
use gpui::{Div, div, prelude::*, px, rgb};

fn panel() -> Div {
    div()
        .min_w(px(220.))
        .p(px(4.))
        .rounded_md()
        .bg(rgb(0xffffff))
        .shadow_md()
        .border_1()
        .border_color(rgb(0xe3e2df))
}

pub fn slash_menu() -> ViewPlugin {
    slash_picker(|menu: SlashMenu, _, _| {
        let list = menu.list(6, |item| {
            item.element()
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded_sm()
                .px(px(8.))
                .h(px(32.))
                .text_size(px(14.))
                .line_height(px(20.))
                .when(item.is_active(), |el| el.bg(rgb(0xf1f1ef)))
                .child(super::menu_icons::icon(item.value.id, 18.).text_color(rgb(0x64635e)))
                .child(item.value.label)
        });
        menu.popover(panel().child(list))
    })
}

pub fn mention_menu() -> ViewPlugin {
    mention_picker(super::completion::people, |menu: MentionMenu, _, _| {
        let people = menu.list(6, |item| {
            let person = item.value;
            let name = person
                .label
                .as_ref()
                .or(person.id.as_ref())
                .cloned()
                .unwrap_or_default();
            item.element()
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded_sm()
                .px(px(8.))
                .py(px(2.))
                .when(item.is_active(), |el| el.bg(rgb(0xf1f1ef)))
                .child(div().text_color(rgb(0x787774)).child("@"))
                .child(name)
        });
        menu.popover(panel().child(people))
    })
}
