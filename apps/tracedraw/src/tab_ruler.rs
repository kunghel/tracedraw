//! The tab ruler: a strip along the top edge of the paragraph frame being
//! edited with the Text tool, showing the tab stops of the paragraphs at
//! the caret (#1852). A click on it adds a left stop, a drag moves a stop,
//! a drag away from the strip removes it, and a right click picks a
//! stop's kind. Each change is one undo step. While a stop is dragged
//! the text flows to it as it moves (at most [`LIVE_EVERY`] apart, so a
//! long text keeps the drag smooth), and the whole drag is one undo step.
//! The paragraph's left and right indents show as triangles along the
//! strip's outer edge; dragging one sets the indent the same way (#1993).
//! See docs/behavior/text.md, Tab ruler.

use crate::app::App;
use crate::theme::Tokens;
use crate::tools::Tool;
use egui::{Color32, Painter, Pos2, Stroke, Vec2};
use tracedraw_core::{
    default_tab_step, document::ShapeKind, normalized_tabs, ShapeId, TabKind, TabStop,
};

/// The gap between the frame's top edge and the strip, beyond the
/// selection handles, interface points.
const GAP: f32 = 4.0;
/// The strip's height, interface points.
pub const HEIGHT: f32 = 14.0;
/// How near a marker a press must be to take it, interface points.
const GRAB: f32 = 6.0;
/// How far from the strip a stop must be dragged to be removed.
const OFF: f32 = 18.0;
/// The shortest time between two reflows of the text while a stop is
/// dragged, seconds (#1852, #1912).
pub const LIVE_EVERY: f64 = 1.0 / 30.0;

/// The ruler of the frame being edited, as it stands on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct TabRuler {
    pub shape: ShapeId,
    /// The left and right ends of the frame's top edge on screen.
    pub a: Pos2,
    pub b: Pos2,
    /// The unit vector pointing out of the frame, across the edge.
    pub up: Vec2,
    /// The frame's width, mm (the length the stops are measured along).
    pub width_mm: f64,
    /// The stops shown: the paragraph's own, or the default grid.
    pub stops: Vec<TabStop>,
    /// The paragraph has no stops of its own: `stops` is the default grid
    /// (the setting "Default tab spacing", 12.7 mm unless set).
    pub defaults: bool,
    /// The selection handles' size, interface points.
    pub handle: f32,
    /// The frame's height on screen, interface points.
    pub depth: f32,
    /// The paragraph's left and right indents, mm (from the frame's left
    /// and right edges).
    pub left_indent: f64,
    pub right_indent: f64,
}

/// Which indent of the paragraph a marker on the ruler sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentSide {
    Left,
    Right,
}

/// An indent marker being dragged along the ruler (#1993).
#[derive(Debug, Clone, PartialEq)]
pub struct IndentDrag {
    pub shape: ShapeId,
    pub side: IndentSide,
    /// The indent as it will be, mm.
    pub value: f64,
    /// The undo step before the drag: the drag's live steps join into one
    /// after it.
    pub before: Option<u64>,
    /// The indent the text was last laid out with during the drag, and
    /// when (input time, seconds).
    pub shown: Option<(f64, f64)>,
}

/// What a point of the strip is over: a position along it, mm, and the
/// stop there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RulerHit {
    pub shape: ShapeId,
    pub pos: f64,
    pub stop: Option<usize>,
}

/// A stop being dragged along the ruler: the stops as they will be, the
/// index of the one moving, and whether it is off the strip (to go).
#[derive(Debug, Clone, PartialEq)]
pub struct TabDrag {
    pub shape: ShapeId,
    pub stops: Vec<TabStop>,
    pub index: usize,
    pub off: bool,
    /// The undo step before the drag: the drag's live steps join into one
    /// after it.
    pub before: Option<u64>,
    /// The stops the text was last laid out with during the drag, and
    /// when (input time, seconds); `None` until the first reflow.
    pub shown: Option<(Vec<TabStop>, f64)>,
}

impl TabDrag {
    /// The stops the paragraph takes if the drag ends now: the dragged
    /// one left out when it is off the strip.
    pub fn result(&self) -> Vec<TabStop> {
        let mut stops = self.stops.clone();
        if self.off && self.index < stops.len() {
            stops.remove(self.index);
        }
        stops
    }
}

/// The default grid of a frame `width` mm wide: a left stop every
/// [`default_tab_step`] mm inside it (the setting "Default tab spacing").
pub fn default_stops(width: f64) -> Vec<TabStop> {
    let mut out = Vec::new();
    let step = default_tab_step();
    let mut x = step;
    while x < width - 1e-6 && out.len() < 2000 {
        out.push(TabStop::left(x));
        x += step;
    }
    out
}

