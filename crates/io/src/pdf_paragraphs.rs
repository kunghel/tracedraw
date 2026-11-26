//! PDF text back into paragraph text (#2323).
//!
//! A PDF keeps no paragraphs: each line (often each piece of a line in
//! one font) is drawn on its own. Read as it is, a page of body text
//! becomes a pile of artistic texts, one per line, and editing a word
//! means retyping the lines after it by hand. Here the lines of a page
//! that stand one under the other in the same font, size and spacing,
//! lined up on the left (or centred, or on the right), become one
//! paragraph text frame, and a frame whose last line is full continues
//! in the next column or at the top of the next page: the two frames
//! are marked to be linked ([`FLOW_KEY`]), so text typed in the first
//! flows on into the second.
//!
//! Every frame is laid out with the text engine, when one is registered,
//! before it replaces its lines: unless the engine breaks the text where
//! the file broke it, with the baselines where the file has them, the
//! lines stay as they came (first with each line ended by a line break,
//! then as artistic texts). The look of the page comes first.
//!
//! A line a PDF draws under some letters (it keeps no underlined text,
//! only the line) becomes those letters' underline, and a wide gap in a
//! line that the file left for a tab becomes a tab with a stop where the
//! text after it starts (#2569).

use std::collections::HashMap;
use tracedraw_core::{
    document::{
        text_outline, LeadingUnit, LineStyle, ParagraphStyle, Shape, ShapeKind, TabStop, TextSpan,
    },
    geometry::{Affine, Point, Size},
    Fill, ShapeId, TextAlign,
};

use crate::pdf_import::ImportedPage;

const PT_MM: f64 = 25.4 / 72.0;

/// The object data key of a frame to link: `"<chain>:<place>"`, the
/// frames of one chain numbered in their order from 0. The program
/// links them (`text.next` and `text.prev`) once the frames have the ids
/// they keep in the drawing, and takes the key away.
pub const FLOW_KEY: &str = "pdf.flow";

/// How far apart two lines' left edges (or centres, or right edges) may be
/// and still be lined up, mm.
const LINE_UP_MM: f64 = 0.35;

/// One line of text as the file placed it: one or more artistic texts on
/// one baseline, next to one another.
#[derive(Debug, Clone)]
struct Line {
    /// Indexes in the page's objects of the texts it is made of.
    pieces: Vec<usize>,
    /// The texts' turn (and slant) without their place.
    linear: [f64; 4],
    /// Where the baseline starts and how high it is, measured along and
    /// across the text's own direction (`linear` undone), mm.
    u: f64,
    w: f64,
    /// Where it ends along the baseline.
    right: f64,
    /// Where the text after each tab in it starts along the baseline.
    tabs: Vec<f64>,
    spans: Vec<TextSpan>,
    fill: Fill,
    opacity: f64,
}

impl Line {
    fn width(&self) -> f64 {
        self.right - self.u
    }

    fn centre(&self) -> f64 {
        (self.u + self.right) / 2.0
    }

    /// The span with the most characters: the line's font.
    fn main(&self) -> &TextSpan {
        self.spans
            .iter()
            .max_by_key(|s| s.text.chars().count())
            .unwrap_or(&self.spans[0])
    }

    fn em(&self) -> f64 {
        self.main().size_pt * PT_MM
    }

    fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// The line starts with a piece in another font than its own, a list
    /// item's bullet (a symbol font's dot before the text): such a line
    /// starts a list item, not the next line of the paragraph above. Run
    /// on into it, the bullet came inside the text, and the text broke
    /// over the next bullets (#3551).
    fn leads_with_mark(&self) -> bool {
        let first = self.spans.iter().find(|s| !s.text.trim().is_empty());
        first.is_some_and(|f| f.font_family != self.main().font_family)
    }

    /// How wide the first word may be, with the space before it: on the
    /// wide side (letters of 0.65 em), so that a line followed by a word
    /// that did not fit after it counts as full; a paragraph's last line
    /// leaves far more room.
    fn first_word_width(&self) -> f64 {
        let text = self.text();
        let n = text
            .trim_start()
            .split([' ', '\t'])
            .next()
            .map_or(0, |w| w.chars().count());
        (n as f64 * 0.65 + 0.35) * self.em()
    }
}

