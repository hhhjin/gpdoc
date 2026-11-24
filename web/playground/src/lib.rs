//! Browser editor for the gpdoc documentation and playground.
#![cfg(target_family = "wasm")]

mod icons;
mod paragraph;
mod plugins;
mod view_examples;

#[path = "../../../examples/editor/demo.rs"]
mod demo;

fn application() -> gpui::Application {
    console_error_panic_hook::set_once();
    gpui_web::init_logging();
    let platform = std::rc::Rc::new(gpui_web::WebPlatform::new_with_backend_and_font_fallback(
        false,
        gpui_web::WebBackendPreference::Auto,
        gpui_web::CanvasFontFallback::EmojiAndCjk,
    ));
    let http_client = std::sync::Arc::new(platform.fetch_http_client());
    gpui::Application::with_platform(platform).with_http_client(http_client)
}

#[wasm_bindgen::prelude::wasm_bindgen]
pub fn start() {
    demo::run(application(), |_| {});
}
