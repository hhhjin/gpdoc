//! Window geometry for the visible text, including any in-progress IME composition.
mod content;
pub(super) use content::TextLayout;
pub use content::{ChildWrapper, EditableChild};
mod images;
mod styled;
use super::{buffer::FormatSpan, view::EditorView};
use gpdoc_core::TextAlignment;
use gpui::{
    App, AvailableSpace, Bounds, ElementId, ElementInputHandler, Entity, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Point, Size, Style, TextStyle, Window, WrappedLine, fill,
    point, prelude::*, px, relative, rgb, rgba, size,
};
use std::{cell::RefCell, ops::Range, rc::Rc};
use unicode_bidi::ParagraphBidiInfo;
#[cfg(test)]
use unicode_segmentation::UnicodeSegmentation;

/// Resolve targets offered by document links and media using the host's base URL.
pub(crate) fn activation_url(
    source: &str,
    base: Option<&url::Url>,
    link: bool,
) -> Option<url::Url> {
    let target = if source.starts_with("//") && base.is_none() {
        url::Url::parse(&format!("https:{source}")).ok()?
    } else {
        url::Url::options().base_url(base).parse(source).ok()?
    };
    ((matches!(target.scheme(), "https" | "http") && target.has_host())
        || (link && matches!(target.scheme(), "mailto" | "tel")))
    .then_some(target)
}

pub(super) struct TextElement {
    pub(super) view: Entity<EditorView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaretEdge {
    Leading,
    Trailing,
}

/// A boundary can sit on either side of a bidi run or a soft wrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Caret {
    pub(super) offset: usize,
    pub(super) edge: CaretEdge,
}

impl Caret {
    pub(super) fn leading(offset: usize) -> Self {
        Self {
            offset,
            edge: CaretEdge::Leading,
        }
    }

    fn trailing(offset: usize) -> Self {
        Self {
            offset,
            edge: CaretEdge::Trailing,
        }
    }
}

// A grapheme's leading edge is on the right in an RTL run. At a direction change
// its trailing edge and the next grapheme's leading edge can be far apart.
#[derive(Clone, Debug)]
struct Cell {
    byte: usize,
    units: Range<usize>,
    leading: Pixels,
    trailing: Pixels,
}

struct Row {
    left: Pixels,
    start: usize,
    cells: Vec<Cell>,
    newline: bool,
}

impl Row {
    fn end(&self) -> usize {
        self.cells.last().map_or(self.start, |c| c.units.end)
    }

    fn x(&self, caret: Caret) -> Pixels {
        let i = self.cells.partition_point(|c| c.units.end <= caret.offset);
        let previous = i.checked_sub(1).map(|i| &self.cells[i]);
        if caret.edge == CaretEdge::Trailing
            && let Some(cell) = previous.filter(|cell| cell.units.end == caret.offset)
        {
            return cell.trailing;
        }
        self.cells
            .get(i)
            .map(|c| c.leading)
            .or_else(|| previous.map(|c| c.trailing))
            .unwrap_or(self.left)
    }

    fn edge(&self, right: bool) -> Caret {
        self.cells
            .iter()
            .flat_map(|cell| {
                [
                    (cell.leading, Caret::leading(cell.units.start)),
                    (cell.trailing, Caret::trailing(cell.units.end)),
                ]
            })
            .min_by_key(|&(x, _)| if right { -x } else { x })
            .map_or(Caret::leading(self.start), |(_, caret)| caret)
    }

    fn nearest(&self, x: Pixels) -> Caret {
        // At a bidi run boundary, first choose the glyph on the clicked side,
        // then its closest logical edge.
        let Some(cell) = self.cells.iter().min_by_key(|c| {
            let left = c.leading.min(c.trailing);
            let right = c.leading.max(c.trailing);
            (x - x.clamp(left, right)).abs()
        }) else {
            return Caret::leading(self.start);
        };
        if (cell.leading - x).abs() <= (cell.trailing - x).abs() {
            Caret::leading(cell.units.start)
        } else {
            Caret::trailing(cell.units.end)
        }
    }
}

