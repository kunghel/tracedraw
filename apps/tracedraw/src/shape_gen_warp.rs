//! Shape Generator deform and split, applied after any generator:
//!
//! - Deform: pins dragged in the preview pull the drawing with them, each
//!   moving the points near it by its pull, fading with distance (a soft,
//!   bell-shaped falloff of the Reach), so rings and lines bend smoothly
//!   as with a mesh warp.
//! - Split by shape: a picked outline (text, a logo, any closed shape)
//!   cuts the lines where they cross it; the parts inside take another
//!   colour and width, or only the parts inside or outside are kept. The
//!   lines stay open lines, so they can still be edited and cut.
//!
//! Model space: mm, y up.

use crate::app::App;
use crate::shape_gen::{GenKind, Generated, Paint, ShapeGenSettings};
use tracedraw_core::geometry::{BezPath, PathEl, Point, Rect, Vec2};
use tracedraw_core::shaping::{cut_open, Keep};

/// The most pins a drawing may have.
pub const MAX_PINS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitMode {
    #[default]
    Off,
    /// Inside in the second colour, outside as before.
    Colors,
    /// Only the parts inside.
    Inside,
    /// Only the parts outside.
    Outside,
}

impl SplitMode {
    pub const ALL: [SplitMode; 4] = [
        SplitMode::Off,
        SplitMode::Colors,
        SplitMode::Inside,
        SplitMode::Outside,
    ];
    pub fn key(self) -> &'static str {
        match self {
            SplitMode::Off => "shapegen.split_off",
            SplitMode::Colors => "shapegen.split_colors",
            SplitMode::Inside => "shapegen.split_inside",
            SplitMode::Outside => "shapegen.split_outside",
        }
    }
}

/// Whether deform and split apply to a generator: not the cone template
/// (made at real size), the slice grid (it cuts the selection), the
/// live growth or the soft gradient (a mesh fill).
pub fn applies(kind: GenKind) -> bool {
    !matches!(
        kind,
        GenKind::ConeTemplate | GenKind::SliceGrid | GenKind::Growth | GenKind::SoftGradient
    )
}

/// The box the pins are measured on: Width x Height from the origin.
fn pin_box(s: &ShapeGenSettings) -> Rect {
    Rect::new(0.0, 0.0, s.width, s.height)
}

/// Each pin's anchor and pull in the drawing, mm.
pub fn pins(s: &ShapeGenSettings) -> Vec<(Point, Vec2)> {
    let b = pin_box(s);
    let side = b.width().max(b.height());
    s.warp_pins
        .iter()
        .map(|p| {
            (
                Point::new(b.x0 + p[0] * b.width(), b.y0 + p[1] * b.height()),
                Vec2::new(p[2] * side, p[3] * side),
            )
        })
        .collect()
}

/// How far a point moves: each pin's pull times a bell of the distance
/// from its anchor, its width the Reach.
fn displace(pins: &[(Point, Vec2)], sigma: f64, p: Point) -> Point {
    let k = 1.0 / (2.0 * sigma * sigma);
    let mut d = Vec2::ZERO;
    for (a, pull) in pins {
        let w = (-(p - *a).hypot2() * k).exp();
        d += *pull * w;
    }
    p + d
}

/// A path flattened finely (no straight piece longer than `step`), each
/// point moved, as straight lines.
fn warp_path(path: &BezPath, pins: &[(Point, Vec2)], sigma: f64, tol: f64, step: f64) -> BezPath {
    let mut out = BezPath::new();
    let mut last: Option<Point> = None;
    let mut first: Option<Point> = None;
    let line_to = |out: &mut BezPath, from: Point, to: Point| {
        let n = ((to - from).hypot() / step).ceil().clamp(1.0, 10_000.0) as usize;
        for k in 1..=n {
            let q = from.lerp(to, k as f64 / n as f64);
            out.line_to(displace(pins, sigma, q));
        }
    };
    tracedraw_core::geometry::flatten(path, tol, &mut |el| match el {
        PathEl::MoveTo(p) => {
            out.move_to(displace(pins, sigma, p));
            last = Some(p);
            first = Some(p);
        }
        PathEl::LineTo(p) => {
            if let Some(a) = last {
                line_to(&mut out, a, p);
            }
            last = Some(p);
        }
        PathEl::ClosePath => {
            if let (Some(a), Some(f)) = (last, first) {
                if (a - f).hypot() > 1e-9 {
                    line_to(&mut out, a, f);
                }
            }
            out.close_path();
            last = first;
        }
        _ => {}
    });
    out
}

