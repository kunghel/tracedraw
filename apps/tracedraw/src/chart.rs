//! Charts from data: a table of numbers typed or pasted (rows of cells
//! separated by tabs, semicolons or commas) drawn as one group of plain
//! objects (columns, bars, lines, areas, points, pie slices or a radar
//! web, with axes, labels and a legend), which keeps its table and
//! settings so that the chart can be edited again.
//!
//! The layout works in millimetres with Y up, the chart's box from
//! (0, 0) to (width, height); the caller moves it into place.

use kurbo::{BezPath, Point, Rect, Shape as _};
use tracedraw_core::{Color, Fill, Shape, ShapeKind, Stroke, TextAlign, TextSpan};

/// The object data key that holds a chart's table and settings.
pub const DATA_KEY: &str = "chart";

/// The kinds of chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    /// Upright columns side by side, one per series in each category.
    #[default]
    Columns,
    /// Upright columns, the series of a category stacked.
    StackedColumns,
    /// Level bars side by side, the categories down the left.
    Bars,
    /// Level bars, the series of a category one after another.
    StackedBars,
    /// A line through each series' values.
    Lines,
    /// Each series a filled band stacked on the ones before it.
    Areas,
    /// Points: the first column is X, each other column a series of Y.
    Scatter,
    /// Slices of a circle, one per category, from the first series.
    Pie,
    /// A web: one spoke per category, each series a closed outline.
    Radar,
}

impl ChartKind {
    pub const ALL: [ChartKind; 9] = [
        ChartKind::Columns,
        ChartKind::StackedColumns,
        ChartKind::Bars,
        ChartKind::StackedBars,
        ChartKind::Lines,
        ChartKind::Areas,
        ChartKind::Scatter,
        ChartKind::Pie,
        ChartKind::Radar,
    ];

    /// The name kept in the object's data.
    pub fn key(self) -> &'static str {
        match self {
            ChartKind::Columns => "columns",
            ChartKind::StackedColumns => "stacked_columns",
            ChartKind::Bars => "bars",
            ChartKind::StackedBars => "stacked_bars",
            ChartKind::Lines => "lines",
            ChartKind::Areas => "areas",
            ChartKind::Scatter => "scatter",
            ChartKind::Pie => "pie",
            ChartKind::Radar => "radar",
        }
    }

    pub fn from_key(k: &str) -> Option<ChartKind> {
        ChartKind::ALL.into_iter().find(|c| c.key() == k)
    }
}

/// A chart's settings: its kind, size, legend, value labels, text size
/// and the table typed.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub width_mm: f64,
    pub height_mm: f64,
    pub legend: bool,
    pub values: bool,
    pub size_pt: f64,
    pub font: String,
    pub data: String,
}

impl Default for ChartSpec {
    fn default() -> Self {
        Self {
            kind: ChartKind::Columns,
            width_mm: 120.0,
            height_mm: 80.0,
            legend: true,
            values: false,
            size_pt: 8.0,
            font: "Arial".into(),
            data: "\t2024\t2025\nNorth\t12\t15\nSouth\t9\t11\nEast\t14\t13\nWest\t7\t10".into(),
        }
    }
}

impl ChartSpec {
    /// The settings as kept in the object's data: one setting a line,
    /// then the table (which may hold many lines).
    pub fn to_data(&self) -> String {
        format!(
            "1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.kind.key(),
            self.width_mm,
            self.height_mm,
            self.legend as u8,
            self.values as u8,
            self.size_pt,
            self.font,
            self.data
        )
    }

    pub fn from_data(s: &str) -> Option<ChartSpec> {
        let mut it = s.splitn(9, '\n');
        if it.next()?.trim() != "1" {
            return None;
        }
        let kind = ChartKind::from_key(it.next()?.trim())?;
        let num = |v: Option<&str>| v.and_then(|v| v.trim().parse::<f64>().ok());
        let width_mm = num(it.next())?;
        let height_mm = num(it.next())?;
        let legend = it.next()?.trim() == "1";
        let values = it.next()?.trim() == "1";
        let size_pt = num(it.next())?;
        let font = it.next()?.to_string();
        let data = it.next().unwrap_or("").to_string();
        Some(ChartSpec {
            kind,
            width_mm,
            height_mm,
            legend,
            values,
            size_pt,
            font,
            data,
        })
    }

