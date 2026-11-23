//! The Common Shapes tool's shapes, in five sets: basic shapes, arrows,
//! flowchart symbols, banners and callouts. Each is drawn into the box
//! dragged (`CommonShape::path_with`), as one curve object that fills
//! even-odd like every curve: parts that should look separate are
//! separate subpaths side by side, never overlapping, and inner details
//! are straight two-point lines. Most shapes have adjustments (head
//! length, slant, thickness, a callout's tail...) that the object keeps
//! and the Shape tool shows as handles (#4045).

use crate::i18n::tr;
use tracedraw_core::geometry::{BezPath, Point, Rect};

/// The sets of shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Basic,
    Arrows,
    Flowchart,
    Banners,
    Callouts,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Category::Basic,
        Category::Arrows,
        Category::Flowchart,
        Category::Banners,
        Category::Callouts,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Category::Basic => "shapes.basic",
            Category::Arrows => "shapes.arrows",
            Category::Flowchart => "shapes.flowchart",
            Category::Banners => "shapes.banners",
            Category::Callouts => "shapes.callouts",
        }
    }
}

macro_rules! shapes {
    ($( $cat:ident => [ $( $v:ident = $key:literal ),* $(,)? ] ),* $(,)?) => {
        /// One shape of the Common Shapes tool.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum CommonShape { $( $( $v, )* )* }

        impl CommonShape {
            /// Every shape, set by set.
            pub const ALL: &'static [CommonShape] = &[ $( $( CommonShape::$v, )* )* ];

            pub fn category(self) -> Category {
                match self { $( $( CommonShape::$v => Category::$cat, )* )* }
            }

            /// The i18n key of its name.
            pub fn key(self) -> &'static str {
                match self { $( $( CommonShape::$v => $key, )* )* }
            }
        }
    };
}

shapes! {
    Basic => [
        Parallelogram = "shape.parallelogram",
        Trapezoid = "shape.trapezoid",
        Diamond = "shape.diamond",
        Triangle = "shape.triangle",
        RightTriangle = "shape.right_triangle",
        Hexagon = "shape.hexagon",
        Octagon = "shape.octagon",
        Cross = "shape.cross",
        Heart = "shape.heart",
        Lightning = "shape.lightning",
        Crescent = "shape.crescent",
        Ring = "shape.ring",
        Cylinder = "shape.cylinder",
        Cube = "shape.cube",
        Frame = "shape.frame",
        Teardrop = "shape.teardrop",
        Smiley = "shape.smiley",
        NoSymbol = "shape.no_symbol",
    ],
    Arrows => [
        RightArrow = "shape.arrow",
        LeftArrow = "shape.left_arrow",
        UpArrow = "shape.up_arrow",
        DownArrow = "shape.down_arrow",
        LeftRightArrow = "shape.left_right_arrow",
        UpDownArrow = "shape.up_down_arrow",
        QuadArrow = "shape.quad_arrow",
        Chevron = "shape.chevron",
        Pointer = "shape.pointer",
        NotchedArrow = "shape.notched_arrow",
        StripedArrow = "shape.striped_arrow",
        BentArrow = "shape.bent_arrow",
    ],
    Flowchart => [
        Process = "shape.process",
        AlternateProcess = "shape.alternate_process",
        Decision = "shape.decision",
        Data = "shape.data",
        PredefinedProcess = "shape.predefined_process",
        InternalStorage = "shape.internal_storage",
        Document = "shape.document",
        Multidocument = "shape.multidocument",
        Terminator = "shape.terminator",
        Preparation = "shape.preparation",
        ManualInput = "shape.manual_input",
        ManualOperation = "shape.manual_operation",
        Connector = "shape.connector",
        OffPageConnector = "shape.off_page_connector",
        Card = "shape.card",
        PunchedTape = "shape.punched_tape",
        SummingJunction = "shape.summing_junction",
        Or = "shape.or",
        Collate = "shape.collate",
        Sort = "shape.sort",
        Extract = "shape.extract",
        Merge = "shape.merge",
        StoredData = "shape.stored_data",
        Delay = "shape.delay",
        MagneticDisk = "shape.magnetic_disk",
        DirectAccessStorage = "shape.direct_access_storage",
        Display = "shape.display",
    ],
    Banners => [
        Banner = "shape.banner",
        FoldedRibbon = "shape.folded_ribbon",
        Wave = "shape.wave",
        DoubleWave = "shape.double_wave",
        Scroll = "shape.scroll",
        Explosion = "shape.explosion",
        Burst = "shape.burst",
        Pennant = "shape.pennant",
    ],
    Callouts => [
        Callout = "shape.callout",
        RoundedCallout = "shape.rounded_callout",
        OvalCallout = "shape.oval_callout",
        CloudCallout = "shape.cloud_callout",
        LineCallout = "shape.line_callout",
        BentLineCallout = "shape.bent_line_callout",
    ],
}

/// Builds a path in the unit square (u right, v up, both 0 to 1) and
/// places it in a box.
struct Pen {
    r: Rect,
    path: BezPath,
    /// Heights scaled about a line: (v of the line, scale).
    vmap: Option<(f64, f64)>,
}

impl Pen {
    fn new(r: Rect) -> Pen {
        Pen {
            r,
            path: BezPath::new(),
            vmap: None,
        }
    }

    fn at(&self, u: f64, v: f64) -> Point {
        let v = match self.vmap {
            Some((mid, k)) => mid + (v - mid) * k,
            None => v,
        };
        Point::new(
            self.r.x0 + u * self.r.width(),
            self.r.y0 + v * self.r.height(),
        )
    }

    fn poly(&mut self, pts: &[(f64, f64)]) {
        for (i, &(u, v)) in pts.iter().enumerate() {
            let p = self.at(u, v);
            if i == 0 {
                self.path.move_to(p);
            } else {
                self.path.line_to(p);
            }
        }
        self.path.close_path();
    }

    /// A straight detail line (a two-point subpath: it is stroked, never
    /// filled).
    fn line(&mut self, a: (f64, f64), b: (f64, f64)) {
        let (a, b) = (self.at(a.0, a.1), self.at(b.0, b.1));
        self.path.move_to(a);
        self.path.line_to(b);
    }

    fn move_to(&mut self, u: f64, v: f64) {
        let p = self.at(u, v);
        self.path.move_to(p);
    }

    fn line_to(&mut self, u: f64, v: f64) {
        let p = self.at(u, v);
        self.path.line_to(p);
    }

    fn curve_to(&mut self, c1: (f64, f64), c2: (f64, f64), to: (f64, f64)) {
        let (a, b, c) = (
            self.at(c1.0, c1.1),
            self.at(c2.0, c2.1),
            self.at(to.0, to.1),
        );
        self.path.curve_to(a, b, c);
    }

    fn close(&mut self) {
        self.path.close_path();
    }

    /// An elliptical arc from angle `a0` to `a1` (radians, counter-
    /// clockwise from +u) around (cu, cv) with radii (ru, rv), as cubic
    /// pieces of at most a quarter turn; starts with a line from the
    /// current point (or a move when `start` is set).
    fn arc(&mut self, c: (f64, f64), rad: (f64, f64), a0: f64, a1: f64, start: bool) {
        let point = |a: f64| (c.0 + rad.0 * a.cos(), c.1 + rad.1 * a.sin());
        let p0 = point(a0);
        if start {
            self.move_to(p0.0, p0.1);
        } else {
            self.line_to(p0.0, p0.1);
        }
        let n = ((a1 - a0).abs() / std::f64::consts::FRAC_PI_2)
            .ceil()
            .max(1.0) as usize;
        let step = (a1 - a0) / n as f64;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        for i in 0..n {
            let (s, e) = (a0 + step * i as f64, a0 + step * (i + 1) as f64);
            let (ps, pe) = (point(s), point(e));
            let c1 = (ps.0 - k * rad.0 * s.sin(), ps.1 + k * rad.1 * s.cos());
            let c2 = (pe.0 + k * rad.0 * e.sin(), pe.1 - k * rad.1 * e.cos());
            self.curve_to(c1, c2, pe);
        }
    }

