//! Differences between the desktop GPUI release and the browser backend.
use gpui::{App, FocusHandle, Pixels, Point, ShapedLine, Window};

pub(super) fn is_macos(_cx: &App) -> bool {
    #[cfg(target_family = "wasm")]
    {
        _cx.platform_info().is_mac()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        cfg!(target_os = "macos")
    }
}

pub(super) fn focus(window: &mut Window, handle: &FocusHandle, _cx: &mut App) {
    #[cfg(target_family = "wasm")]
    window.focus(handle, _cx);
    #[cfg(not(target_family = "wasm"))]
    window.focus(handle);
}

/// Paint a shaped line at the given origin with the native or browser backend.
pub fn paint_label(
    line: &ShapedLine,
    origin: Point<Pixels>,
    height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    #[cfg(target_family = "wasm")]
    let _ = line.paint(origin, height, gpui::TextAlign::Left, None, window, cx);
    #[cfg(not(target_family = "wasm"))]
    let _ = line.paint(origin, height, window, cx);
}
