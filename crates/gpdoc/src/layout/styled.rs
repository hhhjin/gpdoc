//! GPUI accepts one font size per shaped line. Shape same-size spans in bidi visual
//! order and use their actual glyph advances for wrapping, painting, and hit testing.
use super::*;
use gpdoc_core::{Baseline, TextAlignment, TextLength};
use std::mem;

pub(super) struct StyledRow {
    pieces: Vec<Piece>,
    pub(super) cells: Vec<Cell>,
    pub(super) left: Pixels,
    pub(super) size: Size<Pixels>,
}

struct Piece {
    content: PieceContent,
    origin: Point<Pixels>,
    height: Pixels,
}

enum PieceContent {
    Text(Box<WrappedLine>),
    Image(std::sync::Arc<gpui::RenderImage>, Size<Pixels>),
}

impl TextSection {
    /// Painted top of a standalone image, including its offset inside the line.
    pub(super) fn image_top(&self) -> Option<Pixels> {
        let LineContent::Styled(row) = &self.lines.first()?.content else {
            return None;
        };
        let [piece] = row.pieces.as_slice() else {
            return None;
        };
        matches!(piece.content, PieceContent::Image(..)).then(|| self.bounds.top() + piece.origin.y)
    }
}

impl StyledRow {
    pub(super) fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        for piece in &self.pieces {
            match &piece.content {
                PieceContent::Image(data, size) => {
                    let bounds = Bounds::new(origin + piece.origin, *size);
                    #[cfg(target_family = "wasm")]
                    let result = window.paint_image(
                        bounds,
                        bounds,
                        Default::default(),
                        data.clone(),
                        0,
                        false,
                    );
                    #[cfg(not(target_family = "wasm"))]
                    let result =
                        window.paint_image(bounds, Default::default(), data.clone(), 0, false);
                    result.expect("GPUI could not paint an image");
                }
                PieceContent::Text(line) => line
                    .paint(
                        origin + piece.origin,
                        piece.height,
                        Default::default(),
                        None,
                        window,
                        cx,
                    )
                    .expect("GPUI could not paint styled text"),
            }
        }
    }
}

struct Paragraph<'a> {
    typography: &'a Typography,
    text: &'a str,
    start: usize,
    objects: &'a [Range<usize>],
    formats: &'a [FormatSpan],
    bidi: ParagraphBidiInfo<'a>,
    align: TextAlignment,
}