    /// The chart settings an object keeps, if it is a chart.
    pub fn of_shape(s: &Shape) -> Option<ChartSpec> {
        s.data
            .iter()
            .find(|(k, _)| k == DATA_KEY)
            .and_then(|(_, v)| ChartSpec::from_data(v))
    }
}

// ----- the table -------------------------------------------------------------

/// A table read from text: the categories (rows), the series (columns)
/// and each series' value in each category.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub categories: Vec<String>,
    pub series: Vec<String>,
    /// `values[series][category]`; an empty or unreadable cell is `None`.
    pub values: Vec<Vec<Option<f64>>>,
}

/// A number as typed: `1234.5`, `-3`, `12%`, and with a decimal comma
/// when commas do not part the cells (`1,5`).
fn number(cell: &str, comma_decimal: bool) -> Option<f64> {
    let t = cell.trim().trim_end_matches('%').trim();
    if t.is_empty() {
        return None;
    }
    let t: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    let t = if comma_decimal && !t.contains('.') {
        t.replace(',', ".")
    } else {
        t
    };
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

impl Table {
    /// Read rows of cells parted by tabs (as spreadsheets copy them),
    /// else semicolons, else commas. The first row names the series when
    /// a cell after its first is not a number; the first column names
    /// the categories when a cell of it under the names is not a number
    /// (and always for a chart other than Scatter when the first row
    /// names the series and starts empty). An empty first cell makes the
    /// first row the names, even of numbers such as years.
    pub fn parse(text: &str) -> Table {
        let sep = if text.contains('\t') {
            '\t'
        } else if text.contains(';') {
            ';'
        } else {
            ','
        };
        let comma_decimal = sep != ',';
        let rows: Vec<Vec<String>> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split(sep).map(|c| c.trim().to_string()).collect())
            .collect();
        if rows.is_empty() {
            return Table::default();
        }
        let is_num = |c: &str| number(c, comma_decimal).is_some();
        // An empty first cell over more rows is a corner: the row names
        // the series even when the names are numbers (years).
        let corner = rows.len() > 1 && rows[0].len() > 1 && rows[0][0].is_empty();
        let header = corner
            || rows[0].iter().skip(1).any(|c| !c.is_empty() && !is_num(c))
            || (rows[0].len() == 1 && !is_num(&rows[0][0]) && rows.len() > 1);
        let body = if header { &rows[1..] } else { &rows[..] };
        let first_empty = header && rows[0].first().is_some_and(|c| c.is_empty());
        let labels = first_empty || body.iter().any(|r| r.first().is_some_and(|c| !is_num(c)));
        let skip = labels as usize;
        let cols = rows
            .iter()
            .map(|r| r.len())
            .max()
            .unwrap_or(0)
            .saturating_sub(skip);
        let series = (0..cols)
            .map(|i| {
                header
                    .then(|| rows[0].get(i + skip).cloned())
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("{}", i + 1))
            })
            .collect();
        let categories = body
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if labels {
                    r.first().cloned().unwrap_or_default()
                } else {
                    format!("{}", i + 1)
                }
            })
            .collect();
        let values = (0..cols)
            .map(|c| {
                body.iter()
                    .map(|r| r.get(c + skip).and_then(|v| number(v, comma_decimal)))
                    .collect()
            })
            .collect();
        Table {
            categories,
            series,
            values,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.values.iter().all(|s| s.iter().all(|v| v.is_none()))
    }
}

// ----- scales ----------------------------------------------------------------

/// Round steps (1, 2, 2.5 or 5 times a power of ten) from at or under
/// `lo` to at or over `hi`, about five of them.
pub fn ticks(lo: f64, hi: f64) -> Vec<f64> {
    let (lo, hi) = if hi - lo < 1e-12 {
        if lo.abs() < 1e-12 {
            (0.0, 1.0)
        } else {
            (lo.min(0.0), hi.max(0.0))
        }
    } else {
        (lo, hi)
    };
    let raw = (hi - lo) / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .into_iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw * 0.999)
        .unwrap_or(10.0 * mag);
    let first = (lo / step + 1e-9).floor();
    let last = (hi / step - 1e-9).ceil();
    let mut out = Vec::new();
    let mut k = first;
    while k <= last + 1e-9 && out.len() < 50 {
        let v = k * step;
        out.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
        k += 1.0;
    }
    out
}

