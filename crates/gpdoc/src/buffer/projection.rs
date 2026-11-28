use crate::{Fold, FoldSpec};
use gpdoc_core::{
    ContentRef, DocPos, Document, EditorConfig, Error, Node, Presentation, Selection, SyntaxToken,
    TextFormat,
};
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::rc::Rc;

#[derive(Clone)]
struct Block {
    doc_start: usize,
    text_start: usize,
    byte_start: usize,
    len: usize,
    doc_len: usize,
    leaf: bool,
    // Start of each atomic label in document units and its projected UTF-16 range.
    labels: Vec<(usize, Range<usize>)>,
}

/// Shaping uses byte offsets. Hit testing and background painting use UTF-16 offsets.
#[derive(Clone)]
pub(crate) struct FormatSpan {
    pub byte_end: usize,
    pub utf16_end: usize,
    pub format: TextFormat,
}

struct HighlightedBlock {
    // Keep the immutable node alive while its allocation address is a cache key.
    node: Node,
    runs: Vec<Vec<SyntaxToken>>,
}

impl HighlightedBlock {
    fn key(&self) -> usize {
        std::ptr::from_ref(&*self.node).addr()
    }
}

/// AppKit addresses the text blocks as one string separated by newlines.
/// Each side of a projected newline maps to an actual text cursor, even when
/// the source blocks have different enclosing lists or quotes.
#[derive(Clone, Default)]
pub(in super::super) struct Projection {
    pub folds: Vec<Fold>,
    hidden_bodies: Vec<Range<DocPos>>,
    pub text: String,
    pub(super) object_bytes: Vec<Range<usize>>,
    blocks: Vec<Block>,
    highlights: HashMap<usize, Rc<HighlightedBlock>>,
    pub(super) dividers: Vec<(DocPos, usize)>,
    pub formats: Vec<FormatSpan>,
    pub alignments: Vec<(usize, gpdoc_core::TextAlignment)>,
}

impl Projection {
    pub fn rebuild(
        &self,
        doc: &Document,
        config: &EditorConfig,
        open_folds: &BTreeSet<DocPos>,
        specs: &std::collections::BTreeMap<String, FoldSpec>,
    ) -> Self {
        let presentation = config.presentation();
        let mut projection = Self::default();
        let mut ancestors: Vec<(usize, TextFormat)> = Vec::new();
        for (before, node) in doc.iter_blocks() {
            if projection.hidden(before) {
                continue;
            }
            if let Some(spec) = specs.get(node.kind.as_str())
                && let Some(region) = spec(node, before)
            {
                let open = region
                    .open_attribute
                    .and_then(|attr| node.attrs.get(attr)?.as_bool())
                    .unwrap_or_else(|| open_folds.contains(&before));
                if !open {
                    // A nested fold in a visible heading can precede an already
                    // registered hidden body. Keep the ranges in document order.
                    let first = projection
                        .hidden_bodies
                        .partition_point(|r| r.end < region.body.start);
                    let end = first
                        + projection.hidden_bodies[first..]
                            .partition_point(|r| r.start <= region.body.end);
                    let mut hidden = region.body.clone();
                    for other in &projection.hidden_bodies[first..end] {
                        hidden.start = hidden.start.min(other.start);
                        hidden.end = hidden.end.max(other.end);
                    }
                    projection.hidden_bodies.splice(first..end, [hidden]);
                }
                projection.folds.push(Fold {
                    before,
                    open,
                    region,
                });
            }
            while ancestors.last().is_some_and(|(end, _)| *end <= before.0) {
                ancestors.pop();
            }
            let mut format = ancestors
                .last()
                .map(|(_, style)| style.clone())
                .unwrap_or_default();
            presentation.style_node(node, &mut format);
            let (runs, doc_start, len) = match node.content() {
                ContentRef::Inline(runs) => (runs, before.0 + 1, node.size() - 2),
                ContentRef::Leaf(_) => (&[][..], before.0, usize::from(!format.horizontal_rule)),
                _ => {
                    ancestors.push((before.0 + node.size(), format));
                    continue;
                }
            };
            if !projection.blocks.is_empty() {
                projection.push_text("\n", TextFormat::default());
            }
            if projection
                .alignments
                .last()
                .map(|(_, a)| *a)
                .unwrap_or_default()
                != format.alignment
            {
                projection
                    .alignments
                    .push((projection.text.len(), format.alignment));
            }
            let text_start = projection.len();
            let leaf = node.is_leaf();
            let byte_start = projection.text.len();
            let mut labels = Vec::new();
            let mut at = doc_start;
            if leaf {
                if format.horizontal_rule {
                    projection.dividers.push((before, text_start));
                } else {
                    projection.push_atom(
                        at,
                        config.atom_label(node).as_deref().unwrap_or(&node.kind),
                        &format,
                        &mut labels,
                    );
                }
            }
            let highlights = self.highlights_for(doc, presentation, node, runs);
            if let Some(block) = &highlights {
                projection.highlights.insert(block.key(), Rc::clone(block));
            }
            for (index, run) in runs.iter().enumerate() {
                let mut run_format = format.clone();
                let text = doc.inline_text(run);
                for mark in run.marks() {
                    presentation.style_mark(mark, &mut run_format);
                }
                if !run.is_text() && text != "\n" {
                    presentation.style_node(run, &mut run_format);
                    projection.push_atom(
                        at,
                        config.atom_label(run).as_deref().unwrap_or(text),
                        &run_format,
                        &mut labels,
                    );
                    at += 1;
                    continue;
                }
                at += run.size();
                let mut offset = 0;
                let tokens = highlights
                    .as_ref()
                    .map_or(&[][..], |block| block.runs[index].as_slice());
                for token in tokens {
                    projection.push_text(&text[offset..token.bytes.start], run_format.clone());
                    let mut colored = run_format.clone();
                    colored.syntax_kind = Some(token.kind);
                    projection.push_text(&text[token.bytes.clone()], colored);
                    offset = token.bytes.end;
                }
                projection.push_text(&text[offset..], run_format);
            }
            projection.blocks.push(Block {
                doc_start,
                text_start,
                byte_start,
                len: projection.len() - text_start,
                doc_len: len,
                leaf,
                labels,
            });
        }
        projection
    }