/// Resolve a glyph's UTF-8 source index to its grapheme, regardless of visual order.
/// The next visual cluster's origin bounds this cluster. A ligature with several
/// graphemes but only one glyph origin shares its advance among its caret stops.
fn paragraph_cells(
    text: &str,
    objects: &[Range<usize>],
    first: usize,
    glyphs: impl Iterator<Item = (usize, Pixels)> + Clone,
    width: Pixels,
    run_rtl: Option<bool>,
) -> Vec<Cell> {
    let mut offset = first;
    let mut cells: Vec<_> = super::buffer::editor_graphemes(text, objects)
        .map(|(byte, grapheme)| {
            let start = offset;
            offset += grapheme.encode_utf16().count();
            Cell {
                byte,
                units: start..offset,
                leading: px(0.),
                trailing: px(0.),
            }
        })
        .collect();
    let bidi = (run_rtl.is_none() && !text.is_ascii()).then(|| ParagraphBidiInfo::new(text, None));
    // Most text has a single glyph origin per grapheme in logical order. In
    // that case the painted advances are already the two edges of each cell.
    if !run_rtl.unwrap_or_else(|| bidi.as_ref().is_some_and(|b| b.has_rtl()))
        && glyphs
            .clone()
            .map(|(byte, _)| byte)
            .eq(cells.iter().map(|c| c.byte))
    {
        let mut edges = glyphs.map(|(_, x)| x).peekable();
        for cell in &mut cells {
            cell.leading = edges.next().unwrap();
            cell.trailing = edges.peek().copied().unwrap_or(width);
        }
        return cells;
    }
    let mut origins: Vec<Option<Pixels>> = vec![None; cells.len()];
    for (byte, x) in glyphs {
        if byte < text.len() {
            let i = cells.partition_point(|c| c.byte <= byte) - 1;
            origins[i] = Some(origins[i].map_or(x, |old| old.min(x)));
        }
    }
    let mut groups: Vec<_> = origins
        .iter()
        .enumerate()
        .filter_map(|(index, x)| x.map(|x| (index, x)))
        .collect();
    let mut ends = vec![cells.len(); cells.len()];
    for adjacent in groups.windows(2) {
        ends[adjacent[0].0] = adjacent[1].0;
    }
    groups.sort_by_key(|&(_, x)| x);
    for (n, &(i, left)) in groups.iter().enumerate() {
        let right = groups.get(n + 1).map_or(width, |&(_, x)| x);
        let end = ends[i];
        let rtl = run_rtl.unwrap_or_else(|| {
            bidi.as_ref()
                .is_some_and(|b| b.levels[cells[i].byte].is_rtl())
        });
        let (start, finish) = if rtl { (right, left) } else { (left, right) };
        let advance = (finish - start) / (end - i) as f32;
        for (k, cell) in cells[i..end].iter_mut().enumerate() {
            cell.leading = start + advance * k as f32;
            cell.trailing = start + advance * (k + 1) as f32;
        }
    }
    cells
}

fn relative_objects(objects: &[Range<usize>], bytes: Range<usize>) -> Vec<Range<usize>> {
    let first = objects.partition_point(|range| range.start < bytes.start);
    objects[first..]
        .iter()
        .take_while(|range| range.end <= bytes.end)
        .map(|range| range.start - bytes.start..range.end - bytes.start)
        .collect()
}

fn line_cells(line: &WrappedLine, objects: &[Range<usize>], offset: usize) -> Vec<Cell> {
    paragraph_cells(
        &line.text,
        objects,
        offset,
        line.runs()
            .iter()
            .flat_map(|r| &r.glyphs)
            .map(|g| (g.index, g.position.x)),
        line.unwrapped_layout.width,
        None,
    )
}

struct PaintLine {
    content: LineContent,
    newline: bool,
}

enum LineContent {
    Plain {
        objects: Vec<Range<usize>>,
        shaped: Box<WrappedLine>,
    },
    Styled(styled::StyledRow),
}

impl PaintLine {
    fn size(&self, line_height: Pixels) -> Size<Pixels> {
        match &self.content {
            LineContent::Plain { shaped, .. } => shaped.size(line_height),
            LineContent::Styled(row) => row.size,
        }
    }
}

// GPUI 0.2 wraps a shaped paragraph in glyph order. For reordered glyphs,
// choose breaks in source order and shape the resulting slices individually.
// This keeps the beginning of an RTL paragraph on its first displayed row.
fn logical_wraps(text: &str, cells: &[Cell], width: Pixels) -> Vec<Range<usize>> {
    let mut rows = Vec::new();
    let mut first = 0;
    while first < cells.len() {
        let mut used = px(0.);
        let mut word_break = None;
        let mut end = first;
        while let Some(cell) = cells.get(end) {
            used += (cell.trailing - cell.leading).abs();
            if used > width && end > first {
                end = word_break.unwrap_or(end);
                break;
            }
            end += 1;
            if text[cell.byte..].starts_with(char::is_whitespace) {
                word_break = Some(end);
            }
        }
        let byte_end = cells.get(end).map_or(text.len(), |c| c.byte);
        rows.push(cells[first].byte..byte_end);
        first = end;
    }
    rows
}