/// Same turn, slant and font as `other`.
fn same_kind(a: &Line, b: &Line) -> bool {
    let (ma, mb) = (a.main(), b.main());
    a.linear
        .iter()
        .zip(&b.linear)
        .all(|(x, y)| (x - y).abs() < 1e-4)
        && ma.font_family == mb.font_family
        && (ma.size_pt - mb.size_pt).abs() < 0.05
        && ma.bold == mb.bold
        && ma.italic == mb.italic
        && (a.opacity - b.opacity).abs() < 1e-6
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Lined {
    Left,
    Centre,
    Right,
}

/// Lines that make one paragraph frame.
#[derive(Debug, Clone)]
struct Block {
    lines: Vec<Line>,
    lined: Lined,
    /// The first line starts this much further right than the others.
    indent: f64,
    /// Baseline to baseline, mm (0 for one line).
    leading: f64,
    /// The frame is at least this wide: a line alone that a frame flows
    /// on to is as wide as that frame.
    at_least: f64,
}

impl Block {
    fn first(&self) -> &Line {
        &self.lines[0]
    }

    fn last(&self) -> &Line {
        &self.lines[self.lines.len() - 1]
    }

    /// The left edge of the lines other than an indented first one.
    fn left(&self) -> f64 {
        match self.lines.get(1..) {
            Some(rest) if !rest.is_empty() => rest.iter().map(|l| l.u).fold(f64::MAX, f64::min),
            _ => self.first().u,
        }
    }

    fn right(&self) -> f64 {
        self.lines.iter().map(|l| l.right).fold(f64::MIN, f64::max)
    }

    /// The frame's width the lines need.
    fn width(&self) -> f64 {
        let own = match self.lined {
            Lined::Left => self.right() - self.left(),
            _ => self.lines.iter().map(Line::width).fold(0.0, f64::max),
        };
        own.max(self.at_least)
    }

    /// The room left at the end of line `i` in a frame of `width`.
    fn room_after(&self, i: usize, width: f64) -> f64 {
        let l = &self.lines[i];
        match self.lined {
            Lined::Left => self.left() + width - l.right,
            _ => width - l.width(),
        }
    }

    /// Line `i` ends its paragraph: the next line's first word would
    /// have fitted after it.
    fn ends_paragraph(&self, i: usize, width: f64) -> bool {
        match self.lines.get(i + 1) {
            // Lines with tabs are rows (a form, a list of items): each
            // ends where the file ended it.
            Some(next) if !next.tabs.is_empty() || !self.lines[i].tabs.is_empty() => true,
            Some(next) => self.room_after(i, width) > next.first_word_width(),
            None => true,
        }
    }

    /// `line` goes on this block, the next one down.
    fn takes(&mut self, line: &Line) -> bool {
        let last = self.last();
        if !same_kind(self.first(), line) || line.leads_with_mark() {
            return false;
        }
        let em = self.first().em();
        let drop = last.w - line.w;
        if drop < 0.8 * em || drop > 2.5 * em {
            return false;
        }
        if self.lines.len() > 1 && (drop - self.leading).abs() > (0.02 * em).max(0.1) {
            return false;
        }
        let first = self.first().clone();
        if self.lines.len() == 1 {
            let lined = if (line.u - first.u).abs() < LINE_UP_MM {
                Some((Lined::Left, 0.0))
            } else if first.u - line.u > LINE_UP_MM && first.u - line.u < 4.0 * em {
                // An indented first line.
                Some((Lined::Left, first.u - line.u))
            } else if (line.centre() - first.centre()).abs() < LINE_UP_MM {
                Some((Lined::Centre, 0.0))
            } else if (line.right - first.right).abs() < LINE_UP_MM {
                Some((Lined::Right, 0.0))
            } else {
                None
            };
            let Some((lined, indent)) = lined else {
                return false;
            };
            self.lined = lined;
            self.indent = indent;
            self.leading = drop;
            self.lines.push(line.clone());
            return true;
        }
        let ok = match self.lined {
            Lined::Left => {
                let left = self.left();
                (line.u - left).abs() < LINE_UP_MM
                    // The indented first line of the next paragraph, after
                    // a line that ended one.
                    || (self.indent > 0.0
                        && (line.u - (left + self.indent)).abs() < LINE_UP_MM
                        && self.right() - last.right > line.first_word_width())
            }
            Lined::Centre => (line.centre() - first.centre()).abs() < LINE_UP_MM,
            Lined::Right => (line.right - first.right).abs() < LINE_UP_MM,
        };
        if ok {
            self.lines.push(line.clone());
        }
        ok
    }
}

/// Make paragraph frames of the pages' lines of text, linking those that
/// continue one another. `widths` gives each artistic text's length
/// along its baseline, mm, as the file draws it, and `char_xs` where each
/// of its characters starts along it (from its origin, mm), where known.
pub fn make_paragraphs(
    pages: &mut [ImportedPage],
    widths: &HashMap<ShapeId, f64>,
    char_xs: &HashMap<ShapeId, Vec<f64>>,
) {
    for page in pages.iter_mut() {
        underlines_into_text(&mut page.shapes, widths, char_xs);
    }
    // The blocks of every page, in order.
    let mut blocks: Vec<Vec<Block>> = pages
        .iter()
        .map(|p| blocks_of(&lines_of(&p.shapes, widths, char_xs)))
        .collect();
    let links = links_between(&blocks);
    // A line alone that a frame flows on to takes that frame's width and
    // spacing.
    for &(a, b) in &links {
        if blocks[b.0][b.1].lines.len() == 1 {
            let (width, leading) = {
                let from = &blocks[a.0][a.1];
                (from.width(), from.leading)
            };
            let to = &mut blocks[b.0][b.1];
            to.at_least = width;
            to.leading = leading;
        }
    }
    let mut chain_of: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
    let mut chains = 0;
    for &(a, b) in &links {
        let (chain, place) = match chain_of.get(&a) {
            Some(&c) => c,
            None => {
                chains += 1;
                chain_of.insert(a, (chains - 1, 0));
                (chains - 1, 0)
            }
        };
        chain_of.insert(b, (chain, place + 1));
    }
    for (pi, page) in pages.iter_mut().enumerate() {
        let mut gone = vec![false; page.shapes.len()];
        for (bi, block) in blocks[pi].iter().enumerate() {
            let chained = chain_of.get(&(pi, bi)).copied();
            // A line alone stays artistic text, unless it is where a
            // frame flows on to.
            if block.lines.len() < 2 && chained.is_none() {
                continue;
            }
            let continues = links.iter().any(|(a, _)| *a == (pi, bi));
            let at = block.first().pieces[0];
            let Some(frame) = frame_of(block, &page.shapes[at], continues, chained) else {
                continue;
            };
            for line in &block.lines {
                for &i in &line.pieces {
                    gone[i] = true;
                }
            }
            gone[at] = false;
            page.shapes[at] = frame;
        }
        let mut i = 0;
        page.shapes.retain(|_| {
            i += 1;
            !gone[i - 1]
        });
    }
}

/// Lines the file draws under letters (a stroked straight line or a thin
/// filled bar, just below a text's baseline, along it) become the
/// underline of the letters they run under, and go: a PDF keeps no
/// underlined text, only the line. Only texts whose characters' places
/// are known (`char_xs`) take one.
fn underlines_into_text(
    shapes: &mut Vec<Shape>,
    widths: &HashMap<ShapeId, f64>,
    char_xs: &HashMap<ShapeId, Vec<f64>>,
) {
    let texts: Vec<usize> = (0..shapes.len())
        .filter(|&i| {
            matches!(
                &shapes[i].kind,
                ShapeKind::Text { spans, frame: None, on_path: None, .. } if spans.len() == 1
            ) && char_xs.contains_key(&shapes[i].id)
        })
        .collect();
    if texts.is_empty() {
        return;
    }
    let mut gone = vec![false; shapes.len()];
    // Per text, the characters to underline and how thick the line is.
    let mut marks: HashMap<usize, (Vec<bool>, bool)> = HashMap::new();
    for j in 0..shapes.len() {
        let Some((a, b, thick)) = bar_of(&shapes[j]) else {
            continue;
        };
        for &i in &texts {
            let t = &shapes[i];
            let (Some(xs), Some(&width)) = (char_xs.get(&t.id), widths.get(&t.id)) else {
                continue;
            };
            let ShapeKind::Text { spans, .. } = &t.kind else {
                continue;
            };
            let em = spans[0].size_pt * PT_MM;
            if em <= 0.0 || t.transform.determinant().abs() < 1e-9 {
                continue;
            }
            let inv = t.transform.inverse();
            let (p, q) = (inv * a, inv * b);
            let (x0, x1) = (p.x.min(q.x), p.x.max(q.x));
            let y = (p.y + q.y) / 2.0;
            // Along the text, just below its baseline, under some of it
            // and not reaching far past it: a longer line is a line of
            // the drawing that passes under the text (a wire leaving a
            // label, the edge of a box under a name), not its underline,
            // and taking it away left a gap in the drawing (#4100).
            if (p.y - q.y).abs() > 0.05 * em
                || !(-0.4 * em..=-0.02 * em).contains(&y)
                || x1 < 0.0
                || x0 > width
                || x0 < -UNDERLINE_REACH_EM * em
                || x1 > width + UNDERLINE_REACH_EM * em
                || thick > 0.2 * em
            {
                continue;
            }
            let n = xs.len();
            let entry = marks.entry(i).or_insert_with(|| (vec![false; n], false));
            let mut any = false;
            for k in 0..n {
                let end = xs.get(k + 1).copied().unwrap_or(width);
                let mid = (xs[k] + end) / 2.0;
                if mid > x0 && mid < x1 {
                    entry.0[k] = true;
                    any = true;
                }
            }
            if any {
                entry.1 |= thick > 0.1 * em;
                gone[j] = true;
                break;
            }
        }
    }
    for (i, (under, thick)) in marks {
        let ShapeKind::Text { spans, .. } = &mut shapes[i].kind else {
            continue;
        };
        let style = if thick {
            LineStyle::SingleThick
        } else {
            LineStyle::SingleThin
        };
        let base = spans[0].clone();
        let mut out: Vec<TextSpan> = Vec::new();
        for (c, u) in base.text.chars().zip(under.iter().copied()) {
            let want = if u { style } else { base.underline };
            match out.last_mut() {
                Some(last) if last.underline == want => last.text.push(c),
                _ => {
                    let mut sp = base.clone();
                    sp.text = c.to_string();
                    sp.underline = want;
                    out.push(sp);
                }
            }
        }
        if !out.is_empty() {
            *spans = out;
        }
    }
    let mut i = 0;
    shapes.retain(|_| {
        i += 1;
        !gone[i - 1]
    });
}

/// How far past either end of a text (in ems) a line under it may reach
/// and still be its underline.
const UNDERLINE_REACH_EM: f64 = 0.5;

/// The two ends and the thickness of a straight horizontal-ish bar the
/// file drew: a stroked line of two points with no fill, or a thin filled
/// rectangle with no outline. `None` for anything else.
fn bar_of(s: &Shape) -> Option<(Point, Point, f64)> {
    use tracedraw_core::geometry::PathEl;
    let ShapeKind::Path { path, .. } = &s.kind else {
        return None;
    };
    let pts: Vec<Point> = path
        .elements()
        .iter()
        .filter_map(|e| match e {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(s.transform * *p),
            PathEl::ClosePath => None,
            _ => Some(Point::new(f64::NAN, f64::NAN)),
        })
        .collect();
    if pts.iter().any(|p| !p.x.is_finite()) {
        return None;
    }
    let filled = !matches!(s.fill, Fill::None);
    match (pts.as_slice(), &s.stroke) {
        ([a, b], Some(st)) if !filled => Some((*a, *b, st.width)),
        ([a, b, c, d] | [a, b, c, d, _], None) if filled => {
            // A rectangle: its long sides' middles, its short side as thick.
            let (ab, bc) = (a.distance(*b), b.distance(*c));
            let (cd, da) = (c.distance(*d), d.distance(*a));
            if (ab - cd).abs() > 0.05 * ab.max(1e-9) || (bc - da).abs() > 0.05 * bc.max(1e-9) {
                return None;
            }
            if ab >= bc {
                Some((a.midpoint(*d), b.midpoint(*c), bc))
            } else {
                Some((a.midpoint(*b), d.midpoint(*c), ab))
            }
        }
        _ => None,
    }
}

/// The page's artistic texts on one baseline joined into lines, in the
/// order they come.
fn lines_of(
    shapes: &[Shape],
    widths: &HashMap<ShapeId, f64>,
    char_xs: &HashMap<ShapeId, Vec<f64>>,
) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for (i, s) in shapes.iter().enumerate() {
        let ShapeKind::Text {
            spans,
            frame: None,
            on_path: None,
            ..
        } = &s.kind
        else {
            continue;
        };
        let Some(&width) = widths.get(&s.id) else {
            continue;
        };
        if spans.is_empty() || spans.iter().any(|sp| sp.text.contains('\n')) {
            continue;
        }
        let c = s.transform.as_coeffs();
        let det = c[0] * c[3] - c[1] * c[2];
        // Mirrored or squashed text is left alone.
        if det < 0.5 {
            continue;
        }
        let linear = [c[0], c[1], c[2], c[3]];
        // The place in the text's own directions.
        let mut u = (c[3] * c[4] - c[2] * c[5]) / det;
        let w = (-c[1] * c[4] + c[0] * c[5]) / det;
        let fill = s.fill.clone();
        let mut spans = spans.clone();
        let right = u + width;
        // Spaces the text starts with move its start to its first letter,
        // and wide gaps inside it are tabs.
        let mut tabs = Vec::new();
        if let Some(xs) = char_xs.get(&s.id) {
            let em = spans[0].size_pt * PT_MM;
            let (lead, inner) = gaps_into_tabs(&mut spans, xs, width, em);
            tabs = inner.into_iter().map(|t| u + t).collect();
            u += lead;
        }
        if spans.iter().all(|sp| sp.text.is_empty()) {
            continue;
        }
        let piece = Line {
            pieces: vec![i],
            linear,
            u,
            w,
            right,
            tabs,
            spans: spans.clone(),
            fill: fill.clone(),
            opacity: s.opacity,
        };
        // The next piece of the line before: right after it, on its
        // baseline.
        if let Some(prev) = lines.last_mut() {
            let em = prev.em().max(piece.em());
            let follows = i > 0
                && prev.pieces.last() == Some(&(i - 1))
                && prev
                    .linear
                    .iter()
                    .zip(&linear)
                    .all(|(x, y)| (x - y).abs() < 1e-4)
                && (prev.w - w).abs() < 0.05 * em
                && u > prev.right - 0.2 * em
                && u < prev.right + TAB_GAP_MAX_EM * em
                && (prev.opacity - s.opacity).abs() < 1e-6;
            if follows {
                // The gap without the spaces the texts end and start with.
                let trailing = prev.text().len() - prev.text().trim_end().len();
                let gap = u - prev.right + trailing as f64 * 0.28 * em;
                let ends_space = prev.text().ends_with(' ');
                let starts_space = spans[0].text.starts_with(' ');
                if gap > TAB_GAP_MIN_EM * em {
                    // A tab: the spaces about it go, the stop is where the
                    // text after it starts.
                    trim_end_spans(&mut prev.spans);
                    let first = &mut spans[0].text;
                    *first = first.trim_start().to_string();
                    first.insert(0, '\t');
                    prev.tabs.push(u);
                } else if gap > 0.18 * em && !ends_space && !starts_space {
                    spans[0].text.insert(0, ' ');
                }
                for sp in &mut spans {
                    if sp.fill.is_none() && fill != prev.fill {
                        sp.fill = Some(fill.clone());
                    }
                }
                prev.spans.extend(spans);
                prev.pieces.push(i);
                prev.right = prev.right.max(right);
                prev.tabs.extend(piece.tabs.iter().copied());
                continue;
            }
        }
        lines.push(piece);
    }
    lines
}