    fn highlights_for(
        &self,
        doc: &Document,
        presentation: &Presentation,
        node: &Node,
        runs: &[Node],
    ) -> Option<Rc<HighlightedBlock>> {
        if !presentation.highlighters.contains_key(node.kind()) {
            return None;
        }
        if let Some(block) = self.highlights.get(&std::ptr::from_ref(&**node).addr()) {
            return Some(Rc::clone(block));
        }
        let runs = runs
            .iter()
            .map(|run| {
                let text = doc.inline_text(run);
                if run.is_text() || text == "\n" {
                    presentation.syntax_tokens(node, text)
                } else {
                    Vec::new()
                }
            })
            .collect();
        Some(Rc::new(HighlightedBlock {
            node: node.clone(),
            runs,
        }))
    }

    pub fn hidden(&self, at: DocPos) -> bool {
        let next = self.hidden_bodies.partition_point(|r| r.end <= at);
        self.hidden_bodies
            .get(next)
            .is_some_and(|r| r.contains(&at))
    }

    pub fn display_text(&self) -> String {
        // The visible placeholder and the native object character have equal UTF-8 and UTF-16 widths.
        let mut text = self.text.clone();
        for bytes in &self.object_bytes {
            if &text[bytes.clone()] == "\u{fffc}" {
                text.replace_range(bytes.clone(), "◇");
            }
        }
        text
    }

