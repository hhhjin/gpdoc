# gpdoc

gpdoc is a headless rich-text editor framework for Rust with GPUI support. Use plugins to add tables, mentions, and task lists.

- [Try the editor in your browser](https://gpdoc.dev/playground/)
- [Add the editor to your app](https://gpdoc.dev/docs/quickstart/)

Open a GPUI window with the paragraph and bold plugins.

```rust
use gpdoc::{DocumentSource, EditorConfig};
use gpdoc_plugin_bold::bold;
use gpdoc_plugin_paragraph::paragraph;
use gpui::{Application, WindowOptions};

fn main() {
    Application::new().run(|cx| {
        let config = EditorConfig::builder()
            .plugin(paragraph()).plugin(bold())
            .build()
            .expect("configure editor");

        cx.open_window(WindowOptions::default(), |_, cx| {
            config.open(DocumentSource::Empty, cx).expect("open editor")
        })
        .expect("open editor window");
    });
}
```

Click the editor, type and select some text, then press **Command+B** on macOS or **Ctrl+B** on Linux to make it bold.

With the matching plugins, gpdoc can read and write ProseMirror and Tiptap document JSON.

[Write a plugin](https://gpdoc.dev/docs/plugins/custom/first-plugin/) · [MIT license](LICENSE)