/// A gap in a line wider than this many ems (a few spaces' worth, more
/// than a justified line stretches its spaces) is a tab.
const TAB_GAP_MIN_EM: f64 = 1.6;
/// Texts on one baseline further apart than this many ems are not one
/// line (columns side by side).
const TAB_GAP_MAX_EM: f64 = 8.0;

/// The spaces `spans` (one text, its characters at `xs` along the
/// baseline, `width` long) start with taken off, and each run of spaces
/// wider than [`TAB_GAP_MIN_EM`] made a tab: how far its first letter is
/// from where it started, and where the text after each tab starts.
fn gaps_into_tabs(spans: &mut [TextSpan], xs: &[f64], width: f64, em: f64) -> (f64, Vec<f64>) {
    let chars: Vec<(usize, char)> = spans
        .iter()
        .enumerate()
        .flat_map(|(si, sp)| sp.text.chars().map(move |c| (si, c)))
        .collect();
    if chars.len() != xs.len() || em <= 0.0 {
        return (0.0, Vec::new());
    }
    let at = |k: usize| xs.get(k).copied().unwrap_or(width);
    let mut keep = vec![true; chars.len()];
    let mut tab_at = vec![false; chars.len()];
    let mut tabs = Vec::new();
    let lead_n = chars.iter().take_while(|(_, c)| *c == ' ').count();
    let lead = if lead_n < chars.len() {
        at(lead_n)
    } else {
        0.0
    };
    for k in keep.iter_mut().take(lead_n) {
        *k = false;
    }
    let mut k = lead_n;
    while k < chars.len() {
        if chars[k].1 != ' ' {
            k += 1;
            continue;
        }
        let end = (k..chars.len())
            .find(|&j| chars[j].1 != ' ')
            .unwrap_or(chars.len());
        if end < chars.len() && at(end) - at(k) > TAB_GAP_MIN_EM * em {
            for k in &mut keep[k..end] {
                *k = false;
            }
            tab_at[k] = true;
            tabs.push(at(end));
        }
        k = end;
    }
    if lead_n == 0 && tabs.is_empty() {
        return (0.0, tabs);
    }
    for sp in spans.iter_mut() {
        sp.text.clear();
    }
    for (k, (si, c)) in chars.into_iter().enumerate() {
        if tab_at[k] {
            spans[si].text.push('\t');
        } else if keep[k] {
            spans[si].text.push(c);
        }
    }
    (lead, tabs)
}