    fn ellipse(&mut self, c: (f64, f64), rad: (f64, f64)) {
        self.arc(c, rad, 0.0, std::f64::consts::TAU, true);
        self.close();
    }

    /// A rectangle with corners rounded by `radius` box units (the same
    /// size across and up, whatever the box's shape).
    fn rounded(&mut self, u0: f64, v0: f64, u1: f64, v1: f64, radius: f64) {
        let (w, h) = (self.r.width().max(1e-9), self.r.height().max(1e-9));
        let rmax = ((u1 - u0) * w).min((v1 - v0) * h) / 2.0;
        let r = radius.min(rmax).max(0.0);
        let (ru, rv) = (r / w, r / h);
        use std::f64::consts::{FRAC_PI_2, PI};
        self.arc((u1 - ru, v0 + rv), (ru, rv), -FRAC_PI_2, 0.0, true);
        self.arc((u1 - ru, v1 - rv), (ru, rv), 0.0, FRAC_PI_2, false);
        self.arc((u0 + ru, v1 - rv), (ru, rv), FRAC_PI_2, PI, false);
        self.arc((u0 + ru, v0 + rv), (ru, rv), PI, PI + FRAC_PI_2, false);
        self.close();
    }

    /// A star around (0.5, 0.5) with the given radii, point by point
    /// (fractions of the half box), the first point straight up.
    fn star(&mut self, radii: &[f64]) {
        let n = radii.len();
        let pts: Vec<(f64, f64)> = radii
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let a = std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / n as f64;
                (0.5 + 0.5 * r * a.cos(), 0.5 + 0.5 * r * a.sin())
            })
            .collect();
        self.poly(&pts);
    }

    /// A band whose bottom and top edges are waves of `periods` periods
    /// and height `amp`.
    fn wave_band(&mut self, v0: f64, v1: f64, amp: f64, periods: usize) {
        let n = periods.max(1);
        self.move_to(0.0, v0);
        for i in 0..n {
            let (a, b) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let d = (b - a) / 3.0;
            self.curve_to((a + d, v0 - amp), (b - d, v0 + amp), (b, v0));
        }
        self.line_to(1.0, v1);
        for i in (0..n).rev() {
            let (a, b) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let d = (b - a) / 3.0;
            self.curve_to((b - d, v1 + amp), (a + d, v1 - amp), (a, v1));
        }
        self.close();
    }

    /// A cylinder standing in the box: the top ellipse and the body under
    /// its front edge, side by side.
    fn cylinder(&mut self, rv: f64) {
        use std::f64::consts::PI;
        self.ellipse((0.5, 1.0 - rv), (0.5, rv));
        // Body: front half of the top ellipse down to the bottom.
        self.arc((0.5, 1.0 - rv), (0.5, rv), PI, 2.0 * PI, true);
        self.arc((0.5, rv), (0.5, rv), 0.0, -PI, false);
        self.close();
    }

    /// A cylinder lying down: the right end's ellipse and the body.
    fn lying_cylinder(&mut self, ru: f64) {
        use std::f64::consts::{FRAC_PI_2, PI};
        self.ellipse((1.0 - ru, 0.5), (ru, 0.5));
        self.arc((1.0 - ru, 0.5), (ru, 0.5), FRAC_PI_2, PI + FRAC_PI_2, true);
        self.arc((ru, 0.5), (ru, 0.5), PI + FRAC_PI_2, FRAC_PI_2, false);
        self.close();
    }

    /// A hole: drawn by `build` like any part, then turned to run against
    /// the outer outlines (which run counter-clockwise), so it stays open
    /// under either fill rule.
    fn hole(&mut self, build: impl FnOnce(&mut Pen)) {
        use tracedraw_core::geometry::Shape as _;
        let mut part = Pen::new(self.r);
        build(&mut part);
        let mut path = part.finish();
        if path.area() > 0.0 {
            path = path.reverse_subpaths();
        }
        self.path.extend(path);
    }

    fn finish(self) -> BezPath {
        self.path
    }
}

fn mapped(pts: &[(f64, f64)], f: impl Fn(f64, f64) -> (f64, f64)) -> Vec<(f64, f64)> {
    pts.iter().map(|&(u, v)| f(u, v)).collect()
}

/// The document symbol's outline (straight top, wavy bottom whose wave
/// is `wave` high) in the part (u0..u1, v0..v1) of the box.
fn document(p: &mut Pen, u0: f64, v0: f64, u1: f64, v1: f64, wave: f64) {
    let (w, h) = (u1 - u0, v1 - v0);
    let at = |u: f64, v: f64| (u0 + u * w, v0 + v * h);
    let s = at(0.0, wave);
    p.move_to(s.0, s.1);
    p.curve_to(at(0.3, -wave), at(0.6, 3.0 * wave), at(1.0, wave * 2.0 / 3.0));
    let t = at(1.0, 1.0);
    p.line_to(t.0, t.1);
    let t = at(0.0, 1.0);
    p.line_to(t.0, t.1);
    p.close();
}

/// One adjustment handle of a shape (#4045): where it sits in the unit
/// box for the shape's current values, and how each value it sets moves
/// it (the change of its place for a change of one in that value). Every
/// handle moves in a straight line with its values, so a drag sets them
/// exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handle {
    /// Its place in the unit box (u right, v up).
    pub at: (f64, f64),
    /// The values it sets, each with its direction.
    pub moves: [Option<(usize, (f64, f64))>; 2],
}

impl Handle {
    fn one(at: (f64, f64), i: usize, d: (f64, f64)) -> Handle {
        Handle {
            at,
            moves: [Some((i, d)), None],
        }
    }

    fn two(at: (f64, f64), i: usize, di: (f64, f64), j: usize, dj: (f64, f64)) -> Handle {
        Handle {
            at,
            moves: [Some((i, di)), Some((j, dj))],
        }
    }

    /// The handle with the box mapped by the affine map `f` (a shape
    /// drawn as another one turned or mirrored).
    fn mapped(self, f: impl Fn(f64, f64) -> (f64, f64)) -> Handle {
        let at = f(self.at.0, self.at.1);
        let moves = self.moves.map(|m| {
            m.map(|(i, d)| {
                let to = f(self.at.0 + d.0, self.at.1 + d.1);
                (i, (to.0 - at.0, to.1 - at.1))
            })
        });
        Handle { at, moves }
    }
}

/// A value of a shape: its default and the range a drag keeps it in.
type Spec = (f64, f64, f64);

/// The values of a right arrow: head length and shaft thickness, as
/// parts of the box.
const ARROW_SPEC: &[Spec] = &[(0.4, 0.0, 1.0), (0.4, 0.02, 1.0)];
/// Head length (up to half the box) and shaft thickness.
const DOUBLE_ARROW_SPEC: &[Spec] = &[(0.3, 0.0, 0.5), (0.4, 0.02, 1.0)];
/// A callout's tail tip, anywhere around the box.
const TAIL_SPEC: &[Spec] = &[(0.15, -2.0, 3.0), (0.0, -2.0, 3.0)];

impl CommonShape {
    pub fn name(self) -> String {
        tr(self.key())
    }

    /// The shape whose name has the key `key` (as files keep it).
    pub fn from_key(key: &str) -> Option<CommonShape> {
        CommonShape::ALL.iter().copied().find(|s| s.key() == key)
    }

    /// The shapes of one set.
    pub fn of(category: Category) -> impl Iterator<Item = CommonShape> {
        CommonShape::ALL
            .iter()
            .copied()
            .filter(move |s| s.category() == category)
    }