impl TabRuler {
    fn len(&self) -> f32 {
        (self.b - self.a).length().max(1e-3)
    }

    fn dir(&self) -> Vec2 {
        (self.b - self.a) / self.len()
    }

    /// The near and far sides of the strip from the edge.
    fn band(&self) -> (f32, f32) {
        let near = self.handle + GAP;
        (near, near + HEIGHT)
    }

    /// The screen point `along` mm from the left end, `out` points out
    /// from the edge.
    pub fn at(&self, along: f64, out: f32) -> Pos2 {
        let t = (along / self.width_mm.max(1e-6)) as f32;
        self.a + (self.b - self.a) * t + self.up * out
    }

    /// The middle of the strip, `along` mm from its left end.
    #[cfg(test)]
    pub fn on_strip(&self, along: f64) -> Pos2 {
        let (near, far) = self.band();
        self.at(along, (near + far) / 2.0)
    }

    /// Where a stop's marker stands: on the strip's bottom line.
    pub fn marker(&self, along: f64) -> Pos2 {
        self.at(along, self.band().0 + 2.0)
    }

    /// The position along the edge (mm) and the distance out from it
    /// (points) of screen point `p`.
    pub fn locate(&self, p: Pos2) -> (f64, f32) {
        let r = p - self.a;
        let along = r.dot(self.dir()) / self.len();
        (along as f64 * self.width_mm, r.dot(self.up))
    }

    /// What screen point `p` is over, if it is on the strip.
    pub fn hit(&self, p: Pos2) -> Option<RulerHit> {
        let (pos, out) = self.locate(p);
        let (near, far) = self.band();
        let px = self.len() as f64 / self.width_mm.max(1e-6);
        let slack = 3.0 / px;
        if out < near - 2.0 || out > far + 2.0 || pos < -slack || pos > self.width_mm + slack {
            return None;
        }
        let stop = self
            .stops
            .iter()
            .enumerate()
            .map(|(i, s)| (i, ((s.pos - pos) * px).abs()))
            .filter(|(_, d)| *d <= GRAB as f64)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        Some(RulerHit {
            shape: self.shape,
            pos: pos.clamp(0.0, self.width_mm),
            stop,
        })
    }

    /// Where the marker of the indent on `side` stands along the strip,
    /// mm from its left end.
    pub fn indent_pos(&self, side: IndentSide) -> f64 {
        match side {
            IndentSide::Left => self.left_indent,
            IndentSide::Right => self.width_mm - self.right_indent,
        }
    }

    /// The indent marker under screen point `p`: in the outer half of
    /// the strip (the stops' markers stand in the inner half), within
    /// grabbing distance of the marker, the nearer one when both are.
    pub fn indent_at(&self, p: Pos2) -> Option<IndentSide> {
        let (pos, out) = self.locate(p);
        let (near, far) = self.band();
        if out < (near + far) / 2.0 || out > far + 3.0 {
            return None;
        }
        let px = self.len() as f64 / self.width_mm.max(1e-6);
        [IndentSide::Left, IndentSide::Right]
            .into_iter()
            .map(|side| (side, ((self.indent_pos(side) - pos) * px).abs()))
            .filter(|(_, d)| *d <= GRAB as f64)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(side, _)| side)
    }

    /// A point in the outer half of the strip, where the indent markers
    /// are, `along` mm from its left end.
    #[cfg(test)]
    pub fn on_indents(&self, along: f64) -> Pos2 {
        let (near, far) = self.band();
        self.at(along, far - (far - near) / 4.0)
    }

    /// Whether screen point `p` is far enough from the strip for a
    /// dragged stop to go.
    pub fn is_off(&self, p: Pos2) -> bool {
        let (_, out) = self.locate(p);
        let (near, far) = self.band();
        out < near - OFF || out > far + OFF
    }
}

