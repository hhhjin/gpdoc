//! Rich-text editing with optional GPUI rendering.
#[cfg(all(feature = "gpui", target_family = "wasm"))]
extern crate gpui_browser as gpui;

pub use gpdoc_core::key;
pub use gpdoc_core::{
    Attrs, Command, CommandOutcome, DocPos, Document, Edit, Editor, Error, JsonError, Mark, Node,
    OpenJsonError, Selection, Shortcut, Step, StoredCommand, SyntaxKind, SyntaxPalette,
    SyntaxToken, Value,
};

mod config;
pub use config::{EditorConfig, EditorConfigBuilder};

#[cfg(feature = "gpui")]
mod buffer;
#[cfg(feature = "gpui")]
mod fold;
#[cfg(feature = "gpui")]
mod gpui_api;
#[cfg(feature = "gpui")]
mod layout;
#[cfg(feature = "gpui")]
mod node_view;
#[cfg(feature = "gpui")]
mod session;
#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
mod view_extensions;

#[cfg(feature = "gpui")]
pub use {
    fold::{Fold, FoldControl, FoldRegion, FoldSpec},
    gpui_api::paint_label,
    layout::{ChildWrapper, EditableChild},
    node_view::{BlockRenderer, EditableBlock, NodeView, editable_block},
    session::{EditSession, SessionUpdate},
    view::{EditorSurface, EditorView, primary_pressed},
    view_extensions::{
        AnchoredElement, BlockDropTarget, DecorationFrame, DocumentSource, EditorPlugin, OpenError,
        PaintLayer, PluginViewExt, ViewBehavior, ViewFrame, ViewPlugin,
    },
};