/// A number for a label: no trailing zeros, at most a few decimals.
pub fn label(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// The colours of the series, in turn.
pub fn series_color(i: usize) -> Color {
    const P: [(u8, u8, u8); 8] = [
        (0x2E, 0x6F, 0xBA),
        (0xE3, 0x7A, 0x1E),
        (0x3A, 0x9A, 0x4A),
        (0xC8, 0x3A, 0x3A),
        (0x7E, 0x5B, 0xB0),
        (0x8C, 0x5E, 0x3C),
        (0xD4, 0x6E, 0xB4),
        (0x7A, 0x7A, 0x7A),
    ];
    let (r, g, b) = P[i % P.len()];
    Color::rgb8(r, g, b)
}

// ----- drawing ---------------------------------------------------------------

/// What makes the shapes: ids come from the caller.
struct Out<'a> {
    next_id: &'a mut dyn FnMut() -> tracedraw_core::ShapeId,
    shapes: Vec<Shape>,
    font: String,
    size_pt: f64,
}

const INK: (u8, u8, u8) = (0x33, 0x33, 0x33);
const GRID: (u8, u8, u8) = (0xCC, 0xCC, 0xCC);

impl Out<'_> {
    fn em(&self) -> f64 {
        self.size_pt * 25.4 / 72.0
    }

    fn path(&mut self, path: BezPath, closed: bool, fill: Fill, stroke: Option<Stroke>) {
        let mut s = Shape::new((self.next_id)(), ShapeKind::Path { path, closed });
        s.fill = fill;
        s.stroke = stroke;
        self.shapes.push(s);
    }

    fn rect(&mut self, r: Rect, color: Color) {
        let r = Rect::new(
            r.x0.min(r.x1),
            r.y0.min(r.y1),
            r.x0.max(r.x1),
            r.y0.max(r.y1),
        );
        self.path(r.to_path(0.1), true, Fill::Solid(color), None);
    }

    fn line(&mut self, pts: &[Point], (r, g, b): (u8, u8, u8), width: f64) {
        if pts.len() < 2 {
            return;
        }
        let mut p = BezPath::new();
        p.move_to(pts[0]);
        for q in &pts[1..] {
            p.line_to(*q);
        }
        self.path(
            p,
            false,
            Fill::None,
            Some(Stroke::new(Color::rgb8(r, g, b), width)),
        );
    }

    /// A text with its baseline at `at`; `mid` puts the middle of the
    /// letters' height there instead.
    fn text(&mut self, t: &str, at: Point, align: TextAlign, mid: bool) {
        if t.is_empty() {
            return;
        }
        let y = if mid { at.y - 0.35 * self.em() } else { at.y };
        let mut s = Shape::new(
            (self.next_id)(),
            ShapeKind::Text {
                spans: vec![TextSpan::new(t, self.font.clone(), self.size_pt)],
                origin: Point::new(at.x, y),
                frame: None,
                align,
                para: Default::default(),
                on_path: None,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(INK.0, INK.1, INK.2));
        s.stroke = None;
        self.shapes.push(s);
    }

    /// About how wide a text is: 0.55 em a character.
    fn width_of(&self, t: &str) -> f64 {
        t.chars().count() as f64 * 0.55 * self.em()
    }
}

/// The chart's objects, laid in the box (0, 0)–(width, height), Y up.
/// None when the table holds no number.
pub fn build(
    spec: &ChartSpec,
    next_id: &mut dyn FnMut() -> tracedraw_core::ShapeId,
) -> Option<Vec<Shape>> {
    let table = Table::parse(&spec.data);
    if table.is_empty() {
        return None;
    }
    let mut o = Out {
        next_id,
        shapes: Vec::new(),
        font: spec.font.clone(),
        size_pt: spec.size_pt.clamp(2.0, 72.0),
    };
    let w = spec.width_mm.max(10.0);
    let h = spec.height_mm.max(10.0);
    let em = o.em();
    // The legend down the right: the categories for a pie, else the
    // series (Scatter's first column is X, not a series).
    let names: Vec<String> = match spec.kind {
        ChartKind::Pie => table.categories.clone(),
        ChartKind::Scatter => table.series.iter().skip(1).cloned().collect(),
        _ => table.series.clone(),
    };
    let mut right = w;
    if spec.legend && !names.is_empty() {
        let lw = names.iter().map(|n| o.width_of(n)).fold(0.0, f64::max) + 2.0 * em;
        let lx = w - lw;
        let line = 1.5 * em;
        let top = h - em * 0.5;
        for (i, n) in names.iter().enumerate() {
            let y = top - i as f64 * line;
            let c = series_color(i);
            o.rect(Rect::new(lx, y - 0.7 * em, lx + 0.7 * em, y), c);
            o.text(
                n,
                Point::new(lx + 1.1 * em, y - 0.35 * em),
                TextAlign::Left,
                true,
            );
        }
        right = lx - em;
    }
    let area = Rect::new(0.0, 0.0, right.max(w * 0.3), h);
    match spec.kind {
        ChartKind::Pie => pie(&mut o, &table, area, spec.values),
        ChartKind::Radar => radar(&mut o, &table, area),
        ChartKind::Scatter => scatter(&mut o, &table, area),
        k => category_chart(&mut o, &table, area, k, spec.values),
    }
    Some(o.shapes)
}

