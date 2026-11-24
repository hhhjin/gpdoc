# Project overview

gpdoc is a headless rich-text editor framework for Rust with GPUI support.

# Repository layout

- `examples/editor/` contains the runnable GPUI editor demo.
- `crates/` contains the core editing engine, shared editing support, and the GPUI editor library.
- `plugins/` contains Rust crates for individual editor features such as tables, mentions, and Markdown.
- `web/` contains the Astro documentation site and the browser playground.

# API design during beta

gpdoc is in beta. Prioritize simple APIs and clear boundaries between the core editor, GPUI integration, and plugins. Change public and internal APIs when doing so improves the design, even if the change breaks compatibility. Update affected callers in this repository in the same change.