impl App {
    /// The tab ruler of the paragraph frame being edited with the Text
    /// tool; `None` for artistic text, text on a path, vertical text and
    /// other tools.
    pub fn tab_ruler(&self) -> Option<TabRuler> {
        if self.tool != Tool::Text {
            return None;
        }
        let id = self.text_edit.as_ref()?.shape;
        let s = self.doc().find_shape(id)?;
        let ShapeKind::Text {
            frame: Some(frame),
            on_path: None,
            ..
        } = &s.kind
        else {
            return None;
        };
        let (_, para) = self.paragraph_style_of(s)?;
        if para.vertical || frame.width <= 0.0 {
            return None;
        }
        let c = s.text_frame_corners()?;
        let pts = c.map(|p| self.view.to_screen(p));
        let (a, b) = (pts[3], pts[2]);
        let edge = b - a;
        if edge.length() < 8.0 {
            return None;
        }
        let mut up = Vec2::new(edge.y, -edge.x).normalized();
        if (a - pts[0]).dot(up) < 0.0 {
            up = -up;
        }
        let (stops, defaults) = match &self.tab_drag {
            Some(d) if d.shape == id => (d.stops.clone(), false),
            _ if para.tabs.is_empty() => (default_stops(frame.width), true),
            _ => (para.tabs.clone(), false),
        };
        let (mut left_indent, mut right_indent) = (para.left_indent, para.right_indent);
        match &self.indent_drag {
            Some(d) if d.shape == id && d.side == IndentSide::Left => left_indent = d.value,
            Some(d) if d.shape == id => right_indent = d.value,
            _ => {}
        }
        Some(TabRuler {
            shape: id,
            a,
            b,
            up,
            depth: (pts[3] - pts[0]).length(),
            width_mm: frame.width,
            stops,
            defaults,
            handle: self.handle_size(),
            left_indent,
            right_indent,
        })
    }

    /// Give the paragraphs at the caret of text `id` the indent `value`
    /// mm on `side`: one undo step.
    pub fn set_indent(&mut self, id: ShapeId, side: IndentSide, value: f64) {
        self.edit_paragraph_style(id, |p| match side {
            IndentSide::Left => p.left_indent = value,
            IndentSide::Right => p.right_indent = value,
        });
    }