/// The picked split outline placed over the drawing: its middle on the
/// drawing's middle (where Create puts the drawing on the outline).
fn split_region(s: &ShapeGenSettings, out: &Generated) -> Option<BezPath> {
    let shape = s.split_shape.as_ref()?;
    let anchor = out.frame.or_else(|| out.bounds())?;
    let mut p = shape.clone();
    p.apply_affine(tracedraw_core::geometry::Affine::translate(
        anchor.center().to_vec2(),
    ));
    Some(p)
}

/// Deform, then split, what a generator drew.
pub(crate) fn post(s: &ShapeGenSettings, out: &mut Generated) {
    if !applies(s.kind) {
        return;
    }
    let pins = pins(s);
    if !pins.is_empty() {
        let b = pin_box(s);
        let side = b.width().max(b.height()).max(1e-9);
        let sigma = (s.warp_reach / 100.0 * side).max(side / 1000.0);
        // The drawing stays placed by its box while it bends.
        if out.frame.is_none() {
            out.frame = out.bounds();
        }
        let (tol, step) = (side / 4000.0, side / 300.0);
        for it in &mut out.items {
            it.path = warp_path(&it.path, &pins, sigma, tol, step);
        }
    }
    if s.split_mode == SplitMode::Off {
        return;
    }
    let Some(region) = split_region(s, out) else {
        return;
    };
    // Placed by the whole drawing, however much of it is kept.
    if out.frame.is_none() {
        out.frame = out.bounds();
    }
    let regions = [region];
    let items = std::mem::take(&mut out.items);
    for it in items {
        if it.paint != Paint::Line {
            out.items.push(it);
            continue;
        }
        let keep = |k: Keep| cut_open(&it.path, &regions, k);
        let parts: Vec<(BezPath, Paint)> = match s.split_mode {
            SplitMode::Off => unreachable!(),
            SplitMode::Colors => vec![
                (keep(Keep::Outside), Paint::Line),
                (keep(Keep::Inside), Paint::Accent),
            ],
            SplitMode::Inside => vec![(keep(Keep::Inside), Paint::Line)],
            SplitMode::Outside => vec![(keep(Keep::Outside), Paint::Line)],
        };
        for (path, paint) in parts {
            if !path.elements().is_empty() {
                out.push(path, false, paint);
            }
        }
    }
}

impl App {
    /// Use Selection for Split by shape: the selection's closed outlines
    /// (text as its letters, welded, holes kept), kept centred on the
    /// origin; Create then centres the drawing on the selection. False
    /// with nothing usable selected.
    pub fn shape_gen_pick_split(&mut self) -> bool {
        let Some((bounds, outline)) = self.shape_gen_selection_outline() else {
            return false;
        };
        let Some(mut outline) = outline else {
            return false;
        };
        outline.apply_affine(tracedraw_core::geometry::Affine::translate(
            -bounds.center().to_vec2(),
        ));
        let s = &mut self.shape_gen;
        s.split_shape = Some(outline);
        s.split_center = Some([bounds.center().x, bounds.center().y]);
        if s.split_mode == SplitMode::Off {
            s.split_mode = SplitMode::Colors;
        }
        true
    }