fn visual_rows(lines: &[PaintLine], mut offset: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    for painted in lines {
        let (line, objects) = match &painted.content {
            LineContent::Plain { shaped, objects } => (shaped, objects),
            LineContent::Styled(styled) => {
                let mut cells = styled.cells.clone();
                for cell in &mut cells {
                    cell.units = cell.units.start + offset..cell.units.end + offset;
                }
                let row = Row {
                    start: offset,
                    left: styled.left,
                    cells,
                    newline: painted.newline,
                };
                offset = row.end() + usize::from(row.newline);
                rows.push(row);
                continue;
            }
        };
        let cells = line_cells(line, objects, offset);
        if line.wrap_boundaries().is_empty() {
            let row = Row {
                left: px(0.),
                start: offset,
                cells,
                newline: painted.newline,
            };
            offset = row.end() + usize::from(row.newline);
            rows.push(row);
            continue;
        }
        let mut first = 0;
        let mut origin = px(0.);
        for (byte, x) in line
            .wrap_boundaries()
            .iter()
            .map(|boundary| {
                let glyph = &line.runs()[boundary.run_ix].glyphs[boundary.glyph_ix];
                (glyph.index, glyph.position.x)
            })
            .chain([(line.len(), line.unwrapped_layout.width)])
        {
            let end = cells.partition_point(|c| c.byte < byte);
            let mut row = cells[first..end].to_vec();
            for cell in &mut row {
                cell.leading -= origin;
                cell.trailing -= origin;
            }
            rows.push(Row {
                left: px(0.),
                start: offset,
                cells: row,
                newline: byte == line.len() && painted.newline,
            });
            offset = rows.last().unwrap().end();
            first = end;
            origin = x;
        }
        offset += usize::from(painted.newline);
    }
    rows
}

#[derive(Clone)]
struct Typography {
    images: Rc<images::Images>,
    style: TextStyle,
    font_size: Pixels,
    line_height: Pixels,
}

fn font_family(name: &str) -> &str {
    match name {
        "sans-serif" | "system-ui" => ".SystemUIFont",
        #[cfg(target_os = "macos")]
        "serif" => "Times",
        #[cfg(target_os = "macos")]
        "monospace" => "Menlo",
        #[cfg(target_os = "macos")]
        "cursive" => "Apple Chancery",
        #[cfg(target_os = "macos")]
        "fantasy" => "Papyrus",
        #[cfg(target_os = "windows")]
        "serif" => "Times New Roman",
        #[cfg(target_os = "windows")]
        "monospace" => "Consolas",
        #[cfg(target_os = "linux")]
        "serif" => "DejaVu Serif",
        #[cfg(target_os = "linux")]
        "monospace" => "DejaVu Sans Mono",
        #[cfg(target_family = "wasm")]
        "monospace" => "Lilex",
        name => name,
    }
}

impl Typography {
    fn runs(&self, formats: &[FormatSpan], bytes: Range<usize>) -> Vec<gpui::TextRun> {
        let first = formats.partition_point(|span| span.byte_end <= bytes.start);
        let mut start = bytes.start;
        let mut runs = Vec::new();
        for span in &formats[first..] {
            if start >= bytes.end {
                break;
            }
            let end = span.byte_end.min(bytes.end);
            let mut run = self.style.to_run(end - start);
            let format = &span.format;
            if let Some(color) = format.color {
                run.color = rgba(u32::from_be_bytes(color)).into();
            }
            if let Some(family) = &format.font_family {
                run.font.family = font_family(family).to_owned().into();
                if let Some(fallbacks) = &format.font_fallbacks {
                    run.font.fallbacks = Some(gpui::FontFallbacks(
                        fallbacks
                            .iter()
                            .map(|name| font_family(name).to_owned())
                            .collect::<Vec<_>>()
                            .into(),
                    ));
                }
            }
            if format.bold {
                run.font.weight = gpui::FontWeight::BOLD;
            }
            if format.italic {
                run.font.style = gpui::FontStyle::Italic;
            }
            if format.underline {
                run.underline = Some(gpui::UnderlineStyle {
                    thickness: px(1.),
                    ..Default::default()
                });
            }
            if format.strike {
                run.strikethrough = Some(gpui::StrikethroughStyle {
                    thickness: px(1.),
                    ..Default::default()
                });
            }
            runs.push(run);
            start = end;
        }
        runs
    }