fn resolve(length: TextLength, parent: Pixels, root: Pixels) -> Pixels {
    match length {
        TextLength::Pixels(n) => px(n),
        TextLength::Em(n) => parent * n,
        TextLength::Rem(n) => root * n,
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Metrics {
    size: Pixels,
    height: Pixels,
    raise: Pixels,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChunkKind {
    Text,
    Image,
    JustificationSpace,
    RtlSpace,
}

struct Chunk {
    bytes: Range<usize>,
    metrics: Metrics,
    kind: ChunkKind,
}

impl Paragraph<'_> {
    fn image(
        &self,
        byte: usize,
    ) -> Option<(&gpdoc_core::AtomImage, &std::sync::Arc<gpui::RenderImage>)> {
        let index = self
            .formats
            .partition_point(|span| span.byte_end <= self.start + byte);
        let spec = self.formats.get(index)?.format.atom_image.as_ref()?;
        Some((spec, self.typography.images.get(&spec.source)?))
    }

    fn metrics(&self, byte: usize, root: Pixels) -> Metrics {
        let index = self
            .formats
            .partition_point(|s| s.byte_end <= self.start + byte);
        let format = self.formats.get(index).map(|s| &s.format);
        let base = self.typography.font_size;
        let size = format
            .and_then(|f| f.font_size)
            .map_or(base, |len| resolve(len, base, root));
        let (size, raise) = match format.map(|f| f.baseline).unwrap_or_default() {
            Baseline::Normal => (size, px(0.)),
            Baseline::Subscript => (size * (5. / 6.), size * -0.20),
            Baseline::Superscript => (size * (5. / 6.), size * 0.35),
        };
        let height = format.and_then(|f| f.line_height).map_or_else(
            || self.typography.line_height * (size / base),
            |len| resolve(len, size, root),
        );
        Metrics {
            size: size.max(px(0.1)),
            height: height.max(px(1.)),
            raise,
        }
    }

    // GPUI resolves each shaped span independently. Keep spaces as separate
    // pieces in RTL runs so resizing a neutral span cannot reverse its caret stops.
    fn chunks(
        &self,
        run: Range<usize>,
        rtl: bool,
        justify_to: Option<usize>,
        root: Pixels,
    ) -> Vec<Chunk> {
        let mut chunks: Vec<Chunk> = Vec::new();
        for (byte, grapheme) in super::super::buffer::editor_graphemes(
            &self.text[run.clone()],
            &relative_objects(self.objects, run.clone()),
        ) {
            let byte = run.start + byte;
            let metrics = self.metrics(byte, root);
            let kind = if self.image(byte).is_some() {
                ChunkKind::Image
            } else if justify_to.is_some_and(|end| byte < end) && grapheme == " " {
                ChunkKind::JustificationSpace
            } else if rtl && grapheme.chars().all(char::is_whitespace) {
                ChunkKind::RtlSpace
            } else {
                ChunkKind::Text
            };
            if let Some(last) = chunks.last_mut()
                && kind == ChunkKind::Text
                && last.kind == kind
                && last.metrics == metrics
            {
                last.bytes.end += grapheme.len();
            } else {
                chunks.push(Chunk {
                    bytes: byte..byte + grapheme.len(),
                    metrics,
                    kind,
                });
            }
        }
        if rtl {
            chunks.reverse()
        }
        chunks
    }

    fn row(
        &self,
        range: Range<usize>,
        width: Option<Pixels>,
        justify: bool,
        window: &mut Window,
    ) -> StyledRow {
        let mut pieces = Vec::new();
        let mut cells = Vec::new();
        let mut x = px(0.);
        // Keep the normal baseline even on a row containing only superscripts.
        let base = self.typography;
        let strut = window.text_system().shape_line(
            " ".into(),
            base.font_size,
            &[base.style.to_run(1)],
            None,
        );
        let mut ascent = (base.line_height + strut.ascent - strut.descent) / 2.;
        let mut descent = base.line_height - ascent;
        let mut spaces = Vec::new();

        if !range.is_empty() {
            let (levels, runs) = self.bidi.visual_runs(range.clone());
            let last_word = self.text[range.clone()].trim_end().len() + range.start;
            for run in runs {
                let rtl = levels[run.start].is_rtl();
                for part in self.chunks(run, rtl, justify.then_some(last_word), window.rem_size()) {
                    let chunk = part.bytes;
                    let Metrics {
                        size,
                        height,
                        raise,
                    } = part.metrics;
                    if let Some((spec, data)) = self.image(chunk.start) {
                        let dims = images::dimensions(spec, data, width);
                        ascent = ascent.max(dims.height);
                        let start = chunk.start;
                        cells.push(Cell {
                            byte: start,
                            units: 0..self.text[chunk].encode_utf16().count(),
                            leading: if rtl { x + dims.width } else { x },
                            trailing: if rtl { x } else { x + dims.width },
                        });
                        pieces.push(Piece {
                            content: PieceContent::Image(data.clone(), dims),
                            origin: point(x, -dims.height),
                            height: dims.height,
                        });
                        x += dims.width;
                        continue;
                    }
                    let mut runs = self.typography.runs(
                        self.formats,
                        self.start + chunk.start..self.start + chunk.end,
                    );
                    let source = &self.text[chunk.clone()];
                    // A run containing only neutral punctuation still inherits the paragraph direction.
                    let prefix = if rtl { "\u{200f}" } else { "" };
                    if let Some(first) = runs.first_mut() {
                        first.len += prefix.len();
                    }
                    if let Some(last) = runs.last_mut() {
                        last.len += prefix.len();
                    }
                    let text = format!("{prefix}{source}{prefix}");
                    let shaped = window
                        .text_system()
                        .shape_text(text.into(), size, &runs, None, None)
                        .expect("GPUI could not shape styled text")
                        .pop()
                        .unwrap();
                    let baseline = (height + shaped.unwrapped_layout.ascent
                        - shaped.unwrapped_layout.descent)
                        / 2.
                        + raise;
                    ascent = ascent.max(baseline);
                    descent = descent.max(height - baseline);
                    let first_cell = cells.len();
                    let glyphs = shaped
                        .runs()
                        .iter()
                        .flat_map(|r| &r.glyphs)
                        .filter_map(|g| {
                            g.index
                                .checked_sub(prefix.len())
                                .map(|byte| (byte, g.position.x))
                        });
                    for mut cell in paragraph_cells(
                        source,
                        &relative_objects(self.objects, chunk.clone()),
                        0,
                        glyphs,
                        shaped.unwrapped_layout.width,
                        Some(rtl),
                    ) {
                        cell.byte += chunk.start;
                        cell.leading += x;
                        cell.trailing += x;
                        cells.push(cell);
                    }
                    if part.kind == ChunkKind::JustificationSpace {
                        spaces.push((pieces.len(), first_cell));
                    }
                    let advance = shaped.unwrapped_layout.width;
                    pieces.push(Piece {
                        origin: point(x, -baseline),
                        height,
                        content: PieceContent::Text(Box::new(shaped)),
                    });
                    x += advance;
                }
            }
        }
        let extra = width.map_or(px(0.), |w| (w - x).max(px(0.)));
        let left = match self.align {
            TextAlignment::Right => extra,
            TextAlignment::Center => extra / 2.,
            _ => px(0.),
        };
        let stretch = if spaces.is_empty() {
            px(0.)
        } else {
            extra / spaces.len() as f32
        };
        for (index, piece) in pieces.iter_mut().enumerate() {
            piece.origin.x += left + stretch * spaces.partition_point(|(p, _)| *p < index);
            piece.origin.y += ascent;
        }
        for (index, cell) in cells.iter_mut().enumerate() {
            let i = spaces.partition_point(|(_, c)| *c < index);
            let shift = left + stretch * i;
            cell.leading += shift;
            cell.trailing += shift;
            if spaces.get(i).is_some_and(|(_, c)| *c == index) {
                if cell.leading <= cell.trailing {
                    cell.trailing += stretch
                } else {
                    cell.leading += stretch
                }
            }
        }
        cells.sort_by_key(|c| c.byte);
        let mut offset = 0;
        for cell in &mut cells {
            let len = cell.units.len();
            cell.units = offset..offset + len;
            offset += len;
        }
        let height = if pieces.is_empty() {
            self.typography.line_height
        } else {
            ascent + descent
        };
        let ruby = self.annotation_pieces(range, &cells, window);
        let ruby_height = ruby.iter().fold(px(0.), |height, p| height.max(p.height));
        for piece in &mut pieces {
            piece.origin.y += ruby_height;
        }
        pieces.extend(ruby);
        StyledRow {
            pieces,
            cells,
            left,
            size: size(x + stretch * spaces.len(), height + ruby_height),
        }
    }
    /// Reserve space above the base text and center each reading over its
    /// visible span. Long readings shrink to fit their base.
    fn annotation_pieces(
        &self,
        row: Range<usize>,
        cells: &[Cell],
        window: &mut Window,
    ) -> Vec<Piece> {
        let mut labels: Vec<(Range<usize>, &str)> = Vec::new();
        let mut start = self.start;
        for span in self.formats {
            let end = span.byte_end;
            let from = start.saturating_sub(self.start).max(row.start);
            let to = end.saturating_sub(self.start).min(row.end);
            start = end;
            let Some(rt) = span.format.ruby_text.as_deref().filter(|s| !s.is_empty()) else {
                continue;
            };
            if from >= to {
                continue;
            }
            if let Some((last, text)) = labels.last_mut()
                && last.end == from
                && *text == rt
            {
                last.end = to
            } else {
                labels.push((from..to, rt));
            }
        }
        labels
            .into_iter()
            .filter_map(|(range, rt)| {
                let mut selected = cells.iter().filter(|c| range.contains(&c.byte));
                let first = selected.next()?;
                let (left, right) = selected.fold(
                    (
                        first.leading.min(first.trailing),
                        first.leading.max(first.trailing),
                    ),
                    |(left, right), c| {
                        (
                            left.min(c.leading.min(c.trailing)),
                            right.max(c.leading.max(c.trailing)),
                        )
                    },
                );
                let size = self.metrics(range.start, window.rem_size()).size;
                let rt = rt.replace(['\r', '\n'], " ");
                let run = self.typography.style.to_run(rt.len());
                let shape = |size| {
                    window
                        .text_system()
                        .shape_text(
                            rt.clone().into(),
                            size,
                            std::slice::from_ref(&run),
                            None,
                            None,
                        )
                        .expect("GPUI could not shape a ruby annotation")
                        .pop()
                        .unwrap()
                };
                let mut height = size * 0.5;
                let mut line = shape(height);
                let width = right - left;
                if width > px(0.) && line.unwrapped_layout.width > width {
                    height = (height * (width / line.unwrapped_layout.width)).max(px(1.));
                    line = shape(height);
                }
                Some(Piece {
                    origin: point(left + (width - line.unwrapped_layout.width) / 2., px(0.)),
                    height,
                    content: PieceContent::Text(Box::new(line)),
                })
            })
            .collect()
    }
}

pub(super) fn shape(
    typography: &Typography,
    text: &str,
    start: usize,
    objects: &[Range<usize>],
    formats: &[FormatSpan],
    align: TextAlignment,
    width: Option<Pixels>,
    window: &mut Window,
) -> Vec<StyledRow> {
    // A label is one selectable object, including when its visible text mixes scripts.
    // Analyze it as neutral bytes so a bidi run cannot start halfway through an object.
    let mut bidi_text = text.to_owned();
    for object in objects {
        bidi_text.replace_range(object.clone(), &".".repeat(object.len()));
    }
    let p = Paragraph {
        typography,
        text,
        start,
        objects,
        formats,
        align,
        bidi: ParagraphBidiInfo::new(&bidi_text, None),
    };
    let mut full = p.row(0..text.len(), width, false, window);
    if let Some(width) = width
        && full.size.width > width
    {
        let cells = mem::take(&mut full.cells);
        return logical_wraps(text, &cells, width)
            .into_iter()
            .map(|r| {
                let justify = align == TextAlignment::Justify && r.end < text.len();
                p.row(r, Some(width), justify, window)
            })
            .collect();
    }
    vec![full]
}

#[cfg(all(test, feature = "native-tests"))]
mod tests {
    use super::*;
    use gpdoc_core::TextFormat;

    #[test]
    fn a_resized_neutral_run_keeps_its_rtl_caret_edges() {
        let mut app = gpui::TestAppContext::single();
        let window = app.add_empty_window();
        window.update(|window, _| {
            let typography = Typography {
                syntax_palette: SyntaxPalette::Light,
                style: TextStyle::default(),
                font_size: px(18.),
                line_height: px(26.),
                images: Default::default(),
            };
            let text = "ID אב  גד 42";
            let start = text.find("  ").unwrap();
            let formats: Vec<_> = [
                (start, None),
                (start + 2, Some(TextLength::Em(1.5))),
                (text.len(), None),
            ]
            .into_iter()
            .map(|(end, size)| FormatSpan {
                byte_end: end,
                utf16_end: text[..end].encode_utf16().count(),
                format: TextFormat {
                    font_size: size,
                    ..Default::default()
                },
            })
            .collect();
            let row = shape(
                &typography,
                text,
                0,
                &[],
                &formats,
                TextAlignment::Center,
                Some(px(300.)),
                window,
            )
            .remove(0);
            let spaces: Vec<_> = row
                .cells
                .iter()
                .filter(|c| (start..start + 2).contains(&c.byte))
                .collect();
            assert_eq!(spaces.len(), 2);
            assert!(
                spaces[0].leading > spaces[0].trailing && spaces[1].leading > spaces[1].trailing
            );
            assert!(spaces[0].leading > spaces[1].leading);
            assert!(row.left > px(0.));
        });
    }

    #[test]
    fn scripts_shape_arbitrary_letters_at_reduced_size_and_shift_the_painted_baseline() {
        let mut app = gpui::TestAppContext::single();
        let window = app.add_empty_window();
        window.update(|window, _| {
            let typography = Typography {
                syntax_palette: SyntaxPalette::Light,
                style: TextStyle::default(),
                font_size: px(20.),
                line_height: px(30.),
                images: Default::default(),
            };
            let text = "한한한";
            let formats: Vec<_> = [Baseline::Normal, Baseline::Superscript, Baseline::Subscript]
                .into_iter()
                .enumerate()
                .map(|(i, baseline)| FormatSpan {
                    byte_end: (i + 1) * '한'.len_utf8(),
                    utf16_end: i + 1,
                    format: TextFormat {
                        baseline,
                        ..Default::default()
                    },
                })
                .collect();
            let row = shape(
                &typography,
                text,
                0,
                &[],
                &formats,
                TextAlignment::Left,
                None,
                window,
            )
            .remove(0);
            let pieces: Vec<_> = row
                .pieces
                .iter()
                .map(|piece| {
                    let PieceContent::Text(line) = &piece.content else {
                        panic!("expected glyphs")
                    };
                    let layout = &line.unwrapped_layout;
                    let baseline =
                        piece.origin.y + (piece.height + layout.ascent - layout.descent) / 2.;
                    (layout.font_size, baseline)
                })
                .collect();
            let [normal, superscript, subscript] = pieces.as_slice() else {
                panic!("expected three shaped pieces")
            };
            assert!(superscript.0 < normal.0);
            assert_eq!(superscript.0, subscript.0);
            assert!(superscript.1 < normal.1 && normal.1 < subscript.1);
        });
    }
}