/// The spaces at the end of `spans` taken off.
fn trim_end_spans(spans: &mut Vec<TextSpan>) {
    while let Some(last) = spans.last_mut() {
        let keep = last.text.trim_end().len();
        last.text.truncate(keep);
        if !last.text.is_empty() || spans.len() == 1 {
            break;
        }
        spans.pop();
    }
}

/// Lines that follow one another in the page's objects, one under the
/// other, gathered into blocks.
fn blocks_of(lines: &[Line]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    for line in lines {
        if let Some(b) = blocks.last_mut() {
            let next_object = b.last().pieces.last().map(|i| i + 1) == line.pieces.first().copied();
            if next_object && b.takes(line) {
                continue;
            }
        }
        blocks.push(Block {
            lines: vec![line.clone()],
            lined: Lined::Left,
            indent: 0.0,
            leading: 0.0,
            at_least: 0.0,
        });
    }
    blocks
}

/// Which blocks continue in which: (page, block) pairs, the first
/// flowing into the second. A frame of body text whose last line is full
/// goes on in the next block when that one is in the same font, about
/// as wide and as spaced, and starts the next column of the page or the
/// next page.
fn links_between(blocks: &[Vec<Block>]) -> Vec<((usize, usize), (usize, usize))> {
    let mut out = Vec::new();
    for (pi, page) in blocks.iter().enumerate() {
        for (bi, a) in page.iter().enumerate() {
            if a.lines.len() < 2 || a.lined != Lined::Left {
                continue;
            }
            let (b_at, b) = if bi + 1 < page.len() {
                ((pi, bi + 1), &page[bi + 1])
            } else {
                match blocks.get(pi + 1).and_then(|p| p.first()) {
                    Some(b) => ((pi + 1, 0), b),
                    None => continue,
                }
            };
            if !same_kind(a.first(), b.first()) || b.lined != Lined::Left {
                continue;
            }
            let width = a.width();
            // The text goes on: the first word of the next block would
            // not have fitted at the end of this one.
            let room = a.left() + width - a.last().right;
            if room > b.first().first_word_width() {
                continue;
            }
            // As wide (its ragged lines may all end a little short) and
            // as spaced.
            if b.width() > width + 1.0
                || (b.lines.len() > 1
                    && (b.width() < width * 0.8 || (b.leading - a.leading).abs() > 0.1))
            {
                continue;
            }
            if b_at.0 == pi {
                // The next column: higher up than this block's end and to
                // its right.
                if b.first().w <= a.last().w || b.left() < a.left() + width * 0.5 {
                    continue;
                }
            }
            out.push(((pi, bi), b_at));
        }
    }
    out
}

