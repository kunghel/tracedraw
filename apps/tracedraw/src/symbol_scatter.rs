//! Effects > Symbol Scatter: fills the selected objects' outlines (shapes,
//! curves, text) with copies of a symbol, spread with a minimum spacing
//! so they never bunch up, each with its own size and turn from a seed.
//! The symbols are built-in shapes or objects picked from the drawing;
//! the copies take their own colours, the container's colour, random
//! document palette colours or one colour. The result is one group on
//! top of the containers, in one undo step.

use tracedraw_core::document::{Shape, ShapeKind};
use tracedraw_core::geometry::{Affine, BezPath, PathEl, Point, Shape as _};
use tracedraw_core::{Color, Command, Fill};

/// The built-in symbols, in the order of the panel's list (Mixed, past
/// the end, picks among all of them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Star,
    Heart,
    Circle,
    Sparkle,
    Flower,
    Triangle,
    Diamond,
    Drop,
}

impl Builtin {
    pub const ALL: [Builtin; 8] = [
        Builtin::Star,
        Builtin::Heart,
        Builtin::Circle,
        Builtin::Sparkle,
        Builtin::Flower,
        Builtin::Triangle,
        Builtin::Diamond,
        Builtin::Drop,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Builtin::Star => "docker.scatter_star",
            Builtin::Heart => "docker.scatter_heart",
            Builtin::Circle => "docker.scatter_circle",
            Builtin::Sparkle => "docker.scatter_sparkle",
            Builtin::Flower => "docker.scatter_flower",
            Builtin::Triangle => "docker.scatter_triangle",
            Builtin::Diamond => "docker.scatter_diamond",
            Builtin::Drop => "docker.scatter_drop",
        }
    }

    /// The symbol's outline, centred on the origin, its longer side 1,
    /// y up.
    pub fn path(self) -> BezPath {
        let polar = |n: usize, r: &dyn Fn(f64) -> f64| {
            let mut p = BezPath::new();
            for i in 0..n {
                let a = std::f64::consts::TAU * i as f64 / n as f64 + std::f64::consts::FRAC_PI_2;
                let q = Point::new(r(a) * a.cos(), r(a) * a.sin());
                if i == 0 {
                    p.move_to(q);
                } else {
                    p.line_to(q);
                }
            }
            p.close_path();
            p
        };
        let raw = match self {
            Builtin::Star => polar(10, &|a: f64| {
                let k =
                    ((a - std::f64::consts::FRAC_PI_2) / (std::f64::consts::TAU / 10.0)).round();
                if (k as i64).rem_euclid(2) == 0 {
                    0.5
                } else {
                    0.2
                }
            }),
            Builtin::Circle => polar(48, &|_| 0.5),
            Builtin::Sparkle => {
                // Four points joined by inward curves.
                let mut p = BezPath::new();
                let tips = [(0.0, 0.5), (0.5, 0.0), (0.0, -0.5), (-0.5, 0.0)];
                p.move_to(tips[0]);
                for i in 0..4 {
                    let b = tips[(i + 1) % 4];
                    p.quad_to((0.0, 0.0), b);
                }
                p.close_path();
                p
            }
            Builtin::Flower => polar(120, &|a: f64| {
                0.5 * (0.45 + 0.55 * (2.5 * (a - std::f64::consts::FRAC_PI_2)).cos().abs())
            }),
            Builtin::Triangle => polar(3, &|_| 0.5),
            Builtin::Diamond => {
                let mut p = BezPath::new();
                p.move_to((0.0, 0.5));
                p.line_to((0.32, 0.0));
                p.line_to((0.0, -0.5));
                p.line_to((-0.32, 0.0));
                p.close_path();
                p
            }
            Builtin::Heart => {
                let mut p = BezPath::new();
                p.move_to((0.0, -0.5));
                p.curve_to((-0.35, -0.2), (-0.55, 0.05), (-0.45, 0.28));
                p.curve_to((-0.36, 0.5), (-0.06, 0.52), (0.0, 0.28));
                p.curve_to((0.06, 0.52), (0.36, 0.5), (0.45, 0.28));
                p.curve_to((0.55, 0.05), (0.35, -0.2), (0.0, -0.5));
                p.close_path();
                p
            }
            Builtin::Drop => {
                let mut p = BezPath::new();
                p.move_to((0.0, 0.5));
                p.curve_to((0.12, 0.22), (0.34, 0.02), (0.34, -0.17));
                p.curve_to((0.34, -0.36), (0.19, -0.5), (0.0, -0.5));
                p.curve_to((-0.19, -0.5), (-0.34, -0.36), (-0.34, -0.17));
                p.curve_to((-0.34, 0.02), (-0.12, 0.22), (0.0, 0.5));
                p.close_path();
                p
            }
        };
        unit(&raw).unwrap_or(raw)
    }
}

