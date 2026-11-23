//! Shape Generator differential growth: a closed or open line that grows
//! inside the box (or the picked outline). Each step every point is
//! pulled toward its neighbours, smoothed and pushed away from the
//! points near it; edges that get too long are split, and a few random
//! edges are split every step so the line keeps folding. Growth stops on
//! its own when the space is full or the point limit is reached.
//!
//! The same steps always give the same drawing, so the step count saved
//! with a generated group makes it again on Update. The panel runs the
//! growth live (Space starts and pauses it, Stop makes it curves); while
//! paused, the preview follows the Steps value.
//!
//! Model space: mm, y up.

use crate::app::App;
use crate::shape_gen::{smooth_through, GenKind, Generated, Paint, Rng, ShapeGenSettings};
use crate::shape_gen_cells::inside;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracedraw_core::geometry::{Point, Rect, Vec2};
use tracedraw_core::ShapeId;

/// The most steps a growth runs.
pub const MAX_STEPS: u32 = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrowthStart {
    #[default]
    Circle,
    Line,
    Seeds,
}

impl GrowthStart {
    pub const ALL: [GrowthStart; 3] = [GrowthStart::Circle, GrowthStart::Line, GrowthStart::Seeds];
    pub fn key(self) -> &'static str {
        match self {
            GrowthStart::Circle => "shapegen.growth_circle",
            GrowthStart::Line => "shapegen.growth_line",
            GrowthStart::Seeds => "shapegen.growth_seeds",
        }
    }
}

/// Where growth may go inside a picked outline: inside flags on a grid
/// over the box, about 400 cells along its longer side.
struct Mask {
    w: usize,
    h: usize,
    cell: f64,
    bits: Vec<bool>,
}

impl Mask {
    fn new(s: &ShapeGenSettings) -> Option<Mask> {
        let b = s.boundary.as_ref()?;
        let cell = (s.width.max(s.height) / 400.0).max(1e-6);
        let w = ((s.width / cell).ceil() as usize).max(1);
        let h = ((s.height / cell).ceil() as usize).max(1);
        let mut bits = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let p = Point::new((x as f64 + 0.5) * cell, (y as f64 + 0.5) * cell);
                bits.push(inside(Some(b), p));
            }
        }
        Some(Mask { w, h, cell, bits })
    }

    fn at(&self, p: Point) -> bool {
        if p.x < 0.0 || p.y < 0.0 {
            return false;
        }
        let (x, y) = ((p.x / self.cell) as usize, (p.y / self.cell) as usize);
        x < self.w && y < self.h && self.bits[y * self.w + x]
    }
}

/// A growth in progress.
pub struct GrowthSim {
    /// Each line's points, and whether it is closed.
    paths: Vec<(Vec<Point>, bool)>,
    /// Steps run so far.
    pub steps: u32,
    /// True once nothing moves or splits any more: it has filled its
    /// space or reached the point limit.
    pub settled: bool,
    /// Steps run since the point limit was reached.
    full_steps: u32,
    rect: Rect,
    mask: Option<Mask>,
    rng: Rng,
    /// Edge length, mm.
    d: f64,
    /// How far points push each other away, mm.
    radius: f64,
    align: f64,
    rate: u32,
    max_nodes: usize,
}

