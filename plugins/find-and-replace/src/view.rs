//! Paint search results below the selection, using the editor's own text geometry.
use super::{SearchOptions, SearchState, results};
use gpdoc::{DecorationFrame, PaintLayer, ViewBehavior, ViewPlugin};
use gpdoc_plugin::core::{DocPos, Editor, EditorConfig, Node};
use gpui::{App, Bounds, Pixels, Window, fill};
#[cfg(target_family = "wasm")]
use gpui_browser as gpui;
use std::{cell::RefCell, ops::Range, rc::Rc};

/// One visible fragment of a committed search result. Wrapped results can have several fragments.
pub struct MatchHighlight<'a> {
    pub range: &'a Range<DocPos>,
    pub bounds: Bounds<Pixels>,
    pub current: bool,
}

type MatchPainter = dyn Fn(MatchHighlight<'_>, &mut Window, &mut App);

/// Project cached search results through the current layout before painting each fragment.
pub fn search_highlights(
    paint: impl Fn(MatchHighlight<'_>, &mut Window, &mut App) + 'static,
) -> ViewPlugin {
    let paint: Rc<MatchPainter> = Rc::new(paint);
    ViewPlugin::new("findAndReplace", move || SearchView {
        matches: RefCell::default(),
        paint: paint.clone(),
    })
    .for_plugin("findAndReplace")
}

/// Tint search results using the editor text color, emphasizing the current result.
pub fn paint_match(hit: MatchHighlight<'_>, window: &mut Window, _: &mut App) {
    let color = window
        .text_style()
        .color
        .opacity(if hit.current { 0.32 } else { 0.15 });
    window.paint_quad(fill(hit.bounds, color));
}

struct Matches {
    config: EditorConfig,
    root: Node,
    options: SearchOptions,
    ranges: Vec<Range<DocPos>>,
}

struct SearchView {
    paint: Rc<MatchPainter>,
    matches: RefCell<Option<Matches>>,
}

impl SearchView {
    fn for_each_match(&self, editor: &Editor, mut draw: impl FnMut(Range<DocPos>, bool)) {
        let mut cache = self.matches.borrow_mut();
        let Some(state) = editor.plugin_state::<SearchState>() else {
            *cache = None;
            return;
        };
        let root = editor.document().root();
        let matches = match cache.as_ref() {
            Some(saved)
                if saved.root.ptr_eq(root)
                    && saved.config.same_instance(editor.config())
                    && saved.options == state.options =>
            {
                saved
            }
            _ => cache.insert(Matches {
                config: editor.config().clone(),
                root: root.clone(),
                options: state.options.clone(),
                ranges: results(editor).ranges,
            }),
        };
        let current = state.current_index(matches.ranges.len());
        for (i, range) in matches.ranges.iter().enumerate() {
            draw(range.clone(), Some(i) == current);
        }
    }
}

impl ViewBehavior for SearchView {
    fn paint(
        &self,
        layer: PaintLayer,
        frame: &DecorationFrame<'_>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if layer == PaintLayer::TextBackground {
            self.for_each_match(frame.view.editor(), |range, current| {
                for bounds in frame.rects_for_committed_range(range.clone()) {
                    (self.paint)(
                        MatchHighlight {
                            range: &range,
                            bounds,
                            current,
                        },
                        window,
                        cx,
                    );
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GoToNextResult, SetSearchTerm, find_and_replace_with};
    use gpdoc_plugin::EditorConfig;
    use gpdoc_plugin_paragraph::paragraph;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn painting_reuses_matches_across_sections_and_moves_the_current_highlight() {
        let scans = Arc::new(AtomicUsize::new(0));
        let count = scans.clone();
        let config = EditorConfig::builder()
            .plugins([
                paragraph(),
                find_and_replace_with(move |_| {
                    count.fetch_add(1, Ordering::Relaxed);
                    false
                }),
            ])
            .build()
            .unwrap();
        let mut editor = gpdoc_plugin::core::Editor::from_document(
            &config,
            vec![Node::text("paragraph", "a a")],
        )
        .unwrap();
        editor.run(SetSearchTerm("a".to_owned())).unwrap();
        let view = SearchView {
            matches: RefCell::default(),
            paint: Rc::new(paint_match),
        };
        let colors = |editor: &Editor| {
            let mut colors = vec![];
            view.for_each_match(editor, |_, current| colors.push(current));
            colors
        };
        assert_eq!(colors(&editor), [true, false]);
        let initial = scans.load(Ordering::Relaxed);
        assert!(initial > 0);
        assert_eq!(colors(&editor), [true, false]);
        assert_eq!(scans.load(Ordering::Relaxed), initial);

        editor.run(GoToNextResult).unwrap();
        editor.run(GoToNextResult).unwrap();
        let after_navigation = scans.load(Ordering::Relaxed);
        assert_eq!(colors(&editor), [false, true]);
        assert_eq!(scans.load(Ordering::Relaxed), after_navigation);

        editor.run(SetSearchTerm("z".to_owned())).unwrap();
        assert!(colors(&editor).is_empty());
        editor.insert_text("z").unwrap();
        assert_eq!(colors(&editor), [true]);
    }
}