/// `path` moved to the origin and scaled so its longer side is 1; None
/// when it has no area.
pub fn unit(path: &BezPath) -> Option<BezPath> {
    let b = path.bounding_box();
    let side = b.width().max(b.height());
    if !(side > 1e-9 && side.is_finite()) {
        return None;
    }
    Some(Affine::scale(1.0 / side) * Affine::translate(-b.center().to_vec2()) * path.clone())
}

/// Which symbols the copies are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    /// A built-in symbol (or all of them).
    #[default]
    Builtin,
    /// The objects picked with Use Selection, at random.
    Picked,
}

/// The copies' colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// Picked objects keep theirs; built-in symbols take the container's.
    #[default]
    Own,
    /// The fill colour of the object they fill.
    Container,
    /// A random colour of the document palette each.
    Palette,
    /// The colour chosen in the panel.
    One,
}

impl ColorMode {
    pub const ALL: [ColorMode; 4] = [
        ColorMode::Own,
        ColorMode::Container,
        ColorMode::Palette,
        ColorMode::One,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ColorMode::Own => "docker.scatter_color_own",
            ColorMode::Container => "docker.scatter_color_container",
            ColorMode::Palette => "docker.scatter_color_palette",
            ColorMode::One => "docker.scatter_color_one",
        }
    }
}

/// The most copies one Apply makes.
pub const MAX_COPIES: usize = 5000;

#[derive(Debug, Clone, PartialEq)]
pub struct ScatterSettings {
    pub source: Source,
    /// Index into [`Builtin::ALL`]; past its end, Mixed.
    pub builtin: usize,
    /// The objects Use Selection picked (their page outlines and looks).
    pub picked: Vec<Shape>,
    /// The least distance between two copies' centres, in mm.
    pub spacing: f64,
    /// The copies' size, smallest and largest, in % of the spacing.
    pub size_min: f64,
    pub size_max: f64,
    /// Random turn, up to this many degrees either way of upright (180
    /// turns freely).
    pub rotation: f64,
    /// Every copy wholly inside the outline (shrunk or left out when it
    /// does not fit); off, only its centre is.
    pub inside: bool,
    pub color: ColorMode,
    pub one_color: Color,
    pub seed: u64,
    /// Keep the objects filled; off, they are removed.
    pub keep_container: bool,
    /// The brush: a drag over the selected group changes the objects
    /// under the nib (`symbol_brush`).
    pub brush_on: bool,
    pub brush: crate::symbol_brush::SymbolBrush,
    /// The nib's radius, mm.
    pub brush_radius: f64,
    /// How much a stroke does, 1 to 100 %.
    pub brush_strength: f64,
}

impl Default for ScatterSettings {
    fn default() -> Self {
        ScatterSettings {
            source: Source::Builtin,
            builtin: 0,
            picked: Vec::new(),
            spacing: 5.0,
            size_min: 50.0,
            size_max: 90.0,
            rotation: 180.0,
            inside: true,
            color: ColorMode::Own,
            one_color: Color::BLACK,
            seed: 1,
            keep_container: true,
            brush_on: false,
            brush: crate::symbol_brush::SymbolBrush::Move,
            brush_radius: 10.0,
            brush_strength: 50.0,
        }
    }
}

impl ScatterSettings {
    /// Every setting back to its default; the symbols picked stay.
    pub fn reset(&mut self) {
        *self = Self {
            source: self.source,
            builtin: self.builtin,
            picked: std::mem::take(&mut self.picked),
            brush_on: self.brush_on,
            ..Self::default()
        };
    }
}