/// The value range of a chart: from zero or under to zero or over, the
/// stacked sums for a stacked chart.
fn range(table: &Table, stacked: bool) -> (f64, f64) {
    let (mut lo, mut hi) = (0.0f64, 0.0f64);
    if stacked {
        for c in 0..table.categories.len() {
            let (mut neg, mut pos) = (0.0, 0.0);
            for s in &table.values {
                let v = s.get(c).copied().flatten().unwrap_or(0.0);
                if v < 0.0 {
                    neg += v
                } else {
                    pos += v
                }
            }
            lo = lo.min(neg);
            hi = hi.max(pos);
        }
    } else {
        for v in table.values.iter().flatten().flatten() {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    (lo, hi)
}

/// Columns, bars, lines and areas: categories along one side, values
/// along the other.
fn category_chart(o: &mut Out, table: &Table, area: Rect, kind: ChartKind, values: bool) {
    let em = o.em();
    let level = matches!(kind, ChartKind::Bars | ChartKind::StackedBars);
    let stacked = matches!(
        kind,
        ChartKind::StackedColumns | ChartKind::StackedBars | ChartKind::Areas
    );
    let (lo, hi) = range(table, stacked);
    let t = ticks(lo, hi);
    let (vmin, vmax) = (t[0], *t.last().unwrap());
    let n = table.categories.len().max(1);
    // Room for the labels: the value labels on the left (columns) or
    // under (bars), the category names under (columns) or on the left.
    let tick_w = t.iter().map(|v| o.width_of(&label(*v))).fold(0.0, f64::max);
    let cat_w = table
        .categories
        .iter()
        .map(|c| o.width_of(c))
        .fold(0.0, f64::max);
    let left = if level { cat_w } else { tick_w } + 0.8 * em;
    let plot = Rect::new(
        area.x0 + left,
        area.y0 + 1.8 * em,
        area.x1 - 0.5 * em,
        area.y1 - 0.8 * em,
    );
    if plot.width() <= 0.0 || plot.height() <= 0.0 {
        return;
    }
    let span = (vmax - vmin).max(1e-12);
    // A value's place along the value axis, and a category's band.
    let val = |v: f64| {
        let f = (v - vmin) / span;
        if level {
            plot.x0 + f * plot.width()
        } else {
            plot.y0 + f * plot.height()
        }
    };
    let band = if level { plot.height() } else { plot.width() } / n as f64;
    // The bands run left to right, or top to bottom for bars.
    let band_at = |i: usize| {
        if level {
            plot.y1 - (i as f64 + 0.5) * band
        } else {
            plot.x0 + (i as f64 + 0.5) * band
        }
    };
    // Grid lines and value labels.
    for v in &t {
        let p = val(*v);
        let (a, b) = if level {
            (Point::new(p, plot.y0), Point::new(p, plot.y1))
        } else {
            (Point::new(plot.x0, p), Point::new(plot.x1, p))
        };
        let (col, width) = if v.abs() < 1e-12 {
            (INK, 0.25)
        } else {
            (GRID, 0.15)
        };
        o.line(&[a, b], col, width);
        if level {
            o.text(
                &label(*v),
                Point::new(p, plot.y0 - 1.2 * em),
                TextAlign::Center,
                false,
            );
        } else {
            o.text(
                &label(*v),
                Point::new(plot.x0 - 0.4 * em, p),
                TextAlign::Right,
                true,
            );
        }
    }
    // The category axis and names.
    if level {
        o.line(
            &[Point::new(plot.x0, plot.y0), Point::new(plot.x0, plot.y1)],
            INK,
            0.25,
        );
    } else {
        o.line(
            &[Point::new(plot.x0, plot.y0), Point::new(plot.x1, plot.y0)],
            INK,
            0.25,
        );
    }
    for (i, c) in table.categories.iter().enumerate() {
        let m = band_at(i);
        if level {
            o.text(c, Point::new(plot.x0 - 0.4 * em, m), TextAlign::Right, true);
        } else {
            o.text(
                c,
                Point::new(m, plot.y0 - 1.2 * em),
                TextAlign::Center,
                false,
            );
        }
    }
    let ns = table.values.len().max(1);
    let value = |s: usize, c: usize| table.values[s].get(c).copied().flatten();
    match kind {
        ChartKind::Columns
        | ChartKind::Bars
        | ChartKind::StackedColumns
        | ChartKind::StackedBars => {
            let inner = band * 0.75;
            let mut labels = Vec::new();
            let (mut pos, mut neg) = (vec![0.0; n], vec![0.0; n]);
            for s in 0..table.values.len() {
                for c in 0..n {
                    let Some(v) = value(s, c) else { continue };
                    let (from, to, a, b) = if stacked {
                        let base = if v < 0.0 { &mut neg[c] } else { &mut pos[c] };
                        let from = *base;
                        *base += v;
                        let m = band_at(c);
                        (from, *base, m - inner / 2.0, m + inner / 2.0)
                    } else {
                        let one = inner / ns as f64;
                        let a = band_at(c) - inner / 2.0 + s as f64 * one;
                        // Bars run top to bottom, so the series go down.
                        let a = if level {
                            band_at(c) + inner / 2.0 - (s as f64 + 1.0) * one
                        } else {
                            a
                        };
                        (0.0, v, a, a + one)
                    };
                    let r = if level {
                        Rect::new(val(from), a, val(to), b)
                    } else {
                        Rect::new(a, val(from), b, val(to))
                    };
                    o.rect(r, series_color(s));
                    if values {
                        labels.push((v, r));
                    }
                }
            }
            for (v, r) in labels {
                // A stacked piece has its value in its middle.
                if stacked {
                    o.text(&label(v), r.center(), TextAlign::Center, true);
                    continue;
                }
                let at = if level {
                    Point::new(r.x0.max(r.x1) + 0.3 * em, r.center().y)
                } else {
                    Point::new(r.center().x, r.y0.max(r.y1) + 0.3 * em)
                };
                let align = if level {
                    TextAlign::Left
                } else {
                    TextAlign::Center
                };
                o.text(&label(v), at, align, level);
            }
        }
        ChartKind::Lines => {
            for s in 0..table.values.len() {
                let pts: Vec<Point> = (0..n)
                    .filter_map(|c| value(s, c).map(|v| Point::new(band_at(c), val(v))))
                    .collect();
                o.line(&pts, rgb(series_color(s)), 0.5);
                for p in &pts {
                    let dot = kurbo::Circle::new(*p, 0.25 * em).to_path(0.05);
                    o.path(dot, true, Fill::Solid(series_color(s)), None);
                }
                if values {
                    for c in 0..n {
                        if let Some(v) = value(s, c) {
                            o.text(
                                &label(v),
                                Point::new(band_at(c), val(v) + 0.5 * em),
                                TextAlign::Center,
                                false,
                            );
                        }
                    }
                }
            }
        }
        ChartKind::Areas => {
            // Each band from the sum of the series before it to that sum
            // plus its own value; negative values count as zero.
            let mut base = vec![0.0; n];
            for s in 0..table.values.len() {
                let top: Vec<f64> = (0..n)
                    .map(|c| base[c] + value(s, c).unwrap_or(0.0).max(0.0))
                    .collect();
                let mut p = BezPath::new();
                p.move_to(Point::new(band_at(0), val(top[0])));
                for (c, t) in top.iter().enumerate().skip(1) {
                    p.line_to(Point::new(band_at(c), val(*t)));
                }
                for c in (0..n).rev() {
                    p.line_to(Point::new(band_at(c), val(base[c])));
                }
                p.close_path();
                o.path(p, true, Fill::Solid(series_color(s)), None);
                base = top;
            }
        }
        _ => {}
    }
}

fn rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb { r, g, b } => (
            (r * 255.0).round() as u8,
            (g * 255.0).round() as u8,
            (b * 255.0).round() as u8,
        ),
        _ => INK,
    }
}

