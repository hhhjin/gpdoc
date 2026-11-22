//! Receive dropped files and clipboard images in a GPUI editor.
use gpdoc::{EditorView, ViewBehavior, ViewPlugin};
use gpdoc_plugin::core::DocPos;
use gpui::{ClipboardEntry, ClipboardItem, Context, ExternalPaths, Image, Window};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{path::PathBuf, rc::Rc};

pub enum FileInput<'a> {
    Drop { paths: &'a [PathBuf], at: DocPos },
    Paste(&'a [&'a Image]),
}

type Callback =
    dyn Fn(&mut EditorView, FileInput<'_>, &mut Window, &mut Context<EditorView>) -> bool;

pub fn file_handler(
    handle: impl Fn(&mut EditorView, FileInput<'_>, &mut Window, &mut Context<EditorView>) -> bool
    + 'static,
) -> ViewPlugin {
    let handle: Rc<Callback> = Rc::new(handle);
    let paste = handle.clone();
    ViewPlugin::new("fileHandler", move || FilePaste(paste.clone())).on_text_drop(
        move |view, files: &ExternalPaths, at, window, cx| {
            handle(
                view,
                FileInput::Drop {
                    paths: files.paths(),
                    at,
                },
                window,
                cx,
            )
        },
    )
}
struct FilePaste(Rc<Callback>);
impl ViewBehavior for FilePaste {
    fn paste(
        &self,
        view: &mut EditorView,
        item: &ClipboardItem,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> bool {
        let images: Vec<_> = item
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image),
                _ => None,
            })
            .collect();
        !images.is_empty() && (self.0)(view, FileInput::Paste(&images), window, cx)
    }
}