/// One copy: where its centre goes, its size (longer side, mm), its turn
/// (radians) and which symbol of the set it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub centre: Point,
    pub size: f64,
    pub angle: f64,
    pub symbol: usize,
}

/// A small seeded random source (splitmix64), numbers in 0..1.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn inside(boundary: &BezPath, p: Point) -> bool {
    boundary.winding(p) % 2 != 0
}

/// Points along `path` (its corners and flattened curves), at most
/// about 64, to test whether a copy fits.
fn outline_points(path: &BezPath) -> Vec<Point> {
    let mut pts = Vec::new();
    tracedraw_core::geometry::flatten(path, 0.02, &mut |el| match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => pts.push(p),
        _ => {}
    });
    if pts.len() > 64 {
        let step = pts.len() as f64 / 64.0;
        pts = (0..64).map(|i| pts[(i as f64 * step) as usize]).collect();
    }
    pts
}

/// Where the copies go inside `boundary` (even-odd: holes stay empty):
/// random points no closer than the spacing to each other, each with a
/// size and turn. `symbols` are unit outlines (longer side 1, centred).
/// Err(n) when more than [`MAX_COPIES`] would be made.
pub fn placements(
    boundary: &BezPath,
    symbols: &[BezPath],
    s: &ScatterSettings,
) -> Result<Vec<Placement>, usize> {
    let b = boundary.bounding_box();
    let spacing = if s.spacing.is_finite() {
        s.spacing.clamp(0.1, 1000.0)
    } else {
        5.0
    };
    if symbols.is_empty() || !(b.width() > 0.0 && b.height() > 0.0) {
        return Ok(Vec::new());
    }
    let estimate = b.area() / (spacing * spacing);
    if estimate > MAX_COPIES as f64 * 4.0 {
        return Err(estimate as usize);
    }
    let (lo, hi) = {
        let lo = s.size_min.clamp(1.0, 400.0);
        let hi = s.size_max.clamp(1.0, 400.0);
        (lo.min(hi) / 100.0, lo.max(hi) / 100.0)
    };
    let turn = s.rotation.clamp(0.0, 180.0).to_radians();
    let outlines: Vec<Vec<Point>> = symbols.iter().map(outline_points).collect();
    // A grid of cells no wider than spacing / sqrt(2) holds at most one
    // point each, so a check looks at the 5 x 5 cells round a point.
    let cell = spacing / std::f64::consts::SQRT_2;
    let nx = ((b.width() / cell).ceil() as usize).max(1);
    let ny = ((b.height() / cell).ceil() as usize).max(1);
    let mut grid: Vec<Option<u32>> = vec![None; nx * ny];
    let mut out: Vec<Placement> = Vec::new();
    let mut rng = Rng(s.seed ^ 0xD1B5_4A32_D192_ED03);
    let attempts = ((estimate * 30.0) as usize).clamp(200, 600_000);
    for _ in 0..attempts {
        let p = Point::new(
            b.x0 + rng.next() * b.width(),
            b.y0 + rng.next() * b.height(),
        );
        let size_r = rng.next();
        let angle = (rng.next() * 2.0 - 1.0) * turn;
        let symbol = (rng.next() * symbols.len() as f64) as usize % symbols.len();
        if !inside(boundary, p) {
            continue;
        }
        let gx = (((p.x - b.x0) / cell) as usize).min(nx - 1);
        let gy = (((p.y - b.y0) / cell) as usize).min(ny - 1);
        let mut clear = true;
        'look: for y in gy.saturating_sub(2)..(gy + 3).min(ny) {
            for x in gx.saturating_sub(2)..(gx + 3).min(nx) {
                if let Some(i) = grid[y * nx + x] {
                    if (out[i as usize].centre - p).hypot() < spacing {
                        clear = false;
                        break 'look;
                    }
                }
            }
        }
        if !clear || grid[gy * nx + gx].is_some() {
            continue;
        }
        let mut size = spacing * (lo + (hi - lo) * size_r);
        if s.inside {
            // Shrink a copy that crosses the outline, twice at most.
            let fits = |size: f64| {
                let m =
                    Affine::translate(p.to_vec2()) * Affine::rotate(angle) * Affine::scale(size);
                outlines[symbol].iter().all(|q| inside(boundary, m * *q))
            };
            let mut ok = fits(size);
            for _ in 0..2 {
                if ok {
                    break;
                }
                size *= 0.6;
                ok = fits(size);
            }
            if !ok {
                continue;
            }
        }
        if out.len() >= MAX_COPIES {
            return Err(out.len() + 1);
        }
        grid[gy * nx + gx] = Some(out.len() as u32);
        out.push(Placement {
            centre: p,
            size,
            angle,
            symbol,
        });
    }
    Ok(out)
}