    /// An indent marker being dragged follows the pointer until the
    /// release; the text flows to it as it moves, as for a stop.
    fn indent_drag_input(&mut self, mut d: IndentDrag, response: &egui::Response, screen: Pos2) {
        let Some(r) = self.tab_ruler().filter(|r| r.shape == d.shape) else {
            return;
        };
        let now = response
            .ctx
            .pointer_latest_pos()
            .or(response.hover_pos())
            .unwrap_or(screen);
        let held = response.ctx.input(|i| i.pointer.primary_down());
        let (pos, _) = r.locate(now);
        // The two indents keep a millimetre of line between them.
        let room = |other: f64| (r.width_mm - other - 1.0).max(0.0);
        let value = match d.side {
            IndentSide::Left => pos.clamp(0.0, room(r.right_indent)),
            IndentSide::Right => (r.width_mm - pos).clamp(0.0, room(r.left_indent)),
        };
        d.value = (value * 10.0).round() / 10.0;
        let time = response.ctx.input(|i| i.time);
        let ended = !(held && !response.drag_stopped());
        let changed = d.shown.is_none_or(|(v, _)| v != d.value);
        let due = d.shown.is_none_or(|(_, at)| time - at >= LIVE_EVERY);
        if changed && (due || ended) {
            self.set_indent(d.shape, d.side, d.value);
            let label = self.engine.undo_label().unwrap_or("Indents");
            self.engine.join_group(d.before, label);
            d.shown = Some((d.value, time));
        } else if changed {
            let wait = d.shown.map_or(0.0, |(_, at)| LIVE_EVERY - (time - at));
            response
                .ctx
                .request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.0)));
        }
        if !ended {
            self.indent_drag = Some(d);
        }
        response.ctx.request_repaint();
    }

    /// Give the paragraphs at the caret of text `id` the stops `tabs`
    /// (sorted, doubles dropped): one undo step.
    pub fn set_tab_stops(&mut self, id: ShapeId, tabs: Vec<TabStop>) {
        let tabs = normalized_tabs(&tabs);
        self.edit_paragraph_style(id, |p| p.tabs = tabs);
    }

    /// The canvas input for the tab ruler (Text tool): true when the
    /// ruler took it.
    pub fn tab_ruler_input(&mut self, response: &egui::Response, screen: Pos2) -> bool {
        use egui::PointerButton;
        if let Some(d) = self.indent_drag.take() {
            self.indent_drag_input(d, response, screen);
            return true;
        }
        // A stop being dragged follows the pointer until the release.
        if let Some(mut d) = self.tab_drag.take() {
            let Some(r) = self.tab_ruler().filter(|r| r.shape == d.shape) else {
                return true;
            };
            let now = response
                .ctx
                .pointer_latest_pos()
                .or(response.hover_pos())
                .unwrap_or(screen);
            let held = response.ctx.input(|i| i.pointer.primary_down());
            let (pos, _) = r.locate(now);
            if let Some(s) = d.stops.get_mut(d.index) {
                s.pos = ((pos.clamp(0.0, r.width_mm)) * 10.0).round() / 10.0;
            }
            d.off = r.is_off(now);
            let time = response.ctx.input(|i| i.time);
            let ended = !(held && !response.drag_stopped());
            let stops = d.result();
            let changed = d.shown.as_ref().is_none_or(|(s, _)| *s != stops);
            let due = d
                .shown
                .as_ref()
                .is_none_or(|(_, at)| time - at >= LIVE_EVERY);
            if changed && (due || ended) {
                // The text flows to the stops as they are now; the drag's
                // steps join into one undo step.
                self.set_tab_stops(d.shape, stops.clone());
                let label = self.engine.undo_label().unwrap_or("Tab Stops");
                self.engine.join_group(d.before, label);
                d.shown = Some((stops, time));
            } else if changed {
                // Laid out again once the pause is over, even if the
                // pointer stops here.
                let wait = d
                    .shown
                    .as_ref()
                    .map_or(0.0, |(_, at)| LIVE_EVERY - (time - at));
                response
                    .ctx
                    .request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.0)));
            }
            if !ended {
                self.tab_drag = Some(d);
            }
            response.ctx.request_repaint();
            return true;
        }
        let Some(r) = self.tab_ruler() else {
            return false;
        };
        if response.secondary_clicked() {
            let Some(hit) = r.hit(screen) else {
                return false;
            };
            self.tab_ruler_menu = Some(hit);
            self.context_menu = Some((screen, self.view.to_page(screen)));
            return true;
        }
        let press = response.ctx.input(|i| i.pointer.press_origin());
        if response.drag_started_by(PointerButton::Primary) {
            if let Some(side) = r.indent_at(press.unwrap_or(screen)) {
                self.indent_drag = Some(IndentDrag {
                    shape: r.shape,
                    side,
                    value: match side {
                        IndentSide::Left => r.left_indent,
                        IndentSide::Right => r.right_indent,
                    },
                    before: self.engine.last_step_id(),
                    shown: None,
                });
                return true;
            }
            let Some(hit) = r.hit(press.unwrap_or(screen)) else {
                return false;
            };
            let mut stops = r.stops.clone();
            let index = match hit.stop {
                Some(i) => i,
                None => {
                    stops.push(TabStop::left(hit.pos));
                    stops.len() - 1
                }
            };
            self.tab_drag = Some(TabDrag {
                shape: r.shape,
                stops,
                index,
                off: false,
                before: self.engine.last_step_id(),
                shown: None,
            });
            return true;
        }
        if response.clicked_by(PointerButton::Primary) {
            let Some(hit) = r.hit(screen) else {
                return false;
            };
            // A click on an indent marker adds no stop.
            if hit.stop.is_none() && r.indent_at(screen).is_none() {
                let mut stops = r.stops.clone();
                stops.push(TabStop::left((hit.pos * 10.0).round() / 10.0));
                self.set_tab_stops(r.shape, stops);
            }
            return true;
        }
        // A press on the strip waits for its click or drag; the text
        // under it does not take it.
        response.is_pointer_button_down_on() && press.is_some_and(|p| r.hit(p).is_some())
    }

    /// The ruler's right-click menu picked `kind` (or `None`: remove) for
    /// what `hit` was over: the stop there, or a new stop at that place.
    pub fn tab_ruler_pick(&mut self, hit: RulerHit, kind: Option<TabKind>) {
        let Some(r) = self.tab_ruler().filter(|r| r.shape == hit.shape) else {
            return;
        };
        let mut stops = r.stops.clone();
        match (hit.stop, kind) {
            (Some(i), Some(k)) => {
                if let Some(s) = stops.get_mut(i) {
                    s.kind = k;
                }
            }
            (Some(i), None) => {
                if i < stops.len() {
                    stops.remove(i);
                }
            }
            (None, Some(k)) => stops.push(TabStop::new((hit.pos * 10.0).round() / 10.0, k)),
            (None, None) => return,
        }
        self.set_tab_stops(hit.shape, stops);
    }
}