/// The paragraph frame of `block`, in place of the line `like` (the
/// block's first text, whose look it keeps): `None` when the text engine
/// would not lay its text out as the file has it. `continues`: the frame
/// flows on into another, so its text ends with the space between the two.
fn frame_of(
    block: &Block,
    like: &Shape,
    continues: bool,
    chained: Option<(usize, usize)>,
) -> Option<Shape> {
    let first = block.first();
    let em = first.em();
    let leading = if block.leading > 0.0 {
        block.leading
    } else {
        1.2 * em
    };
    let width = block.width();
    let align = match block.lined {
        Lined::Left => {
            // Justified: every line that does not end a paragraph reaches
            // the right edge.
            let full: Vec<f64> = (0..block.lines.len() - 1)
                .filter(|&i| !block.ends_paragraph(i, width))
                .map(|i| block.lines[i].right)
                .collect();
            // A line or two a little short (a producer that spreads a
            // line of several fonts unevenly) leaves it justified while
            // most reach the edge.
            let edge = full.iter().fold(f64::MIN, |a, &b| a.max(b));
            let reach = full.iter().filter(|&&r| edge - r < 0.3).count();
            if full.len() >= 2
                && (reach == full.len() || (reach >= 3 && reach * 4 >= full.len() * 3))
            {
                TextAlign::Justify
            } else {
                TextAlign::Left
            }
        }
        Lined::Centre => TextAlign::Center,
        Lined::Right => TextAlign::Right,
    };
    // Tries: the lines run on with the file's breaks found again by the
    // engine, the frame a little wider each time; then each line ended by
    // a line break.
    let tries: [(f64, bool); 4] = [
        (0.3, false),
        (width * 0.015, false),
        (width * 0.04, false),
        (width * 0.1, true),
    ];
    for (more, broken) in tries {
        let frame_width = width + more;
        let indent = if broken { 0.0 } else { block.indent };
        let (spans, starts) = block_text(block, width, broken, continues);
        let left = match block.lined {
            Lined::Left if broken => broken_left(block),
            Lined::Left => block.left(),
            Lined::Centre => first.centre() - frame_width / 2.0,
            Lined::Right => first.right - frame_width,
        };
        // A stop where the text after each tab starts, from the frame's
        // left edge.
        let mut stops: Vec<f64> = block
            .lines
            .iter()
            .flat_map(|l| l.tabs.iter().map(|t| t - left))
            .chain(
                block
                    .lines
                    .iter()
                    .filter(|_| broken && block.lined == Lined::Left)
                    .map(|l| l.u - left)
                    .filter(|p| *p > LINE_UP_MM),
            )
            .filter(|p| *p > 0.0)
            .collect();
        stops.sort_by(f64::total_cmp);
        stops.dedup_by(|a, b| (*a - *b).abs() < LINE_UP_MM);
        let para = ParagraphStyle {
            leading_pct: leading / PT_MM,
            leading_unit: LeadingUnit::Points,
            first_line_indent: indent,
            tabs: stops.into_iter().map(TabStop::left).collect(),
            ..ParagraphStyle::default()
        };
        let req = text_outline::TextRequest {
            spans: &spans,
            frame: Some(Size::new(
                frame_width,
                leading * (block.lines.len() as f64 + 4.0),
            )),
            align,
            para: &para,
            on_path: None,
        };
        // Where the engine puts the first baseline below the frame's top,
        // and how deep the last line goes.
        let (first_base, last_depth) = match text_outline::lines(&req) {
            Some(laid) => {
                let fits = laid.len() == block.lines.len()
                    && laid.iter().zip(&starts).all(|(l, s)| l.start_char == *s)
                    && laid.iter().zip(&block.lines).all(|(l, line)| {
                        let want = line.w - first.w;
                        (l.baseline - laid[0].baseline - want).abs() < 0.2
                    });
                if !fits {
                    continue;
                }
                (laid[0].baseline, laid[laid.len() - 1].descent)
            }
            None => (-0.9 * em, 0.3 * em),
        };
        let top = first.w - first_base;
        let bottom = block.last().w - last_depth - (0.15 * em).min(leading * 0.3);
        let l = first.linear;
        let mut shape = like.clone();
        shape.kind = ShapeKind::Text {
            spans,
            origin: Point::ZERO,
            frame: Some(Size::new(frame_width, top - bottom)),
            align,
            para,
            on_path: None,
        };
        shape.transform =
            Affine::new([l[0], l[1], l[2], l[3], 0.0, 0.0]) * Affine::translate((left, bottom));
        shape.fill = first.fill.clone();
        shape.opacity = first.opacity;
        if let Some((chain, place)) = chained {
            shape.data.retain(|(k, _)| k != FLOW_KEY);
            shape
                .data
                .push((FLOW_KEY.to_string(), format!("{chain}:{place}")));
        }
        return Some(shape);
    }
    None
}