impl GrowthSim {
    pub fn new(s: &ShapeGenSettings) -> GrowthSim {
        let rect = Rect::new(0.0, 0.0, s.width, s.height);
        let d = s.growth_spacing;
        let mut sim = GrowthSim {
            paths: Vec::new(),
            steps: 0,
            settled: false,
            full_steps: 0,
            rect,
            mask: Mask::new(s),
            rng: Rng::new(s.seed, 0x6a0e),
            d,
            radius: d * s.growth_repel,
            align: s.growth_align,
            rate: s.growth_rate,
            max_nodes: s.growth_max_nodes as usize,
        };
        let side = s.width.min(s.height);
        let size = s.growth_size / 100.0;
        let center = sim.start_point();
        match s.growth_start {
            GrowthStart::Circle => {
                if let Some(c) = center {
                    if let Some(ring) = sim.ring(c, (side * size / 2.0).max(d * 2.0)) {
                        sim.paths.push((ring, true));
                    }
                }
            }
            GrowthStart::Line => {
                if let Some(c) = center {
                    let mut half = (s.width * size / 2.0).max(d * 2.0);
                    for _ in 0..8 {
                        let n = ((2.0 * half / d).round() as usize).max(2);
                        let pts: Vec<Point> = (0..=n)
                            .map(|k| Point::new(c.x - half + 2.0 * half * k as f64 / n as f64, c.y))
                            .collect();
                        if pts.iter().all(|p| sim.ok(*p)) {
                            sim.paths.push((pts, false));
                            break;
                        }
                        half /= 2.0;
                    }
                }
            }
            GrowthStart::Seeds => {
                let n = s.growth_seed_count.max(1);
                let r = (side * size / 2.0 / (n as f64).sqrt()).max(d * 2.0);
                let mut rng = Rng::new(s.seed, 0x5eed);
                for _ in 0..n {
                    for _ in 0..60 {
                        let c = Point::new(
                            rect.x0 + rng.unit() * rect.width(),
                            rect.y0 + rng.unit() * rect.height(),
                        );
                        if let Some(ring) = sim.ring(c, r) {
                            sim.paths.push((ring, true));
                            break;
                        }
                    }
                }
            }
        }
        if sim.paths.is_empty() {
            sim.settled = true;
        }
        sim
    }

    /// Whether a point may be there: inside the box less half an edge,
    /// and inside the outline with half an edge to spare.
    fn ok(&self, p: Point) -> bool {
        let m = self.d * 0.5;
        p.x >= self.rect.x0 + m
            && p.x <= self.rect.x1 - m
            && p.y >= self.rect.y0 + m
            && p.y <= self.rect.y1 - m
            && self.mask.as_ref().is_none_or(|k| {
                // Half a spacing clear of the outline too.
                k.at(p)
                    && k.at(Point::new(p.x - m, p.y))
                    && k.at(Point::new(p.x + m, p.y))
                    && k.at(Point::new(p.x, p.y - m))
                    && k.at(Point::new(p.x, p.y + m))
            })
    }

    /// The box's middle, or when the outline leaves it out, the inside
    /// cell nearest to it.
    fn start_point(&self) -> Option<Point> {
        let c = self.rect.center();
        if self.ok(c) {
            return Some(c);
        }
        let k = self.mask.as_ref()?;
        let mut best: Option<(f64, Point)> = None;
        for y in 0..k.h {
            for x in 0..k.w {
                let p = Point::new((x as f64 + 0.5) * k.cell, (y as f64 + 0.5) * k.cell);
                if !self.ok(p) {
                    continue;
                }
                let dd = (p - c).hypot2();
                if best.is_none_or(|(b, _)| dd < b) {
                    best = Some((dd, p));
                }
            }
        }
        best.map(|(_, p)| p)
    }