    fn shape(
        &self,
        source: &TextSource,
        span: &TextSpan,
        width: Option<Pixels>,
        window: &mut Window,
    ) -> Vec<PaintLine> {
        let TextSource {
            text,
            objects,
            formats,
            alignments,
        } = source;
        let mut paragraphs = text[span.bytes.clone()].split('\n').peekable();
        let mut result = Vec::new();
        let mut byte_start = span.bytes.start;
        while let Some(paragraph) = paragraphs.next() {
            let newline = paragraphs.peek().is_some();
            let paragraph_start = byte_start;
            let end = byte_start + paragraph.len();
            let bytes = paragraph_start..end;
            byte_start = end + usize::from(newline);
            let line_objects = relative_objects(objects, bytes.clone());
            let first_span = formats.partition_point(|s| s.byte_end <= paragraph_start);
            let span_end = formats.partition_point(|s| s.byte_end < end);
            let spans = &formats[first_span..(span_end + 1).min(formats.len()).max(first_span)];
            let alignment = alignments
                .partition_point(|(at, _)| *at <= paragraph_start)
                .checked_sub(1)
                .map(|i| alignments[i].1)
                .unwrap_or_default();
            if alignment != TextAlignment::Left
                || spans.iter().any(|s| {
                    s.format.baseline != gpdoc_core::Baseline::Normal
                        || s.format.font_size.is_some()
                        || s.format.line_height.is_some()
                        || s.format.ruby_text.is_some()
                        || s.format.atom_image.is_some()
                })
            {
                let rows = styled::shape(
                    self,
                    paragraph,
                    paragraph_start,
                    &line_objects,
                    spans,
                    alignment,
                    width,
                    window,
                );
                let count = rows.len();
                result.extend(rows.into_iter().enumerate().map(|(i, row)| PaintLine {
                    content: LineContent::Styled(row),
                    newline: newline && i + 1 == count,
                }));
                continue;
            }
            let runs = self.runs(spans, bytes);
            let line = window
                .text_system()
                .shape_text(
                    paragraph.to_owned().into(),
                    self.font_size,
                    &runs,
                    width,
                    None,
                )
                .expect("GPUI could not shape the editor text")
                .pop()
                .unwrap();
            let indices = || line.runs().iter().flat_map(|r| &r.glyphs).map(|g| g.index);
            if let Some(width) = width
                && !line.wrap_boundaries().is_empty()
                && (!line_objects.is_empty() || !indices().is_sorted())
            {
                let ranges = logical_wraps(&line.text, &line_cells(&line, &line_objects, 0), width);
                for range in ranges {
                    let last = range.end == line.len();
                    let runs = self.runs(
                        formats,
                        paragraph_start + range.start..paragraph_start + range.end,
                    );
                    let objects = relative_objects(&line_objects, range.clone());
                    let text = line.text[range].to_owned();
                    let shaped = window
                        .text_system()
                        .shape_text(text.into(), self.font_size, &runs, None, None)
                        .expect("GPUI could not shape a soft-wrapped row")
                        .pop()
                        .unwrap();
                    result.push(PaintLine {
                        content: LineContent::Plain {
                            objects,
                            shaped: Box::new(shaped),
                        },
                        newline: last && newline,
                    });
                }
            } else {
                result.push(PaintLine {
                    content: LineContent::Plain {
                        objects: line_objects,
                        shaped: Box::new(line),
                    },
                    newline,
                });
            }
        }
        if span.bytes.end < text.len()
            && let Some(last) = result.last_mut()
        {
            last.newline = true;
        }
        result
    }
}

// Taffy can measure one element at several widths before assigning its bounds.
struct TextSource {
    text: String,
    objects: Vec<Range<usize>>,
    formats: Vec<FormatSpan>,
    alignments: Vec<(usize, TextAlignment)>,
}

impl TextSource {
    fn read(buffer: &super::buffer::InputBuffer) -> Self {
        Self {
            text: buffer.display_text(),
            objects: buffer.object_bytes().to_vec(),
            formats: buffer.formats().to_vec(),
            alignments: buffer.alignments().to_vec(),
        }
    }
}

#[derive(Clone)]
struct TextSpan {
    bytes: Range<usize>,
    units: Range<usize>,
}

struct Measure {
    source: Rc<TextSource>,
    span: TextSpan,
    typography: Typography,
    wrapped: Option<(Option<Pixels>, Vec<PaintLine>)>,
}

impl Measure {
    fn wrap(&mut self, width: Option<Pixels>, window: &mut Window) {
        if self.wrapped.as_ref().is_none_or(|(old, _)| *old != width) {
            self.wrapped = Some((
                width,
                self.typography
                    .shape(&self.source, &self.span, width, window),
            ));
        }
    }

    fn layout(&mut self, bounds: Bounds<Pixels>, window: &mut Window) -> TextSection {
        self.wrap(Some(bounds.size.width), window);
        let (_, lines) = self.wrapped.take().unwrap();
        let height = self.typography.line_height;
        TextSection {
            bounds,
            typography: self.typography.clone(),
            row_bottoms: row_bottoms(&lines, height),
            rows: visual_rows(&lines, self.span.units.start),
            lines,
        }
    }
}

pub(crate) struct TextSection {
    bounds: Bounds<Pixels>,
    typography: Typography,
    lines: Vec<PaintLine>,
    rows: Vec<Row>,
    row_bottoms: Vec<Pixels>,
}