/// The left edge of a frame whose lines each end with a line break.
fn broken_left(block: &Block) -> f64 {
    block.lines.iter().map(|l| l.u).fold(f64::MAX, f64::min)
}

/// The block's text, its lines run on (or each ended by a line break,
/// `broken`), with the character each line starts at. `continues`: a
/// space (or nothing after a hyphen) at the end, for the text of the
/// next frame to follow.
fn block_text(
    block: &Block,
    width: f64,
    broken: bool,
    continues: bool,
) -> (Vec<TextSpan>, Vec<usize>) {
    let first = block.first();
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut starts = Vec::new();
    let mut chars = 0;
    let n = block.lines.len();
    let left = broken_left(block);
    for (i, line) in block.lines.iter().enumerate() {
        starts.push(chars);
        let mut own = line.spans.clone();
        if i == 0 {
            if let Some(s) = own.first_mut() {
                s.text = s.text.trim_start().to_string();
            }
        }
        // Each line its own paragraph: one that starts further in (the
        // first of a paragraph) starts with a tab to its place.
        if broken && block.lined == Lined::Left && line.u - left > LINE_UP_MM {
            if let Some(s) = own.first_mut() {
                s.text.insert(0, '\t');
            }
        }
        // The line's own colour where it is not the frame's.
        if line.fill != first.fill {
            for s in &mut own {
                if s.fill.is_none() {
                    s.fill = Some(line.fill.clone());
                }
            }
        }
        let text = line.text();
        let trimmed = text.trim_end();
        // The spaces at the end of the line go: the break stands for them.
        let mut cut = text.len() - trimmed.len();
        while cut > 0 {
            let Some(last) = own.last_mut() else { break };
            let keep = last.text.len().saturating_sub(cut);
            cut -= last.text.len() - keep;
            last.text.truncate(keep);
            if last.text.is_empty() {
                own.pop();
            }
        }
        let hyphen = trimmed.ends_with('-') || trimmed.ends_with('\u{ad}');
        let sep = if i + 1 == n {
            if continues && !hyphen {
                " "
            } else {
                ""
            }
        } else if broken || block.ends_paragraph(i, width) {
            "\n"
        } else if hyphen {
            ""
        } else {
            " "
        };
        if let Some(last) = own.last_mut() {
            last.text.push_str(sep);
        }
        chars += own.iter().map(|s| s.text.chars().count()).sum::<usize>();
        spans.extend(own);
    }
    // Spans next to one another that differ only in text become one.
    let mut joined: Vec<TextSpan> = Vec::with_capacity(spans.len());
    for s in spans {
        if let Some(prev) = joined.last_mut() {
            let mut a = prev.clone();
            a.text.clear();
            let mut b = s.clone();
            b.text.clear();
            if a == b {
                prev.text.push_str(&s.text);
                continue;
            }
        }
        joined.push(s);
    }
    (joined, starts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Affine;

    fn line(id: u64, text: &str, x: f64, y: f64, width: f64) -> (Shape, f64) {
        let mut s = Shape::new(
            ShapeId(id),
            ShapeKind::Text {
                spans: vec![TextSpan::new(text, "Arial", 10.0)],
                origin: Point::ZERO,
                frame: None,
                align: TextAlign::Left,
                para: ParagraphStyle::default(),
                on_path: None,
            },
        );
        s.transform = Affine::translate((x, y));
        (s, width)
    }

    fn page(lines: &[(&str, f64, f64, f64)]) -> (ImportedPage, HashMap<ShapeId, f64>) {
        let mut widths = HashMap::new();
        let mut shapes = Vec::new();
        for (i, (t, x, y, w)) in lines.iter().enumerate() {
            let (s, w) = line(i as u64 + 1, t, *x, *y, *w);
            widths.insert(s.id, w);
            shapes.push(s);
        }
        (
            ImportedPage {
                size: Size::new(210.0, 297.0),
                shapes,
            },
            widths,
        )
    }

    fn texts(p: &ImportedPage) -> Vec<String> {
        p.shapes
            .iter()
            .filter_map(|s| match &s.kind {
                ShapeKind::Text { spans, .. } => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect())
                }
                _ => None,
            })
            .collect()
    }

    /// Three lines one under the other, lined up on the left, become one
    /// frame: the full lines run on, the short last one ends it.
    #[test]
    fn lines_one_under_another_become_one_frame() {
        let (p, widths) = page(&[
            ("The quick brown fox jumps", 20.0, 250.0, 80.0),
            ("over the lazy dog and runs", 20.0, 245.0, 79.5),
            ("away.", 20.0, 240.0, 12.0),
        ]);
        let mut pages = vec![p];
        make_paragraphs(&mut pages, &widths, &HashMap::new());
        assert_eq!(pages[0].shapes.len(), 1);
        assert_eq!(
            texts(&pages[0]),
            ["The quick brown fox jumps over the lazy dog and runs away."]
        );
        let s = &pages[0].shapes[0];
        let ShapeKind::Text {
            frame: Some(f),
            para,
            ..
        } = &s.kind
        else {
            panic!("not a frame: {:?}", s.kind);
        };
        assert!((f.width - 80.3).abs() < 1e-6, "{f:?}");
        assert!((para.leading_pct * PT_MM - 5.0).abs() < 1e-9);
        assert_eq!(para.leading_unit, LeadingUnit::Points);
        // The frame starts at the lines' left edge.
        let origin = s.transform * Point::ZERO;
        assert!((origin.x - 20.0).abs() < 1e-6, "{origin:?}");
        assert!(origin.y < 240.0, "{origin:?}");
    }

    /// A short line in the middle ends its paragraph: a line break, not a
    /// space.
    #[test]
    fn a_short_line_ends_its_paragraph() {
        let (p, widths) = page(&[
            ("First paragraph goes on and", 20.0, 250.0, 80.0),
            ("ends here.", 20.0, 245.0, 20.0),
            ("Second paragraph starts", 20.0, 240.0, 70.0),
        ]);
        let mut pages = vec![p];
        make_paragraphs(&mut pages, &widths, &HashMap::new());
        assert_eq!(
            texts(&pages[0]),
            ["First paragraph goes on and ends here.\nSecond paragraph starts"]
        );
    }

    /// Lines in other fonts, far apart or out of line stay as they are.
    #[test]
    fn unrelated_lines_stay_artistic() {
        let (mut p, widths) = page(&[
            ("Title", 20.0, 280.0, 30.0),
            ("Body far below", 20.0, 200.0, 50.0),
            ("Off to the right", 90.0, 195.0, 50.0),
        ]);
        if let ShapeKind::Text { spans, .. } = &mut p.shapes[0].kind {
            spans[0].size_pt = 24.0;
        }
        let mut pages = vec![p];
        make_paragraphs(&mut pages, &widths, &HashMap::new());
        assert_eq!(pages[0].shapes.len(), 3);
        assert!(pages[0]
            .shapes
            .iter()
            .all(|s| matches!(s.kind, ShapeKind::Text { frame: None, .. })));
    }

    /// Body text whose last line on a page is full goes on at the top of
    /// the next page: both frames are marked as one chain, the first one's
    /// text ending in the space between them.
    #[test]
    fn text_running_over_the_page_end_is_chained() {
        let (p1, w1) = page(&[
            ("Lorem ipsum dolor sit amet", 20.0, 30.0, 80.0),
            ("consectetur adipiscing elit", 20.0, 25.0, 79.8),
        ]);
        let (mut p2, w2) = page(&[
            ("sed do eiusmod tempor", 20.0, 270.0, 79.9),
            ("incididunt.", 20.0, 265.0, 20.0),
        ]);
        for (i, s) in p2.shapes.iter_mut().enumerate() {
            s.id = ShapeId(100 + i as u64);
        }
        let mut widths = w1;
        for (k, v) in w2 {
            widths.insert(ShapeId(k.0 + 99), v);
        }
        let mut pages = vec![p1, p2];
        make_paragraphs(&mut pages, &widths, &HashMap::new());
        let flow = |s: &Shape| {
            s.data
                .iter()
                .find(|(k, _)| k == FLOW_KEY)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(flow(&pages[0].shapes[0]).as_deref(), Some("0:0"));
        assert_eq!(flow(&pages[1].shapes[0]).as_deref(), Some("0:1"));
        assert_eq!(
            texts(&pages[0]),
            ["Lorem ipsum dolor sit amet consectetur adipiscing elit "]
        );
        assert_eq!(texts(&pages[1]), ["sed do eiusmod tempor incididunt."]);
    }

    /// Pieces of one line in different fonts (a bold word) join the line.
    #[test]
    fn pieces_of_a_line_join_it() {
        let (mut p, widths) = page(&[
            ("A line with a", 20.0, 250.0, 30.0),
            ("bold", 51.5, 250.0, 10.0),
            ("word in it and more", 62.5, 250.0, 37.5),
            ("text on the next line.", 20.0, 245.0, 40.0),
        ]);
        if let ShapeKind::Text { spans, .. } = &mut p.shapes[1].kind {
            spans[0].bold = true;
        }
        let mut pages = vec![p];
        make_paragraphs(&mut pages, &widths, &HashMap::new());
        assert_eq!(pages[0].shapes.len(), 1);
        let ShapeKind::Text { spans, .. } = &pages[0].shapes[0].kind else {
            panic!()
        };
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            text,
            "A line with a bold word in it and more text on the next line."
        );
        assert!(spans.iter().any(|s| s.bold && s.text.trim() == "bold"));
    }

    /// A list item's line, a bullet in a symbol font before the text,
    /// starts an item of its own: it is not the next line of the item
    /// above (#3551).
    #[test]
    fn a_bulleted_line_is_not_run_on() {
        let (mut p, widths) = page(&[
            ("First item text that goes on", 20.0, 250.0, 80.0),
            ("l", 12.0, 245.0, 3.0),
            ("Second item", 20.0, 245.0, 30.0),
        ]);
        if let ShapeKind::Text { spans, .. } = &mut p.shapes[1].kind {
            spans[0].font_family = "Wingdings".into();
        }
        let mut pages = vec![p];
        make_paragraphs(&mut pages, &widths, &HashMap::new());
        let texts = texts(&pages[0]);
        let first = "First item text that goes on".to_string();
        assert!(texts.contains(&first), "{texts:?}");
        assert!(!texts.iter().any(|t| t.contains("on l")), "{texts:?}");
    }

    /// A line the file draws under two words becomes their underline,
    /// and goes (#2569).
    #[test]
    fn a_line_under_words_becomes_their_underline() {
        let (mut p, mut widths) = page(&[
            ("Some words underlined here and", 20.0, 250.0, 80.0),
            ("the rest of the text.", 20.0, 245.0, 40.0),
        ]);
        // "words underlined" is characters 5..21, each 80/30 mm wide.
        let cw = 80.0 / 30.0;
        let mut char_xs = HashMap::new();
        char_xs.insert(
            ShapeId(1),
            (0..30).map(|k| k as f64 * cw).collect::<Vec<_>>(),
        );
        let mut bar = Shape::new(
            ShapeId(9),
            ShapeKind::Path {
                path: {
                    let mut b = tracedraw_core::geometry::BezPath::new();
                    b.move_to((20.0 + 5.0 * cw, 249.5));
                    b.line_to((20.0 + 21.0 * cw - 0.3, 249.5));
                    b
                },
                closed: false,
            },
        );
        bar.fill = Fill::None;
        bar.stroke = Some(tracedraw_core::style::Stroke {
            width: 0.2,
            ..Default::default()
        });
        p.shapes.insert(1, bar);
        widths.insert(ShapeId(9), 0.0);
        let mut pages = vec![p];
        make_paragraphs(&mut pages, &widths, &char_xs);
        assert_eq!(pages[0].shapes.len(), 1, "{:?}", texts(&pages[0]));
        let ShapeKind::Text { spans, .. } = &pages[0].shapes[0].kind else {
            panic!()
        };
        let under: String = spans
            .iter()
            .filter(|s| s.underline != LineStyle::None)
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(under, "words underlined");
    }

    /// A line of the drawing that runs under a label and far past it (a
    /// wire leaving a pin name in a circuit diagram) is not the label's
    /// underline: it stays a line, and the label is not underlined
    /// (#4100).
    #[test]
    fn a_long_line_passing_under_a_label_stays_a_line() {
        let (mut p, mut widths) = page(&[("DOUT", 20.0, 250.0, 12.0)]);
        let mut char_xs = HashMap::new();
        char_xs.insert(ShapeId(1), vec![0.0, 3.0, 6.0, 9.0]);
        let mut wire = Shape::new(
            ShapeId(9),
            ShapeKind::Path {
                path: {
                    let mut b = tracedraw_core::geometry::BezPath::new();
                    b.move_to((18.0, 249.5));
                    b.line_to((80.0, 249.5));
                    b
                },
                closed: false,
            },
        );
        wire.fill = Fill::None;
        wire.stroke = Some(tracedraw_core::style::Stroke {
            width: 0.2,
            ..Default::default()
        });
        p.shapes.push(wire);
        widths.insert(ShapeId(9), 0.0);
        let mut pages = vec![p];
        make_paragraphs(&mut pages, &widths, &char_xs);
        assert_eq!(pages[0].shapes.len(), 2, "{:?}", texts(&pages[0]));
        let ShapeKind::Text { spans, .. } = &pages[0].shapes[0].kind else {
            panic!()
        };
        assert!(spans.iter().all(|s| s.underline == LineStyle::None));
    }

    /// A wide gap in a line is a tab, with a stop where the text after it
    /// starts, and rows with tabs keep their own lines (#2569).
    #[test]
    fn a_wide_gap_is_a_tab_with_its_stop() {
        let (p, widths) = page(&[
            ("Name:", 20.0, 250.0, 12.0),
            ("John Smith", 50.0, 250.0, 22.0),
            ("Address:", 20.0, 245.0, 17.0),
            ("12 Long Street", 50.0, 245.0, 30.0),
        ]);
        let mut pages = vec![p];
        make_paragraphs(&mut pages, &widths, &HashMap::new());
        assert_eq!(
            texts(&pages[0]),
            ["Name:\tJohn Smith\nAddress:\t12 Long Street"]
        );
        let ShapeKind::Text { para, .. } = &pages[0].shapes[0].kind else {
            panic!()
        };
        assert_eq!(para.tabs.len(), 1, "{:?}", para.tabs);
        assert!((para.tabs[0].pos - 30.0).abs() < 1e-6, "{:?}", para.tabs);
    }

    /// Spaces a text starts with move its start to its first letter, and
    /// a wide run of spaces inside it is a tab to where the next letter
    /// is (#2569).
    #[test]
    fn leading_spaces_and_wide_inner_gaps() {
        let mut spans = vec![TextSpan::new("  Ab   Cd", "Arial", 10.0)];
        // 1 mm a character, but the letters after the gap at 20 mm.
        let xs = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 20.0, 21.0];
        let em = 10.0 * PT_MM;
        let (lead, tabs) = gaps_into_tabs(&mut spans, &xs, 22.0, em);
        assert!((lead - 2.0).abs() < 1e-9);
        assert_eq!(tabs, vec![20.0]);
        assert_eq!(spans[0].text, "Ab\tCd");
        // Ordinary word spaces stay.
        let mut spans = vec![TextSpan::new("Ab Cd", "Arial", 10.0)];
        let (lead, tabs) = gaps_into_tabs(&mut spans, &[0.0, 1.0, 2.0, 3.0, 4.0], 5.0, em);
        assert_eq!((lead, tabs.len()), (0.0, 0));
        assert_eq!(spans[0].text, "Ab Cd");
    }
}