    /// A ring of points one edge apart around `c`, shrunk until it fits.
    fn ring(&self, c: Point, mut r: f64) -> Option<Vec<Point>> {
        for _ in 0..8 {
            let n = ((std::f64::consts::TAU * r / self.d).round() as usize).max(8);
            let pts: Vec<Point> = (0..n)
                .map(|k| {
                    let a = std::f64::consts::TAU * k as f64 / n as f64;
                    Point::new(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            if pts.iter().all(|p| self.ok(*p)) {
                return Some(pts);
            }
            r /= 2.0;
            if r < self.d {
                break;
            }
        }
        None
    }

    pub fn node_count(&self) -> usize {
        self.paths.iter().map(|(p, _)| p.len()).sum()
    }

    /// One step: move every point, then split. False once settled.
    pub fn step(&mut self) -> bool {
        if self.settled {
            return false;
        }
        let d = self.d;
        let r = self.radius.max(1e-9);
        let key = |p: Point| ((p.x / r).floor() as i64, (p.y / r).floor() as i64);
        let mut grid: HashMap<(i64, i64), Vec<(u32, u32)>> = HashMap::new();
        for (pi, (pts, _)) in self.paths.iter().enumerate() {
            for (i, p) in pts.iter().enumerate() {
                grid.entry(key(*p)).or_default().push((pi as u32, i as u32));
            }
        }
        let mut next = self.paths.clone();
        let mut moved = 0.0f64;
        for (pi, (pts, closed)) in self.paths.iter().enumerate() {
            let n = pts.len();
            for i in 0..n {
                let p = pts[i];
                let prev = if i > 0 {
                    Some(i - 1)
                } else if *closed {
                    Some(n - 1)
                } else {
                    None
                };
                let nxt = if i + 1 < n {
                    Some(i + 1)
                } else if *closed {
                    Some(0)
                } else {
                    None
                };
                let mut f = Vec2::ZERO;
                // Neighbours hold the edge length; the point drifts to
                // their middle.
                let mut mid = Vec2::ZERO;
                let mut cnt = 0;
                for j in [prev, nxt].into_iter().flatten() {
                    let v = pts[j] - p;
                    let len = v.hypot();
                    if len > 1e-12 {
                        f += v * ((len - d) / len) * 0.25;
                    }
                    mid += pts[j].to_vec2();
                    cnt += 1;
                }
                if cnt == 2 {
                    f += (mid / 2.0 - p.to_vec2()) * self.align;
                }
                // Points near it push it away.
                let (kx, ky) = key(p);
                let mut push = Vec2::ZERO;
                for gx in kx - 1..=kx + 1 {
                    for gy in ky - 1..=ky + 1 {
                        let Some(bucket) = grid.get(&(gx, gy)) else {
                            continue;
                        };
                        for &(qj, qi) in bucket {
                            let (qj, qi) = (qj as usize, qi as usize);
                            if qj == pi && (qi == i || Some(qi) == prev || Some(qi) == nxt) {
                                continue;
                            }
                            let v = p - self.paths[qj].0[qi];
                            let dd = v.hypot2();
                            if dd < r * r && dd > 1e-18 {
                                let dist = dd.sqrt();
                                push += v / dist * ((r - dist) / r);
                            }
                        }
                    }
                }
                f += push * (d * 0.5);
                let len = f.hypot();
                if len > d * 0.5 {
                    f = f * (d * 0.5 / len);
                }
                let np = p + f;
                if self.ok(np) {
                    next[pi].0[i] = np;
                    moved = moved.max(f.hypot());
                }
            }
        }
        self.paths = next;
        // Long edges split at their middle.
        let mut total = self.node_count();
        let mut added = 0usize;
        for (pts, closed) in &mut self.paths {
            let n = pts.len();
            let mut out = Vec::with_capacity(n + n / 4);
            for i in 0..n {
                out.push(pts[i]);
                let j = if i + 1 < n {
                    i + 1
                } else if *closed {
                    0
                } else {
                    continue;
                };
                if total < self.max_nodes && (pts[j] - pts[i]).hypot() > d * 1.4 {
                    out.push(pts[i].midpoint(pts[j]));
                    total += 1;
                    added += 1;
                }
            }
            *pts = out;
        }
        // A few random edges split too, so the line keeps growing.
        for _ in 0..self.rate {
            if total >= self.max_nodes || total == 0 {
                break;
            }
            let mut k = self.rng.below(total as u32) as usize;
            for (pts, closed) in &mut self.paths {
                if k < pts.len() {
                    let n = pts.len();
                    let j = if k + 1 < n {
                        Some(k + 1)
                    } else if *closed {
                        Some(0)
                    } else {
                        None
                    };
                    if let Some(j) = j {
                        let m = pts[k].midpoint(pts[j]);
                        pts.insert(k + 1, m);
                        total += 1;
                        added += 1;
                    }
                    break;
                }
                k -= pts.len();
            }
        }
        self.steps += 1;
        // At the point limit it relaxes a little longer, then stops.
        if added == 0 && total >= self.max_nodes {
            self.full_steps += 1;
        }
        if (added == 0 && moved < d * 0.01) || self.full_steps >= 50 {
            self.settled = true;
        }
        !self.settled
    }

    /// The lines as smooth curves.
    pub fn draw(&self, out: &mut Generated) {
        out.frame = Some(self.rect);
        for (pts, closed) in &self.paths {
            if pts.len() < 2 {
                continue;
            }
            if out.full() {
                out.cut = true;
                return;
            }
            out.push(smooth_through(pts, *closed), *closed, Paint::Line);
        }
    }

    /// Run to `target` steps (or until settled).
    pub fn run_to(&mut self, target: u32) {
        while self.steps < target && !self.settled {
            self.step();
        }
    }
}

/// The growth after the settings' steps, for the preview and Create.
pub(crate) fn growth(s: &ShapeGenSettings, out: &mut Generated) {
    let mut sim = GrowthSim::new(s);
    sim.run_to(s.growth_steps);
    sim.draw(out);
}

/// The panel's live growth: the settings it started from (steps left
/// out) and whether it is running.
pub struct GrowthLive {
    key: ShapeGenSettings,
    pub sim: GrowthSim,
    pub running: bool,
}

/// The settings that change what grows: all but the step count and how
/// the result is painted.
fn growth_key(s: &ShapeGenSettings) -> ShapeGenSettings {
    let mut k = s.clone();
    k.growth_steps = 0;
    k.line_width = 0.0;
    k.combine = false;
    k
}

impl App {
    /// Keep the live growth in step with the panel and advance it for up
    /// to `budget`: while running it grows and Steps follows it, while
    /// paused it catches up with Steps. True while more frames are needed.
    pub fn shape_gen_growth_tick(&mut self, budget: Duration) -> bool {
        let s = self.shape_gen.sanitized();
        if s.kind != GenKind::Growth {
            self.shape_gen_growth = None;
            return false;
        }
        let k = growth_key(&s);
        let reset = match &self.shape_gen_growth {
            Some(l) => l.key != k || (!l.running && l.sim.steps > s.growth_steps),
            None => true,
        };
        if reset {
            self.shape_gen_growth = Some(GrowthLive {
                sim: GrowthSim::new(&s),
                key: k,
                running: false,
            });
        }
        let Some(live) = self.shape_gen_growth.as_mut() else {
            return false;
        };
        let target = if live.running {
            MAX_STEPS
        } else {
            s.growth_steps
        };
        let start = Instant::now();
        while live.sim.steps < target && !live.sim.settled {
            live.sim.step();
            if start.elapsed() >= budget {
                break;
            }
        }
        if live.running {
            if live.sim.settled || live.sim.steps >= MAX_STEPS {
                live.running = false;
            }
            self.shape_gen.growth_steps = live.sim.steps;
        }
        live.sim.steps < target && !live.sim.settled
    }

    /// Space in the panel: start the growth, or pause it.
    pub fn shape_gen_growth_toggle(&mut self) {
        if self.shape_gen.kind != GenKind::Growth {
            return;
        }
        if self.shape_gen_growth.is_none() {
            self.shape_gen_growth_tick(Duration::ZERO);
        }
        if let Some(l) = self.shape_gen_growth.as_mut() {
            l.running = !l.running && !l.sim.settled && l.sim.steps < MAX_STEPS;
        }
    }

    /// Whether the live growth is running.
    pub fn shape_gen_growth_running(&self) -> bool {
        self.shape_gen_growth.as_ref().is_some_and(|l| l.running)
    }

    /// Stop: the growth stops where it is and becomes curves (Create),
    /// its step count kept for Update.
    pub fn shape_gen_growth_stop(&mut self) -> Option<ShapeId> {
        if self.shape_gen.kind != GenKind::Growth {
            return None;
        }
        if let Some(l) = self.shape_gen_growth.as_mut() {
            l.running = false;
            self.shape_gen.growth_steps = l.sim.steps;
        }
        self.shape_gen_create()
    }

    /// The live growth drawn, when it is exactly what the settings make
    /// (Create then needs no second run).
    pub(crate) fn shape_gen_growth_cached(&self, s: &ShapeGenSettings) -> Option<Generated> {
        if s.kind != GenKind::Growth {
            return None;
        }
        let l = self.shape_gen_growth.as_ref()?;
        let same = l.key == growth_key(s)
            && (l.sim.steps == s.growth_steps || (l.sim.settled && l.sim.steps <= s.growth_steps));
        if !same {
            return None;
        }
        let mut g = Generated::default();
        l.sim.draw(&mut g);
        Some(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::generate;
    use tracedraw_core::geometry::BezPath;

    fn grow() -> ShapeGenSettings {
        ShapeGenSettings {
            kind: GenKind::Growth,
            width: 60.0,
            height: 60.0,
            growth_spacing: 2.0,
            growth_steps: 60,
            growth_max_nodes: 600,
            ..ShapeGenSettings::default()
        }
    }

    #[test]
    fn growth_adds_points_stays_inside_and_repeats() {
        let s = grow();
        let mut sim = GrowthSim::new(&s);
        let start = sim.node_count();
        assert!(start >= 8);
        sim.run_to(s.growth_steps);
        assert!(sim.node_count() > start, "{} {}", sim.node_count(), start);
        assert!(sim.node_count() <= 600);
        for (pts, closed) in &sim.paths {
            assert!(*closed);
            for p in pts {
                assert!(
                    p.x >= 0.9 && p.x <= 59.1 && p.y >= 0.9 && p.y <= 59.1,
                    "{p:?}"
                );
            }
        }
        // The same settings grow the same line.
        let a = generate(&s);
        let b = generate(&s);
        assert_eq!(a.items.len(), 1);
        assert_eq!(a.items[0].path, b.items[0].path);
        // Lines and seeds start as they say.
        let line = GrowthSim::new(&ShapeGenSettings {
            growth_start: GrowthStart::Line,
            ..grow()
        });
        assert_eq!(line.paths.len(), 1);
        assert!(!line.paths[0].1);
        let seeds = GrowthSim::new(&ShapeGenSettings {
            growth_start: GrowthStart::Seeds,
            growth_seed_count: 4,
            ..grow()
        });
        assert!(seeds.paths.len() >= 2);
    }

    #[test]
    fn growth_stops_at_the_point_limit_and_keeps_to_the_outline() {
        // A triangle outline: every point stays inside it.
        let mut tri = BezPath::new();
        tri.move_to(Point::new(0.0, 0.0));
        tri.line_to(Point::new(60.0, 0.0));
        tri.line_to(Point::new(30.0, 60.0));
        tri.close_path();
        let s = ShapeGenSettings {
            boundary: Some(tri.clone()),
            growth_max_nodes: 150,
            growth_steps: 400,
            ..grow()
        };
        let mut sim = GrowthSim::new(&s);
        sim.run_to(s.growth_steps);
        assert!(sim.node_count() <= 150);
        assert!(sim.settled, "never settled after {} steps", sim.steps);
        for (pts, _) in &sim.paths {
            for p in pts {
                assert!(inside(Some(&tri), *p), "{p:?}");
            }
        }
    }

    #[test]
    fn live_growth_runs_pauses_and_stops_into_curves() {
        let mut app = App::headless();
        app.new_shape(tracedraw_core::ShapeKind::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
        app.shape_gen = ShapeGenSettings {
            growth_steps: 0,
            ..grow()
        };
        assert!(!app.shape_gen_growth_tick(Duration::from_millis(50)));
        app.shape_gen_growth_toggle();
        assert!(app.shape_gen_growth_running());
        for _ in 0..20 {
            app.shape_gen_growth_tick(Duration::ZERO);
        }
        // Each tick ran at least one step, and Steps follows it.
        assert!(app.shape_gen.growth_steps >= 20);
        app.shape_gen_growth_toggle();
        assert!(!app.shape_gen_growth_running());
        let steps = app.shape_gen.growth_steps;
        let live = app.shape_gen_growth_cached(&app.shape_gen.sanitized());
        let fresh = generate(&app.shape_gen);
        assert_eq!(live.expect("live").items[0].path, fresh.items[0].path);
        let before = app.engine.history_labels().0.len();
        let gid = app.shape_gen_growth_stop().expect("made");
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        let (_, saved) = app.shape_gen_selected().expect("generated group");
        assert_eq!(saved.growth_steps, steps);
        assert_eq!(saved.kind, GenKind::Growth);
        assert!(app.doc().find_shape(gid).is_some());
    }
}