/// The symbols to place: (unit outline, the object to copy when picked).
fn symbol_set(s: &ScatterSettings) -> Vec<(BezPath, Option<&Shape>)> {
    match s.source {
        Source::Picked => s
            .picked
            .iter()
            .filter_map(|sh| unit(&sh.page_path()).map(|u| (u, Some(sh))))
            .collect(),
        Source::Builtin => match Builtin::ALL.get(s.builtin) {
            Some(b) => vec![(b.path(), None)],
            None => Builtin::ALL.iter().map(|b| (b.path(), None)).collect(),
        },
    }
}

fn solid(fill: &Fill) -> Option<Color> {
    match fill {
        Fill::Solid(c) => Some(*c),
        _ => None,
    }
}

impl crate::app::App {
    /// Use Selection: the selected objects become the symbols to scatter.
    /// False when none has an outline.
    pub fn pick_scatter_symbols(&mut self) -> bool {
        let picked: Vec<Shape> = self
            .selected_shapes()
            .into_iter()
            .filter(|sh| unit(&sh.page_path()).is_some())
            .collect();
        if picked.is_empty() {
            return false;
        }
        self.symbol_scatter.picked = picked;
        self.symbol_scatter.source = Source::Picked;
        true
    }

    /// Apply Symbol Scatter to the selection: the copies, grouped, on top
    /// of it (the selection removed unless Keep objects), in one undo
    /// step. False with a status message when nothing came out.
    pub fn apply_symbol_scatter(&mut self, s: &ScatterSettings) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        let containers: Vec<Shape> = self
            .selected_shapes()
            .into_iter()
            .filter(|sh| !matches!(sh.kind, ShapeKind::Bitmap { .. }))
            .collect();
        let mut boundary = BezPath::new();
        for c in &containers {
            boundary.extend(c.page_path());
        }
        let set = symbol_set(s);
        if set.is_empty() {
            self.status = crate::i18n::tr("status.scatter_no_symbols");
            return false;
        }
        let units: Vec<BezPath> = set.iter().map(|(u, _)| u.clone()).collect();
        let places = match placements(&boundary, &units, s) {
            Ok(p) => p,
            Err(n) => {
                self.status = crate::i18n::trf("status.scatter_too_many", &[("n", &n.to_string())]);
                return false;
            }
        };
        if places.is_empty() {
            self.status = crate::i18n::tr("status.scatter_empty");
            return false;
        }
        let container_color = containers
            .iter()
            .find_map(|c| solid(&c.fill))
            .unwrap_or(Color::BLACK);
        let palette: Vec<Color> = self.palette.iter().map(|(_, c)| *c).collect();
        let mut rng = Rng(s.seed.wrapping_mul(31).wrapping_add(7));
        let mut children = Vec::with_capacity(places.len());
        for pl in &places {
            let (u, picked) = &set[pl.symbol];
            let place = Affine::translate(pl.centre.to_vec2())
                * Affine::rotate(pl.angle)
                * Affine::scale(pl.size);
            let color = match s.color {
                ColorMode::Own => None,
                ColorMode::Container => Some(container_color),
                ColorMode::Palette => palette
                    .get((rng.next() * palette.len() as f64) as usize)
                    .copied()
                    .or(Some(container_color)),
                ColorMode::One => Some(s.one_color),
            };
            let mut sh = match picked {
                Some(src) => {
                    let pb = src.page_path().bounding_box();
                    let side = pb.width().max(pb.height());
                    let t = place
                        * Affine::scale(1.0 / side)
                        * Affine::translate(-pb.center().to_vec2());
                    let engine = &mut self.engine;
                    let mut c = src.with_fresh_ids(&mut || engine.new_shape_id());
                    c.carry_fills(t, Affine::IDENTITY);
                    c.transform_by(t);
                    c
                }
                None => {
                    let mut c = Shape::new(
                        self.engine.new_shape_id(),
                        ShapeKind::Path {
                            path: place * u.clone(),
                            closed: true,
                        },
                    );
                    c.fill = Fill::Solid(container_color);
                    c.stroke = None;
                    c
                }
            };
            if let Some(color) = color {
                recolor(&mut sh, color);
            }
            children.push(sh);
        }
        let group_id = self.engine.new_shape_id();
        let mut group = Shape::new(group_id, ShapeKind::Group { children });
        group.name = Some(crate::i18n::tr("docker.symbol_scatter"));
        let mut cmds = Vec::new();
        if !s.keep_container {
            cmds.push(Command::DeleteShapes {
                shapes: containers.iter().map(|c| c.id).collect(),
            });
        }
        cmds.push(Command::AddShape {
            layer,
            shape: group,
        });
        if let Err(e) = self.engine.run_batch("Symbol Scatter", &cmds) {
            self.status = e.to_string();
            return false;
        }
        self.selection = vec![group_id];
        true
    }
}