    /// Clear the split outline.
    pub fn shape_gen_clear_split(&mut self) {
        self.shape_gen.split_shape = None;
        self.shape_gen.split_center = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::generate;

    fn rings() -> ShapeGenSettings {
        ShapeGenSettings {
            kind: GenKind::Rings,
            width: 100.0,
            height: 100.0,
            rings: 20,
            ..ShapeGenSettings::default()
        }
    }

    fn square(c: f64, r: f64) -> BezPath {
        let mut p = BezPath::new();
        p.move_to(Point::new(c - r, c - r));
        p.line_to(Point::new(c + r, c - r));
        p.line_to(Point::new(c + r, c + r));
        p.line_to(Point::new(c - r, c + r));
        p.close_path();
        p
    }

    #[test]
    fn pins_pull_the_lines_near_them_and_leave_far_ones() {
        // A straight line under a pin pulling 10 mm up bends to it and
        // stays put far away.
        let mut line = BezPath::new();
        line.move_to(Point::new(0.0, 50.0));
        line.line_to(Point::new(100.0, 50.0));
        let pins = [(Point::new(50.0, 50.0), Vec2::new(0.0, 10.0))];
        let bent = warp_path(&line, &pins, 10.0, 0.01, 1.0);
        let mut pts = Vec::new();
        tracedraw_core::geometry::flatten(&bent, 0.01, &mut |el| {
            if let PathEl::LineTo(p) | PathEl::MoveTo(p) = el {
                pts.push(p);
            }
        });
        assert!(pts.len() >= 100);
        let top = pts.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        assert!((top - 60.0).abs() < 0.1, "{top}");
        assert!((pts[0].y - 50.0).abs() < 1e-3 && (pts[pts.len() - 1].y - 50.0).abs() < 1e-3);
        // Through the settings: the same rings, bent.
        let plain = generate(&rings());
        let pulled = generate(&ShapeGenSettings {
            warp_pins: vec![[0.9, 0.5, 0.0, 0.1]],
            warp_reach: 10.0,
            ..rings()
        });
        assert_eq!(plain.items.len(), pulled.items.len());
        assert_ne!(plain.items[0].path, pulled.items[0].path);
        assert!(pulled.items.iter().all(|i| i.closed));
    }

    #[test]
    fn a_shape_splits_lines_into_inside_and_outside_parts() {
        let shape = square(0.0, 35.0);
        let s = ShapeGenSettings {
            split_mode: SplitMode::Colors,
            split_shape: Some(shape),
            ..rings()
        };
        let g = generate(&s);
        let inside = g.items.iter().filter(|i| i.paint == Paint::Accent).count();
        let outside = g.items.iter().filter(|i| i.paint == Paint::Line).count();
        assert!(inside > 0 && outside > 0, "{inside} {outside}");
        assert!(g.items.iter().all(|i| !i.closed));
        // Every inside part lies in the square over the drawing's middle.
        let mid = g.frame.or_else(|| g.bounds()).unwrap().center();
        for it in g.items.iter().filter(|i| i.paint == Paint::Accent) {
            use tracedraw_core::geometry::Shape as _;
            let b = it.path.bounding_box();
            assert!(b.x0 >= mid.x - 35.5 && b.x1 <= mid.x + 35.5, "{b:?}");
        }
        // Inside only keeps just those parts, as plain lines.
        let only = generate(&ShapeGenSettings {
            split_mode: SplitMode::Inside,
            ..s.clone()
        });
        assert_eq!(only.items.len(), inside);
        assert!(only.items.iter().all(|i| i.paint == Paint::Line));
    }

    #[test]
    fn picking_a_shape_centres_the_drawing_on_it_with_the_second_colour() {
        let mut app = App::headless();
        let id = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: Rect::new(200.0, 100.0, 300.0, 180.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        app.shape_gen = rings();
        assert!(app.shape_gen_pick_split());
        assert_eq!(app.shape_gen.split_mode, SplitMode::Colors);
        app.shape_gen.split_color = [10, 20, 30];
        app.shape_gen.split_width = 0.8;
        let gid = app.shape_gen_create().expect("made");
        let g = app.doc().find_shape(gid).expect("group").clone();
        let b = g.bounds();
        assert!((b.center().x - 250.0).abs() < 1.0 && (b.center().y - 140.0).abs() < 1.0);
        let tracedraw_core::ShapeKind::Group { children } = &g.kind else {
            panic!("not a group");
        };
        let accent = children
            .iter()
            .filter_map(|c| c.stroke.as_ref())
            .find(|st| st.color == tracedraw_core::Color::rgb8(10, 20, 30))
            .expect("inside lines in the second colour");
        assert!((accent.width - 0.8).abs() < 1e-9);
        app.shape_gen_clear_split();
        assert!(app.shape_gen.split_shape.is_none());
    }
}