    /// Its adjustable values: default and range of each (#4045). Values
    /// are parts of the box unless their handle says otherwise.
    fn spec(self) -> &'static [Spec] {
        use CommonShape::*;
        match self {
            // Slant of the top edge.
            Parallelogram => &[(0.25, 0.0, 0.95)],
            Data => &[(0.2, 0.0, 0.95)],
            // How far the short edge's ends come in.
            Trapezoid | Hexagon | Preparation => &[(0.25, 0.0, 0.5)],
            ManualOperation => &[(0.2, 0.0, 0.5)],
            // Where the apex stands along its edge.
            Triangle | Extract | Merge => &[(0.5, 0.0, 1.0)],
            Octagon => &[(0.2929, 0.0, 0.5)],
            // Arm thickness.
            Cross => &[(0.3, 0.02, 0.98)],
            // Width of the inner curve, as a part of the outer one's.
            Crescent => &[(0.6, 0.0, 0.95)],
            // Inner radius (part of the box).
            Ring => &[(0.3, 0.02, 0.48)],
            NoSymbol => &[(0.36, 0.05, 0.48)],
            // Height of the ellipse at the end.
            Cylinder | MagneticDisk | DirectAccessStorage => &[(0.12, 0.01, 0.5)],
            // Depth of the top and side faces.
            Cube => &[(0.25, 0.01, 0.9)],
            // Border thickness, a part of the shorter side.
            Frame => &[(0.15, 0.01, 0.49)],
            // How much the mouth smiles: 1 a smile, -1 a frown.
            Smiley => &[(1.0, -1.0, 1.0)],
            RightArrow | LeftArrow | UpArrow | DownArrow | StripedArrow => ARROW_SPEC,
            LeftRightArrow | UpDownArrow => DOUBLE_ARROW_SPEC,
            // Head length, head half width, shaft half thickness.
            QuadArrow => &[(0.2, 0.02, 0.48), (0.18, 0.01, 0.5), (0.08, 0.005, 0.25)],
            // Depth of the point.
            Chevron | Pointer => &[(0.3, 0.0, 0.99)],
            // Arrow, then depth of the notch.
            NotchedArrow => &[(0.4, 0.0, 1.0), (0.4, 0.02, 1.0), (0.15, 0.0, 0.95)],
            // Shaft thickness, head length, head half width.
            BentArrow => &[(0.3, 0.02, 0.98), (0.35, 0.02, 0.98), (0.35, 0.01, 0.5)],
            // Corner radius, a part of the shorter side.
            AlternateProcess => &[(0.15, 0.0, 0.5)],
            PredefinedProcess => &[(0.1, 0.0, 0.5)],
            // Where the two lines cross.
            InternalStorage => &[(0.15, 0.0, 1.0), (0.15, 0.0, 1.0)],
            // Height of the wavy edge.
            Document | Multidocument => &[(0.12, 0.0, 0.4)],
            // Height of the sloping top's low end.
            ManualInput => &[(0.7, 0.0, 1.0)],
            OffPageConnector => &[(0.35, 0.0, 1.0)],
            // Size of the cut corner.
            Card => &[(0.2, 0.0, 1.0)],
            // Height of the waves.
            PunchedTape => &[(0.16, 0.0, 0.5)],
            Wave => &[(0.2, 0.0, 0.5)],
            DoubleWave => &[(0.16, 0.0, 0.5)],
            // How deep the curved sides go.
            StoredData => &[(0.15, 0.0, 0.5)],
            Display => &[(0.2, 0.0, 0.8)],
            // Depth of the notches at the ends.
            Banner => &[(0.15, 0.0, 0.5)],
            // Inner radius of the points (part of the outer one).
            Burst => &[(0.82, 0.1, 1.0)],
            // Height of the flag.
            Pennant => &[(0.5, 0.05, 1.0)],
            Callout | RoundedCallout | LineCallout | BentLineCallout => TAIL_SPEC,
            OvalCallout => &[(0.12, -2.0, 3.0), (0.0, -2.0, 3.0)],
            CloudCallout => &[(0.08, -2.0, 3.0), (0.05, -2.0, 3.0)],
            Diamond | RightTriangle | Heart | Lightning | Teardrop | Process | Decision
            | Terminator | Connector | SummingJunction | Or | Collate | Sort | Delay
            | FoldedRibbon | Scroll | Explosion => &[],
        }
    }

    /// Its values as it is first drawn.
    pub fn defaults(self) -> Vec<f64> {
        self.spec().iter().map(|s| s.0).collect()
    }

    /// `values` made fit for the shape: one a value it has, each in its
    /// range (missing or broken ones take their default).
    pub fn sanitized(self, values: &[f64]) -> Vec<f64> {
        self.spec()
            .iter()
            .enumerate()
            .map(|(i, &(d, lo, hi))| match values.get(i) {
                Some(v) if v.is_finite() => v.clamp(lo, hi),
                _ => d,
            })
            .collect()
    }

    /// The shape drawn into `r`, as it is first drawn.
    pub fn path(self, r: Rect) -> BezPath {
        self.path_with(r, &self.defaults())
    }

    /// The handles of the shape drawn into `r` with `values`, in the
    /// box's own space (#4045).
    pub fn handles(self, r: Rect, values: &[f64]) -> Vec<Point> {
        let pen = Pen::new(r);
        self.unit_handles(r, &self.sanitized(values))
            .into_iter()
            .map(|h| pen.at(h.at.0, h.at.1))
            .collect()
    }

    /// The values after the handle `index` of the shape drawn into `r`
    /// with `values` is dragged to `to` (the box's own space); `None`
    /// when there is no such handle.
    pub fn dragged(self, r: Rect, values: &[f64], index: usize, to: Point) -> Option<Vec<f64>> {
        let mut v = self.sanitized(values);
        let h = *self.unit_handles(r, &v).get(index)?;
        let (w, ht) = (r.width(), r.height());
        if w.abs() < 1e-9 || ht.abs() < 1e-9 || !to.is_finite() {
            return None;
        }
        let q = ((to.x - r.x0) / w, (to.y - r.y0) / ht);
        // Solved in box millimetres, so a drag on a long box weighs both
        // ways alike.
        let d = ((q.0 - h.at.0) * w, (q.1 - h.at.1) * ht);
        let dirs: Vec<(usize, (f64, f64))> = h
            .moves
            .iter()
            .flatten()
            .map(|&(i, m)| (i, (m.0 * w, m.1 * ht)))
            .collect();
        let dot = |a: (f64, f64), b: (f64, f64)| a.0 * b.0 + a.1 * b.1;
        let steps: Vec<(usize, f64)> = match dirs.as_slice() {
            [(i, a)] => {
                let aa = dot(*a, *a);
                if aa < 1e-18 {
                    return None;
                }
                vec![(*i, dot(d, *a) / aa)]
            }
            [(i, a), (j, b)] => {
                let det = a.0 * b.1 - a.1 * b.0;
                if det.abs() > 1e-12 {
                    let x = (d.0 * b.1 - d.1 * b.0) / det;
                    let y = (a.0 * d.1 - a.1 * d.0) / det;
                    vec![(*i, x), (*j, y)]
                } else {
                    let (aa, bb) = (dot(*a, *a).max(1e-18), dot(*b, *b).max(1e-18));
                    vec![(*i, dot(d, *a) / aa), (*j, dot(d, *b) / bb)]
                }
            }
            _ => return None,
        };
        for (i, s) in steps {
            if let Some(x) = v.get_mut(i) {
                *x += s;
            }
        }
        Some(self.sanitized(&v))
    }

    /// The handles in the unit box for `v` (already sanitized), drawn
    /// into `r` (sizes kept as parts of the shorter side need its shape).
    fn unit_handles(self, r: Rect, v: &[f64]) -> Vec<Handle> {
        use CommonShape::*;
        let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
        let (w, h) = (r.width().abs().max(1e-9), r.height().abs().max(1e-9));
        let min_side = w.min(h);
        let (su, sv) = (min_side / w, min_side / h);
        let arrow = |a: f64, t: f64| Handle::two((1.0 - a, 0.5 + t / 2.0), 0, (-1.0, 0.0), 1, (0.0, 0.5));
        match self {
            Parallelogram | Data | Trapezoid | Hexagon | Preparation | Octagon | Card => {
                vec![Handle::one((g(0), 1.0), 0, (1.0, 0.0))]
            }
            ManualOperation => vec![Handle::one((g(0), 0.0), 0, (1.0, 0.0))],
            Triangle | Extract => vec![Handle::one((g(0), 1.0), 0, (1.0, 0.0))],
            Merge => vec![Handle::one((g(0), 0.0), 0, (1.0, 0.0))],
            Cross => vec![Handle::one((0.5 - g(0) / 2.0, 1.0), 0, (-0.5, 0.0))],
            Crescent => vec![Handle::one((1.0 - g(0), 0.5), 0, (-1.0, 0.0))],
            Ring | NoSymbol => vec![Handle::one((0.5 + g(0), 0.5), 0, (1.0, 0.0))],
            Cylinder | MagneticDisk => vec![Handle::one((0.5, 1.0 - 2.0 * g(0)), 0, (0.0, -2.0))],
            DirectAccessStorage => vec![Handle::one((1.0 - 2.0 * g(0), 0.5), 0, (-2.0, 0.0))],
            Cube => vec![Handle::one((g(0), 1.0), 0, (1.0, 0.0))],
            Frame => vec![Handle::one((g(0) * su, 0.5), 0, (su, 0.0))],
            Smiley => vec![Handle::one((0.5, SMILE_MID - 0.15 * g(0)), 0, (0.0, -0.15))],
            RightArrow => vec![arrow(g(0), g(1))],
            LeftArrow => vec![arrow(g(0), g(1)).mapped(|u, v| (1.0 - u, v))],
            UpArrow => vec![arrow(g(0), g(1)).mapped(|u, v| (v, u))],
            DownArrow => vec![arrow(g(0), g(1)).mapped(|u, v| (v, 1.0 - u))],
            StripedArrow => vec![arrow(g(0), g(1)).mapped(|u, v| (STRIPES + (1.0 - STRIPES) * u, v))],
            LeftRightArrow => vec![arrow(g(0), g(1))],
            UpDownArrow => vec![arrow(g(0), g(1)).mapped(|u, v| (v, u))],
            QuadArrow => vec![
                Handle::two((1.0 - g(0), 0.5 + g(1)), 0, (-1.0, 0.0), 1, (0.0, 1.0)),
                Handle::one((0.5 + g(2), 1.0 - g(0)), 2, (1.0, 0.0)),
            ],
            Chevron => vec![Handle::one((g(0), 0.5), 0, (1.0, 0.0))],
            Pointer => vec![Handle::one((1.0 - g(0), 1.0), 0, (-1.0, 0.0))],
            NotchedArrow => vec![arrow(g(0), g(1)), Handle::one((g(2), 0.5), 2, (1.0, 0.0))],
            BentArrow => vec![
                Handle::one((0.0, g(0)), 0, (0.0, 1.0)),
                Handle::two((0.5 + g(2), 1.0 - g(1)), 2, (1.0, 0.0), 1, (0.0, -1.0)),
            ],
            AlternateProcess => vec![Handle::one((g(0) * su, 1.0), 0, (su, 0.0))],
            PredefinedProcess => vec![Handle::one((g(0), 0.5), 0, (1.0, 0.0))],
            InternalStorage => vec![Handle::two((g(0), 1.0 - g(1)), 0, (1.0, 0.0), 1, (0.0, -1.0))],
            Document => vec![Handle::one((0.0, g(0)), 0, (0.0, 1.0))],
            Multidocument => vec![Handle::one((0.0, 0.8 * g(0)), 0, (0.0, 0.8))],
            ManualInput | OffPageConnector => vec![Handle::one((0.0, g(0)), 0, (0.0, 1.0))],
            PunchedTape | Wave | DoubleWave => {
                let k = self.wave_edge();
                vec![Handle::one((0.0, k * g(0)), 0, (0.0, k))]
            }
            StoredData => vec![Handle::one((g(0), 1.0), 0, (1.0, 0.0))],
            Display => vec![Handle::one((g(0), 1.0), 0, (1.0, 0.0))],
            Banner => vec![Handle::one((g(0), 0.5), 0, (1.0, 0.0))],
            Burst => {
                let a = std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU / BURST_POINTS as f64;
                let d = (0.5 * a.cos(), 0.5 * a.sin());
                vec![Handle::one((0.5 + g(0) * d.0, 0.5 + g(0) * d.1), 0, d)]
            }
            Pennant => vec![Handle::one((POLE, 1.0 - g(0)), 0, (0.0, -1.0))],
            Callout | RoundedCallout | OvalCallout | CloudCallout | LineCallout
            | BentLineCallout => vec![Handle::two((g(0), g(1)), 0, (1.0, 0.0), 1, (0.0, 1.0))],
            _ => {
                let _ = sv;
                Vec::new()
            }
        }
    }

    /// The part of the wave height the band's straight ends sit in from
    /// the box's edges.
    fn wave_edge(self) -> f64 {
        match self {
            CommonShape::DoubleWave => 0.625,
            _ => 0.75,
        }
    }

    /// The shape drawn into `r` with the values `values` (#4045).
    pub fn path_with(self, r: Rect, values: &[f64]) -> BezPath {
        use std::f64::consts::{FRAC_PI_2, PI, TAU};
        let v = self.sanitized(values);
        let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
        let mut p = Pen::new(r);
        let min_side = r.width().abs().min(r.height().abs());
        // A right arrow with head length `a` and shaft thickness `t`.
        let arrow = |a: f64, t: f64| {
            let (lo, hi) = (0.5 - t / 2.0, 0.5 + t / 2.0);
            [
                (0.0, lo),
                (1.0 - a, lo),
                (1.0 - a, 0.0),
                (1.0, 0.5),
                (1.0 - a, 1.0),
                (1.0 - a, hi),
                (0.0, hi),
            ]
        };
        let double_arrow = |a: f64, t: f64| {
            let (lo, hi) = (0.5 - t / 2.0, 0.5 + t / 2.0);
            [
                (0.0, 0.5),
                (a, 1.0),
                (a, hi),
                (1.0 - a, hi),
                (1.0 - a, 1.0),
                (1.0, 0.5),
                (1.0 - a, 0.0),
                (1.0 - a, lo),
                (a, lo),
                (a, 0.0),
            ]
        };
        match self {
            // ----- Basic shapes -----
            CommonShape::Parallelogram | CommonShape::Data => {
                let s = g(0);
                p.poly(&[(0.0, 0.0), (1.0 - s, 0.0), (1.0, 1.0), (s, 1.0)])
            }
            CommonShape::Trapezoid => {
                let i = g(0);
                p.poly(&[(0.0, 0.0), (1.0, 0.0), (1.0 - i, 1.0), (i, 1.0)])
            }
            CommonShape::Diamond | CommonShape::Decision => {
                p.poly(&[(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)])
            }
            CommonShape::Triangle | CommonShape::Extract => {
                p.poly(&[(g(0), 1.0), (1.0, 0.0), (0.0, 0.0)])
            }
            CommonShape::Merge => p.poly(&[(0.0, 1.0), (1.0, 1.0), (g(0), 0.0)]),
            CommonShape::RightTriangle => p.poly(&[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]),
            CommonShape::Hexagon => {
                let i = g(0);
                p.poly(&[
                    (i, 0.0),
                    (1.0 - i, 0.0),
                    (1.0, 0.5),
                    (1.0 - i, 1.0),
                    (i, 1.0),
                    (0.0, 0.5),
                ])
            }
            CommonShape::Octagon => {
                let c = g(0);
                p.poly(&[
                    (c, 0.0),
                    (1.0 - c, 0.0),
                    (1.0, c),
                    (1.0, 1.0 - c),
                    (1.0 - c, 1.0),
                    (c, 1.0),
                    (0.0, 1.0 - c),
                    (0.0, c),
                ])
            }
            CommonShape::Cross => {
                let (a, b) = (0.5 - g(0) / 2.0, 0.5 + g(0) / 2.0);
                p.poly(&[
                    (a, 0.0),
                    (b, 0.0),
                    (b, a),
                    (1.0, a),
                    (1.0, b),
                    (b, b),
                    (b, 1.0),
                    (a, 1.0),
                    (a, b),
                    (0.0, b),
                    (0.0, a),
                    (a, a),
                ])
            }
            CommonShape::Heart => {
                p.move_to(0.5, 0.0);
                p.curve_to((0.1, 0.35), (0.0, 0.55), (0.0, 0.72));
                p.curve_to((0.0, 0.95), (0.25, 1.05), (0.5, 0.78));
                p.curve_to((0.75, 1.05), (1.0, 0.95), (1.0, 0.72));
                p.curve_to((1.0, 0.55), (0.9, 0.35), (0.5, 0.0));
                p.close();
            }
            CommonShape::Lightning => p.poly(&[
                (0.57, 1.0),
                (0.0, 0.45),
                (0.43, 0.45),
                (0.21, 0.0),
                (1.0, 0.6),
                (0.57, 0.6),
                (0.79, 1.0),
            ]),
            CommonShape::Crescent => {
                // Out along a wide half ellipse, back along a narrower one
                // with the same ends.
                p.arc((1.0, 0.5), (1.0, 0.5), FRAC_PI_2, PI + FRAC_PI_2, true);
                p.arc((1.0, 0.5), (g(0), 0.5), PI + FRAC_PI_2, FRAC_PI_2, false);
                p.close();
            }
            CommonShape::Ring => {
                p.ellipse((0.5, 0.5), (0.5, 0.5));
                let r = g(0);
                p.hole(|p| p.ellipse((0.5, 0.5), (r, r)));
            }
            CommonShape::Cylinder | CommonShape::MagneticDisk => p.cylinder(g(0)),
            CommonShape::Cube => {
                let d = g(0);
                // Front, top and side faces, edge to edge.
                p.poly(&[
                    (0.0, 0.0),
                    (1.0 - d, 0.0),
                    (1.0 - d, 1.0 - d),
                    (0.0, 1.0 - d),
                ]);
                p.poly(&[(0.0, 1.0 - d), (1.0 - d, 1.0 - d), (1.0, 1.0), (d, 1.0)]);
                p.poly(&[(1.0 - d, 0.0), (1.0, d), (1.0, 1.0), (1.0 - d, 1.0 - d)]);
            }
            CommonShape::Frame => {
                // The same border all round, whatever the box's shape.
                let (w, h) = (r.width().abs().max(1e-9), r.height().abs().max(1e-9));
                let (tu, tv) = (g(0) * min_side / w, g(0) * min_side / h);
                p.poly(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
                p.hole(|p| p.poly(&[(tu, tv), (1.0 - tu, tv), (1.0 - tu, 1.0 - tv), (tu, 1.0 - tv)]));
            }
            CommonShape::Teardrop => {
                // Round at the bottom, the sides running straight up to
                // the point (tangent where they meet the round part).
                let a = 1.0;
                p.arc(
                    (0.5, 0.37),
                    (0.45, 0.37),
                    FRAC_PI_2 + a,
                    TAU + FRAC_PI_2 - a,
                    true,
                );
                p.line_to(0.5, 1.0);
                p.close();
            }
            CommonShape::Smiley => {
                p.ellipse((0.5, 0.5), (0.5, 0.5));
                p.hole(|p| p.ellipse((0.35, 0.65), (0.07, 0.09)));
                p.hole(|p| p.ellipse((0.65, 0.65), (0.07, 0.09)));
                // The mouth: a crescent under the middle, its height
                // scaled about its middle by how much it smiles (turned
                // over, a frown).
                let smile = g(0);
                if smile.abs() > 0.02 {
                    p.hole(|p| {
                        p.vmap = Some((SMILE_MID, smile));
                        p.arc((0.5, 0.45), (0.25, 0.2), PI + 0.35, TAU - 0.35, true);
                        p.arc((0.5, 0.38), (0.21, 0.1), -0.35, -PI + 0.35, false);
                        p.close();
                    });
                } else {
                    // Straight: a thin bar.
                    p.hole(|p| {
                        p.poly(&[(0.27, 0.39), (0.73, 0.39), (0.73, 0.41), (0.27, 0.41)])
                    });
                }
            }
            CommonShape::NoSymbol => {
                p.ellipse((0.5, 0.5), (0.5, 0.5));
                // The inner circle less the bar from top left to bottom
                // right: two openings.
                let r = g(0);
                // The bar keeps its width as the ring grows or thins.
                let a = (0.115 / r).clamp(0.0, 1.0).asin();
                for start in [-PI / 4.0, 3.0 * PI / 4.0] {
                    p.hole(|p| {
                        p.arc((0.5, 0.5), (r, r), start + a, start + PI - a, true);
                        p.close();
                    });
                }
            }
            // ----- Arrows -----
            CommonShape::RightArrow => p.poly(&arrow(g(0), g(1))),
            CommonShape::LeftArrow => p.poly(&mapped(&arrow(g(0), g(1)), |u, v| (1.0 - u, v))),
            CommonShape::UpArrow => p.poly(&mapped(&arrow(g(0), g(1)), |u, v| (v, u))),
            CommonShape::DownArrow => p.poly(&mapped(&arrow(g(0), g(1)), |u, v| (v, 1.0 - u))),
            CommonShape::LeftRightArrow => p.poly(&double_arrow(g(0), g(1))),
            CommonShape::UpDownArrow => p.poly(&mapped(&double_arrow(g(0), g(1)), |u, v| (v, u))),
            CommonShape::QuadArrow => {
                let (h, hw, s) = (g(0), g(1), g(2));
                // The arms' heads stay outside the shafts' crossing.
                let s = s.min(hw);
                let (a, b) = (1.0 - h, h);
                p.poly(&[
                    (0.5, 1.0),
                    (0.5 + hw, a),
                    (0.5 + s, a),
                    (0.5 + s, 0.5 + s),
                    (a, 0.5 + s),
                    (a, 0.5 + hw),
                    (1.0, 0.5),
                    (a, 0.5 - hw),
                    (a, 0.5 - s),
                    (0.5 + s, 0.5 - s),
                    (0.5 + s, b),
                    (0.5 + hw, b),
                    (0.5, 0.0),
                    (0.5 - hw, b),
                    (0.5 - s, b),
                    (0.5 - s, 0.5 - s),
                    (b, 0.5 - s),
                    (b, 0.5 - hw),
                    (0.0, 0.5),
                    (b, 0.5 + hw),
                    (b, 0.5 + s),
                    (0.5 - s, 0.5 + s),
                    (0.5 - s, a),
                    (0.5 - hw, a),
                ])
            }
            CommonShape::Chevron => {
                let d = g(0);
                p.poly(&[
                    (0.0, 0.0),
                    (1.0 - d, 0.0),
                    (1.0, 0.5),
                    (1.0 - d, 1.0),
                    (0.0, 1.0),
                    (d, 0.5),
                ])
            }
            CommonShape::Pointer => {
                let d = 1.0 - g(0);
                p.poly(&[(0.0, 0.0), (d, 0.0), (1.0, 0.5), (d, 1.0), (0.0, 1.0)])
            }
            CommonShape::NotchedArrow => {
                let mut pts = arrow(g(0), g(1)).to_vec();
                pts.push((g(2), 0.5));
                p.poly(&pts)
            }
            CommonShape::StripedArrow => {
                let t = g(1);
                let (lo, hi) = (0.5 - t / 2.0, 0.5 + t / 2.0);
                p.poly(&[(0.0, lo), (0.05, lo), (0.05, hi), (0.0, hi)]);
                p.poly(&[(0.1, lo), (0.18, lo), (0.18, hi), (0.1, hi)]);
                p.poly(&mapped(&arrow(g(0), t), |u, v| {
                    (STRIPES + (1.0 - STRIPES) * u, v)
                }));
            }
            CommonShape::BentArrow => {
                let (t, h, hw) = (g(0), g(1), g(2));
                let (a, b) = (0.5 - t / 2.0, 0.5 + t / 2.0);
                let hw = hw.max(t / 2.0);
                let top = 1.0 - h;
                p.poly(&[
                    (0.0, 0.0),
                    (b, 0.0),
                    (b, top),
                    (0.5 + hw, top),
                    (0.5, 1.0),
                    (0.5 - hw, top),
                    (a, top),
                    (a, t.min(top)),
                    (0.0, t.min(top)),
                ])
            }
            // ----- Flowchart -----
            CommonShape::Process => p.poly(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
            CommonShape::AlternateProcess => p.rounded(0.0, 0.0, 1.0, 1.0, min_side * g(0)),
            CommonShape::PredefinedProcess => {
                let i = g(0);
                p.poly(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
                p.line((i, 0.0), (i, 1.0));
                p.line((1.0 - i, 0.0), (1.0 - i, 1.0));
            }
            CommonShape::InternalStorage => {
                let (a, b) = (g(0), 1.0 - g(1));
                p.poly(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
                p.line((a, 0.0), (a, 1.0));
                p.line((0.0, b), (1.0, b));
            }
            CommonShape::Document => document(&mut p, 0.0, 0.0, 1.0, 1.0, g(0)),
            CommonShape::Multidocument => {
                document(&mut p, 0.0, 0.0, 0.8, 0.8, g(0));
                // The two pages behind: the strips that show.
                let w = g(0);
                p.poly(&[
                    (0.1, 0.8),
                    (0.1, 0.9),
                    (0.9, 0.9),
                    (0.9, 0.1 + 0.5 * w),
                    (0.8, 0.1 + 0.5 * w),
                    (0.8, 0.8),
                ]);
                p.poly(&[
                    (0.2, 0.9),
                    (0.2, 1.0),
                    (1.0, 1.0),
                    (1.0, 0.2 + 0.5 * w),
                    (0.9, 0.2 + 0.5 * w),
                    (0.9, 0.9),
                ]);
            }
            CommonShape::Terminator => p.rounded(0.0, 0.0, 1.0, 1.0, min_side / 2.0),
            CommonShape::Preparation => {
                let i = g(0);
                p.poly(&[
                    (0.0, 0.5),
                    (i, 1.0),
                    (1.0 - i, 1.0),
                    (1.0, 0.5),
                    (1.0 - i, 0.0),
                    (i, 0.0),
                ])
            }
            CommonShape::ManualInput => {
                p.poly(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, g(0))])
            }
            CommonShape::ManualOperation => {
                let i = g(0);
                p.poly(&[(0.0, 1.0), (1.0, 1.0), (1.0 - i, 0.0), (i, 0.0)])
            }
            CommonShape::Connector => p.ellipse((0.5, 0.5), (0.5, 0.5)),
            CommonShape::OffPageConnector => {
                let s = g(0);
                p.poly(&[(0.0, 1.0), (1.0, 1.0), (1.0, s), (0.5, 0.0), (0.0, s)])
            }
            CommonShape::Card => {
                let c = g(0);
                p.poly(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (c, 1.0), (0.0, 1.0 - c)])
            }
            CommonShape::PunchedTape | CommonShape::Wave | CommonShape::DoubleWave => {
                let (amp, k) = (g(0), self.wave_edge());
                let periods = if self == CommonShape::DoubleWave { 2 } else { 1 };
                p.wave_band(k * amp, 1.0 - k * amp, amp, periods)
            }
            CommonShape::SummingJunction => {
                p.ellipse((0.5, 0.5), (0.5, 0.5));
                let d = 0.5 * std::f64::consts::FRAC_1_SQRT_2;
                p.line((0.5 - d, 0.5 - d), (0.5 + d, 0.5 + d));
                p.line((0.5 - d, 0.5 + d), (0.5 + d, 0.5 - d));
            }
            CommonShape::Or => {
                p.ellipse((0.5, 0.5), (0.5, 0.5));
                p.line((0.0, 0.5), (1.0, 0.5));
                p.line((0.5, 0.0), (0.5, 1.0));
            }
            CommonShape::Collate => {
                p.poly(&[(0.0, 1.0), (1.0, 1.0), (0.5, 0.5)]);
                p.poly(&[(0.0, 0.0), (1.0, 0.0), (0.5, 0.5)]);
            }
            CommonShape::Sort => {
                p.poly(&[(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)]);
                p.line((0.0, 0.5), (1.0, 0.5));
            }
            CommonShape::StoredData => {
                let c = g(0);
                p.move_to(c, 1.0);
                p.line_to(1.0, 1.0);
                p.curve_to((1.0 - 0.8 * c, 0.8), (1.0 - 0.8 * c, 0.2), (1.0, 0.0));
                p.line_to(c, 0.0);
                p.curve_to((-c / 3.0, 0.2), (-c / 3.0, 0.8), (c, 1.0));
                p.close();
            }
            CommonShape::Delay => {
                p.move_to(0.0, 0.0);
                p.arc((0.5, 0.5), (0.5, 0.5), -FRAC_PI_2, FRAC_PI_2, false);
                p.line_to(0.0, 1.0);
                p.close();
            }
            CommonShape::DirectAccessStorage => p.lying_cylinder(g(0)),
            CommonShape::Display => {
                let i = g(0);
                p.move_to(0.0, 0.5);
                p.line_to(i, 1.0);
                p.line_to(0.8, 1.0);
                p.curve_to((1.07, 0.85), (1.07, 0.15), (0.8, 0.0));
                p.line_to(i, 0.0);
                p.close();
            }
            // ----- Banners -----
            CommonShape::Banner => {
                let n = g(0);
                p.poly(&[
                    (0.0, 0.0),
                    (n, 0.5),
                    (0.0, 1.0),
                    (1.0, 1.0),
                    (1.0 - n, 0.5),
                    (1.0, 0.0),
                ])
            }
            CommonShape::FoldedRibbon => {
                p.poly(&[(0.15, 0.25), (0.85, 0.25), (0.85, 1.0), (0.15, 1.0)]);
                let tail = [
                    (0.0, 0.0),
                    (0.25, 0.0),
                    (0.25, 0.25),
                    (0.15, 0.25),
                    (0.15, 0.7),
                    (0.0, 0.7),
                    (0.08, 0.35),
                ];
                p.poly(&tail);
                p.poly(&mapped(&tail, |u, v| (1.0 - u, v)));
            }
            CommonShape::Scroll => {
                p.poly(&[(0.1, 0.12), (0.9, 0.12), (0.9, 0.88), (0.1, 0.88)]);
                p.rounded(0.0, 0.04, 0.1, 0.96, min_side * 0.05);
                p.rounded(0.9, 0.04, 1.0, 0.96, min_side * 0.05);
            }
            CommonShape::Explosion => p.star(&[
                1.0, 0.55, 0.85, 0.5, 0.95, 0.6, 0.75, 0.45, 1.0, 0.55, 0.8, 0.5, 0.9, 0.45, 0.85,
                0.6,
            ]),
            CommonShape::Burst => {
                let inner = g(0);
                let radii: Vec<f64> = (0..BURST_POINTS)
                    .map(|i| if i % 2 == 0 { 1.0 } else { inner })
                    .collect();
                p.star(&radii)
            }
            CommonShape::Pennant => {
                let d = g(0);
                p.poly(&[(0.0, 0.0), (POLE, 0.0), (POLE, 1.0), (0.0, 1.0)]);
                p.poly(&[(POLE, 1.0), (1.0, 1.0 - d / 2.0), (POLE, 1.0 - d)]);
            }
            // ----- Callouts -----
            CommonShape::Callout => {
                let (tu, tv) = (g(0), g(1));
                let (a, b) = tail_root(tu, 0.0, 1.0);
                p.move_to(0.0, 0.25);
                p.line_to(0.0, 1.0);
                p.line_to(1.0, 1.0);
                p.line_to(1.0, 0.25);
                p.line_to(b, 0.25);
                p.line_to(tu, tv);
                p.line_to(a, 0.25);
                p.close();
            }
            CommonShape::RoundedCallout => {
                let (tu, tv) = (g(0), g(1));
                let (w, h) = (r.width().abs().max(1e-9), r.height().abs().max(1e-9));
                let rad = (min_side * 0.12).min(0.75 * h * 0.5);
                let (ru, rv) = (rad / w, rad / h);
                let (a, b) = tail_root(tu, ru, 1.0 - ru);
                p.arc((1.0 - ru, 0.25 + rv), (ru, rv), -FRAC_PI_2, 0.0, true);
                p.arc((1.0 - ru, 1.0 - rv), (ru, rv), 0.0, FRAC_PI_2, false);
                p.arc((ru, 1.0 - rv), (ru, rv), FRAC_PI_2, PI, false);
                p.arc((ru, 0.25 + rv), (ru, rv), PI, PI + FRAC_PI_2, false);
                p.line_to(a, 0.25);
                p.line_to(tu, tv);
                p.line_to(b, 0.25);
                p.close();
            }
            CommonShape::OvalCallout => {
                // The ellipse open where the tail leaves it, toward the
                // tip.
                let (tu, tv) = (g(0), g(1));
                let mut dir = ((tv - 0.6) / 0.4).atan2((tu - 0.5) / 0.5);
                if !dir.is_finite() {
                    dir = PI + 1.05;
                }
                let (a0, a1) = (dir - 0.2, dir + 0.2);
                p.arc((0.5, 0.6), (0.5, 0.4), a1, a0 + TAU, true);
                p.line_to(tu, tv);
                p.close();
            }
            CommonShape::CloudCallout => {
                // Bumps around an ellipse, then two small puffs toward
                // the tail's tip.
                let n = 10;
                let (cu, cv, eu, ev) = (0.55, 0.6, 0.33, 0.27);
                let pts: Vec<(f64, f64)> = (0..n)
                    .map(|i| {
                        let a = TAU * i as f64 / n as f64;
                        (cu + eu * a.cos(), cv + ev * a.sin())
                    })
                    .collect();
                p.move_to(pts[0].0, pts[0].1);
                for i in 0..n {
                    let (a, b) = (pts[i], pts[(i + 1) % n]);
                    let mid = TAU * (i as f64 + 0.5) / n as f64;
                    let (nu, nv) = (mid.cos() * 0.1, mid.sin() * 0.1);
                    p.curve_to((a.0 + nu, a.1 + nv), (b.0 + nu, b.1 + nv), b);
                }
                p.close();
                let (tu, tv) = (g(0), g(1));
                let along = |t: f64| (cu + (tu - cu) * t, cv + (tv - cv) * t);
                p.ellipse(along(0.72), (0.07, 0.06));
                p.ellipse((tu, tv), (0.04, 0.035));
            }
            CommonShape::LineCallout => {
                p.poly(&[(0.3, 0.4), (1.0, 0.4), (1.0, 1.0), (0.3, 1.0)]);
                p.line((0.3, 0.55), (g(0), g(1)));
            }
            CommonShape::BentLineCallout => {
                p.poly(&[(0.3, 0.4), (1.0, 0.4), (1.0, 1.0), (0.3, 1.0)]);
                p.line((0.3, 0.55), (0.15, 0.55));
                p.line((0.15, 0.55), (g(0), g(1)));
            }
        }
        p.finish()
    }
}