/// Paint the object (and everything in a group) one colour: solid fills
/// and outlines take it, unfilled objects stay unfilled.
fn recolor(sh: &mut Shape, color: Color) {
    if !matches!(sh.fill, Fill::None) {
        sh.fill = Fill::Solid(color);
    }
    if let Some(st) = sh.stroke.as_mut() {
        st.color = color;
    }
    if let ShapeKind::Group { children } = &mut sh.kind {
        for c in children {
            recolor(c, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    fn square(side: f64) -> BezPath {
        Rect::new(0.0, 0.0, side, side).to_path(0.01)
    }

    #[test]
    fn builtins_are_unit_symbols() {
        for b in Builtin::ALL {
            let r = b.path().bounding_box();
            let side = r.width().max(r.height());
            assert!((side - 1.0).abs() < 1e-6, "{b:?} {r:?}");
            assert!(r.center().to_vec2().hypot() < 1e-6, "{b:?}");
            assert!(b.path().area().abs() > 0.1, "{b:?}");
        }
    }

    #[test]
    fn copies_keep_their_spacing_and_stay_inside() {
        let s = ScatterSettings::default();
        let boundary = square(50.0);
        let syms = vec![Builtin::Star.path()];
        let p = placements(&boundary, &syms, &s).unwrap();
        // About one copy per spacing squared, give or take the packing.
        assert!(p.len() > 40 && p.len() < 120, "{}", p.len());
        for (i, a) in p.iter().enumerate() {
            for b in &p[i + 1..] {
                assert!((a.centre - b.centre).hypot() >= s.spacing - 1e-9);
            }
            assert!(a.size <= s.spacing * 0.9 + 1e-9);
            let m = Affine::translate(a.centre.to_vec2())
                * Affine::rotate(a.angle)
                * Affine::scale(a.size);
            let r = (m * syms[0].clone()).bounding_box();
            assert!(r.x0 >= -1e-6 && r.y0 >= -1e-6 && r.x1 <= 50.0 + 1e-6 && r.y1 <= 50.0 + 1e-6);
        }
        // The same seed gives the same copies, another seed others.
        assert_eq!(placements(&boundary, &syms, &s).unwrap(), p);
        let other = ScatterSettings {
            seed: 2,
            ..s.clone()
        };
        assert_ne!(placements(&boundary, &syms, &other).unwrap(), p);
        // No turn keeps them upright.
        let upright = ScatterSettings {
            rotation: 0.0,
            ..s.clone()
        };
        assert!(placements(&boundary, &syms, &upright)
            .unwrap()
            .iter()
            .all(|q| q.angle == 0.0));
    }

    #[test]
    fn holes_stay_empty_and_huge_counts_are_refused() {
        // A 40 mm square with a 20 mm hole in the middle.
        let mut ring = square(40.0);
        ring.extend(Affine::translate((10.0, 10.0)) * square(20.0));
        let s = ScatterSettings {
            spacing: 2.0,
            ..Default::default()
        };
        let p = placements(&ring, &[Builtin::Circle.path()], &s).unwrap();
        assert!(p.len() > 50);
        assert!(p.iter().all(|q| !(q.centre.x > 10.0
            && q.centre.x < 30.0
            && q.centre.y > 10.0
            && q.centre.y < 30.0)));
        let tiny = ScatterSettings {
            spacing: 0.1,
            ..Default::default()
        };
        assert!(placements(&square(1000.0), &[Builtin::Circle.path()], &tiny).is_err());
    }

    #[test]
    fn applying_groups_the_copies_in_one_step() {
        let mut app = crate::app::App::headless();
        let id = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 40.0, 40.0),
                arc: None,
            })
            .expect("ellipse");
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(Color::rgb8(200, 30, 60)),
        });
        // A picked symbol: a small blue square elsewhere on the page.
        let sym = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(100.0, 100.0, 110.0, 110.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.run(Command::SetFill {
            shapes: vec![sym],
            fill: Fill::Solid(Color::rgb8(0, 0, 255)),
        });
        app.select(vec![sym]);
        assert!(app.pick_scatter_symbols());
        assert_eq!(app.symbol_scatter.source, Source::Picked);
        app.select(vec![id]);
        let depth = app.engine.history_labels().0.len();
        let s = app.symbol_scatter.clone();
        assert!(app.apply_symbol_scatter(&s));
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let g = app.selected_shapes().into_iter().next().expect("group");
        let ShapeKind::Group { children } = &g.kind else {
            panic!("not a group");
        };
        assert!(children.len() > 10, "{}", children.len());
        for c in children {
            // Picked copies keep their colour and sit inside the circle.
            assert_eq!(solid(&c.fill), Some(Color::rgb8(0, 0, 255)));
            let b = c.page_path().bounding_box();
            assert!((b.center() - Point::new(20.0, 20.0)).hypot() < 20.0);
            // A square turned 45 degrees spans its side times sqrt(2).
            let most = s.spacing * s.size_max / 100.0 * std::f64::consts::SQRT_2;
            assert!(b.width().max(b.height()) <= most + 1e-6);
        }
        assert!(app.doc().find_shape(id).is_some());
        // Built-in symbols in the container's colour, the container gone.
        let s = ScatterSettings {
            source: Source::Builtin,
            builtin: Builtin::ALL.len(),
            color: ColorMode::Container,
            keep_container: false,
            ..s
        };
        app.undo();
        app.select(vec![id]);
        assert!(app.apply_symbol_scatter(&s));
        assert!(app.doc().find_shape(id).is_none());
        let g = app.selected_shapes().into_iter().next().expect("group");
        let ShapeKind::Group { children } = &g.kind else {
            panic!("not a group");
        };
        assert!(children
            .iter()
            .all(|c| solid(&c.fill) == Some(Color::rgb8(200, 30, 60))));
        app.undo();
        assert!(app.doc().find_shape(id).is_some());
        // Nothing selected makes nothing.
        app.select(vec![]);
        assert!(!app.apply_symbol_scatter(&s));
    }

    #[test]
    fn text_is_filled_and_its_counters_stay_empty() {
        tracedraw_text::install();
        let mut app = crate::app::App::headless();
        let id = app
            .new_shape(ShapeKind::Text {
                spans: vec![tracedraw_core::TextSpan::new("O", "DejaVu Sans", 200.0)],
                origin: Point::new(10.0, 60.0),
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            })
            .expect("text");
        let outline = app.doc().find_shape(id).expect("text").page_path();
        let middle = outline.bounding_box().center();
        // The middle of the O is its counter, outside the letter.
        assert!(!inside(&outline, middle));
        app.select(vec![id]);
        let s = ScatterSettings {
            spacing: 1.5,
            ..Default::default()
        };
        assert!(app.apply_symbol_scatter(&s));
        let g = app.selected_shapes().into_iter().next().expect("group");
        let ShapeKind::Group { children } = &g.kind else {
            panic!("not a group");
        };
        assert!(children.len() > 20, "{}", children.len());
        for c in children {
            let centre = c.page_path().bounding_box().center();
            assert!(inside(&outline, centre), "{centre:?}");
        }
    }
}