fn row_bottoms(lines: &[PaintLine], base: Pixels) -> Vec<Pixels> {
    let mut bottoms = Vec::new();
    let mut bottom = px(0.);
    for line in lines {
        let (count, height) = match &line.content {
            LineContent::Plain { shaped, .. } => (shaped.wrap_boundaries().len() + 1, base),
            LineContent::Styled(row) => (1, row.size.height),
        };
        for _ in 0..count {
            bottom += height;
            bottoms.push(bottom);
        }
    }
    bottoms
}

impl TextSection {
    fn row_for_offset(&self, offset: usize) -> Option<usize> {
        if offset > self.rows.last()?.end() {
            return None;
        }
        self.rows
            .partition_point(|row| row.start <= offset)
            .checked_sub(1)
    }

    fn row_for_caret(&self, caret: Caret) -> Option<usize> {
        let row = self.row_for_offset(caret.offset)?;
        if caret.edge == CaretEdge::Trailing
            && let Some(previous) = row.checked_sub(1)
            && self.rows[row].start == caret.offset
            && self.rows[previous].end() == caret.offset
        {
            return Some(previous);
        }
        Some(row)
    }

    fn caret_bounds(&self, caret: Caret) -> Option<Bounds<Pixels>> {
        let row = self.row_for_caret(caret)?;
        let x = self.rows[row].x(caret);
        Some(self.rect(row, x, x))
    }

    pub(super) fn index_for_point(&self, position: Point<Pixels>) -> usize {
        let local = position - self.bounds.origin;
        if local.y < px(0.) {
            return self.rows[0].start;
        }
        let index = self.row_bottoms.partition_point(|&y| y <= local.y);
        match self.rows.get(index) {
            Some(row) => row.nearest(local.x).offset,
            None => self.rows.last().unwrap().end(),
        }
    }

    fn rect(&self, row: usize, left: Pixels, right: Pixels) -> Bounds<Pixels> {
        let top = row.checked_sub(1).map_or(px(0.), |i| self.row_bottoms[i]);
        Bounds::new(
            self.bounds.origin + point(left, top),
            size((right - left).max(px(1.)), self.row_bottoms[row] - top),
        )
    }

    fn row_segments(&self, i: usize, range: Range<usize>) -> Vec<(Pixels, Pixels)> {
        let row = &self.rows[i];
        let first = row.cells.partition_point(|c| c.units.end <= range.start);
        let mut spans: Vec<_> = row.cells[first..]
            .iter()
            .take_while(|c| c.units.start < range.end)
            .map(|c| (c.leading.min(c.trailing), c.leading.max(c.trailing)))
            .collect();
        if row.newline && range.contains(&row.end()) {
            let x = row.x(Caret::trailing(row.end()));
            let w = self.typography.font_size * 0.4;
            spans.push(
                if row.cells.last().is_some_and(|c| c.leading > c.trailing) {
                    (x - w, x)
                } else {
                    (x, x + w)
                },
            );
        }
        spans.sort_by_key(|&(left, _)| left);
        let mut merged: Vec<(Pixels, Pixels)> = Vec::new();
        for (left, right) in spans {
            if let Some(last) = merged.last_mut()
                && left <= last.1
            {
                last.1 = last.1.max(right);
            } else {
                merged.push((left, right));
            }
        }
        merged
    }

    /// AppKit places its candidate window at the first visual row touched by a range.
    pub(super) fn bounds_for_range(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        if range.start > range.end {
            return None;
        }
        let i = self.row_for_offset(range.start)?;
        if !range.is_empty() {
            let spans = self.row_segments(i, range.clone());
            if let (Some(first), Some(last)) = (spans.first(), spans.last()) {
                return Some(self.rect(i, first.0, last.1));
            }
        }
        let x = self.rows[i].x(Caret::leading(range.start));
        Some(self.rect(i, x, x))
    }

    pub(crate) fn rects(&self, range: Range<usize>) -> impl Iterator<Item = Bounds<Pixels>> + '_ {
        let first = self
            .rows
            .partition_point(|row| row.end() + usize::from(row.newline) <= range.start);
        let end = if range.is_empty() {
            first
        } else {
            self.rows.partition_point(|row| row.start < range.end)
        };
        (first..end).flat_map(move |i| {
            self.row_segments(i, range.clone())
                .into_iter()
                .map(move |(left, right)| self.rect(i, left, right))
        })
    }
}

