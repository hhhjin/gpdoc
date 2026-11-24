mod demo;

fn main() {
    demo::run(gpui::Application::new(), |cx| {
        #[cfg(target_os = "macos")]
        {
            gpui::actions!(gpdoc_demo, [Quit]);
            cx.bind_keys([gpui::KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
        }
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}