/// Points: X from the first column, each other column a series of Y.
fn scatter(o: &mut Out, table: &Table, area: Rect) {
    let em = o.em();
    let Some(xs) = table.values.first() else {
        return;
    };
    let pairs: Vec<(usize, f64, f64)> = table
        .values
        .iter()
        .enumerate()
        .skip(1)
        .flat_map(|(s, ys)| {
            xs.iter()
                .zip(ys)
                .filter_map(move |(x, y)| Some((s - 1, (*x)?, (*y)?)))
        })
        .collect();
    if pairs.is_empty() {
        return;
    }
    let fold = |f: fn(&(usize, f64, f64)) -> f64| {
        pairs
            .iter()
            .map(f)
            .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)))
    };
    let (x0, x1) = fold(|p| p.1);
    let (y0, y1) = fold(|p| p.2);
    let tx = ticks(x0.min(0.0), x1.max(0.0));
    let ty = ticks(y0.min(0.0), y1.max(0.0));
    let tick_w = ty
        .iter()
        .map(|v| o.width_of(&label(*v)))
        .fold(0.0, f64::max);
    let plot = Rect::new(
        area.x0 + tick_w + 0.8 * em,
        area.y0 + 1.8 * em,
        area.x1 - 0.5 * em,
        area.y1 - 0.8 * em,
    );
    if plot.width() <= 0.0 || plot.height() <= 0.0 {
        return;
    }
    let (ax, bx) = (tx[0], *tx.last().unwrap());
    let (ay, by) = (ty[0], *ty.last().unwrap());
    let px = |x: f64| plot.x0 + (x - ax) / (bx - ax).max(1e-12) * plot.width();
    let py = |y: f64| plot.y0 + (y - ay) / (by - ay).max(1e-12) * plot.height();
    for v in &tx {
        let x = px(*v);
        o.line(
            &[Point::new(x, plot.y0), Point::new(x, plot.y1)],
            GRID,
            0.15,
        );
        o.text(
            &label(*v),
            Point::new(x, plot.y0 - 1.2 * em),
            TextAlign::Center,
            false,
        );
    }
    for v in &ty {
        let y = py(*v);
        o.line(
            &[Point::new(plot.x0, y), Point::new(plot.x1, y)],
            GRID,
            0.15,
        );
        o.text(
            &label(*v),
            Point::new(plot.x0 - 0.4 * em, y),
            TextAlign::Right,
            true,
        );
    }
    o.line(
        &[
            Point::new(plot.x0, plot.y1),
            Point::new(plot.x0, plot.y0),
            Point::new(plot.x1, plot.y0),
        ],
        INK,
        0.25,
    );
    for (s, x, y) in pairs {
        let dot = kurbo::Circle::new(Point::new(px(x), py(y)), 0.3 * em).to_path(0.05);
        o.path(dot, true, Fill::Solid(series_color(s)), None);
    }
}