/// A curve drawn with the Common Shapes tool that is still the shape its
/// settings draw (#4045): the shape, its box and its values. `None` for
/// any other object, and for one whose outline was changed some other
/// way (its settings no longer tell how it looks).
pub fn live(s: &tracedraw_core::Shape) -> Option<(CommonShape, Rect, Vec<f64>)> {
    let params = s.common_shape.as_deref()?;
    let tracedraw_core::ShapeKind::Path { path, closed: true } = &s.kind else {
        return None;
    };
    let shape = CommonShape::from_key(&params.shape)?;
    let values = shape.sanitized(&params.values);
    let drawn = shape.path_with(params.rect, &values);
    same_outline(path, &drawn).then_some((shape, params.rect, values))
}

/// Whether two outlines are the same, element by element (within a
/// rounding error, as a file read back keeps them).
fn same_outline(a: &BezPath, b: &BezPath) -> bool {
    use tracedraw_core::geometry::PathEl;
    let close = |p: &Point, q: &Point| (p.x - q.x).abs() < 1e-6 && (p.y - q.y).abs() < 1e-6;
    a.elements().len() == b.elements().len()
        && a.elements().iter().zip(b.elements()).all(|(x, y)| match (x, y) {
            (PathEl::MoveTo(p), PathEl::MoveTo(q)) | (PathEl::LineTo(p), PathEl::LineTo(q)) => {
                close(p, q)
            }
            (PathEl::QuadTo(p1, p2), PathEl::QuadTo(q1, q2)) => close(p1, q1) && close(p2, q2),
            (PathEl::CurveTo(p1, p2, p3), PathEl::CurveTo(q1, q2, q3)) => {
                close(p1, q1) && close(p2, q2) && close(p3, q3)
            }
            (PathEl::ClosePath, PathEl::ClosePath) => true,
            _ => false,
        })
}