/// Draw the tab ruler of the frame being edited: the strip with a tick
/// every millimetre (every 5 when they would crowd), a longer one every
/// 10, the stops as markers of their kind (the default grid lighter) and,
/// while a stop is dragged, a line down the frame where it would go.
pub fn draw(app: &App, painter: &Painter) {
    let Some(r) = app.tab_ruler() else {
        return;
    };
    let (near, far) = (r.handle + GAP, r.handle + GAP + HEIGHT);
    let fill = Tokens::ruler_bg();
    let ink = Tokens::text();
    let dim = Tokens::ruler_tick();
    painter.add(egui::Shape::convex_polygon(
        vec![
            r.at(0.0, near),
            r.at(r.width_mm, near),
            r.at(r.width_mm, far),
            r.at(0.0, far),
        ],
        fill,
        Stroke::new(1.0, dim),
    ));
    let px = r.len() as f64 / r.width_mm.max(1e-6);
    let step = if px >= 3.0 { 1.0 } else { 5.0 };
    let mut x = 0.0;
    let mut n = 0usize;
    while x <= r.width_mm + 1e-6 && n < 5000 {
        let major = (x / 10.0 - (x / 10.0).round()).abs() < 1e-6;
        let mid = (x / 5.0 - (x / 5.0).round()).abs() < 1e-6;
        let h = if major {
            HEIGHT * 0.6
        } else if mid {
            HEIGHT * 0.4
        } else {
            HEIGHT * 0.2
        };
        painter.line_segment([r.at(x, far), r.at(x, far - h)], Stroke::new(1.0, dim));
        x += step;
        n += 1;
    }
    let dragging = app.tab_drag.as_ref().filter(|d| d.shape == r.shape);
    for (i, s) in r.stops.iter().enumerate() {
        let moving = dragging.is_some_and(|d| d.index == i);
        let color = if moving && dragging.is_some_and(|d| d.off) {
            dim.gamma_multiply(0.4)
        } else if moving {
            Tokens::SELECTION
        } else if r.defaults {
            dim
        } else {
            ink
        };
        marker(painter, &r, s, color);
    }
    // Where the dragged stop would go: a line down the frame.
    if let Some(s) = dragging.and_then(|d| d.stops.get(d.index).filter(|_| !d.off)) {
        let top = r.at(s.pos, near);
        painter.line_segment(
            [top, top - r.up * (near + r.depth)],
            Stroke::new(1.0, Tokens::SELECTION.gamma_multiply(0.6)),
        );
    }
    // The indents: a triangle each along the outer edge, pointing at the
    // frame; the one dragged with a line down the frame.
    let moving = app
        .indent_drag
        .as_ref()
        .filter(|d| d.shape == r.shape)
        .map(|d| d.side);
    for side in [IndentSide::Left, IndentSide::Right] {
        let x = r.indent_pos(side);
        let color = if moving == Some(side) {
            Tokens::SELECTION
        } else {
            ink
        };
        indent_marker(painter, &r, x, color);
        if moving == Some(side) {
            let top = r.at(x, near);
            painter.line_segment(
                [top, top - r.up * (near + r.depth)],
                Stroke::new(1.0, Tokens::SELECTION.gamma_multiply(0.6)),
            );
        }
    }
}

/// An indent's marker: a small triangle hanging from the strip's outer
/// edge at `along` mm, its point toward the frame.
fn indent_marker(painter: &Painter, r: &TabRuler, along: f64, color: Color32) {
    let (near, far) = r.band();
    let half = (far - near) / 2.0;
    let dir = r.dir();
    let top = r.at(along, far);
    painter.add(egui::Shape::convex_polygon(
        vec![top - dir * 4.0, top + dir * 4.0, top - r.up * (half - 1.0)],
        color,
        Stroke::NONE,
    ));
}

/// One stop's marker: a corner opening to the right (left stop), to the
/// left (right stop), an upturned T (centre stop), and the T with a dot
/// (decimal stop).
fn marker(painter: &Painter, r: &TabRuler, s: &TabStop, color: Color32) {
    let base = r.marker(s.pos);
    let dir = r.dir();
    let up = r.up;
    let p = |x: f32, y: f32| base + dir * x + up * y;
    let st = Stroke::new(2.0, color);
    let stem = [p(0.0, 0.0), p(0.0, 7.0)];
    painter.line_segment(stem, st);
    match s.kind {
        TabKind::Left => {
            painter.line_segment([p(0.0, 0.0), p(5.0, 0.0)], st);
        }
        TabKind::Right => {
            painter.line_segment([p(-5.0, 0.0), p(0.0, 0.0)], st);
        }
        TabKind::Center => {
            painter.line_segment([p(-4.0, 0.0), p(4.0, 0.0)], st);
        }
        TabKind::Decimal => {
            painter.line_segment([p(-4.0, 0.0), p(4.0, 0.0)], st);
            painter.circle_filled(p(3.0, 4.0), 1.3, color);
        }
    }
}