/// Slices from the first series, one per category, from the top
/// clockwise; zero and negative values make no slice.
fn pie(o: &mut Out, table: &Table, area: Rect, values: bool) {
    let em = o.em();
    let Some(vals) = table.values.first() else {
        return;
    };
    let total: f64 = vals.iter().flatten().filter(|v| **v > 0.0).sum();
    if total <= 0.0 {
        return;
    }
    let r = (area.width().min(area.height()) / 2.0 - em).max(1.0);
    let c = Point::new(
        area.x0 + area.width().min(area.height()) / 2.0,
        area.center().y,
    );
    let mut a = std::f64::consts::FRAC_PI_2;
    for (i, v) in vals.iter().enumerate() {
        let Some(v) = v.filter(|v| *v > 0.0) else {
            continue;
        };
        let sweep = v / total * std::f64::consts::TAU;
        let mut p = BezPath::new();
        if sweep < std::f64::consts::TAU - 1e-9 {
            p.move_to(c);
            p.line_to(c + kurbo::Vec2::from_angle(a) * r);
        } else {
            p.move_to(c + kurbo::Vec2::from_angle(a) * r);
        }
        let arc = kurbo::Arc::new(c, (r, r), a, -sweep, 0.0);
        arc.to_cubic_beziers(0.05, |p1, p2, p3| p.curve_to(p1, p2, p3));
        p.close_path();
        let mut white = Stroke::new(Color::rgb8(0xFF, 0xFF, 0xFF), 0.3);
        white.scale_with_object = true;
        o.path(p, true, Fill::Solid(series_color(i)), Some(white));
        if values {
            let mid = a - sweep / 2.0;
            let at = c + kurbo::Vec2::from_angle(mid) * r * 0.65;
            let pct = label((v / total * 1000.0).round() / 10.0);
            o.text(&format!("{pct}%"), at, TextAlign::Center, true);
        }
        a -= sweep;
    }
}