/// The settings a curve of `shape` drawn into `rect` with `values`
/// keeps (see [`tracedraw_core::Shape::common_shape`]).
pub fn params(shape: CommonShape, rect: Rect, values: &[f64]) -> tracedraw_core::CommonShapeParams {
    tracedraw_core::CommonShapeParams {
        shape: shape.key().to_string(),
        rect,
        values: shape.sanitized(values),
    }
}

/// The command that draws the shape `s` (see [`live`]) again with its
/// handle `index` dragged to `to` (page space); `None` when nothing
/// changes.
pub fn adjust_command(s: &tracedraw_core::Shape, index: usize, to: Point) -> Option<tracedraw_core::Command> {
    let (shape, rect, values) = live(s)?;
    let det = s.transform.determinant();
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let local = s.transform.inverse() * to;
    let next = shape.dragged(rect, &values, index, local)?;
    if next == values {
        return None;
    }
    Some(tracedraw_core::Command::SetCommonShape {
        shape: s.id,
        path: shape.path_with(rect, &next),
        params: Box::new(params(shape, rect, &next)),
    })
}

/// Where a smiley's mouth is scaled about (its middle, v).
const SMILE_MID: f64 = 0.4;
/// The part of the box a striped arrow's stripes take.
const STRIPES: f64 = 0.23;
/// The points of a burst (outer and inner).
const BURST_POINTS: usize = 48;
/// The width of a pennant's pole.
const POLE: f64 = 0.04;