fn paint_section(
    entity: &Entity<EditorView>,
    layout: &TextSection,
    document: &TextLayout,
    window: &mut Window,
    cx: &mut App,
) {
    let bounds = layout.bounds;
    crate::view_extensions::paint_decorations(
        entity,
        document,
        layout,
        super::PaintLayer::Background,
        window,
        cx,
    );
    let view = entity.read(cx);
    // Use the same visual rectangles as selection, including disjoint spans in bidi text.
    let section_start = layout.rows[0].start;
    let last_row = layout.rows.last().unwrap();
    let section_end = last_row.end() + usize::from(last_row.newline);
    let formats = view.buffer.formats();
    let first = formats.partition_point(|span| span.utf16_end <= section_start);
    let mut start = section_start;
    for span in &formats[first..] {
        if start >= section_end {
            break;
        }
        if let Some(color) = span.format.background {
            let color = rgba(u32::from_be_bytes(color));
            for rect in layout.rects(start..span.utf16_end) {
                window.paint_quad(fill(rect, color));
            }
        }
        start = span.utf16_end;
    }
    crate::view_extensions::paint_decorations(
        entity,
        document,
        layout,
        super::PaintLayer::TextBackground,
        window,
        cx,
    );
    let view = entity.read(cx);
    let selection = view.buffer.state().1;
    let (anchor, head) = view.buffer.selection();
    let caret = view.caret_at(head);
    let marked = view.buffer.marked_range();
    let focused = view.focus.is_focused(window);
    let native_selection = matches!(
        selection,
        gpdoc_core::Selection::Text { .. } | gpdoc_core::Selection::Node { .. }
    );
    if native_selection && anchor != head {
        let selection_color = view
            .behaviors
            .iter()
            .find_map(|behavior| behavior.selection_color(focused))
            .unwrap_or_else(|| {
                if focused {
                    rgba(0x659ff355)
                } else {
                    rgba(0x8795a33a)
                }
                .into()
            });
        for rect in layout.rects(anchor.min(head)..anchor.max(head)) {
            window.paint_quad(fill(rect, selection_color));
        }
    }
    let dividers = view.buffer.dividers();
    let first = dividers.partition_point(|(_, offset)| *offset < section_start);
    for &(before, offset) in dividers[first..]
        .iter()
        .take_while(|(_, offset)| *offset <= section_end)
    {
        if let Some(row) = layout.bounds_for_range(offset..offset) {
            let selected = view.editor().selection() == gpdoc_core::Selection::Node { before };
            let color = if selected && focused {
                rgb(0x659ff3)
            } else {
                rgb(0x8795a3)
            };
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left(), row.center().y),
                    size(bounds.size.width, px(1.)),
                ),
                color,
            ));
        }
    }
    let height = layout.typography.line_height;
    let mut origin = bounds.origin;
    let clip = window.content_mask().bounds;
    for painted in &layout.lines {
        let bottom = origin.y + painted.size(height).height;
        if origin.y > clip.bottom() {
            break;
        }
        if bottom > clip.top() {
            match &painted.content {
                LineContent::Plain { shaped, .. } => shaped
                    .paint(origin, height, Default::default(), None, window, cx)
                    .expect("GPUI could not paint the editor text"),
                LineContent::Styled(row) => row.paint(origin, window, cx),
            }
        }
        origin.y = bottom;
    }
    if let Some(range) = marked {
        for rect in layout.rects(range) {
            window.paint_quad(fill(
                Bounds::new(
                    point(rect.left(), rect.bottom() - px(3.)),
                    size(rect.size.width, px(1.)),
                ),
                rgb(0x346bb0),
            ));
        }
    }
    if focused
        && native_selection
        && anchor == head
        && let Some(rect) = layout.caret_bounds(caret)
    {
        window.paint_quad(fill(
            Bounds::new(
                point(rect.left(), rect.top() + px(3.)),
                size(px(1.5), (rect.size.height - px(6.)).max(px(1.))),
            ),
            rgb(0x294e7d),
        ));
    }
    crate::view_extensions::paint_decorations(
        entity,
        document,
        layout,
        super::PaintLayer::Foreground,
        window,
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_activation_resolves_urls_inside_the_painted_atom() {
        let layout = TextSection {
            bounds: Bounds::new(point(px(0.), px(0.)), size(px(80.), px(24.))),
            typography: Typography {
                images: Default::default(),
                style: TextStyle::default(),
                font_size: px(16.),
                line_height: px(24.),
            },
            lines: vec![],
            rows: vec![Row {
                left: px(0.),
                start: 0,
                newline: false,
                cells: vec![Cell {
                    byte: 0,
                    units: 0..3,
                    leading: px(0.),
                    trailing: px(40.),
                }],
            }],
            row_bottoms: vec![px(24.)],
        };
        let layout = TextLayout {
            bounds: layout.bounds,
            sections: vec![Rc::new(layout)],
            children: Vec::new(),
        };
        let span = |src: &str| {
            [FormatSpan {
                byte_end: 3,
                utf16_end: 3,
                format: gpdoc_core::TextFormat {
                    media_source: Some(src.into()),
                    ..Default::default()
                },
            }]
        };
        let inside = point(px(20.), px(10.));
        let base = url::Url::parse("https://example.org/docs/").unwrap();
        for (source, base, result) in [
            (
                "https://media.example.org/clip?t=8s",
                None,
                Some("https://media.example.org/clip?t=8s"),
            ),
            (
                "//media.example.org/clip",
                None,
                Some("https://media.example.org/clip"),
            ),
            (
                "../intro.ogg",
                Some(&base),
                Some("https://example.org/intro.ogg"),
            ),
            ("../intro.ogg", None, None),
            ("data:audio/mp3;base64,AA", None, None),
        ] {
            let target = layout.url_at(inside, &span(source), base);
            assert_eq!(target.as_ref().map(url::Url::as_str), result);
            assert!(
                layout
                    .url_at(point(px(60.), px(10.)), &span(source), base)
                    .is_none()
            );
        }
    }

    #[test]
    fn hebrew_caret_edges_follow_the_painted_letters() {
        let text = "שלום";
        let glyphs = [(6, px(0.)), (4, px(8.)), (2, px(14.)), (0, px(24.))];
        let row = Row {
            left: px(0.),
            start: 0,
            cells: paragraph_cells(text, &[], 0, glyphs.into_iter(), px(36.), None),
            newline: false,
        };
        let positions: Vec<_> = (0..=4)
            .map(|offset| row.x(Caret::leading(offset)))
            .collect();
        for (i, x) in positions.iter().enumerate() {
            assert_eq!(row.nearest(*x).offset, i);
        }
        assert_eq!(positions, [px(36.), px(24.), px(14.), px(8.), px(0.)]);
    }

    #[test]
    fn arabic_ligature_has_a_caret_between_its_two_letters() {
        // Core Text shapes the middle lam and alif as a single glyph. Its first
        // logical byte is 2, but the cluster occupies the visual span 10..22.
        let text = "سلام";
        let glyphs = [(6, px(0.)), (2, px(10.)), (0, px(22.))];
        let row = Row {
            left: px(0.),
            start: 0,
            cells: paragraph_cells(text, &[], 0, glyphs.into_iter(), px(36.), None),
            newline: false,
        };
        let xs = [px(36.), px(22.), px(16.), px(10.), px(0.)];
        for (i, x) in xs.into_iter().enumerate() {
            assert_eq!(row.x(Caret::leading(i)), x);
            assert_eq!(row.nearest(x).offset, i);
        }
    }

    #[test]
    fn rtl_wraps_keep_logical_text_and_graphemes_in_reading_order() {
        let text = "אבגד הוזחט";
        // One equal-width glyph per grapheme, painted in visual order.
        let glyphs = text
            .char_indices()
            .rev()
            .enumerate()
            .map(|(n, (byte, _))| (byte, px(n as f32 * 10.)));
        let cells = paragraph_cells(text, &[], 0, glyphs, px(100.), None);
        let ranges = logical_wraps(text, &cells, px(50.));
        assert_eq!(
            ranges.iter().map(|r| &text[r.clone()]).collect::<Vec<_>>(),
            ["אבגד ", "הוזחט"]
        );
        let narrow = logical_wraps(text, &cells, px(5.));
        assert_eq!(narrow.len(), text.graphemes(true).count());
        assert_eq!(
            narrow.into_iter().map(|r| &text[r]).collect::<String>(),
            text
        );
    }

    #[test]
    fn logical_selection_can_have_two_separated_visual_rectangles() {
        let glyphs = [(0, px(0.)), (3, px(10.)), (1, px(20.)), (5, px(32.))];
        let text = "AאבB";
        let row = Row {
            left: px(0.),
            start: 0,
            cells: paragraph_cells(text, &[], 0, glyphs.into_iter(), px(44.), None),
            newline: false,
        };
        let layout = TextSection {
            bounds: Bounds::default(),
            typography: Typography {
                images: Default::default(),
                style: TextStyle::default(),
                font_size: px(16.),
                line_height: px(24.),
            },
            lines: vec![],
            rows: vec![row],
            row_bottoms: vec![px(24.)],
        };
        // A and alef are logically adjacent, but bet sits between them on screen.
        let rects: Vec<_> = layout.rects(0..2).map(|r| (r.left(), r.right())).collect();
        assert_eq!(rects, [(px(0.), px(10.)), (px(20.), px(32.))]);
        assert_eq!(layout.index_for_point(point(px(21.), px(8.))), 2);
        assert_eq!(layout.index_for_point(point(px(31.), px(8.))), 1);
    }

    #[test]
    fn glyph_hits_never_select_a_surrogate_or_part_of_a_family_emoji() {
        let text = "A👨‍👩‍👧B";
        let glyphs = [(0, px(0.)), (1, px(14.)), ("A👨‍👩‍👧".len(), px(40.))];
        let row = Row {
            left: px(0.),
            start: 3,
            cells: paragraph_cells(text, &[], 3, glyphs.into_iter(), px(54.), None),
            newline: false,
        };
        assert_eq!(row.nearest(px(13.)).offset, 4);
        assert_eq!(row.nearest(px(24.)).offset, 4);
        assert_eq!(row.nearest(px(35.)).offset, 12);
        assert_eq!(row.nearest(px(100.)).offset, 13);
    }

    #[test]
    fn vertical_navigation_crosses_padding_and_preserves_the_visual_column() {
        let section = |start, x, y, widths: &[usize]| {
            let mut offset = start;
            let rows = widths
                .iter()
                .map(|&width| {
                    let start = offset;
                    offset += width;
                    Row {
                        start,
                        left: px(0.),
                        newline: false,
                        cells: (0..width)
                            .map(|i| Cell {
                                byte: i,
                                units: start + i..start + i + 1,
                                leading: px(10. * i as f32),
                                trailing: px(10. * (i + 1) as f32),
                            })
                            .collect(),
                    }
                })
                .collect();
            let height = px(24.);
            Rc::new(TextSection {
                bounds: Bounds::new(
                    point(px(x), px(y)),
                    size(px(80.), height * widths.len() as f32),
                ),
                typography: Typography {
                    images: Default::default(),
                    style: TextStyle::default(),
                    font_size: px(16.),
                    line_height: height,
                },
                lines: vec![],
                rows,
                row_bottoms: (1..=widths.len()).map(|i| height * i as f32).collect(),
            })
        };
        let layout = TextLayout {
            bounds: Bounds::default(),
            sections: vec![
                section(0, 0., 0., &[4]),
                section(5, 0., 52., &[0]),
                section(6, 90., 52., &[4]),
                section(11, 0., 92., &[1, 4]),
            ],
            children: vec![],
        };
        let step = |head, x, down| {
            layout
                .vertical_destination(Caret::leading(head), x, down, |next| next)
                .map(|(destination, x)| (destination.offset, x))
        };
        let x = Some(px(30.));
        assert_eq!(step(3, None, true), Some((5, px(30.))));
        // These sections share a screen row. Their document order still wins.
        assert_eq!(step(5, x, true), Some((6, px(30.))));
        assert_eq!(step(6, x, true), Some((12, px(30.))));
        assert_eq!(step(11, x, true), Some((15, px(30.))));
        assert_eq!(step(15, x, false), Some((12, px(30.))));
        assert_eq!(step(11, x, false), Some((6, px(30.))));
        assert_eq!(step(6, x, false), Some((5, px(30.))));
        assert_eq!(step(5, x, false), Some((3, px(30.))));
    }

    #[test]
    fn one_geometry_drives_wrapped_rows_empty_paragraphs_and_window_coordinates() {
        let row = |data: &[(usize, f32)], newline| Row {
            left: px(0.),
            start: data[0].0,
            cells: data
                .windows(2)
                .map(|pair| Cell {
                    byte: 0,
                    units: pair[0].0..pair[1].0,
                    leading: px(pair[0].1),
                    trailing: px(pair[1].1),
                })
                .collect(),
            newline,
        };
        let layout = TextSection {
            bounds: Bounds::new(point(px(24.), px(-8.)), size(px(36.), px(96.))),
            typography: Typography {
                images: Default::default(),
                style: TextStyle::default(),
                font_size: px(15.),
                line_height: px(24.),
            },
            lines: vec![],
            row_bottoms: vec![px(24.), px(48.), px(72.), px(96.)],
            rows: vec![
                row(&[(0, 0.), (1, 15.), (3, 35.)], false),
                row(&[(3, 0.), (4, 15.)], true),
                row(&[(5, 0.)], true),
                row(&[(6, 0.), (7, 15.)], false),
            ],
        };
        assert_eq!(layout.index_for_point(point(px(80.), px(-30.))), 0);
        assert_eq!(layout.index_for_point(point(px(24.), px(100.))), 7);
        assert_eq!(layout.index_for_point(point(px(24.), px(17.))), 3);
        assert_eq!(layout.index_for_point(point(px(24.), px(44.))), 5);
        assert_eq!(
            layout.bounds_for_range(3..5).unwrap().origin,
            point(px(24.), px(16.))
        );
        assert_eq!(layout.bounds_for_range(5..5).unwrap().top(), px(40.));
        assert_eq!(layout.bounds_for_range(8..8), None);
        let rects = layout.rects(1..6).collect::<Vec<_>>();
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].origin, point(px(39.), px(-8.)));
        assert_eq!(rects[2].size.width, px(6.));
    }
}