/// A web: one spoke per category, rings at the value steps, each series
/// a closed outline.
fn radar(o: &mut Out, table: &Table, area: Rect) {
    let em = o.em();
    let n = table.categories.len();
    if n < 3 {
        return;
    }
    let (_, hi) = range(table, false);
    let t: Vec<f64> = ticks(0.0, hi.max(0.0))
        .into_iter()
        .filter(|v| *v >= 0.0)
        .collect();
    let top = t.last().copied().unwrap_or(1.0).max(1e-12);
    let side = area.width().min(area.height());
    let r = (side / 2.0 - 2.0 * em).max(1.0);
    let c = Point::new(area.x0 + side / 2.0, area.center().y);
    let dir = |i: usize| {
        kurbo::Vec2::from_angle(
            std::f64::consts::FRAC_PI_2 - i as f64 / n as f64 * std::f64::consts::TAU,
        )
    };
    for v in t.iter().skip(1) {
        let mut ring: Vec<Point> = (0..n).map(|i| c + dir(i) * (v / top * r)).collect();
        ring.push(ring[0]);
        o.line(&ring, GRID, 0.15);
        o.text(
            &label(*v),
            c + dir(0) * (v / top * r) + kurbo::Vec2::new(0.3 * em, 0.0),
            TextAlign::Left,
            true,
        );
    }
    for (i, name) in table.categories.iter().enumerate() {
        o.line(&[c, c + dir(i) * r], GRID, 0.15);
        let d = dir(i);
        let at = c + d * (r + 0.8 * em);
        let align = if d.x > 0.2 {
            TextAlign::Left
        } else if d.x < -0.2 {
            TextAlign::Right
        } else {
            TextAlign::Center
        };
        o.text(name, at, align, true);
    }
    for (s, vals) in table.values.iter().enumerate() {
        let mut pts: Vec<Point> = (0..n)
            .map(|i| {
                let v = vals.get(i).copied().flatten().unwrap_or(0.0).max(0.0);
                c + dir(i) * (v / top * r)
            })
            .collect();
        pts.push(pts[0]);
        o.line(&pts, rgb(series_color(s)), 0.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> impl FnMut() -> tracedraw_core::ShapeId {
        let mut src = tracedraw_core::id::IdSource::default();
        move || src.shape()
    }

    #[test]
    fn a_table_reads_names_and_numbers() {
        let t = Table::parse("\tA\tB\nx\t1\t2\ny\t3\t\nz\t5,5\t6");
        assert_eq!(t.categories, ["x", "y", "z"]);
        assert_eq!(t.series, ["A", "B"]);
        assert_eq!(t.values[0], [Some(1.0), Some(3.0), Some(5.5)]);
        assert_eq!(t.values[1], [Some(2.0), None, Some(6.0)]);
        // Commas part the cells; no names.
        let t = Table::parse("1, 2\n3, 4");
        assert_eq!(t.series, ["1", "2"]);
        assert_eq!(t.categories, ["1", "2"]);
        assert_eq!(t.values[1], [Some(2.0), Some(4.0)]);
        // Semicolons, with a decimal comma.
        let t = Table::parse("Mês;Vendas\nJan;1,5\nFev;2");
        assert_eq!(t.series, ["Vendas"]);
        assert_eq!(t.categories, ["Jan", "Fev"]);
        assert_eq!(t.values[0], [Some(1.5), Some(2.0)]);
        assert!(Table::parse("a\tb").is_empty());
        // Years over the columns: names, not numbers.
        let t = Table::parse("\t2024\t2025\nNorth\t12\t15");
        assert_eq!(t.series, ["2024", "2025"]);
        assert_eq!(t.categories, ["North"]);
        assert_eq!(t.values[1], [Some(15.0)]);
    }

    #[test]
    fn steps_are_round() {
        assert_eq!(ticks(0.0, 15.0), [0.0, 5.0, 10.0, 15.0]);
        assert_eq!(ticks(0.0, 12.0), [0.0, 2.5, 5.0, 7.5, 10.0, 12.5]);
        assert_eq!(ticks(-3.0, 7.0), [-4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 8.0]);
        let unit: Vec<String> = ticks(0.0, 0.0).into_iter().map(label).collect();
        assert_eq!(unit, ["0", "0.2", "0.4", "0.6", "0.8", "1"]);
        assert_eq!(label(2.5), "2.5");
        assert_eq!(label(10.0), "10");
        assert_eq!(label(-0.0), "0");
    }

    #[test]
    fn the_settings_go_through_the_object_data() {
        let spec = ChartSpec {
            kind: ChartKind::Radar,
            values: true,
            data: "a\t1\nb\t2\nc\t3".into(),
            ..Default::default()
        };
        assert_eq!(ChartSpec::from_data(&spec.to_data()), Some(spec));
        assert_eq!(ChartSpec::from_data("2\ncolumns"), None);
    }

    /// Every kind makes objects inside its box; columns make one
    /// rectangle per value, as tall as the value.
    #[test]
    fn every_kind_is_drawn_in_its_box() {
        for kind in ChartKind::ALL {
            let spec = ChartSpec {
                kind,
                values: true,
                ..Default::default()
            };
            let shapes = build(&spec, &mut ids()).expect("drawn");
            assert!(shapes.len() > 5, "{kind:?}");
            for s in &shapes {
                if let ShapeKind::Path { path, .. } = &s.kind {
                    let b = path.bounding_box();
                    assert!(
                        b.x0 > -1.0 && b.y0 > -1.0 && b.x1 < 121.0 && b.y1 < 81.0,
                        "{kind:?} {b:?}"
                    );
                }
            }
        }
        let spec = ChartSpec {
            legend: false,
            data: "\tA\nx\t10\ny\t5".into(),
            ..Default::default()
        };
        let shapes = build(&spec, &mut ids()).unwrap();
        let cols: Vec<Rect> = shapes
            .iter()
            .filter(|s| matches!(s.fill, Fill::Solid(ref c) if *c == series_color(0)))
            .filter_map(|s| match &s.kind {
                ShapeKind::Path { path, closed: true } => Some(path.bounding_box()),
                _ => None,
            })
            .collect();
        assert_eq!(cols.len(), 2);
        assert!(
            (cols[0].height() - 2.0 * cols[1].height()).abs() < 1e-6,
            "{cols:?}"
        );
        assert!((cols[0].y0 - cols[1].y0).abs() < 1e-9);
        assert!(cols[0].x1 < cols[1].x0);
    }

    #[test]
    fn a_stacked_column_holds_the_sum() {
        let spec = ChartSpec {
            kind: ChartKind::StackedColumns,
            legend: false,
            data: "\tA\tB\nx\t3\t7".into(),
            ..Default::default()
        };
        let shapes = build(&spec, &mut ids()).unwrap();
        let rect = |i: usize| {
            shapes
                .iter()
                .find(|s| matches!(s.fill, Fill::Solid(ref c) if *c == series_color(i)))
                .map(|s| s.bounds())
                .unwrap()
        };
        let (a, b) = (rect(0), rect(1));
        assert!((a.y1 - b.y0).abs() < 1e-6, "{a:?} {b:?}");
        assert!((b.height() / a.height() - 7.0 / 3.0).abs() < 1e-6);
        assert!((a.x0 - b.x0).abs() < 1e-9);
    }

    #[test]
    fn a_pie_has_a_slice_per_category() {
        let spec = ChartSpec {
            kind: ChartKind::Pie,
            data: "a\t1\nb\t1\nc\t2\nd\t0".into(),
            ..Default::default()
        };
        let shapes = build(&spec, &mut ids()).unwrap();
        let slices = shapes
            .iter()
            .filter(|s| s.stroke.as_ref().is_some_and(|k| k.width == 0.3))
            .count();
        assert_eq!(slices, 3);
        // The legend names the four categories.
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|s| match &s.kind {
                ShapeKind::Text { spans, .. } => Some(spans[0].text.clone()),
                _ => None,
            })
            .collect();
        for n in ["a", "b", "c", "d"] {
            assert!(texts.iter().any(|t| t == n), "{texts:?}");
        }
    }

    #[test]
    fn an_empty_table_draws_nothing() {
        let spec = ChartSpec {
            data: "".into(),
            ..Default::default()
        };
        assert!(build(&spec, &mut ids()).is_none());
    }
}