/// A callout tail's two ends on the body's bottom edge (u), beside the
/// tip `tu`, kept between `lo` and `hi`.
fn tail_root(tu: f64, lo: f64, hi: f64) -> (f64, f64) {
    let width = 0.15_f64.min((hi - lo).max(0.0));
    let a = (tu + 0.05).clamp(lo, (hi - width).max(lo));
    (a, a + width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Shape as _;

    #[test]
    fn every_shape_fills_its_box() {
        let r = Rect::new(10.0, 20.0, 50.0, 40.0);
        let mut bad = Vec::new();
        for shape in CommonShape::ALL {
            let path = shape.path(r);
            assert!(!path.elements().is_empty(), "{shape:?}");
            let b = path.bounding_box();
            // Within the box (curves may round off a corner), and filling
            // most of it both ways.
            let inside = b.x0 >= r.x0 - 0.5
                && b.x1 <= r.x1 + 0.5
                && b.y0 >= r.y0 - 0.5
                && b.y1 <= r.y1 + 0.5;
            let fills = b.width() > r.width() * 0.8 && b.height() > r.height() * 0.8;
            if !(inside && fills) {
                bad.push(format!("{shape:?} {b:?}"));
            }
            // Every name is in the string table.
            assert_ne!(shape.name(), shape.key(), "{shape:?}");
        }
        assert!(bad.is_empty(), "{bad:#?}");
        for c in Category::ALL {
            assert!(CommonShape::of(c).count() >= 6, "{c:?}");
            assert_ne!(tr(c.key()), c.key());
        }
        assert_eq!(CommonShape::ALL.len(), 71);
    }

    #[test]
    fn rounded_corners_keep_their_size_in_long_boxes() {
        // A terminator in a wide box: half-circle ends as high as the box.
        let path = CommonShape::Terminator.path(Rect::new(0.0, 0.0, 100.0, 20.0));
        assert!((path.bounding_box().height() - 20.0).abs() < 1e-6);
        let area = path.area().abs();
        let expected = 80.0 * 20.0 + std::f64::consts::PI * 100.0;
        assert!((area - expected).abs() < 2.0, "{area} {expected}");
        // Holes run against the outline, so they stay open under either
        // fill rule: the ring is the outer disk less the inner one.
        let ring = CommonShape::Ring.path(Rect::new(0.0, 0.0, 10.0, 10.0));
        let expected = std::f64::consts::PI * (25.0 - 9.0);
        assert!(
            (ring.area().abs() - expected).abs() < 0.2,
            "{}",
            ring.area()
        );
        let frame = CommonShape::Frame.path(Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!((frame.area().abs() - (100.0 - 49.0)).abs() < 1e-6);
    }

    /// #4045: every handle follows a drag exactly, within its values'
    /// ranges, and the shape drawn with the new values still fills its
    /// box (callouts' tails aside, which may leave it).
    #[test]
    fn every_handle_follows_a_drag() {
        let r = Rect::new(10.0, 20.0, 70.0, 60.0);
        let mut with_handles = 0;
        for &shape in CommonShape::ALL {
            let values = shape.defaults();
            let handles = shape.handles(r, &values);
            assert_eq!(
                handles.len(),
                shape.unit_handles(r, &values).len(),
                "{shape:?}"
            );
            if !values.is_empty() {
                assert!(!handles.is_empty(), "{shape:?} has values but no handle");
            }
            with_handles += usize::from(!handles.is_empty());
            for (i, at) in handles.iter().enumerate() {
                // Dropped where it is: nothing changes.
                let same = shape.dragged(r, &values, i, *at).expect("handle");
                for (a, b) in same.iter().zip(&values) {
                    assert!((a - b).abs() < 1e-9, "{shape:?} {i}");
                }
                // A small move along each of its directions lands it
                // where it was dropped.
                for m in shape.unit_handles(r, &values)[i].moves.iter().flatten() {
                    let (vi, d) = *m;
                    // Inward from the end of its range.
                    let (_, _, hi) = shape.spec()[vi];
                    let step = if values[vi] + 0.02 > hi { -0.02 } else { 0.02 };
                    let to = Point::new(
                        at.x + d.0 * step * r.width(),
                        at.y + d.1 * step * r.height(),
                    );
                    let next = shape.dragged(r, &values, i, to).expect("handle");
                    let moved = shape.handles(r, &next)[i];
                    assert!(
                        (moved - to).hypot() < 1e-6,
                        "{shape:?} handle {i}: {moved:?} for {to:?}"
                    );
                    let b = shape.path_with(r, &next).bounding_box();
                    assert!(b.is_finite() && b.width() > 0.0, "{shape:?}");
                }
            }
            // Out of range values come back in range.
            let wild: Vec<f64> = values.iter().map(|_| 1e9).collect();
            for (v, &(_, lo, hi)) in shape.sanitized(&wild).iter().zip(shape.spec()) {
                assert!(*v >= lo && *v <= hi, "{shape:?}");
            }
            assert_eq!(shape.sanitized(&[f64::NAN; 3]), values, "{shape:?}");
        }
        assert!(with_handles >= 50, "{with_handles}");
    }

    /// The arrow's handle sets the head's length and the shaft's
    /// thickness together; a callout's tail goes where it is dragged,
    /// outside the box too.
    #[test]
    fn arrow_and_callout_handles_shape_them() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        let v = CommonShape::RightArrow.defaults();
        let h = CommonShape::RightArrow.handles(r, &v)[0];
        assert!((h - Point::new(60.0, 35.0)).hypot() < 1e-9, "{h:?}");
        let next = CommonShape::RightArrow
            .dragged(r, &v, 0, Point::new(30.0, 30.0))
            .expect("drag");
        assert!((next[0] - 0.7).abs() < 1e-9 && (next[1] - 0.2).abs() < 1e-9, "{next:?}");
        let path = CommonShape::RightArrow.path_with(r, &next);
        // Shaft 10 mm thick, head 70 mm long.
        let area = path.area().abs();
        assert!((area - (30.0 * 10.0 + 70.0 * 50.0 / 2.0)).abs() < 1e-6, "{area}");
        let tail = CommonShape::Callout
            .dragged(r, &CommonShape::Callout.defaults(), 0, Point::new(-20.0, -30.0))
            .expect("drag");
        let b = CommonShape::Callout.path_with(r, &tail).bounding_box();
        assert!((b.x0 + 20.0).abs() < 1e-9 && (b.y0 + 30.0).abs() < 1e-9, "{b:?}");
        // A wide box: the frame's border is as thick across as up.
        let frame = CommonShape::Frame.path_with(Rect::new(0.0, 0.0, 100.0, 20.0), &[0.25]);
        assert!((frame.area().abs() - (2000.0 - 90.0 * 10.0)).abs() < 1e-6);
    }

    /// The object keeps its settings while it is the shape they draw;
    /// any other outline makes it an ordinary curve.
    #[test]
    fn a_curve_is_live_while_it_is_its_shape() {
        use tracedraw_core::{Shape, ShapeId, ShapeKind};
        let r = Rect::new(0.0, 0.0, 40.0, 30.0);
        let shape = CommonShape::Trapezoid;
        let v = vec![0.4];
        let mut s = Shape::new(
            ShapeId(1),
            ShapeKind::Path {
                path: shape.path_with(r, &v),
                closed: true,
            },
        );
        assert!(live(&s).is_none(), "no settings yet");
        s.common_shape = Some(Box::new(params(shape, r, &v)));
        let (got, rect, values) = live(&s).expect("live");
        assert_eq!((got, rect, values), (shape, r, v.clone()));
        // Through the file format and back.
        let json = serde_json::to_string(&s).expect("save");
        let back: Shape = serde_json::from_str(&json).expect("open");
        assert_eq!(back.common_shape, s.common_shape);
        assert!(live(&back).is_some());
        // A node moved some other way: no longer the trapezoid.
        if let ShapeKind::Path { path, .. } = &mut s.kind {
            path.apply_affine(tracedraw_core::Affine::translate((1.0, 0.0)));
        }
        assert!(live(&s).is_none());
        // An unknown shape (a newer version's) is an ordinary curve.
        let mut other = back.clone();
        if let Some(p) = other.common_shape.as_mut() {
            p.shape = "shape.unknown".into();
        }
        assert!(live(&other).is_none());
        // The command draws it again with the handle dragged.
        let cmd = adjust_command(&back, 0, Point::new(4.0, 30.0)).expect("command");
        let tracedraw_core::Command::SetCommonShape { params: p, path, .. } = cmd else {
            panic!("{cmd:?}");
        };
        assert!((p.values[0] - 0.1).abs() < 1e-9, "{:?}", p.values);
        assert_eq!(path, shape.path_with(r, &p.values));
    }
}