    fn push_atom(
        &mut self,
        at: usize,
        fallback: &str,
        format: &TextFormat,
        labels: &mut Vec<(usize, Range<usize>)>,
    ) {
        let text = format
            .atom_label
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback)
            .replace(['\r', '\n'], " ");
        let start = self.len();
        let byte = self.text.len();
        self.push_text(&text, format.clone());
        self.object_bytes.push(byte..self.text.len());
        labels.push((at, start..self.len()));
    }

    pub(super) fn block_leaf_at(&self, offset: usize) -> Option<DocPos> {
        let block = self.block_at(offset)?;
        (block.leaf && offset <= block.text_start + block.len).then_some(DocPos(block.doc_start))
    }

    fn push_text(&mut self, text: &str, format: TextFormat) {
        if text.is_empty() {
            return;
        }
        self.text.push_str(text);
        let span = FormatSpan {
            byte_end: self.text.len(),
            utf16_end: self.len() + text.encode_utf16().count(),
            format,
        };
        if let Some(last) = self.formats.last_mut()
            && last.format == span.format
        {
            *last = span;
        } else {
            self.formats.push(span);
        }
    }

    pub fn len(&self) -> usize {
        self.formats.last().map_or(0, |span| span.utf16_end)
    }

    pub fn document_edge(&self, end: bool) -> Option<usize> {
        let mut text_blocks = self.blocks.iter().filter(|block| !block.leaf);
        if end {
            let block = text_blocks.next_back().or_else(|| self.blocks.last())?;
            Some(block.text_start + block.len)
        } else {
            let block = text_blocks.next().or_else(|| self.blocks.first())?;
            Some(block.text_start)
        }
    }

    fn block_at(&self, offset: usize) -> Option<&Block> {
        let index = self
            .blocks
            .partition_point(|block| block.text_start <= offset);
        index.checked_sub(1).map(|i| &self.blocks[i])
    }

    pub fn selection(&self, doc: &Document, selection: Selection) -> (usize, usize) {
        match selection {
            Selection::Text { anchor, head } => (self.offset(anchor), self.offset(head)),
            Selection::Gap { at } => (self.offset(at), self.offset(at)),
            Selection::Custom { anchor, head, .. } | Selection::NodeRange { anchor, head } => {
                let range = selection.bounds(doc);
                let offsets = self
                    .block_offsets(range.clone())
                    .unwrap_or_else(|| self.offset(range.start)..self.offset(range.end));
                let (start, end) = (offsets.start, offsets.end);
                if anchor < head {
                    (start, end)
                } else {
                    (end, start)
                }
            }
            Selection::Node { before } => {
                let offset = self.offset(before);
                if let Some(block) = self.block_at(offset)
                    && block.leaf
                    && block.doc_start == before.0
                {
                    return (block.text_start, block.text_start + block.len);
                }
                let end = before.0 + doc.node_at(before).map_or(0, |n| n.size());
                let contained =
                    &self.blocks[self.blocks.partition_point(|b| b.doc_start <= before.0)
                        ..self.blocks.partition_point(|b| b.doc_start < end)];
                if let (Some(first), Some(last)) = (contained.first(), contained.last()) {
                    (first.text_start, last.text_start + last.len)
                } else {
                    let pos = self.offset(before);
                    (pos, self.offset(DocPos(end)))
                }
            }
        }
    }

    pub(in super::super) fn block_indices(&self, range: Range<DocPos>) -> Range<usize> {
        self.blocks.partition_point(|b| b.doc_start < range.start.0)
            ..self.blocks.partition_point(|b| b.doc_start < range.end.0)
    }

    pub(in super::super) fn block_span(
        &self,
        indices: Range<usize>,
    ) -> Option<(Range<usize>, Range<usize>)> {
        let blocks = &self.blocks[indices.clone()];
        let first = blocks.first()?;
        let last = blocks.last()?;
        let byte_end = self
            .blocks
            .get(indices.end)
            .map_or(self.text.len(), |next| next.byte_start - 1);
        Some((
            first.byte_start..byte_end,
            first.text_start..last.text_start + last.len,
        ))
    }

    pub(super) fn block_offsets(&self, range: Range<DocPos>) -> Option<Range<usize>> {
        let blocks = &self.blocks[self.block_indices(range)];
        Some(blocks.first()?.text_start..blocks.last()?.text_start + blocks.last()?.len)
    }

    pub fn offset(&self, position: DocPos) -> usize {
        let index = self
            .blocks
            .partition_point(|block| block.doc_start <= position.0);
        index.checked_sub(1).map_or(0, |i| {
            let block = &self.blocks[i];
            let pos = position.0.min(block.doc_start + block.doc_len);
            let i = block.labels.partition_point(|(doc, _)| *doc < pos);
            if let Some((doc, range)) = i.checked_sub(1).map(|i| &block.labels[i]) {
                range.end + pos - (doc + 1)
            } else {
                block.text_start + pos - block.doc_start
            }
        })
    }

    pub fn position(&self, offset: usize) -> Result<DocPos, Error> {
        if let Some(block) = self.block_at(offset)
            && offset - block.text_start <= block.len
        {
            // The editor validates UTF-16 scalar boundaries before accepting this position.
            let i = block
                .labels
                .partition_point(|(_, range)| range.start <= offset);
            let pos = if let Some((doc, range)) = i.checked_sub(1).map(|i| &block.labels[i]) {
                if offset < range.end {
                    doc + usize::from(offset - range.start > range.end - offset)
                } else {
                    doc + 1 + offset - range.end
                }
            } else {
                block.doc_start + offset - block.text_start
            };
            Ok(DocPos(pos))
        } else {
            Err(Error::InvalidEdit(format!(
                "text offset {offset} is outside the editor"
            )))
        }
    }

    /// Convert an offset to the nearest scalar boundary in the requested direction.
    fn byte_boundary(&self, offset: usize, round_up: bool) -> Option<(usize, usize)> {
        if offset == self.len() {
            return Some((self.text.len(), offset));
        }
        let block = self.block_at(offset)?;
        let mut units = block.text_start;
        for (byte, ch) in self.text[block.byte_start..].char_indices() {
            if units == offset || units + ch.len_utf16() > offset {
                return Some(if units != offset && round_up {
                    (
                        block.byte_start + byte + ch.len_utf8(),
                        units + ch.len_utf16(),
                    )
                } else {
                    (block.byte_start + byte, units)
                });
            }
            units += ch.len_utf16();
        }
        None
    }

    pub fn substring(&self, range: Range<usize>) -> Option<(String, Range<usize>)> {
        if range.start > range.end || range.start > self.len() {
            return None;
        }
        let (start, adjusted_start) = self.byte_boundary(range.start, false)?;
        let (end, adjusted_end) = self.byte_boundary(range.end.min(self.len()), true)?;
        Some((
            self.text[start..end].to_owned(),
            adjusted_start..adjusted_end,
        ))
    }
}
