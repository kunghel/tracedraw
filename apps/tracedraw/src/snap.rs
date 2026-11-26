//! Snapping to the document grid, the baseline grid, pixels, guidelines,
//! the page and objects. Objects and the page offer points by the
//! snapping modes (see `snap_points`); a point wins over guidelines and
//! the grids, which win over edges and text baselines.

use crate::app::App;
use crate::live_guides;
use crate::snap_points::{self, Geo, Target};
use tracedraw_core::{
    document::GuideLine,
    geometry::{Affine, Point, Rect, Vec2},
    Shape, ShapeId, ShapeKind,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapSettings {
    pub grid: bool,
    pub guides: bool,
    pub objects: bool,
    pub page: bool,
    pub pixels: bool,
    pub baseline_grid: bool,
    pub alignment_guides: bool,
    pub dynamic_guides: bool,
    /// Alt+Q: all snapping off.
    pub off: bool,
}

impl Default for SnapSettings {
    /// A first start: guidelines, the page and objects snap. Objects
    /// were off at first (#1814) because a moved box was pulled sideways
    /// onto the edges of objects anywhere on the page; now only the
    /// objects it faces attract it (see [`App::snap_candidates_near`]),
    /// so a move still goes where the pointer puts it away from them.
    /// Alt+Z, the Snap To list and Options > Snapping turn objects off,
    /// and that is kept.
    fn default() -> Self {
        SnapSettings {
            grid: false,
            guides: true,
            objects: true,
            page: true,
            pixels: false,
            baseline_grid: false,
            alignment_guides: false,
            dynamic_guides: false,
            off: false,
        }
    }
}

/// At most this many acquired anchors of dynamic guides.
const MAX_ANCHORS: usize = 4;
/// At most this many object boxes for alignment guides.
const MAX_BOXES: usize = 20_000;

/// What the alignment targets were built from.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignKey {
    doc: usize,
    revision: u64,
    page: tracedraw_core::PageId,
    exclude: Vec<ShapeId>,
    view: Option<[f64; 4]>,
    multipage: bool,
    options: live_guides::AlignOptions,
}

/// What stays put of the curves whose nodes are being dragged, on the
/// page (#673): their other nodes and the segments between two of them,
/// in pieces of at most [`STILL_PIECE`] segments (or nodes not on such a
/// segment), each with its bounds. Built once a drag; a move looks only
/// at the pieces near the pointer, so a long curve costs no more a frame
/// than a short one.
#[derive(Debug, Clone, Default)]
pub struct StillGeometry(Vec<(Rect, Geo)>);

/// Segments (or lone nodes) in one piece of [`StillGeometry`].
const STILL_PIECE: usize = 16;

impl StillGeometry {
    pub fn of(start: &[crate::node_edit::StartCurve]) -> Self {
        use tracedraw_core::geometry::{BezPath, PathEl, Shape as _};
        let mut out = Vec::new();
        for c in start {
            let moving: std::collections::HashSet<usize> = c.indices.iter().copied().collect();
            let path = c.transform
                * tracedraw_core::nodes::segments_where(&c.path, |i| !moving.contains(&i));
            // The segments in pieces, each starting where the last ended.
            let mut ends: std::collections::HashSet<(i64, i64)> = Default::default();
            let key = |p: Point| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64);
            let mut piece = BezPath::new();
            let mut count = 0;
            let mut last = Point::ZERO;
            let mut flush = |piece: &mut BezPath, count: &mut usize, at: Point| {
                if *count > 0 {
                    let done = std::mem::take(piece);
                    out.push((
                        done.bounding_box(),
                        Geo::Outline {
                            path: done,
                            points: true,
                        },
                    ));
                }
                *count = 0;
                piece.move_to(at);
            };
            for el in path.elements() {
                match *el {
                    PathEl::MoveTo(p) => {
                        flush(&mut piece, &mut count, p);
                        last = p;
                    }
                    PathEl::ClosePath => {}
                    el => {
                        if count == STILL_PIECE {
                            flush(&mut piece, &mut count, last);
                        }
                        ends.insert(key(last));
                        piece.push(el);
                        count += 1;
                        if let Some(p) = el.end_point() {
                            ends.insert(key(p));
                            last = p;
                        }
                    }
                }
            }
            flush(&mut piece, &mut count, last);
            // Nodes on no still segment, in pieces of their own.
            let lone: Vec<Point> = tracedraw_core::nodes::nodes(&c.path)
                .iter()
                .filter(|n| !moving.contains(&n.index))
                .map(|n| c.transform * n.pos)
                .filter(|p| !ends.contains(&key(*p)))
                .collect();
            for chunk in lone.chunks(STILL_PIECE) {
                let b = chunk
                    .iter()
                    .fold(Rect::from_points(chunk[0], chunk[0]), |r, q| r.union_pt(*q));
                out.push((b, Geo::Nodes(chunk.to_vec())));
            }
        }
        StillGeometry(out)
    }

    /// The snapping geometry of `shapes` where they stand, outlines in
    /// pieces of at most [`STILL_PIECE`] segments: what a move snaps to of
    /// where the selection started (#1765). Built once a move, so a long
    /// curve costs a frame no more than a short one.
    pub fn of_shapes(shapes: &[Shape]) -> Self {
        use tracedraw_core::geometry::{BezPath, PathEl, Shape as _};
        let mut g = Vec::new();
        for s in shapes {
            snap_points::object_geometry(s, &text_baselines, &mut g);
        }
        let mut out = Vec::new();
        for geo in g {
            let (path, points) = match geo {
                Geo::Outline { path, points } => (path, points),
                Geo::Nodes(ps) => {
                    for chunk in ps.chunks(STILL_PIECE) {
                        let b = chunk
                            .iter()
                            .fold(None::<Rect>, |r, q| {
                                Some(r.map_or(Rect::from_points(*q, *q), |r| r.union_pt(*q)))
                            })
                            .unwrap_or(Rect::ZERO);
                        out.push((b, Geo::Nodes(chunk.to_vec())));
                    }
                    continue;
                }
                e @ Geo::Ellipse { center, rx, ry, .. } => {
                    let r = rx.hypot() + ry.hypot();
                    out.push((Rect::from_points(center, center).inflate(r, r), e));
                    continue;
                }
                Geo::Center(c) => {
                    out.push((Rect::from_points(c, c), Geo::Center(c)));
                    continue;
                }
                Geo::Baseline(a, b) => {
                    out.push((Rect::from_points(a, b), Geo::Baseline(a, b)));
                    continue;
                }
            };
            // The outline in pieces, each starting where the last ended;
            // a closed subpath's closing side is spelled out.
            let mut piece = BezPath::new();
            let mut count = 0;
            let mut first = Point::ZERO;
            let mut last = Point::ZERO;
            let mut flush = |piece: &mut BezPath, count: &mut usize, at: Point| {
                if *count > 0 {
                    let done = std::mem::take(piece);
                    out.push((done.bounding_box(), Geo::Outline { path: done, points }));
                }
                *count = 0;
                piece.move_to(at);
            };
            for el in path.elements() {
                let el = match *el {
                    PathEl::MoveTo(p) => {
                        flush(&mut piece, &mut count, p);
                        first = p;
                        last = p;
                        continue;
                    }
                    PathEl::ClosePath if last != first => PathEl::LineTo(first),
                    PathEl::ClosePath => continue,
                    el => el,
                };
                if count == STILL_PIECE {
                    flush(&mut piece, &mut count, last);
                }
                piece.push(el);
                count += 1;
                if let Some(p) = el.end_point() {
                    last = p;
                }
            }
            flush(&mut piece, &mut count, last);
        }
        StillGeometry(out)
    }
}

/// The alignment targets of the last frame and what they were built from.
#[derive(Debug, Clone)]
pub struct AlignCache {
    key: AlignKey,
    targets: std::rc::Rc<live_guides::AlignTargets>,
}

impl App {
    /// The snapping radius (Options > Snapping, screen pixels) in mm.
    pub(crate) fn snap_tolerance(&self) -> f64 {
        self.settings.snap.threshold_px.clamp(1.0, 100.0) / self.view.zoom.max(1e-6) as f64
    }

    /// The pieces of `still` within the snapping radius of `p`: what a
    /// node dragged there snaps to of its own curves.
    pub(crate) fn still_near<'a>(&self, still: &'a StillGeometry, p: Point) -> Vec<&'a Geo> {
        if self.snaps_off() || !self.snaps_to_objects() || !self.snap_enabled() {
            return Vec::new();
        }
        let tol = self.snap_tolerance();
        still
            .0
            .iter()
            .filter(|(b, _)| b.inflate(tol, tol).contains(p))
            .map(|(_, g)| g)
            .collect()
    }

    /// Candidate x and y values to snap to; `bounds` adds the page's and
    /// the objects' edges and centres.
    fn snap_candidates(
        &self,
        exclude: &[ShapeId],
        guides: bool,
        bounds: bool,
    ) -> (Vec<f64>, Vec<f64>) {
        self.snap_candidates_near(exclude, guides, bounds, None)
    }

    /// [`App::snap_candidates`] for a box being moved to `near`: an
    /// object's box edges and centre attract it only where the two boxes
    /// face each other, its x values where their heights overlap (or
    /// come within the snapping radius) and its y values where their
    /// widths do. An object far above or below the moved one no longer
    /// pulls it sideways just because one of its edges is level with
    /// the moved box somewhere on the page; lining up from afar is what
    /// alignment guides are for.
    fn snap_candidates_near(
        &self,
        exclude: &[ShapeId],
        guides: bool,
        bounds: bool,
        near: Option<Rect>,
    ) -> (Vec<f64>, Vec<f64>) {
        let tol = self.snap_tolerance();
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        let s = self.snap;
        let clean_sheet = self.symbol_edit_index().is_some();
        let page = self.page_rect();
        if !clean_sheet && s.page && bounds {
            xs.extend([page.x0, page.x1, page.center().x]);
            ys.extend([page.y0, page.y1, page.center().y]);
        }
        if !clean_sheet && s.baseline_grid {
            ys.extend(self.baseline_ys());
        }
        if !clean_sheet && s.guides && guides {
            // The page's own guidelines and the master page's (#1928).
            if let Ok(guides) = self.doc().guides_on_page(self.page) {
                for g in &guides {
                    match g.line {
                        GuideLine::Horizontal { y } => ys.push(y),
                        GuideLine::Vertical { x } => xs.push(x),
                        GuideLine::Angled { .. } => {}
                    }
                }
            }
        }
        if s.objects && bounds {
            let big: Option<std::collections::HashSet<ShapeId>> =
                (exclude.len() > 16).then(|| exclude.iter().copied().collect());
            let excluded = |id: &ShapeId| match &big {
                Some(s) => s.contains(id),
                None => exclude.contains(id),
            };
            for sh in self
                .page_layers()
                .into_iter()
                .filter(|l| l.visible)
                .flat_map(|l| &l.shapes)
            {
                if excluded(&sh.id) || (clean_sheet && self.outside_clip_edit(sh.id)) {
                    continue;
                }
                let b = self.snap_bounds(sh);
                let faces = |lo: f64, hi: f64, a: f64, z: f64| lo <= z + tol && a <= hi + tol;
                if near.is_none_or(|m| faces(b.y0, b.y1, m.y0, m.y1)) {
                    xs.extend([b.x0, b.x1, b.center().x]);
                }
                if near.is_none_or(|m| faces(b.x0, b.x1, m.x0, m.x1)) {
                    ys.extend([b.y0, b.y1, b.center().y]);
                }
                // Its straight upright and level sides inside the box
                // too: the inner edges of a frame or of a shape with a
                // hole hold a moved box as its outer edges do (#3970).
                if matches!(sh.kind, ShapeKind::Path { .. }) {
                    inner_sides(sh, b, &mut |upright, at, lo, hi| {
                        if upright {
                            if near.is_none_or(|m| faces(lo, hi, m.y0, m.y1)) {
                                xs.push(at);
                            }
                        } else if near.is_none_or(|m| faces(lo, hi, m.x0, m.x1)) {
                            ys.push(at);
                        }
                    });
                }
            }
        }
        (xs, ys)
    }

    /// Baselines of the baseline grid, from the first one under the page
    /// top downwards (page coordinates, Y up).
    pub fn baseline_ys(&self) -> Vec<f64> {
        let g = self.doc().metadata.grid;
        let page = self.page_rect();
        let step = g.baseline_spacing.max(0.1);
        let mut out = Vec::new();
        let mut y = page.y1 - g.baseline_start.max(0.0);
        while y >= page.y0 - 1e-9 && out.len() < 100_000 {
            out.push(y);
            y -= step;
        }
        out
    }

    fn snap_axis(&self, v: f64, candidates: &[f64], horizontal: bool) -> Option<f64> {
        let tol = self.snap_tolerance();
        let mut best: Option<(f64, f64)> = None;
        for c in candidates {
            let d = (c - v).abs();
            if d <= tol && best.map(|b| d < b.1).unwrap_or(true) {
                best = Some((*c, d));
            }
        }
        if best.is_none() && self.snap.grid {
            // Grid lines pass through the grid origin (#2990); their
            // subdivisions and gutters count too (#4395). An angled grid
            // is snapped to point by point instead.
            if let Some(g) = self.grid_geom().snap_axis_value(v, horizontal, tol) {
                return Some(g);
            }
        }
        if best.is_none() && self.snap.pixels {
            // Whole pixels of the document resolution from the page corner.
            let step = 25.4 / self.document_dpi().max(1.0);
            let g = (v / step).round() * step;
            if (g - v).abs() <= tol {
                return Some(g);
            }
        }
        best.map(|b| b.0)
    }

    /// Snap an x value (a vertical guideline being dragged) to everything
    /// but guidelines.
    pub fn snap_x_without_guides(&self, x: f64) -> f64 {
        self.snap_guide_axis(x, true, None)
    }

    /// [`App::snap_x_without_guides`] for a guideline dragged with the
    /// pointer at `y` (or a horizontal one, [`App::snap_y_without_guides`],
    /// with the pointer at `x`): the objects nearest the pointer along
    /// the guideline come first (#3359).
    pub fn snap_guide_near(&self, v: f64, x_axis: bool, pointer: f64) -> f64 {
        self.snap_guide_axis(v, x_axis, Some(pointer))
    }

    /// Snap a y value (a horizontal guideline being dragged) to everything
    /// but guidelines.
    pub fn snap_y_without_guides(&self, y: f64) -> f64 {
        self.snap_guide_axis(y, false, None)
    }

    /// A guideline being dragged at `v` (an x with `x_axis`, else a y):
    /// onto the objects' points and the places where it touches their
    /// outlines (marked as the pointer's snaps are, #2004), else the
    /// page's and the objects' boxes, the grid or the pixels.
    fn snap_guide_axis(&self, v: f64, x_axis: bool, pointer: Option<f64>) -> f64 {
        if !self.snap_enabled() {
            return v;
        }
        if let Some((c, t)) = self.axis_stop_near(v, x_axis, None, &[], pointer) {
            self.snap_mark.set(Some(t));
            return c;
        }
        let (xs, ys) = self.snap_candidates(&[], false, true);
        let c = if x_axis { &xs } else { &ys };
        self.snap_axis(v, c, x_axis).unwrap_or(v)
    }

    /// The objects' stop nearest `v` along one axis within the snapping
    /// radius, with the point it marks: for a line along the other axis
    /// at `v` (an x with `x_axis`), a node, midpoint, quadrant or centre
    /// (by the modes on) it lines up with, a place where it touches an
    /// outline, or a straight side it lies on. A guideline dragged near
    /// a node lands on it, not only on the objects' boxes (#1645); a
    /// side of a box being resized stops on the points and lines it
    /// meets along `span`, its stretch of the other axis (#2004).
    /// Objects in `exclude` (the selection being resized) do not count.
    pub(crate) fn axis_stop(
        &self,
        v: f64,
        x_axis: bool,
        span: Option<(f64, f64)>,
        exclude: &[ShapeId],
    ) -> Option<(f64, Target)> {
        self.axis_stop_near(v, x_axis, span, exclude, None)
    }

    /// [`App::axis_stop`] with the pointer at `pointer` along the line
    /// (a y for a vertical line, `x_axis`): the stops of the objects the
    /// pointer is over, or nearest to, win over those of objects farther
    /// along the line, however close those line up. Dragging a guideline
    /// onto a rectangle among others about level with it snapped it to
    /// one far from the pointer (#3359).
    fn axis_stop_near(
        &self,
        v: f64,
        x_axis: bool,
        span: Option<(f64, f64)>,
        exclude: &[ShapeId],
        pointer: Option<f64>,
    ) -> Option<(f64, Target)> {
        if !self.snaps_to_objects() || !v.is_finite() {
            return None;
        }
        let tol = self.snap_tolerance();
        let mut g = Vec::new();
        // Each object's stops and how far the pointer is from the object
        // along the line (0 over it or within the snapping radius).
        let mut stops: Vec<(f64, (f64, Point, snap_points::SnapMode))> = Vec::new();
        let clean_sheet = self.symbol_edit_index().is_some();
        for sh in self
            .page_layers()
            .into_iter()
            .filter(|l| l.visible)
            .flat_map(|l| &l.shapes)
        {
            if !sh.visible
                || exclude.contains(&sh.id)
                || (clean_sheet && self.outside_clip_edit(sh.id))
            {
                continue;
            }
            // Only the objects whose box comes near the line (and, for a
            // side, near its stretch).
            let b = self.snap_bounds(sh);
            let ((lo, hi), (o0, o1)) = if x_axis {
                ((b.x0, b.x1), (b.y0, b.y1))
            } else {
                ((b.y0, b.y1), (b.x0, b.x1))
            };
            if v < lo - tol || v > hi + tol {
                continue;
            }
            if let Some((a, z)) = span {
                if o1 < a.min(z) - tol || o0 > a.max(z) + tol {
                    continue;
                }
            }
            let away = pointer.map_or(0.0, |q| {
                let d = if q < o0 { o0 - q } else if q > o1 { q - o1 } else { 0.0 };
                (d - tol).max(0.0)
            });
            g.clear();
            snap_points::object_geometry(&self.snap_shape(sh), &text_baselines, &mut g);
            stops.extend(
                snap_points::axis_stops(&g, &self.settings.snap.modes, x_axis, span, tol)
                    .into_iter()
                    .map(|s| (away, s)),
            );
        }
        stops
            .into_iter()
            .filter(|(_, (c, _, _))| (c - v).abs() <= tol)
            .min_by(|(wa, a), (wb, b)| {
                wa.total_cmp(wb)
                    .then((a.0 - v).abs().total_cmp(&(b.0 - v).abs()))
            })
            .map(|(_, (c, point, mode))| {
                (
                    c,
                    Target {
                        point,
                        mode,
                        edge: None,
                    },
                )
            })
    }

    /// Snap a point to everything but guidelines.
    pub fn snap_point_without_guides(&self, p: Point) -> Point {
        Point::new(
            self.snap_x_without_guides(p.x),
            self.snap_y_without_guides(p.y),
        )
    }

    /// Where a line being drawn starts: the last node of the curve in
    /// progress or the last point a dimension or callout tool placed
    /// (tangent and perpendicular points are seen from there).
    pub fn snap_from(&self) -> Option<Point> {
        self.curve
            .as_ref()
            .and_then(|c| c.nodes.last().map(|n| n.0))
            .or_else(|| self.dimension_points.last().copied())
    }

    /// Whether the active tool is one of the dimension tools.
    pub fn is_dimension_tool(&self) -> bool {
        use crate::tools::Tool;
        matches!(
            self.tool,
            Tool::ParallelDimension
                | Tool::HorizontalVerticalDimension
                | Tool::SegmentDimension
                | Tool::AngularDimension
        )
    }

    /// Objects are snapped to: Snap To Objects is on, or a dimension tool
    /// is measuring, which always takes the objects' ends and edges so a
    /// dimension starts and ends on them (live test of 0.41).
    pub fn snaps_to_objects(&self) -> bool {
        self.snap.objects || self.is_dimension_tool()
    }

    /// Snapping geometry of the page and of the objects whose bounds come
    /// within `tol` of `p`, leaving out `exclude`.
    fn snap_geometry(&self, p: Point, tol: f64, exclude: &[ShapeId]) -> Vec<Geo> {
        let mut g = Vec::new();
        if self.snap.page {
            snap_points::page_geometry(self.page_rect(), &mut g);
        }
        if self.snaps_to_objects() {
            // A drag can carry a whole large drawing's ids: a Vec's
            // contains made every ask here quadratic in it (#3559).
            let big: Option<std::collections::HashSet<ShapeId>> =
                (exclude.len() > 16).then(|| exclude.iter().copied().collect());
            let excluded = |id: &ShapeId| match &big {
                Some(s) => s.contains(id),
                None => exclude.contains(id),
            };
            let clean_sheet = self.symbol_edit_index().is_some();
            for sh in self
                .page_layers()
                .into_iter()
                .filter(|l| l.visible)
                .flat_map(|l| &l.shapes)
            {
                if excluded(&sh.id) || !sh.visible || (clean_sheet && self.outside_clip_edit(sh.id))
                {
                    continue;
                }
                if !self.snap_bounds(sh).inflate(tol, tol).contains(p) {
                    continue;
                }
                snap_points::object_geometry(&self.snap_shape(sh), &text_baselines, &mut g);
            }
        }
        g
    }

    /// The object or page point nearest `p` within the snapping radius:
    /// points (`points`), else edges and baselines (`lines`).
    fn object_snap(
        &self,
        p: Point,
        exclude: &[ShapeId],
        own: &[&Geo],
        points: bool,
        lines: bool,
    ) -> Option<Target> {
        let tol = self.snap_tolerance();
        let mut geo = self.snap_geometry(p, tol, exclude);
        // What stays put of the curves being edited (#673): only its few
        // pieces near the pointer, each of at most 16 segments.
        geo.extend(own.iter().map(|g| (*g).clone()));
        let found = snap_points::targets(&geo, p, tol, self.snap_from(), &self.settings.snap.modes);
        snap_points::best(&found, points, lines)
    }

    /// The selection's own snap point under `p` (a node, midpoint,
    /// quadrant or centre, else the point of an edge under it): the one
    /// the pointer marks, which a move carries onto other objects' points
    /// and edges.
    pub fn selection_snap_source(&self, p: Point) -> Option<Point> {
        self.selection_hover_target(p).map(|t| t.point)
    }

    /// The selection's own snap point under `p`, or its edge when no
    /// point is near: what the Pick tool marks, with its name, while the
    /// pointer rests over the selection.
    pub fn selection_hover_target(&self, p: Point) -> Option<Target> {
        if !self.snap_enabled() || !self.snaps_to_objects() {
            return None;
        }
        let tol = self.snap_tolerance();
        let mut g = Vec::new();
        let shapes = self.selected_rc();
        let bounds = self.selected_bounds();
        for (s, b) in shapes.iter().zip(bounds.iter()) {
            // Only the objects whose bounds come near the pointer.
            if !b.inflate(tol, tol).contains(p) {
                continue;
            }
            snap_points::object_geometry(&self.snap_shape(s), &text_baselines, &mut g);
        }
        // Several objects: the middle of them all too, so a move can
        // carry the selection by its centre (#2004).
        if bounds.len() > 1 {
            if let Some(all) = bounds.iter().copied().reduce(|a, b| a.union(b)) {
                if all.inflate(tol, tol).contains(p) {
                    g.push(Geo::Center(all.center()));
                }
            }
        }
        let found = snap_points::targets(&g, p, tol, None, &self.settings.snap.modes);
        snap_points::best(&found, true, true)
    }

    /// Where the selection stands, as a move snaps to it once it has
    /// left its start (#1765): the document keeps the selection there
    /// until the move ends.
    pub fn selection_start_geometry(&self) -> StillGeometry {
        let shapes = self.selected_rc();
        if !shapes
            .iter()
            .any(|s| matches!(s.kind, ShapeKind::SymbolInstance { .. }))
        {
            return StillGeometry::of_shapes(&shapes);
        }
        let shapes: Vec<Shape> = shapes
            .iter()
            .map(|s| self.snap_shape(s).into_owned())
            .collect();
        StillGeometry::of_shapes(&shapes)
    }

    /// `s` (a top-level object) as snapping reads it: a symbol instance
    /// as a group of its symbol's objects where the instance shows them
    /// (#2135). An instance alone knows neither its symbol's outline nor
    /// its box (a 10 x 10 mm box at its origin stands for both), so a
    /// symbol offered no node, edge or midpoint to snap to or to be
    /// carried by, only the middle of that box by its lower left corner.
    /// Every other object is read as it is.
    pub(crate) fn snap_shape<'a>(&self, s: &'a Shape) -> std::borrow::Cow<'a, Shape> {
        if !matches!(s.kind, ShapeKind::SymbolInstance { .. }) {
            return std::borrow::Cow::Borrowed(s);
        }
        let mut group = Shape::new(
            s.id,
            ShapeKind::Group {
                children: s.expand(&self.doc().symbols),
            },
        );
        group.transform = s.transform;
        group.visible = s.visible;
        std::borrow::Cow::Owned(group)
    }

    /// The box of `s` (an object of the drawing) for snapping: a symbol
    /// instance's is its symbol's, placed by the instance (see
    /// [`Self::snap_shape`]). It is asked for every object of the page on
    /// every pointer move, so the box is measured once per change of the
    /// drawing and kept with the other measured boxes (#3559).
    fn snap_bounds(&self, s: &Shape) -> Rect {
        if let Some(b) = self.plain_bounds_cached(s) {
            return b;
        }
        if !matches!(s.kind, ShapeKind::SymbolInstance { .. }) {
            return s.bounds();
        }
        let revision = self.engine.revision();
        let Ok(mut cache) = self.visual_cache.try_borrow_mut() else {
            return self.visual_bounds(s);
        };
        if cache.0 != revision {
            cache.0 = revision;
            cache.1.clear();
        }
        if let Some(b) = cache.1.get(&s.id) {
            return *b;
        }
        // An instance's box comes straight from its symbol: nothing here
        // asks for the cache again.
        let b = self.visual_bounds(s);
        cache.1.insert(s.id, b);
        b
    }

    /// Snap a pointer position (drawing tools, text placement).
    pub fn snap_point(&self, p: Point) -> Point {
        self.snap_point_excluding(p, &[])
    }

    /// Snap a pointer position to everything but `exclude` (a handle of
    /// the selection snaps to other objects, not to the selection's own
    /// points).
    pub fn snap_point_excluding(&self, p: Point, exclude: &[ShapeId]) -> Point {
        self.snap_point_with(p, exclude, &[])
    }

    /// Snap the pointer turning the selection: onto a point of another
    /// object or of the page (a node, a midpoint, a centre) only, so that
    /// the turn aims at it. Lines are not taken (page edges and centre
    /// lines, guidelines, grid lines, the bounds of other objects): the
    /// turning handle stands off the object, so lining it up with a line
    /// means nothing, and the turn stuck behind the pointer while it
    /// crossed one, anywhere on the canvas (#933).
    pub fn snap_turn_point(&self, p: Point, exclude: &[ShapeId]) -> Point {
        if !self.snap_enabled() {
            return p;
        }
        match self.object_snap(p, exclude, &[], true, false) {
            Some(t) => {
                self.snap_mark.set(Some(t));
                t.point
            }
            None => p,
        }
    }

    /// Snap the selection's own point `grab` turned about `center` by the
    /// pointer at `p` (rotate mode, a node dragged, #933): the point goes
    /// round its circle towards the pointer; near another object's point
    /// the turn aims at that point, and near an edge the point lands
    /// where its circle crosses the edge, so a node turned onto a line
    /// ends on the line; a guideline takes it the same way (#3056).
    /// Returns where the turn aims (only its direction from `center`
    /// counts).
    pub fn snap_turn_grab(
        &self,
        center: Point,
        grab: Point,
        p: Point,
        exclude: &[ShapeId],
    ) -> Point {
        let r = (grab - center).hypot();
        let d = p - center;
        let len = d.hypot();
        if !(r > 1e-9 && len > 1e-9 && r.is_finite() && len.is_finite()) {
            return p;
        }
        let q = center + d * (r / len);
        if !self.snap_enabled() {
            return q;
        }
        let reach = 2.0 * self.snap_tolerance();
        let mut best: Option<Target> = None;
        if let Some(t) = self.object_snap(q, exclude, &[], true, true) {
            if t.mode.is_point() {
                self.snap_mark.set(Some(t));
                return t.point;
            }
            if let Some(x) = t.edge.and_then(|seg| circle_crossing(seg, center, r, q)) {
                if (x - q).hypot() <= reach {
                    best = Some(Target { point: x, ..t });
                }
            }
        }
        // Guidelines: the point lands where its circle crosses the
        // nearest one, as on an object's edge (#3056).
        if let Some(x) = self.guide_circle_crossing(center, r, q) {
            if (x - q).hypot() <= reach
                && best.is_none_or(|b| (x - q).hypot() < (b.point - q).hypot())
            {
                best = Some(Target {
                    point: x,
                    mode: crate::snap_points::SnapMode::Guideline,
                    edge: None,
                });
            }
        }
        match best {
            Some(t) => {
                self.snap_mark.set(Some(t));
                t.point
            }
            None => q,
        }
    }

    /// Where the circle about `center` of radius `r` crosses a guideline
    /// of the page (or its master page), the crossing nearest `near`;
    /// `None` with Snap to Guidelines off.
    fn guide_circle_crossing(&self, center: Point, r: f64, near: Point) -> Option<Point> {
        if !self.snap.guides {
            return None;
        }
        let guides = self.doc().guides_on_page(self.page).ok()?;
        let mut best: Option<(f64, Point)> = None;
        for g in &guides {
            let (o, dir) = g.line.point_and_direction();
            let dl = dir.hypot();
            if dl < 1e-12 {
                continue;
            }
            let dir = dir / dl;
            // Foot of the perpendicular from the centre, then the two
            // crossings either side of it.
            let foot = o + dir * (center - o).dot(dir);
            let h2 = r * r - (foot - center).hypot2();
            if h2 < 0.0 {
                continue;
            }
            let h = h2.sqrt();
            for x in [foot + dir * h, foot - dir * h] {
                let dist = (x - near).hypot();
                if best.is_none_or(|(bd, _)| dist < bd) {
                    best = Some((dist, x));
                }
            }
        }
        best.map(|(_, x)| x)
    }

    /// [`App::snap_point_excluding`], with `own` snapping geometry of the
    /// excluded objects that stays put meanwhile (a node being dragged
    /// snaps to the other nodes and segments of its own curve, #673).
    pub fn snap_point_with(&self, p: Point, exclude: &[ShapeId], own: &[&Geo]) -> Point {
        if self.snaps_off() {
            return p;
        }
        let enabled = self.snap_enabled();
        if enabled {
            if let Some(t) = self.object_snap(p, exclude, own, true, false) {
                self.acquire_anchor(t);
                self.snap_mark.set(Some(t));
                return t.point;
            }
            if self.is_dimension_tool() {
                if let Some(t) = self.object_snap(p, exclude, own, false, true) {
                    self.snap_mark.set(Some(t));
                    return t.point;
                }
            }
        }
        if let Some(q) = self.live_guide_point(p, exclude, enabled) {
            return q;
        }
        if !enabled {
            return p;
        }
        // Where an angled guideline crosses another guideline: a point,
        // before either line alone takes the pointer (#4234).
        if let Some(q) = self.guide_crossing_near(p) {
            self.snap_mark.set(Some(crate::snap_points::Target {
                point: q,
                mode: crate::snap_points::SnapMode::Guideline,
                edge: None,
            }));
            return q;
        }
        let (xs, ys) = self.snap_candidates(exclude, true, false);
        let axis = Point::new(
            self.snap_axis(p.x, &xs, true).unwrap_or(p.x),
            self.snap_axis(p.y, &ys, false).unwrap_or(p.y),
        );
        if axis != p {
            self.mark_axis_snap(p, axis);
            return axis;
        }
        // An angled grid (#4395): its crossings first, then its lines.
        if let Some(q) = self.angled_grid_snap(p) {
            return q;
        }
        if !self.snap.guides {
            return self.edge_snap(p, exclude, own);
        }
        // Angled guides: project onto the nearest one within tolerance.
        let tol = self.snap_tolerance();
        if let Ok(guides) = self.doc().guides_on_page(self.page) {
            let mut best: Option<(f64, Point)> = None;
            for g in &guides {
                if let GuideLine::Angled { .. } = g.line {
                    let d = g.distance(p).abs();
                    if d <= tol && best.map(|b| d < b.0).unwrap_or(true) {
                        let (o, dir) = g.line.point_and_direction();
                        let t = (p - o).dot(dir);
                        best = Some((d, o + dir * t));
                    }
                }
            }
            if let Some((_, q)) = best {
                self.snap_mark.set(Some(crate::snap_points::Target {
                    point: q,
                    mode: crate::snap_points::SnapMode::Guideline,
                    edge: None,
                }));
                return q;
            }
        }
        self.edge_snap(p, exclude, own)
    }

    /// `p` snapped to an angled document grid (#4395): onto the crossing
    /// of its lines nearest it, or square onto its nearest line, within
    /// the snap tolerance; marked as a grid snap. None for a grid of
    /// horizontal and vertical lines, which the axes take.
    pub(crate) fn angled_grid_snap(&self, p: Point) -> Option<Point> {
        if !self.snap.grid {
            return None;
        }
        let geom = self.grid_geom();
        if geom.rectilinear() {
            return None;
        }
        let tol = self.snap_tolerance();
        let q = geom.snap_node(p, tol).or_else(|| geom.snap_line(p, tol))?;
        self.snap_mark.set(Some(crate::snap_points::Target {
            point: q,
            mode: crate::snap_points::SnapMode::Grid,
            edge: None,
        }));
        Some(q)
    }

    /// The crossing nearest `p`, within the snap tolerance, of an angled
    /// guideline with another guideline of the page (#4234); crossings of
    /// two straight ones are found by the axes already.
    pub(crate) fn guide_crossing_near(&self, p: Point) -> Option<Point> {
        if !self.snap.guides {
            return None;
        }
        let guides = self.doc().guides_on_page(self.page).ok()?;
        let tol = self.snap_tolerance();
        let lines: Vec<(bool, Point, tracedraw_core::geometry::Vec2)> = guides
            .iter()
            .map(|g| {
                let (o, d) = g.line.point_and_direction();
                (matches!(g.line, GuideLine::Angled { .. }), o, d)
            })
            .collect();
        let mut best: Option<(f64, Point)> = None;
        for (i, &(ai, oa, da)) in lines.iter().enumerate() {
            // Only lines that pass near the pointer can cross near it.
            if (p - oa).cross(da).abs() > tol {
                continue;
            }
            for &(bi, ob, db) in &lines[i + 1..] {
                if !(ai || bi) {
                    continue;
                }
                let den = da.cross(db);
                if den.abs() < 1e-9 {
                    continue;
                }
                let t = (ob - oa).cross(db) / den;
                let q = oa + da * t;
                let d = (q - p).hypot();
                if d <= tol && best.is_none_or(|b| d < b.0) {
                    best = Some((d, q));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// The pointer at `p` snapped to `q` along the axes: mark it, with its
    /// screen tip, when a guideline or the grid took it (guidelines first);
    /// the page's and objects' bounds are not marked.
    fn mark_axis_snap(&self, p: Point, q: Point) {
        use crate::snap_points::{SnapMode, Target};
        let moved_x = (q.x - p.x).abs() > 1e-12;
        let moved_y = (q.y - p.y).abs() > 1e-12;
        let on_guide = self.snap.guides
            && self.doc().guides_on_page(self.page).is_ok_and(|guides| {
                guides.iter().any(|g| match g.line {
                    GuideLine::Horizontal { y } => moved_y && (y - q.y).abs() < 1e-9,
                    GuideLine::Vertical { x } => moved_x && (x - q.x).abs() < 1e-9,
                    GuideLine::Angled { .. } => false,
                })
            });
        let on_grid = || {
            if !self.snap.grid {
                return false;
            }
            let geom = self.grid_geom();
            (moved_x && geom.on_line(q.x, true)) || (moved_y && geom.on_line(q.y, false))
        };
        let on_baseline = || {
            self.snap.baseline_grid
                && moved_y
                && self.baseline_ys().iter().any(|y| (y - q.y).abs() < 1e-9)
        };
        let mode = if on_guide {
            SnapMode::Guideline
        } else if on_grid() || on_baseline() {
            SnapMode::Grid
        } else {
            return;
        };
        self.snap_mark.set(Some(Target {
            point: q,
            mode,
            edge: None,
        }));
    }

    /// Where the centre of rotation goes for the pointer at `p`. Unless
    /// snapping is off, it takes the nearest point within the snap radius
    /// of the selection's box (corners, middles of the sides, centre) and
    /// of the objects of the page, the selection's own included (nodes,
    /// midpoints, quadrants, centres), whether or not Snap to Objects is
    /// on, so it can be put on the centre or an edge of an object
    /// exactly (#2352); the point it takes is marked with its name. Else
    /// it snaps as the pointer does (guidelines, the grid, the page).
    pub fn snap_pivot(&self, p: Point) -> Point {
        if self.snaps_off() {
            return p;
        }
        let tol = self.snap_tolerance();
        let mut g = Vec::new();
        if let Some(b) = self.selection_bounds() {
            if b.inflate(tol, tol).contains(p) {
                g.push(Geo::Outline {
                    path: tracedraw_core::geometry::Shape::to_path(&b, 0.01),
                    points: true,
                });
                g.push(Geo::Center(b.center()));
            }
        }
        let clean_sheet = self.symbol_edit_index().is_some();
        for sh in self
            .page_layers()
            .into_iter()
            .filter(|l| l.visible)
            .flat_map(|l| &l.shapes)
        {
            if !sh.visible
                || (clean_sheet && self.outside_clip_edit(sh.id))
                || !self.snap_bounds(sh).inflate(tol, tol).contains(p)
            {
                continue;
            }
            snap_points::object_geometry(&self.snap_shape(sh), &text_baselines, &mut g);
        }
        let found = snap_points::targets(&g, p, tol, None, &self.settings.snap.modes);
        if let Some(t) = snap_points::best(&found, true, false) {
            self.snap_mark.set(Some(t));
            return t.point;
        }
        if !self.snap_enabled() {
            return p;
        }
        self.snap_point(p)
    }

    /// Where the point of the selection box that a scale handle stands for
    /// goes for `raw` (the pointer less its grab), and whether snapping
    /// put it there on the x and on the y axis (#2004). The point snaps as
    /// the pointer does, to everything but the selection as it changes,
    /// and, once it has left its start, to the selection's own nodes and
    /// edges where they stood (#3074); a side it moves
    /// that found nothing then stops on the points of other objects (a
    /// node, a midpoint, a centre) and on their lines (a straight side
    /// along it, the outermost point of a curve) that it meets along its
    /// length, as a guideline does. A corner keeping the proportions takes
    /// one axis only, the nearer: its other side follows the scale.
    pub(crate) fn snap_scale_handle(&self, raw: Point) -> (Point, (bool, bool)) {
        use crate::app::{Drag, Handle};
        let Drag::Scale {
            handle,
            anchor,
            start_bounds,
            mods,
            start_geometry,
            ..
        } = &self.drag
        else {
            return (raw, (false, false));
        };
        // Once away from its start, the selection's own nodes and edges
        // where they stood count as another object's (#3074).
        let own = match start_geometry {
            Some(g) => self.still_near(g, raw),
            None => Vec::new(),
        };
        let mut q = self.snap_point_with(raw, &self.selection, &own);
        let (moves_x, moves_y) = match handle {
            Handle::E | Handle::W => (true, false),
            Handle::N | Handle::S => (false, true),
            _ => (true, true),
        };
        let mut sx = moves_x && (q.x - raw.x).abs() > 1e-12;
        let mut sy = moves_y && (q.y - raw.y).abs() > 1e-12;
        let proportional = handle.is_corner() && !self.rotate_mode && !mods.free;
        if !self.snap_enabled() || (proportional && (sx || sy)) {
            return (q, (sx, sy));
        }
        let b = *start_bounds;
        let from = if mods.from_center {
            b.center()
        } else {
            *anchor
        };
        // The stretch of the other axis a moving side runs along: the
        // box's for a side handle, from the fixed point to the pointer
        // (both ways from the centre) for a corner.
        let stretch = |fixed: f64, to: f64| {
            if mods.from_center {
                let r = (to - fixed).abs();
                (fixed - r, fixed + r)
            } else {
                (fixed.min(to), fixed.max(to))
            }
        };
        let x_span = match handle {
            Handle::E | Handle::W => (b.y0, b.y1),
            _ => stretch(from.y, raw.y),
        };
        let y_span = match handle {
            Handle::N | Handle::S => (b.x0, b.x1),
            _ => stretch(from.x, raw.x),
        };
        let mut hx = if moves_x && !sx {
            self.axis_stop(raw.x, true, Some(x_span), &self.selection)
        } else {
            None
        };
        let mut hy = if moves_y && !sy {
            self.axis_stop(raw.y, false, Some(y_span), &self.selection)
        } else {
            None
        };
        if proportional {
            if let (Some((x, _)), Some((y, _))) = (hx, hy) {
                if (x - raw.x).abs() <= (y - raw.y).abs() {
                    hy = None;
                } else {
                    hx = None;
                }
            }
        }
        if let Some((x, t)) = hx {
            q.x = x;
            sx = true;
            self.snap_mark.set(Some(t));
        }
        if let Some((y, t)) = hy {
            q.y = y;
            sy = true;
            self.snap_mark.set(Some(t));
        }
        (q, (sx, sy))
    }

    /// An object's or the page's edge, or a text baseline, near `p`.
    fn edge_snap(&self, p: Point, exclude: &[ShapeId], own: &[&Geo]) -> Point {
        match self.object_snap(p, exclude, own, false, true) {
            Some(t) => {
                self.snap_mark.set(Some(t));
                t.point
            }
            None => p,
        }
    }

    /// Snap a translation of the selection: its snap point `source`
    /// (when the move began on one) onto another object's point or edge;
    /// otherwise an edge or the centre of its bounds onto a guideline, the
    /// grid, the page or another object's bounds. With `start` (built
    /// once the pointer has left the snapping radius of where the move
    /// began, see [`App::selection_start_geometry`]) the
    /// selection's own points and edges where it started count as another
    /// object's: a node carried onto where another of its nodes was, or
    /// back onto where it was, or along where one of its edges ran
    /// (#1765). Its box does not: only the grabbed point snaps to its
    /// start. The selection as it moves never attracts itself.
    pub fn snap_move_from(
        &self,
        source: Option<Point>,
        bounds: Rect,
        d: Vec2,
        start: Option<&StillGeometry>,
    ) -> Vec2 {
        if self.snaps_off() {
            return d;
        }
        let enabled = self.snap_enabled();
        if let (true, Some(src)) = (enabled, source) {
            let own = match start {
                Some(g) => self.still_near(g, src + d),
                None => Vec::new(),
            };
            if let Some(t) = self.object_snap(src + d, &self.selection, &own, true, true) {
                self.acquire_anchor(t);
                self.snap_mark.set(Some(t));
                return t.point - src;
            }
        }
        let (mut snapped, mut caught) = if enabled {
            self.snap_move_caught(bounds, d)
        } else {
            (d, false)
        };
        // The grabbed point (a node under the pointer) lands on a
        // guideline, a baseline or a grid line it comes near, on each
        // axis, ahead of the box's edges and centre: a curve moved by one
        // of its nodes puts that node on the guideline (#1645).
        if let (true, Some(src)) = (enabled, source) {
            let (xs, ys) = self.snap_candidates(&self.selection, true, false);
            let q = src + d;
            let mut hit = false;
            if let Some(x) = self.snap_axis(q.x, &xs, true) {
                snapped.x = x - src.x;
                hit = true;
            }
            if let Some(y) = self.snap_axis(q.y, &ys, false) {
                snapped.y = y - src.y;
                hit = true;
            }
            if hit {
                self.mark_axis_snap(q, src + snapped);
                caught = true;
            }
        }
        self.live_guide_move(source, bounds, d, snapped, caught)
    }

    /// Alignment guides on a move (edges, centres, margins and equal
    /// gaps, which win over the grid and guidelines on their axis), else
    /// a dynamic guide for the grabbed point (the selection's snap point,
    /// else its centre); the guides to draw at the place reached.
    fn live_guide_move(
        &self,
        source: Option<Point>,
        bounds: Rect,
        d: Vec2,
        snapped: Vec2,
        caught: bool,
    ) -> Vec2 {
        let o = &self.settings.snap.align;
        let tol = self.snap_tolerance();
        let mut out = snapped;
        let mut aligned = false;
        let moving: Vec<Rect> = if o.each_object {
            self.selected_shapes()
                .iter()
                .map(|s| self.visual_bounds(s))
                .collect()
        } else {
            vec![bounds]
        };
        if self.alignment_guides_on() && !moving.is_empty() {
            let t = self.align_targets(&self.selection);
            let m = live_guides::align_move(&moving, d, &t, o, tol);
            if let Some(dx) = m.dx {
                out.x = d.x + dx;
                aligned = true;
            }
            if let Some(dy) = m.dy {
                out.y = d.y + dy;
                aligned = true;
            }
        }
        // A move already caught by a guideline, the grid or an edge on
        // either axis stays there: a dynamic guide would pull it off a
        // guideline crossing onto a slanted line (#3952).
        if !aligned && !caught && self.dynamic_guides_on() {
            let grab = source.unwrap_or_else(|| bounds.center());
            let mut anchors = self.dynamic_anchors();
            live_guides::acquire(&mut anchors, grab, MAX_ANCHORS + 1);
            if let Some(h) = self.dynamic_hit(grab + d, &anchors) {
                self.set_overlay(Vec::new(), Vec::new(), Some(h));
                return h.point - grab;
            }
        }
        if self.alignment_guides_on() {
            // The lines of what lines up where the move ends, whichever
            // snapping put it there.
            let t = self.align_targets(&self.selection);
            let at: Vec<Rect> = moving.iter().map(|r| *r + out).collect();
            let lines = live_guides::lines_for(&at, &t, o);
            let gaps = match (
                o.equal_spacing,
                at.iter().copied().reduce(|a, b| a.union(b)),
            ) {
                (true, Some(w)) => {
                    let mut g = live_guides::equal_gaps(w, &t.boxes, true);
                    g.extend(live_guides::equal_gaps(w, &t.boxes, false));
                    g
                }
                _ => Vec::new(),
            };
            self.set_overlay(lines, gaps, None);
        }
        out
    }

    pub fn alignment_guides_on(&self) -> bool {
        self.snap.alignment_guides && !self.snaps_off()
    }

    pub fn dynamic_guides_on(&self) -> bool {
        self.snap.dynamic_guides && !self.snaps_off()
    }

    fn set_overlay(
        &self,
        lines: Vec<live_guides::Seg>,
        gaps: Vec<live_guides::Seg>,
        dynamic: Option<live_guides::DynHit>,
    ) {
        self.put_overlay(live_guides::Overlay {
            lines,
            gaps,
            sizes: Vec::new(),
            dynamic,
        });
    }

    fn put_overlay(&self, overlay: live_guides::Overlay) {
        if let Ok(mut o) = self.live_guides.try_borrow_mut() {
            *o = overlay;
        }
    }

    /// Forget this frame's guides (each frame computes them again).
    pub fn clear_live_guides(&self) {
        self.set_overlay(Vec::new(), Vec::new(), None);
    }

    /// The page area in view, when the canvas has been laid out.
    fn view_area(&self) -> Option<Rect> {
        self.canvas_rect
            .is_positive()
            .then(|| self.view.visible_page_rect(self.canvas_rect))
    }

    /// The boxes of the visible objects on the page in view (leaving out
    /// `exclude`) and the values they offer, from the cache while nothing
    /// changed.
    fn align_targets(&self, exclude: &[ShapeId]) -> std::rc::Rc<live_guides::AlignTargets> {
        let o = &self.settings.snap.align;
        let view = self.view_area();
        let key = AlignKey {
            doc: self.doc() as *const _ as usize,
            revision: self.engine.revision(),
            page: self.page,
            exclude: exclude.to_vec(),
            view: view.map(|v| [v.x0, v.y0, v.x1, v.y1]),
            multipage: self.multipage,
            options: o.clone(),
        };
        if let Ok(c) = self.align_cache.try_borrow() {
            if let Some(c) = c.as_ref().filter(|c| c.key == key) {
                return c.targets.clone();
            }
        }
        let outside = |b: Rect| {
            view.map(|v| b.x1 < v.x0 || b.x0 > v.x1 || b.y1 < v.y0 || b.y0 > v.y1)
                .unwrap_or(false)
        };
        // The current page at its place, and in the Multipage View the
        // other pages in view, each moved to where it is displayed.
        let mut pages = vec![(self.page, self.page_rect())];
        if self.shows_other_pages() {
            pages.extend(
                self.shown_pages()
                    .into_iter()
                    .filter(|(id, r)| *id != self.page && !outside(*r)),
            );
        }
        let mut boxes = Vec::new();
        let doc = self.doc();
        'pages: for (id, r) in &pages {
            let shift = if *id == self.page {
                Vec2::ZERO
            } else {
                Vec2::new(r.x0, r.y0)
            };
            let layers = doc.layers_for_page(*id).unwrap_or_default();
            for sh in layers
                .into_iter()
                .filter(|l| l.visible)
                .flat_map(|l| &l.shapes)
            {
                if !sh.visible || exclude.contains(&sh.id) {
                    continue;
                }
                let b = self.visual_bounds(sh) + shift;
                if outside(b) {
                    continue;
                }
                boxes.push(b);
                if boxes.len() >= MAX_BOXES {
                    break 'pages;
                }
            }
        }
        let page_rects: Vec<Rect> = if o.page {
            pages.iter().map(|(_, r)| *r).collect()
        } else {
            Vec::new()
        };
        let targets = std::rc::Rc::new(live_guides::AlignTargets::new(boxes, &page_rects, o));
        if let Ok(mut c) = self.align_cache.try_borrow_mut() {
            *c = Some(AlignCache {
                key,
                targets: targets.clone(),
            });
        }
        targets
    }

    /// Remember a point snapped to (a node, a centre, a midpoint...) as
    /// an anchor of dynamic guides.
    fn acquire_anchor(&self, t: Target) {
        use snap_points::SnapMode as M;
        if !self.dynamic_guides_on()
            || !matches!(
                t.mode,
                M::Node | M::Intersection | M::Midpoint | M::Quadrant | M::Center
            )
        {
            return;
        }
        if let Ok(mut a) = self.guide_anchors.try_borrow_mut() {
            live_guides::acquire(&mut a, t.point, MAX_ANCHORS);
        }
    }

    /// The anchors of dynamic guides: the points acquired that are in
    /// view, and the start of the line being drawn.
    fn dynamic_anchors(&self) -> Vec<Point> {
        let view = self.view_area();
        let mut out: Vec<Point> = self
            .guide_anchors
            .try_borrow()
            .map(|a| a.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| view.map(|v| v.contains(*p)).unwrap_or(true))
            .collect();
        if let Some(from) = self.snap_from() {
            live_guides::acquire(&mut out, from, MAX_ANCHORS + 1);
        }
        out
    }

    /// The dynamic guide `p` snaps to (and its tick, when ticks snap).
    fn dynamic_hit(&self, p: Point, anchors: &[Point]) -> Option<live_guides::DynHit> {
        let o = &self.settings.snap.dynamic;
        let tol = self.snap_tolerance();
        let h = live_guides::dynamic_snap(p, anchors, &o.clean_angles(), tol)?;
        Some(if o.ticks && o.snap_ticks {
            live_guides::snap_to_tick(h, o.tick_mm.max(0.01), tol)
        } else {
            h
        })
    }

    /// The box being drawn or resized: its fixed point, how many times the
    /// pointer's distance its size is (2 from the centre) and the axes the
    /// handle changes.
    fn size_context(&self) -> Option<(Point, f64, (bool, bool))> {
        use crate::app::{Drag, Handle};
        match &self.drag {
            Drag::Box {
                start, from_center, ..
            } => Some((*start, if *from_center { 2.0 } else { 1.0 }, (true, true))),
            Drag::Scale {
                handle,
                anchor,
                start_bounds,
                mods,
                ..
            } => {
                let (anchor, factor) = if mods.from_center {
                    (start_bounds.center(), 2.0)
                } else {
                    (*anchor, 1.0)
                };
                let axes = match handle {
                    Handle::E | Handle::W => (true, false),
                    Handle::N | Handle::S => (false, true),
                    _ => (true, true),
                };
                Some((anchor, factor, axes))
            }
            _ => None,
        }
    }

    /// Alignment guides for a point (a drawing tool's pointer, a scale
    /// handle): its x or y onto other objects' edges or centres (the other
    /// coordinate still snaps to guidelines and the grids), else onto a
    /// dynamic guide. Not while turning the selection.
    fn live_guide_point(&self, p: Point, exclude: &[ShapeId], enabled: bool) -> Option<Point> {
        if matches!(
            self.drag,
            crate::app::Drag::Rotate { .. } | crate::app::Drag::Pivot
        ) {
            return None;
        }
        let tol = self.snap_tolerance();
        if self.alignment_guides_on() {
            let t = self.align_targets(exclude);
            let (mut x, mut y, mut lines) = live_guides::align_point(p, &t, tol);
            // Equal sizes while a box is drawn or resized: on each axis the
            // nearer of an alignment and a size match wins.
            let mut sizes = Vec::new();
            if let (true, Some((anchor, factor, axes))) =
                (self.settings.snap.align.equal_size, self.size_context())
            {
                let (sx, sy, _) = live_guides::size_snap(p, anchor, factor, axes, &t, tol);
                let wins = |s: Option<f64>, a: Option<f64>, v: f64| match (s, a) {
                    (Some(s), Some(a)) => (s - v).abs() <= (a - v).abs(),
                    (Some(_), None) => true,
                    _ => false,
                };
                let (wx, wy) = (wins(sx, x, p.x), wins(sy, y, p.y));
                if wx {
                    x = sx;
                    // Vertical lines belong to the x axis.
                    lines.retain(|l| (l.a.x - l.b.x).abs() > 1e-9);
                }
                if wy {
                    y = sy;
                    lines.retain(|l| (l.a.y - l.b.y).abs() > 1e-9);
                }
                if wx || wy {
                    // The marks of the place reached.
                    let q = Point::new(x.unwrap_or(p.x), y.unwrap_or(p.y));
                    let (_, _, marks) =
                        live_guides::size_snap(q, anchor, factor, (wx, wy), &t, 1e-6);
                    sizes = marks;
                }
            }
            if x.is_some() || y.is_some() {
                let (xs, ys) = if enabled {
                    self.snap_candidates(exclude, true, false)
                } else {
                    (Vec::new(), Vec::new())
                };
                let q = Point::new(
                    x.or_else(|| self.snap_axis(p.x, &xs, true).filter(|_| enabled))
                        .unwrap_or(p.x),
                    y.or_else(|| self.snap_axis(p.y, &ys, false).filter(|_| enabled))
                        .unwrap_or(p.y),
                );
                self.put_overlay(live_guides::Overlay {
                    lines,
                    gaps: Vec::new(),
                    sizes,
                    dynamic: None,
                });
                return Some(q);
            }
        }
        if self.dynamic_guides_on() {
            let anchors = self.dynamic_anchors();
            if let Some(h) = self.dynamic_hit(p, &anchors) {
                self.set_overlay(Vec::new(), Vec::new(), Some(h));
                return Some(h.point);
            }
        }
        None
    }

    /// Snap a translation of the selection so an edge or centre of its
    /// bounds lands on a candidate: a guideline, the grids, the page, or
    /// an edge or centre of an object it faces (see
    /// [`App::snap_candidates_near`]).
    pub fn snap_move(&self, bounds: Rect, d: Vec2) -> Vec2 {
        if !self.snap_enabled() {
            return d;
        }
        self.snap_move_caught(bounds, d).0
    }

    /// [`App::snap_move`], and whether an edge or the centre lies on a
    /// candidate on either axis (also when the pointer put it exactly
    /// there).
    fn snap_move_caught(&self, bounds: Rect, d: Vec2) -> (Vec2, bool) {
        let moved = bounds + d;
        let (xs, ys) = self.snap_candidates_near(&self.selection, true, true, Some(moved));
        let mut best_dx: Option<f64> = None;
        for edge in [moved.x0, moved.x1, moved.center().x] {
            if let Some(t) = self.snap_axis(edge, &xs, true) {
                let dx = t - edge;
                if best_dx.map(|b| dx.abs() < b.abs()).unwrap_or(true) {
                    best_dx = Some(dx);
                }
            }
        }
        let mut best_dy: Option<f64> = None;
        for edge in [moved.y0, moved.y1, moved.center().y] {
            if let Some(t) = self.snap_axis(edge, &ys, false) {
                let dy = t - edge;
                if best_dy.map(|b| dy.abs() < b.abs()).unwrap_or(true) {
                    best_dy = Some(dy);
                }
            }
        }
        // A slanted guideline the selection's outline comes near: the
        // move takes it onto the guideline, across it, when that is a
        // nearer catch than the straight lines' (#4383).
        if let Some(n) = self.angled_guide_catch(d) {
            let axis = if best_dx.is_none() && best_dy.is_none() {
                f64::INFINITY
            } else {
                best_dx.unwrap_or(0.0).hypot(best_dy.unwrap_or(0.0))
            };
            if n.hypot() < axis {
                return (d + n, true);
            }
        }
        if best_dx.is_none() && best_dy.is_none() && self.snap.grid {
            // An angled grid (#4395): a corner or the centre of the moved
            // box onto the crossing nearest it.
            let geom = self.grid_geom();
            if !geom.rectilinear() {
                let tol = self.snap_tolerance();
                let mut best: Option<(f64, Vec2)> = None;
                for c in [
                    Point::new(moved.x0, moved.y0),
                    Point::new(moved.x1, moved.y0),
                    Point::new(moved.x0, moved.y1),
                    Point::new(moved.x1, moved.y1),
                    moved.center(),
                ] {
                    if let Some(q) = geom.snap_node(c, tol) {
                        let off = q - c;
                        if best.is_none_or(|b| off.hypot() < b.0) {
                            best = Some((off.hypot(), off));
                        }
                    }
                }
                if let Some((_, off)) = best {
                    return (d + off, true);
                }
            }
        }
        (
            Vec2::new(d.x + best_dx.unwrap_or(0.0), d.y + best_dy.unwrap_or(0.0)),
            best_dx.is_some() || best_dy.is_some(),
        )
    }

    /// The points of the selection's outline on the page, kept while the
    /// drawing and the selection stay as they are.
    fn selection_outline_points(&self) -> std::rc::Rc<Vec<Point>> {
        let key = (self.engine.revision(), self.selection.clone());
        if let Some((k, pts)) = self.snap_outline.borrow().as_ref() {
            if *k == key {
                return pts.clone();
            }
        }
        let mut pts = Vec::new();
        for s in self.selected_shapes() {
            let parent = self.doc().parent_transform(s.id).unwrap_or(Affine::IDENTITY);
            let path = parent * s.page_path();
            let b = tracedraw_core::geometry::Shape::bounding_box(&path);
            let tol = (b.width().max(b.height()) * 1e-3).max(1e-3);
            tracedraw_core::geometry::flatten(&path, tol, &mut |el| {
                use tracedraw_core::geometry::PathEl;
                if let PathEl::MoveTo(p) | PathEl::LineTo(p) = el {
                    if pts.len() < 20_000 {
                        pts.push(p);
                    }
                }
            });
        }
        let pts = std::rc::Rc::new(pts);
        *self.snap_outline.borrow_mut() = Some((key, pts.clone()));
        pts
    }

    /// Whether the selection's outline, moved by `d`, touches slanted
    /// guideline `line` with its edge or its middle.
    fn outline_on_line(&self, line: GuideLine, d: Vec2) -> bool {
        let pts = self.selection_outline_points();
        if pts.is_empty() {
            return false;
        }
        let (o, dir) = line.point_and_direction();
        let n = Vec2::new(-dir.y, dir.x);
        let at = o.to_vec2().dot(n);
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in pts.iter() {
            let v = (p.to_vec2() + d).dot(n);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        [lo, hi, (lo + hi) / 2.0].iter().any(|v| (v - at).abs() < 1e-6)
    }

    /// The shift that puts the selection, moved by `d`, on the nearest
    /// slanted guideline within the snap distance: its outline's edge or
    /// its middle on the line (#4383, they caught horizontal and vertical
    /// guidelines only).
    pub(crate) fn angled_guide_catch(&self, d: Vec2) -> Option<Vec2> {
        if !self.snap.guides || self.symbol_edit_index().is_some() {
            return None;
        }
        let guides = self.doc().guides_on_page(self.page).ok()?;
        if !guides.iter().any(|g| matches!(g.line, GuideLine::Angled { .. }) && !g.hidden) {
            return None;
        }
        let pts = self.selection_outline_points();
        if pts.is_empty() {
            return None;
        }
        let tol = self.snap_tolerance();
        let mut best: Option<Vec2> = None;
        for g in guides.iter().filter(|g| !g.hidden) {
            if !matches!(g.line, GuideLine::Angled { .. }) {
                continue;
            }
            let (o, dir) = g.line.point_and_direction();
            let n = Vec2::new(-dir.y, dir.x);
            let line = o.to_vec2().dot(n);
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for p in pts.iter() {
                let v = (p.to_vec2() + d).dot(n);
                lo = lo.min(v);
                hi = hi.max(v);
            }
            for v in [lo, hi, (lo + hi) / 2.0] {
                let shift = line - v;
                if shift.abs() <= tol && best.is_none_or(|b| shift.abs() < b.hypot()) {
                    best = Some(n * shift);
                }
            }
        }
        best
    }

    /// All snapping off: Alt+Q, or Q held down on its own for the
    /// moment (#3072).
    pub fn snaps_off(&self) -> bool {
        self.snap.off || self.snap_held_off
    }

    /// The guidelines what is being dragged lies on: the edges and
    /// centre of the selection being moved or scaled, or of the box
    /// being drawn, with Snap to Guidelines on. The drawing window
    /// shows them highlighted, so a snap to a guideline is seen in a
    /// cluttered drawing (#3073).
    pub fn snapped_guides(&self) -> Vec<GuideLine> {
        use crate::app::Drag;
        if self.snaps_off() || !self.snap.guides {
            return Vec::new();
        }
        let b = match &self.drag {
            Drag::Move {
                start_bounds,
                total,
                ..
            } => *start_bounds + *total,
            Drag::Scale { start_bounds, .. } => match self.preview_transform_of(&self.drag) {
                Some(t) => t.transform_rect_bbox(*start_bounds),
                None => return Vec::new(),
            },
            Drag::Box { start, current, .. } | Drag::ThreePointBase { start, current } => {
                Rect::from_points(*start, *current)
            }
            _ => return Vec::new(),
        };
        let Ok(guides) = self.doc().guides_on_page(self.page) else {
            return Vec::new();
        };
        let on = |v: f64, edges: [f64; 3]| edges.iter().any(|e| (e - v).abs() < 1e-6);
        let (xs, ys) = ([b.x0, b.x1, b.center().x], [b.y0, b.y1, b.center().y]);
        // A move whose outline lies on a slanted guideline shows it too
        // (#4383).
        let moved = match &self.drag {
            Drag::Move { total, .. } => Some(*total),
            _ => None,
        };
        guides
            .iter()
            .map(|g| g.line)
            .filter(|line| match *line {
                GuideLine::Vertical { x } => on(x, xs),
                GuideLine::Horizontal { y } => on(y, ys),
                GuideLine::Angled { .. } => moved.is_some_and(|d| self.outline_on_line(*line, d)),
            })
            .collect()
    }

    pub fn snap_enabled(&self) -> bool {
        !self.snaps_off()
            && (self.snap.grid
                || self.snap.guides
                || self.snaps_to_objects()
                || self.snap.page
                || self.snap.pixels
                || self.snap.baseline_grid)
    }
}

/// The baselines of a text object on the page, one per laid-out line.
fn text_baselines(s: &Shape, parent: Affine) -> Vec<(Point, Point)> {
    let Some((layout, to_page)) = crate::text_editing::layout_of_text_shape(s, parent) else {
        return Vec::new();
    };
    layout
        .lines
        .iter()
        .filter_map(|l| {
            let (a, b) = l.x_range();
            (b > a).then(|| {
                (
                    to_page * Point::new(a, l.baseline),
                    to_page * Point::new(b, l.baseline),
                )
            })
        })
        .collect()
}

/// Where the circle about `center` of radius `r` crosses the segment
/// `seg` (a curve taken as short straight pieces), the crossing nearest
/// `near`; `None` when it does not cross.
fn circle_crossing(
    seg: tracedraw_core::geometry::PathSeg,
    center: Point,
    r: f64,
    near: Point,
) -> Option<Point> {
    use tracedraw_core::geometry::{ParamCurve, PathSeg};
    let pieces = match seg {
        PathSeg::Line(_) => 1,
        _ => 64,
    };
    let mut best: Option<(f64, Point)> = None;
    let mut a = seg.eval(0.0);
    for i in 1..=pieces {
        let b = seg.eval(i as f64 / pieces as f64);
        // |a + t (b - a) - center|² = r², for t in [0, 1].
        let d = b - a;
        let f = a - center;
        let qa = d.hypot2();
        if qa > 1e-18 {
            let qb = 2.0 * f.dot(d);
            let qc = f.hypot2() - r * r;
            let disc = qb * qb - 4.0 * qa * qc;
            if disc >= 0.0 {
                let s = disc.sqrt();
                for t in [(-qb - s) / (2.0 * qa), (-qb + s) / (2.0 * qa)] {
                    if (-1e-9..=1.0 + 1e-9).contains(&t) {
                        let x = a + d * t;
                        let dist = (x - near).hypot();
                        if best.is_none_or(|(bd, _)| dist < bd) {
                            best = Some((dist, x));
                        }
                    }
                }
            }
        }
        a = b;
    }
    best.map(|(_, x)| x)
        .filter(|x| x.x.is_finite() && x.y.is_finite())
}

/// The straight upright (`true`, at an x) and level (`false`, at a y)
/// sides of `s` on the page that lie inside its box `b`, with the span
/// they cover across: what a moved box snaps to besides the box (#3970).
/// Outlines of very many segments are left to their box.
fn inner_sides(s: &Shape, b: Rect, out: &mut impl FnMut(bool, f64, f64, f64)) {
    use tracedraw_core::geometry::PathSeg;
    let path = s.page_path();
    if path.elements().len() > 2000 {
        return;
    }
    let eps = 1e-6 * (1.0 + b.width().max(b.height()));
    for seg in path.segments() {
        let PathSeg::Line(l) = seg else { continue };
        let (a, z) = (l.p0, l.p1);
        if (a.x - z.x).abs() <= eps && (a.y - z.y).abs() > eps {
            if a.x > b.x0 + eps && a.x < b.x1 - eps {
                out(true, a.x, a.y.min(z.y), a.y.max(z.y));
            }
        } else if (a.y - z.y).abs() <= eps
            && (a.x - z.x).abs() > eps
            && a.y > b.y0 + eps
            && a.y < b.y1 - eps
        {
            out(false, a.y, a.x.min(z.x), a.x.max(z.x));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snap_points::SnapMode;
    use tracedraw_core::{document::Guide, Command};

    /// A headless app at 1 px per mm: the 10 px radius is 10 mm.
    fn app() -> App {
        let mut app = App::headless();
        app.view.zoom = 1.0;
        // The tests here snap to objects, which a first start leaves off.
        app.snap = SnapSettings {
            objects: true,
            ..SnapSettings::default()
        };
        app
    }

    /// #4234: an angled guideline crossing a vertical one gives a point
    /// the pointer snaps to; two straight guidelines are left to the axes.
    #[test]
    fn an_angled_guideline_crossing_another_is_a_snap_point() {
        let mut app = app();
        let page = app.page;
        for line in [
            GuideLine::Vertical { x: 50.0 },
            GuideLine::Angled { x: 0.0, y: 0.0, angle: 45.0 },
        ] {
            app.run(Command::AddGuide { page, guide: line.into() });
        }
        let q = app.guide_crossing_near(Point::new(50.4, 49.7)).expect("crossing");
        assert!((q - Point::new(50.0, 50.0)).hypot() < 1e-9, "{q:?}");
        assert!(app.guide_crossing_near(Point::new(80.0, 20.0)).is_none());
    }

    /// #4383: a moved object comes onto a slanted guideline it nears,
    /// its outline's edge on the line, and the guideline is shown caught.
    #[test]
    fn a_move_snaps_onto_a_slanted_guideline() {
        let mut app = app();
        app.snap = SnapSettings {
            guides: true,
            page: false,
            objects: false,
            grid: false,
            off: false,
            ..SnapSettings::default()
        };
        let page = app.page;
        app.run(Command::AddGuide {
            page,
            guide: GuideLine::Angled { x: 0.0, y: 0.0, angle: 45.0 }.into(),
        });
        let id = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: Rect::new(20.0, 0.0, 30.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .unwrap();
        app.select(vec![id]);
        let b = Rect::new(20.0, 0.0, 30.0, 10.0);
        // Its top left corner (20, 10) moved 6 mm left lies 2.8 mm off
        // the line y = x: it is taken onto it.
        let d = app.snap_move(b, Vec2::new(-6.0, 0.0));
        let corner = Point::new(20.0, 10.0) + d;
        assert!((corner.x - corner.y).abs() < 1e-6, "{corner:?} {d:?}");
        // Far from it nothing happens.
        let far = app.snap_move(b, Vec2::new(30.0, 0.0));
        assert_eq!(far, Vec2::new(30.0, 0.0));
        assert!(app.outline_on_line(GuideLine::Angled { x: 0.0, y: 0.0, angle: 45.0 }, d));
    }

    /// A new drawing's grid starts at the page's top-left corner, so a
    /// page whose height is not a whole number of grid squares has its
    /// part row at the bottom (#2990); a ruler origin moved elsewhere
    /// still has the grid pass through it.
    #[test]
    fn the_grid_starts_at_the_top_of_the_page() {
        let mut app = App::headless();
        app.view.zoom = 1.0;
        app.snap = SnapSettings {
            grid: true,
            page: false,
            guides: false,
            objects: false,
            off: false,
            ..SnapSettings::default()
        };
        let top = app.page_size().height;
        assert_eq!(app.doc().metadata.rulers.origin_y, 0.0);
        assert_eq!(app.grid_origin(), Point::new(0.0, top));
        // A4 is 297 mm tall: lines at 287, 277, ... 7, not at 290 or 10.
        let p = app.snap_point_without_guides(Point::new(30.4, top - 19.6));
        assert!((p.y - (top - 20.0)).abs() < 1e-9, "{p:?}");
        assert!((p.x - 30.0).abs() < 1e-9, "{p:?}");
        let mut metadata = app.doc().metadata.clone();
        metadata.rulers.origin_y = 5.0;
        app.run(Command::SetMetadata { metadata });
        assert_eq!(app.grid_origin(), Point::new(0.0, 5.0));
        let p = app.snap_point_without_guides(Point::new(30.4, 24.6));
        assert!((p.y - 25.0).abs() < 1e-9, "{p:?}");
    }

    fn rect(app: &mut App, x0: f64, y0: f64, x1: f64, y1: f64) -> ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x0, y0, x1, y1),
            radius: 0.0,
            corners: None,
        })
        .expect("a rectangle")
    }

    #[test]
    fn the_pick_tool_names_the_selections_point_under_the_pointer() {
        let mut app = app();
        // Snapping radius 3 mm at 1 px per mm.
        app.settings.snap.threshold_px = 3.0;
        let id = rect(&mut app, 100.0, 100.0, 160.0, 140.0);
        // Nothing selected: nothing to name.
        assert!(app
            .selection_hover_target(Point::new(160.0, 120.0))
            .is_none());
        app.select(vec![id]);
        let mode = |app: &App, x: f64, y: f64| {
            app.selection_hover_target(Point::new(x, y)).map(|t| t.mode)
        };
        assert_eq!(mode(&app, 130.0, 120.5), Some(SnapMode::Center));
        assert_eq!(mode(&app, 101.0, 139.0), Some(SnapMode::Node));
        assert_eq!(mode(&app, 100.5, 120.0), Some(SnapMode::Midpoint));
        // On a side, away from its points: the edge, at the pointer.
        let t = app
            .selection_hover_target(Point::new(160.5, 108.0))
            .expect("an edge");
        assert_eq!(t.mode, SnapMode::Edge);
        assert!((t.point.x - 160.0).abs() < 1e-9 && (t.point.y - 108.0).abs() < 1e-9);
        // Far from everything, or with snapping to objects off: nothing.
        assert!(mode(&app, 130.0, 110.0).is_none());
        app.snap.objects = false;
        assert!(mode(&app, 130.0, 120.5).is_none());
    }

    /// A horizontal guideline dragged onto one of a row of rectangles
    /// about level with each other lands on the top of the one the
    /// pointer is over, not on a closer-lined-up top far away (#3359).
    #[test]
    fn a_dragged_guideline_snaps_to_the_object_under_the_pointer() {
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        // Tops at 60, 61 and 59.
        rect(&mut app, 10.0, 20.0, 40.0, 60.0);
        rect(&mut app, 110.0, 20.0, 140.0, 61.0);
        rect(&mut app, 210.0, 20.0, 240.0, 59.0);
        // The pointer over the middle one at y 60.2: its top, 61, though
        // the first one's 60 is nearer the pointer's height.
        assert!((app.snap_guide_near(60.2, false, 125.0) - 61.0).abs() < 1e-9);
        // Over the last one: 59.
        assert!((app.snap_guide_near(60.2, false, 225.0) - 59.0).abs() < 1e-9);
        // Over the first one: 60.
        assert!((app.snap_guide_near(60.2, false, 25.0) - 60.0).abs() < 1e-9);
        // Between objects, the nearest object along the line counts.
        assert!((app.snap_guide_near(60.2, false, 190.0) - 59.0).abs() < 1e-9);
        // Without the pointer the nearest top along the axis, as before.
        assert!((app.snap_y_without_guides(60.2) - 60.0).abs() < 1e-9);
        // A vertical guideline: the rectangle the pointer is beside.
        rect(&mut app, 39.0, 100.0, 80.0, 130.0);
        assert!((app.snap_guide_near(39.6, true, 115.0) - 39.0).abs() < 1e-9);
        assert!((app.snap_guide_near(39.6, true, 40.0) - 40.0).abs() < 1e-9);
    }

    /// A guideline dragged near a node of a curve lands on the node, not
    /// only on the edges and centre of the object's box; with the Node
    /// mode off it stays where the pointer put it (#1645).
    #[test]
    fn a_dragged_guideline_snaps_to_a_node() {
        use tracedraw_core::geometry::BezPath;
        let mut app = app();
        // Snapping radius 3 mm at 1 px per mm.
        app.settings.snap.threshold_px = 3.0;
        // A triangle whose apex (30, 60) lies on neither its box's edges
        // (x 10 to 70, y 10 to 60) nor its centre (40, 35).
        let mut path = BezPath::new();
        path.move_to((10.0, 10.0));
        path.line_to((70.0, 10.0));
        path.line_to((30.0, 60.0));
        path.close_path();
        app.new_shape(ShapeKind::Path { path, closed: true })
            .expect("a triangle");
        // A vertical guideline 1.5 mm off the apex lands on it.
        assert!((app.snap_x_without_guides(31.5) - 30.0).abs() < 1e-9);
        // So does a horizontal one dragged near the apex's height, where
        // the box's top edge is too (y = 60): the node.
        assert!((app.snap_y_without_guides(58.5) - 60.0).abs() < 1e-9);
        // Beyond the radius of every point (the apex at 30, the centroid
        // at 36.67): nothing.
        assert!((app.snap_x_without_guides(33.5) - 33.5).abs() < 1e-9);
        // Midpoints count too (the left side's middle is at x = 20).
        assert!((app.snap_x_without_guides(21.5) - 20.0).abs() < 1e-9);
        // With the Node and Midpoint modes off, the apex no longer catches
        // the guideline; the box's centre (x = 40) still does.
        app.settings.snap.modes.node = false;
        app.settings.snap.modes.midpoint = false;
        assert!((app.snap_x_without_guides(31.5) - 31.5).abs() < 1e-9);
        assert!((app.snap_x_without_guides(41.5) - 40.0).abs() < 1e-9);
        // Snapping to objects off: the apex is not a target.
        app.settings.snap.modes.node = true;
        app.snap.objects = false;
        assert!((app.snap_x_without_guides(31.5) - 31.5).abs() < 1e-9);
    }

    /// The other way round: a curve moved by one of its nodes (the
    /// selection's snap point under the pointer) puts that node on a
    /// guideline it comes near, also where none of its box's edges or
    /// its centre does (#1645).
    #[test]
    fn a_curve_moved_by_a_node_puts_the_node_on_a_guideline() {
        use tracedraw_core::geometry::BezPath;
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let page = app.page;
        app.run(Command::AddGuide {
            page,
            guide: Guide::vertical(50.0),
        });
        let mut path = BezPath::new();
        path.move_to((10.0, 10.0));
        path.line_to((70.0, 10.0));
        path.line_to((30.0, 60.0));
        path.close_path();
        let id = app
            .new_shape(ShapeKind::Path { path, closed: true })
            .expect("a triangle");
        app.select(vec![id]);
        let b = app.selection_bounds().unwrap();
        // Grabbed at the left side's midpoint (20, 35) and moved so that
        // the point is 1.5 mm off the guideline, while the box's edges
        // (41.5 and 101.5) and centre (71.5) are far from it.
        let grab = Point::new(20.0, 35.0);
        let d = app.snap_move_from(Some(grab), b, Vec2::new(31.5, 0.0), None);
        assert!((d.x - 30.0).abs() < 1e-9 && d.y.abs() < 1e-9, "{d:?}");
        // Beyond the radius the move is as the pointer made it.
        let d = app.snap_move_from(Some(grab), b, Vec2::new(35.0, 0.0), None);
        assert!((d.x - 35.0).abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn a_guideline_catches_an_object_moved_near_it() {
        let mut app = app();
        let page = app.page;
        app.run(Command::AddGuide {
            page,
            guide: Guide::vertical(50.0),
        });
        let id = rect(&mut app, 20.0, 20.0, 40.0, 40.0);
        app.select(vec![id]);
        let b = app.selection_bounds().unwrap();
        // The right edge dragged to x = 51 lands on the guideline.
        let d = app.snap_move_from(None, b, Vec2::new(11.0, 0.0), None);
        assert!((b.x1 + d.x - 50.0).abs() < 1e-9, "{d:?}");
    }

    /// A box moved into the corner of a vertical and a horizontal
    /// guideline lands on both, also with dynamic guides on: a slanted
    /// dynamic guide through an earlier snap point no longer pulls it
    /// off the crossing onto one line, or onto none (#3952).
    #[test]
    fn a_box_lands_on_two_crossing_guidelines_at_once() {
        let mut app = app();
        let page = app.page;
        app.snap.dynamic_guides = true;
        app.run(Command::AddGuide {
            page,
            guide: Guide::vertical(175.0),
        });
        app.run(Command::AddGuide {
            page,
            guide: Guide::horizontal(130.0),
        });
        let id = rect(&mut app, 20.0, 20.0, 120.0, 80.0);
        app.select(vec![id]);
        // Points snapped to before, which dynamic guides start from.
        app.guide_anchors
            .borrow_mut()
            .extend([Point::new(50.0, 200.0), Point::new(178.0, 20.0)]);
        let b = app.selection_bounds().unwrap();
        let corner = Point::new(b.x1, b.y1);
        for grab in [None, Some(corner), Some(b.center())] {
            for (ox, oy) in [(0.0, 0.0), (-5.0, 3.0), (6.0, -4.0), (-2.0, -7.0)] {
                let d = Vec2::new(175.0 - b.x1 + ox, 130.0 - b.y1 + oy);
                let m = b + app.snap_move_from(grab, b, d, None);
                assert!(
                    (m.x1 - 175.0).abs() < 1e-9 && (m.y1 - 130.0).abs() < 1e-9,
                    "{grab:?} {ox} {oy}: {m:?}"
                );
            }
        }
    }

    /// At first objects snap as the page does, within 10 px: a node
    /// 9 px from the pointer catches it, one 11 px away leaves it alone;
    /// with snapping to objects turned off, only the page does (#1814).
    #[test]
    fn a_first_start_snaps_to_objects_and_the_page_within_10_px() {
        let mut app = app();
        app.snap = SnapSettings::default();
        app.settings.snap = crate::settings::SnapPrefs::default();
        let page = app.page_rect();
        // A rectangle well inside the page, far from its points.
        let (x, y) = (page.x0 + 100.0, page.y0 + 100.0);
        rect(&mut app, x, y, x + 40.0, y + 20.0);
        let corner = Point::new(page.x0 + 3.0, page.y0 + 4.0);
        assert_eq!(app.snap_point(corner), Point::new(page.x0, page.y0));
        // 9 px from the node snaps, 11 px does not.
        let p = Point::new(x + 9.0, y - 0.5);
        assert_eq!(app.snap_point(p), Point::new(x, y));
        let far = Point::new(x - 11.0, y - 11.0);
        assert_eq!(app.snap_point(far), far);
        // Objects off (Alt+Z): the node no longer pulls, the page still does.
        app.snap.objects = false;
        assert_eq!(app.snap_point(p), p, "objects off do not snap");
        assert_eq!(app.snap_point(corner), Point::new(page.x0, page.y0));
    }

    /// A moved box snaps to the edges and centres of the objects it
    /// faces, and only to those: an object far above it, one of whose
    /// sides is level with the box's right side, leaves the move as the
    /// pointer makes it; one beside it catches the side, at first, with
    /// no setting changed. This is what made a first start feel sticky
    /// before snapping to objects was turned off at first (#1814).
    #[test]
    fn a_moved_box_snaps_only_to_the_objects_it_faces() {
        let mut app = app();
        app.snap = SnapSettings::default();
        app.settings.snap = crate::settings::SnapPrefs::default();
        // Snapping radius 3 mm at 1 px per mm.
        app.settings.snap.threshold_px = 3.0;
        let page = app.page_rect();
        let (x, y) = (page.x0 + 20.0, page.y0 + 20.0);
        let id = rect(&mut app, x, y, x + 20.0, y + 20.0);
        // Far above, its left side at x + 42.
        rect(&mut app, x + 42.0, y + 180.0, x + 70.0, y + 200.0);
        app.select(vec![id]);
        let b = app.selection_bounds().expect("the square's box");
        // The right side moved to x + 41, 1 mm off the far object's
        // left side: nothing pulls it.
        let d = app.snap_move_from(None, b, Vec2::new(21.0, 0.0), None);
        assert!((d.x - 21.0).abs() < 1e-9 && d.y.abs() < 1e-9, "{d:?}");
        // The same object brought down beside the square: the right side
        // lands on its left side.
        let beside = rect(&mut app, x + 42.0, y + 5.0, x + 70.0, y + 25.0);
        let d = app.snap_move_from(None, b, Vec2::new(21.0, 0.0), None);
        assert!((d.x - 22.0).abs() < 1e-9 && d.y.abs() < 1e-9, "{d:?}");
        // Just under it, within the radius, still faces it: the square
        // stacked below lines its left side up with the object's.
        app.run(Command::DeleteShapes {
            shapes: vec![beside],
        });
        rect(&mut app, x + 30.0, y + 41.5, x + 60.0, y + 60.0);
        let d = app.snap_move_from(None, b, Vec2::new(31.0, 20.0), None);
        assert!((d.x - 30.0).abs() < 1e-9, "left sides line up: {d:?}");
        assert!((d.y - 21.5).abs() < 1e-9, "top meets bottom: {d:?}");
    }

    #[test]
    fn drawing_tools_snap_to_object_points_and_show_the_mark() {
        let mut app = app();
        rect(&mut app, 100.0, 100.0, 140.0, 120.0);
        let p = app.snap_point(Point::new(103.0, 118.0));
        assert_eq!(p, Point::new(100.0, 120.0));
        let mark = app.snap_mark.get().expect("a mark");
        assert_eq!(mark.mode, SnapMode::Node);
        // A point wins over a guideline near it.
        let page = app.page;
        app.run(Command::AddGuide {
            page,
            guide: Guide::horizontal(116.0),
        });
        assert_eq!(
            app.snap_point(Point::new(101.0, 117.0)),
            Point::new(100.0, 120.0)
        );
        // A guideline wins over an edge (2 mm radius).
        app.settings.snap.threshold_px = 2.0;
        let q = app.snap_point(Point::new(141.0, 115.5));
        assert_eq!(q, Point::new(141.0, 116.0));
        // Without the guideline: the edge.
        app.snap.guides = false;
        let q = app.snap_point(Point::new(141.0, 115.5));
        assert_eq!(q, Point::new(140.0, 115.5));
        assert_eq!(app.snap_mark.get().map(|m| m.mode), Some(SnapMode::Edge));
        app.snap.guides = true;
        app.settings.snap.threshold_px = 10.0;
        // Modes can be turned off one by one.
        app.settings.snap.modes.node = false;
        app.snap_mark.set(None);
        let q = app.snap_point(Point::new(103.0, 118.0));
        assert_ne!(q, Point::new(100.0, 120.0));
    }

    /// A box moved inside a frame (a rectangle with a rectangular hole,
    /// made by Combine) snaps to the inner edges, not only to the frame's
    /// outer box (#3970).
    #[test]
    fn a_moved_box_snaps_to_the_inner_edges_of_a_frame() {
        use tracedraw_core::BezPath;
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        app.snap = SnapSettings {
            grid: false,
            guides: false,
            objects: true,
            page: false,
            pixels: false,
            baseline_grid: false,
            alignment_guides: false,
            dynamic_guides: false,
            off: false,
        };
        let mut path = BezPath::new();
        for (x0, y0, x1, y1) in [(100.0, 100.0, 200.0, 200.0), (120.0, 120.0, 180.0, 180.0)] {
            path.move_to((x0, y0));
            path.line_to((x1, y0));
            path.line_to((x1, y1));
            path.line_to((x0, y1));
            path.close_path();
        }
        app.new_shape(ShapeKind::Path { path, closed: true })
            .expect("frame");
        let small = rect(&mut app, 140.0, 140.0, 150.0, 150.0);
        app.select(vec![small]);
        let b = Rect::new(140.0, 140.0, 150.0, 150.0);
        // Its left side brought within a millimetre of the inner left edge,
        // its bottom near the inner bottom edge.
        let d = app.snap_move(b, Vec2::new(-19.0, -18.5));
        assert_eq!(d, Vec2::new(-20.0, -20.0));
        // The inner right edge, by its right side.
        let d = app.snap_move(b, Vec2::new(29.2, 0.0));
        assert!((d.x - 30.0).abs() < 1e-9, "{d:?}");
    }

    /// The centre of rotation snaps to the centre, the corners and the
    /// middles of the sides of the selection and to the centre of another
    /// object, even with Snap to Objects off, and names the point it
    /// took; with snapping off it goes where it is put (#2352).
    #[test]
    fn the_centre_of_rotation_snaps_to_object_centres_and_edges() {
        let mut app = app();
        app.snap = SnapSettings {
            objects: false,
            ..SnapSettings::default()
        };
        app.settings.snap = crate::settings::SnapPrefs::default();
        let page = app.page_rect();
        let (x, y) = (page.x0 + 60.0, page.y0 + 60.0);
        let square = rect(&mut app, x, y, x + 30.0, y + 30.0);
        let circle = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(x, y + 60.0, x + 30.0, y + 90.0),
                arc: None,
            })
            .expect("circle");
        app.select(vec![square]);
        app.rotate_mode = true;
        // The other object's centre.
        let c = Point::new(x + 15.0, y + 75.0);
        assert_eq!(app.snap_pivot(c + Vec2::new(3.0, -2.0)), c);
        assert_eq!(app.snap_mark.get().map(|m| m.mode), Some(SnapMode::Center));
        // The selection's own centre, a corner and the middle of a side.
        app.snap_mark.set(None);
        let mid = Point::new(x + 15.0, y + 15.0);
        assert_eq!(app.snap_pivot(mid + Vec2::new(-2.0, 2.5)), mid);
        assert!(app.snap_mark.get().is_some(), "the point is named");
        let corner = Point::new(x + 30.0, y);
        assert_eq!(app.snap_pivot(corner + Vec2::new(2.0, -1.0)), corner);
        let side = Point::new(x + 15.0, y + 30.0);
        assert_eq!(app.snap_pivot(side + Vec2::new(1.5, 2.0)), side);
        // Far from every point it stays where it is put.
        let free = Point::new(x + 7.0, y + 45.0);
        assert_eq!(app.snap_pivot(free), free);
        // Snapping off: no snap at all.
        app.snap.off = true;
        let near = c + Vec2::new(3.0, -2.0);
        assert_eq!(app.snap_pivot(near), near);
        let _ = circle;
    }

    /// Rotate mode, the selection turned by one of its nodes: the node
    /// stops where its circle crosses a guideline it comes near, as it
    /// does on another object's edge (#3056).
    #[test]
    fn a_node_turned_onto_a_guideline_ends_on_it() {
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let page = app.page;
        app.run(Command::AddGuide {
            page,
            guide: Guide::vertical(32.0),
        });
        let id = rect(&mut app, 10.0, 10.0, 30.0, 30.0);
        app.select(vec![id]);
        app.rotate_mode = true;
        let center = Point::new(20.0, 20.0);
        let grab = Point::new(30.0, 30.0);
        let r = (grab - center).hypot();
        // The circle crosses x = 32 at y = 20 - sqrt(r² - 144).
        let cross = Point::new(32.0, 20.0 - (r * r - 144.0).sqrt());
        let v = cross - center;
        let (sn, cs) = 0.03_f64.sin_cos();
        let near = center + Vec2::new(v.x * cs - v.y * sn, v.x * sn + v.y * cs);
        let t = app.snap_turn_grab(center, grab, near, &[id]);
        assert!((t - cross).hypot() < 1e-9, "{t:?} should be {cross:?}");
        assert_eq!(
            app.snap_mark.get().map(|m| m.mode),
            Some(SnapMode::Guideline)
        );
        // Snap to Guidelines off: the node follows the pointer.
        app.snap.guides = false;
        let t = app.snap_turn_grab(center, grab, near, &[id]);
        assert!((t - cross).hypot() > 0.1, "{t:?}");
        assert!(((t - center).hypot() - r).abs() < 1e-9);
    }

    /// A moved object snaps at its new place at once: the boxes snapping
    /// reads are measured once per revision of the drawing, and a move
    /// bumps the revision, so no stale box survives (#3559).
    #[test]
    fn snapped_bounds_follow_a_moved_object() {
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let id = rect(&mut app, 100.0, 100.0, 110.0, 110.0);
        // Measured now: this ask fills the cache.
        let p = app.snap_point(Point::new(101.0, 101.0));
        assert_eq!(p, Point::new(100.0, 100.0), "the near corner");
        app.run(Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::translate(Vec2::new(50.0, 0.0)),
        });
        let p = app.snap_point(Point::new(101.0, 101.0));
        assert_eq!(p, Point::new(101.0, 101.0), "no snap where it was");
        let p = app.snap_point(Point::new(151.0, 101.0));
        assert_eq!(p, Point::new(150.0, 100.0), "snaps where it is");
    }

    /// An excluded selection of thousands costs a set membership per
    /// object, not a scan of the whole exclusion per object (#3559).
    #[test]
    fn snapping_with_a_large_exclusion_still_finds_others() {
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let mut excluded = Vec::new();
        for i in 0..2000u32 {
            let x = 400.0 + f64::from(i) * 15.0;
            excluded.push(rect(&mut app, x, 0.0, x + 10.0, 10.0));
        }
        rect(&mut app, 100.0, 100.0, 110.0, 110.0);
        let p = app.snap_point_excluding(Point::new(101.0, 101.0), &excluded);
        assert_eq!(p, Point::new(100.0, 100.0), "the kept object snaps");
        let p = app.snap_point_excluding(Point::new(406.0, 6.0), &excluded);
        assert_eq!(p, Point::new(406.0, 6.0), "the excluded one does not");
    }

    #[test]
    fn snapping_to_objects_turns_off_but_the_page_stays() {
        let mut app = app();
        rect(&mut app, 100.0, 100.0, 140.0, 120.0);
        app.snap.objects = false;
        app.snap.guides = false;
        let p = Point::new(103.0, 118.0);
        assert_eq!(app.snap_point(p), p);
        let page = app.page_rect();
        let corner = Point::new(page.x1 - 2.0, page.y1 - 3.0);
        assert_eq!(app.snap_point(corner), Point::new(page.x1, page.y1));
        app.snap.off = true;
        assert_eq!(app.snap_point(corner), corner);
    }

    #[test]
    fn a_move_carries_its_snap_point_onto_another_object() {
        let mut app = app();
        rect(&mut app, 100.0, 100.0, 140.0, 120.0);
        let moving = rect(&mut app, 20.0, 20.0, 30.0, 30.0);
        app.select(vec![moving]);
        // Grabbed at its top right corner node.
        let src = app.selection_snap_source(Point::new(29.0, 29.0));
        assert_eq!(src, Some(Point::new(30.0, 30.0)));
        let b = app.selection_bounds().unwrap();
        // Dropped near the other rectangle's top right corner.
        let d = app.snap_move_from(src, b, Vec2::new(107.0, 88.0), None);
        assert_eq!(Point::new(30.0, 30.0) + d, Point::new(140.0, 120.0));
        assert_eq!(app.snap_mark.get().map(|m| m.mode), Some(SnapMode::Node));
    }

    /// A symbol snaps as the objects it shows (#2135): the pointer over
    /// a selected symbol names its corner, a move carries that corner
    /// onto another symbol's corner, and a point drawn near a symbol's
    /// edge lands on it. A symbol offered only the middle of a 10 x 10 mm
    /// box at its lower left corner, whatever it showed.
    #[test]
    fn symbols_snap_by_what_they_show() {
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let symbol = |app: &mut App, x0: f64, y0: f64, x1: f64, y1: f64| {
            let id = rect(app, x0, y0, x1, y1);
            app.select(vec![id]);
            app.create_symbol_from_selection();
            assert_eq!(app.selection.len(), 1);
            app.selection[0]
        };
        let still = symbol(&mut app, 100.0, 100.0, 160.0, 140.0);
        let moving = symbol(&mut app, 20.0, 20.0, 50.0, 40.0);
        assert!(matches!(
            app.doc().find_shape(moving).map(|s| &s.kind),
            Some(ShapeKind::SymbolInstance { .. })
        ));
        app.select(vec![moving]);
        // Its own points: the top right corner, the middle of a side, its
        // centre.
        let at = |app: &App, x: f64, y: f64| {
            app.selection_hover_target(Point::new(x, y))
                .map(|t| (t.mode, t.point))
        };
        assert_eq!(
            at(&app, 49.0, 39.0),
            Some((SnapMode::Node, Point::new(50.0, 40.0)))
        );
        assert_eq!(
            at(&app, 20.5, 30.0),
            Some((SnapMode::Midpoint, Point::new(20.0, 30.0)))
        );
        assert_eq!(
            at(&app, 35.0, 30.5),
            Some((SnapMode::Center, Point::new(35.0, 30.0)))
        );
        // Nothing where the instance's unit box has its middle.
        assert_eq!(at(&app, 25.0, 25.0), None);
        // Carried by its top right corner next to the other symbol: the
        // corner lands on that symbol's top left corner.
        let src = app.selection_snap_source(Point::new(49.0, 39.0));
        let b = app.selection_bounds().unwrap();
        let d = app.snap_move_from(src, b, Vec2::new(51.0, 99.0), None);
        assert_eq!(Point::new(50.0, 40.0) + d, Point::new(100.0, 140.0));
        assert_eq!(app.snap_mark.get().map(|m| m.mode), Some(SnapMode::Node));
        // A point near the still symbol's right edge lands on the edge.
        let p = app.snap_point_excluding(Point::new(161.5, 112.0), &[moving]);
        assert!(
            (p.x - 160.0).abs() < 1e-9 && (p.y - 112.0).abs() < 1e-9,
            "{p:?}"
        );
        let _ = still;
    }

    #[test]
    fn a_move_grabbed_on_an_edge_follows_another_objects_edge() {
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        // A tall rectangle whose left edge is the line x = 100.
        rect(&mut app, 100.0, 50.0, 140.0, 150.0);
        let moving = rect(&mut app, 20.0, 20.0, 40.0, 60.0);
        app.select(vec![moving]);
        // Grabbed on its right edge, away from the nodes and the midpoint.
        let src = app.selection_snap_source(Point::new(40.5, 30.0));
        assert_eq!(src, Some(Point::new(40.0, 30.0)));
        let b = app.selection_bounds().unwrap();
        // Dragged near the other rectangle's left edge at two heights:
        // the grabbed point lands on that edge each time and slides along
        // it with the pointer.
        for dy in [60.0, 85.0] {
            let d = app.snap_move_from(src, b, Vec2::new(61.5, dy), None);
            assert!((40.0 + d.x - 100.0).abs() < 1e-9, "{d:?}");
            assert!((d.y - dy).abs() < 1e-9, "{d:?}");
            assert_eq!(app.snap_mark.get().map(|m| m.mode), Some(SnapMode::Edge));
            // The mark knows the edge it lies on, to light it up.
            match app.snap_mark.get().and_then(|m| m.edge) {
                Some(tracedraw_core::geometry::PathSeg::Line(l)) => {
                    assert!((l.p0.x - 100.0).abs() < 1e-9 && (l.p1.x - 100.0).abs() < 1e-9);
                }
                other => panic!("not the left edge: {other:?}"),
            }
        }
        // Far from it, it lets go.
        let d = app.snap_move_from(src, b, Vec2::new(50.0, 60.0), None);
        assert!((d.x - 50.0).abs() < 1e-9, "{d:?}");
    }

    /// A scale handle snaps to the selection's own nodes, midpoints and
    /// centre where they stood when it was grabbed, once it has left its
    /// start (#3074): before, it left the selection out altogether.
    #[test]
    fn a_scale_handle_snaps_to_where_the_selection_stood() {
        use crate::app::{Drag, Handle};
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let id = rect(&mut app, 20.0, 20.0, 60.0, 40.0);
        app.select(vec![id]);
        let b = app.selection_bounds().unwrap();
        let drag = |g: Option<std::rc::Rc<StillGeometry>>| Drag::Scale {
            handle: Handle::E,
            anchor: Point::new(b.x0, b.center().y),
            start_bounds: b,
            current: Point::new(b.x1, b.center().y),
            grab: Vec2::ZERO,
            mods: Default::default(),
            snapped: (false, false),
            flat: (false, false),
            start_geometry: g,
        };
        // Without its start geometry the right side passes the middle of
        // the box freely.
        app.drag = drag(None);
        let raw = Point::new(40.6, 30.4);
        let (q, _) = app.snap_scale_handle(raw);
        assert!((q.x - 40.6).abs() < 1e-9, "{q:?}");
        // With it, the side stops on where the box's centre was.
        app.drag = drag(Some(std::rc::Rc::new(app.selection_start_geometry())));
        let (q, (sx, _)) = app.snap_scale_handle(raw);
        assert_eq!(q.x, 40.0, "{q:?}");
        assert!(sx);
        // And on where its top right corner was, dragged past it and back.
        let (q, _) = app.snap_scale_handle(Point::new(60.5, 39.6));
        assert_eq!(q, Point::new(60.0, 40.0));
    }

    /// A move snaps to where the selection itself was when it began (its
    /// nodes, midpoints, centre and edges), once it has left the snapping
    /// radius of its start; before that nothing of its own attracts it,
    /// so it moves freely off its place (#1765).
    #[test]
    fn a_move_snaps_to_where_the_selection_started() {
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let moving = rect(&mut app, 20.0, 20.0, 40.0, 60.0);
        app.select(vec![moving]);
        let b = app.selection_bounds().unwrap();
        let start = app.selection_start_geometry();
        let start = Some(&start);
        // Grabbed at its top right node.
        let src = app.selection_snap_source(Point::new(39.5, 59.5));
        assert_eq!(src, Some(Point::new(40.0, 60.0)));
        // Still near its start: the move is as the pointer made it, the
        // node does not stick to where it was.
        let d = app.snap_move_from(src, b, Vec2::new(1.0, -1.5), None);
        assert_eq!(d, Vec2::new(1.0, -1.5));
        // Carried near where its top left node was: it lands there.
        app.snap_mark.set(None);
        let d = app.snap_move_from(src, b, Vec2::new(-18.5, 1.0), start);
        assert_eq!(Point::new(40.0, 60.0) + d, Point::new(20.0, 60.0));
        assert_eq!(app.snap_mark.get().map(|m| m.mode), Some(SnapMode::Node));
        // Brought back near its start after leaving it: exactly home.
        let d = app.snap_move_from(src, b, Vec2::new(1.0, -1.5), start);
        assert_eq!(d, Vec2::ZERO);
        // Carried near the middle of where its left side was: the midpoint.
        let d = app.snap_move_from(src, b, Vec2::new(-21.5, -19.0), start);
        assert_eq!(Point::new(40.0, 60.0) + d, Point::new(20.0, 40.0));
        assert_eq!(
            app.snap_mark.get().map(|m| m.mode),
            Some(SnapMode::Midpoint)
        );
        // Grabbed at its centre and carried near where its bottom right
        // node was.
        let src = app.selection_snap_source(Point::new(30.5, 40.5));
        assert_eq!(src, Some(Point::new(30.0, 40.0)));
        let d = app.snap_move_from(src, b, Vec2::new(11.0, -19.0), start);
        assert_eq!(Point::new(30.0, 40.0) + d, Point::new(40.0, 20.0));
        // Far from where it was: the pointer's move.
        let d = app.snap_move_from(src, b, Vec2::new(60.0, 70.0), start);
        assert_eq!(d, Vec2::new(60.0, 70.0));
        // Carried along where its right side ran: it slides on that line.
        let src = Some(Point::new(40.0, 60.0));
        let d = app.snap_move_from(src, b, Vec2::new(1.5, -15.0), start);
        assert_eq!(d, Vec2::new(0.0, -15.0));
        assert_eq!(app.snap_mark.get().map(|m| m.mode), Some(SnapMode::Edge));
        // Moved by no point of its own, its box does not line up with
        // where it stood (only other objects' boxes catch it).
        let d = app.snap_move_from(None, b, Vec2::new(70.0, 1.5), start);
        assert_eq!(d, Vec2::new(70.0, 1.5));
        // Snapping to objects off (and to the page, whose left edge the
        // box would meet): nothing of its own counts.
        app.snap.objects = false;
        app.snap.page = false;
        let d = app.snap_move_from(src, b, Vec2::new(-18.5, 1.0), start);
        assert_eq!(d, Vec2::new(-18.5, 1.0));
    }

    #[test]
    fn alignment_guides_pull_a_drawing_point_and_respect_snapping_off() {
        let mut app = app();
        app.settings.snap.threshold_px = 2.0;
        // Only the alignment guides act.
        app.snap.objects = false;
        app.snap.guides = false;
        rect(&mut app, 100.0, 100.0, 140.0, 120.0);
        let p = Point::new(141.5, 170.0);
        // Off by default: nothing moves.
        assert!(!SnapSettings::default().alignment_guides);
        assert_eq!(app.snap_point(p), p);
        assert!(app.live_guides.borrow().is_empty());
        app.snap.alignment_guides = true;
        assert_eq!(app.snap_point(p), Point::new(140.0, 170.0));
        {
            let o = app.live_guides.borrow();
            assert_eq!(o.lines.len(), 1, "{o:?}");
            let l = o.lines[0];
            assert!((l.a.x - 140.0).abs() < 1e-9 && (l.b.x - 140.0).abs() < 1e-9);
            assert!((l.a.y - 100.0).abs() < 1e-9 && (l.b.y - 170.0).abs() < 1e-9);
        }
        // The centre lines up too, on both axes at once.
        assert_eq!(
            app.snap_point(Point::new(119.0, 121.0)),
            Point::new(120.0, 120.0)
        );
        // Alt+Q: no snapping, no guides.
        app.clear_live_guides();
        app.snap.off = true;
        assert_eq!(app.snap_point(p), p);
        assert!(app.live_guides.borrow().is_empty());
        // A move aligns the moved box's edges with the other box.
        app.snap.off = false;
        let moving = rect(&mut app, 10.0, 10.0, 30.0, 30.0);
        app.select(vec![moving]);
        let b = app.selection_bounds().unwrap();
        let d = app.snap_move_from(None, b, Vec2::new(0.0, 91.0), None);
        assert!((b.y1 + d.y - 120.0).abs() < 1e-9, "{d:?}");
        assert!(!app.live_guides.borrow().lines.is_empty());
        // The boxes are measured once while nothing changes.
        let first = app.align_cache.borrow().as_ref().map(|c| c.targets.clone());
        app.snap_move_from(None, b, Vec2::new(0.0, 90.5), None);
        let second = app.align_cache.borrow().as_ref().map(|c| c.targets.clone());
        assert!(matches!((first, second), (Some(a), Some(b)) if std::rc::Rc::ptr_eq(&a, &b)));
    }

    #[test]
    fn equal_sizes_snap_while_drawing_and_resizing() {
        use crate::app::{Drag, Handle};
        let mut app = app();
        app.settings.snap.threshold_px = 2.0;
        app.snap.objects = false;
        app.snap.guides = false;
        app.snap.alignment_guides = true;
        // 40 x 20 mm.
        rect(&mut app, 100.0, 100.0, 140.0, 120.0);
        // Drawing a box from (10, 200): 41 x 21.5 becomes 40 x 20.
        app.drag = Drag::Box {
            start: Point::new(10.0, 200.0),
            current: Point::new(10.0, 200.0),
            from_center: false,
        };
        assert_eq!(
            app.snap_point(Point::new(51.0, 221.5)),
            Point::new(50.0, 220.0)
        );
        assert_eq!(app.live_guides.borrow().sizes.len(), 4);
        // Not while hovering before the press.
        app.drag = Drag::None;
        app.clear_live_guides();
        let p = Point::new(51.0, 221.5);
        assert_eq!(app.snap_point(p), p);
        // Resizing with the right side handle: only the width.
        let id = rect(&mut app, 10.0, 10.0, 30.0, 30.0);
        app.select(vec![id]);
        let b = app.selection_bounds().unwrap();
        app.drag = Drag::Scale {
            handle: Handle::E,
            anchor: Point::new(b.x0, b.center().y),
            start_bounds: b,
            current: Point::new(b.x1, b.center().y),
            grab: Vec2::ZERO,
            mods: Default::default(),
            snapped: (false, false),
            flat: (false, false),
            start_geometry: None,
        };
        let q = app.snap_point_excluding(Point::new(49.0, 21.0), &[id]);
        assert_eq!(q, Point::new(50.0, 21.0));
        assert_eq!(app.live_guides.borrow().sizes.len(), 2);
        // The option turns it off.
        app.settings.snap.align.equal_size = false;
        let p = Point::new(49.0, 21.0);
        assert_eq!(app.snap_point_excluding(p, &[id]), p);
    }

    #[test]
    fn the_multipage_view_aligns_with_objects_of_the_other_pages() {
        let mut app = app();
        app.settings.snap.threshold_px = 2.0;
        app.snap.objects = false;
        app.snap.guides = false;
        app.snap.alignment_guides = true;
        let first = app.page;
        app.add_page();
        let second = app.doc().pages[1].id;
        app.page = second;
        rect(&mut app, 10.0, 10.0, 50.0, 30.0);
        app.page = first;
        app.set_multipage(true);
        let r = app
            .multipage_layout()
            .into_iter()
            .find(|(id, _)| *id == second)
            .map(|(_, r)| r)
            .expect("the second page");
        // The rectangle's right side as displayed.
        let right = r.x0 + 50.0;
        let p = Point::new(right + 1.5, r.y0 + 170.0);
        let q = app.snap_point(p);
        assert!((q.x - right).abs() < 1e-9, "{q:?}");
        // Without the Multipage View the other page is not in sight.
        app.set_multipage(false);
        assert_eq!(app.snap_point(p), p);
    }

    #[test]
    fn dynamic_guides_come_from_acquired_points() {
        let mut app = app();
        app.settings.snap.threshold_px = 2.0;
        app.snap.dynamic_guides = true;
        rect(&mut app, 100.0, 100.0, 140.0, 120.0);
        // Resting on the corner acquires it.
        assert_eq!(
            app.snap_point(Point::new(100.5, 100.5)),
            Point::new(100.0, 100.0)
        );
        assert_eq!(*app.guide_anchors.borrow(), vec![Point::new(100.0, 100.0)]);
        // Away from the object, near the 45 degree guide from it.
        app.clear_live_guides();
        let q = app.snap_point(Point::new(150.5, 149.0));
        assert!(
            (q.x - 149.75).abs() < 1e-9 && (q.y - 149.75).abs() < 1e-9,
            "{q:?}"
        );
        let h = app.live_guides.borrow().dynamic.expect("a guide");
        assert_eq!(h.anchor, Point::new(100.0, 100.0));
        assert!((h.angle - 45.0).abs() < 1e-9);
        // Only the angles chosen: without 45 the point stays.
        app.settings.snap.dynamic.angles = vec![0.0, 90.0];
        let p = Point::new(150.5, 149.0);
        assert_eq!(app.snap_point(p), p);
        // Dynamic guides off, or snapping off: nothing.
        app.settings.snap.dynamic.angles = vec![45.0];
        app.snap.off = true;
        assert_eq!(app.snap_point(p), p);
        app.snap.off = false;
        app.snap.dynamic_guides = false;
        assert_eq!(app.snap_point(p), p);
    }

    #[test]
    fn lines_snap_to_tangent_and_perpendicular_points_from_their_start() {
        let mut app = app();
        app.settings.snap.threshold_px = 2.0;
        rect(&mut app, 100.0, 100.0, 140.0, 120.0);
        app.curve = Some(crate::app::CurveInProgress {
            nodes: vec![(Point::new(125.0, 60.0), None)],
            smooth: false,
            dragging_handle: false,
            bspline: false,
            ..Default::default()
        });
        let p = app.snap_point(Point::new(125.8, 99.5));
        assert_eq!(p, Point::new(125.0, 100.0));
        assert_eq!(
            app.snap_mark.get().map(|m| m.mode),
            Some(SnapMode::Perpendicular)
        );
    }

    /// A guideline dragged near the top of a curve lands where it touches
    /// it (no node there), and the place is marked; one dragged near a
    /// group's middle lands on the group's centre (#2004).
    #[test]
    fn a_dragged_guideline_touches_curves_and_takes_centres() {
        use tracedraw_core::geometry::BezPath;
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        // A bump from (10, 10) to (50, 10), at its highest y = 25 at x = 30.
        let mut path = BezPath::new();
        path.move_to((10.0, 10.0));
        path.curve_to((20.0, 30.0), (40.0, 30.0), (50.0, 10.0));
        app.new_shape(ShapeKind::Path {
            path,
            closed: false,
        })
        .expect("a bump");
        // Midpoints off: the bump's halfway point is its top too.
        app.settings.snap.modes.midpoint = false;
        app.snap_mark.set(None);
        assert!((app.snap_y_without_guides(23.5) - 25.0).abs() < 1e-9);
        let mark = app.snap_mark.get().expect("a mark");
        assert_eq!(mark.mode, SnapMode::Tangent);
        assert!((mark.point - Point::new(30.0, 25.0)).hypot() < 1e-9);
        // Two boxes grouped: the group's middle (x = 150) is no point of
        // either box.
        let a = rect(&mut app, 100.0, 100.0, 120.0, 120.0);
        let b = rect(&mut app, 180.0, 100.0, 200.0, 140.0);
        app.select(vec![a, b]);
        app.group_selection();
        assert!((app.snap_x_without_guides(151.5) - 150.0).abs() < 1e-9);
        assert_eq!(app.snap_mark.get().map(|m| m.mode), Some(SnapMode::Center));
        // The Pick tool names the group's centre under the pointer.
        assert_eq!(
            app.selection_hover_target(Point::new(150.5, 120.5))
                .map(|t| (t.mode, t.point)),
            Some((SnapMode::Center, Point::new(150.0, 120.0)))
        );
        // Snapping to objects off: neither.
        app.snap.objects = false;
        assert!((app.snap_x_without_guides(151.5) - 151.5).abs() < 1e-9);
    }

    /// The right side handle stops where the side meets another object's
    /// node along its length, far from the handle itself (#2004).
    #[test]
    fn a_side_handle_stops_on_a_point_the_side_meets() {
        use crate::app::{Drag, Handle};
        use tracedraw_core::geometry::BezPath;
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let id = rect(&mut app, 10.0, 10.0, 30.0, 30.0);
        // A wedge whose leftmost node (50, 12) is 8 mm from the handle's
        // point (49, 20) but on the right side's stretch (y 10 to 30).
        let mut path = BezPath::new();
        path.move_to((50.0, 12.0));
        path.line_to((70.0, 0.0));
        path.line_to((70.0, 5.0));
        path.close_path();
        app.new_shape(ShapeKind::Path { path, closed: true })
            .expect("a wedge");
        app.select(vec![id]);
        let b = app.selection_bounds().expect("bounds");
        let scale = |start_bounds: Rect, y: f64| Drag::Scale {
            handle: Handle::E,
            anchor: Point::new(10.0, y),
            start_bounds,
            current: Point::new(30.0, y),
            grab: Vec2::ZERO,
            mods: Default::default(),
            snapped: (false, false),
            flat: (false, false),
            start_geometry: None,
        };
        app.drag = scale(b, 20.0);
        let (q, axes) = app.snap_scale_handle(Point::new(49.0, 20.0));
        assert_eq!((q, axes), (Point::new(50.0, 20.0), (true, false)));
        assert_eq!(app.snap_mark.get().map(|m| m.mode), Some(SnapMode::Node));
        // Off the side's stretch (a side from y 20 to 30), nothing.
        app.drag = scale(Rect::new(10.0, 20.0, 30.0, 30.0), 25.0);
        let (q, axes) = app.snap_scale_handle(Point::new(49.0, 25.0));
        assert_eq!((q, axes), (Point::new(49.0, 25.0), (false, false)));
    }

    /// An object moved onto guidelines shows them highlighted: the ones
    /// its edges or centre lie on, and only while Snap to Guidelines is
    /// on (#3073).
    #[test]
    fn guidelines_an_object_snapped_to_are_highlighted() {
        use crate::app::Drag;
        let mut app = app();
        let page = app.page;
        for guide in [
            Guide::vertical(50.0),
            Guide::horizontal(100.0),
            Guide::vertical(80.0),
        ] {
            app.run(Command::AddGuide { page, guide });
        }
        let id = rect(&mut app, 10.0, 10.0, 30.0, 30.0);
        app.select(vec![id]);
        let b = app.selection_bounds().expect("bounds");
        let moved = |total: Vec2| Drag::Move {
            start: Point::ZERO,
            last: Point::ZERO,
            total,
            start_bounds: b,
            source: None,
            start_geometry: None,
        };
        // The left edge onto x = 50, the top onto y = 100.
        app.drag = moved(Vec2::new(50.0 - b.x0, 100.0 - b.y1));
        assert_eq!(
            app.snapped_guides(),
            vec![
                GuideLine::Vertical { x: 50.0 },
                GuideLine::Horizontal { y: 100.0 }
            ]
        );
        // Off the guidelines: none.
        app.drag = moved(Vec2::new(3.0, 3.0));
        assert!(app.snapped_guides().is_empty());
        // Snap to Guidelines off: none.
        app.drag = moved(Vec2::new(50.0 - b.x0, 100.0 - b.y1));
        app.snap.guides = false;
        assert!(app.snapped_guides().is_empty());
        app.snap.guides = true;
        app.drag = Drag::None;
        assert!(app.snapped_guides().is_empty());
    }

    /// A corner keeping the proportions, snapped onto a vertical
    /// guideline, stays on it while the pointer drifts up or down: it
    /// scales by the snapped axis, not by a mix of both (#2004).
    #[test]
    fn a_proportional_corner_stays_on_the_guideline_it_snapped_to() {
        use crate::app::{Drag, Handle};
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let page = app.page;
        app.run(Command::AddGuide {
            page,
            guide: Guide::vertical(50.0),
        });
        let id = rect(&mut app, 10.0, 10.0, 30.0, 30.0);
        app.select(vec![id]);
        let b = app.selection_bounds().expect("bounds");
        let anchor = Point::new(b.x0, b.y0);
        for y in [40.0, 45.0, 52.0] {
            app.drag = Drag::Scale {
                handle: Handle::NE,
                anchor,
                start_bounds: b,
                current: Point::new(b.x1, b.y1),
                grab: Vec2::ZERO,
                mods: Default::default(),
                snapped: (false, false),
                flat: (false, false),
                start_geometry: None,
            };
            let (q, snapped) = app.snap_scale_handle(Point::new(49.5, y));
            assert_eq!(snapped, (true, false), "{y}");
            if let Drag::Scale {
                current,
                snapped: s,
                ..
            } = &mut app.drag
            {
                *current = q;
                *s = snapped;
            }
            let t = app.preview_transform_of(&app.drag).expect("a scale");
            let corner = t * Point::new(b.x1, b.y1);
            assert!((corner.x - 50.0).abs() < 1e-9, "{y}: {corner:?}");
            // Kept in proportion.
            let r = t.transform_rect_bbox(b);
            assert!((r.width() - r.height()).abs() < 1e-9, "{y}: {r:?}");
        }
    }

    /// The nodes of an interactive fill snap while dragged (#2004).
    #[test]
    fn fill_nodes_snap_while_dragged() {
        use crate::app::Drag;
        use tracedraw_core::Fill;
        let mut app = app();
        app.settings.snap.threshold_px = 3.0;
        let id = rect(&mut app, 10.0, 10.0, 50.0, 30.0);
        rect(&mut app, 80.0, 10.0, 100.0, 30.0);
        app.select(vec![id]);
        app.fill_from_drag(id, Point::new(15.0, 20.0), Point::new(45.0, 20.0));
        assert!(matches!(
            app.doc().find_shape(id).map(|s| &s.fill),
            Some(Fill::Fountain(_))
        ));
        let (_, f, bb) = app.selected_fountain().expect("a fountain");
        let end = crate::fill_tool::fountain_handles(&f, bb)
            .into_iter()
            .find(|(h, _)| *h == crate::fill_tool::FountainHandle::End)
            .map(|(h, _)| h)
            .expect("the end node");
        app.drag = Drag::FountainHandle {
            handle: end,
            begun: false,
        };
        // Near the other box's corner (80, 10): onto it.
        assert!(app.fountain_drag_to(Point::new(81.5, 11.0), false));
        let (_, f, bb) = app.selected_fountain().expect("a fountain");
        let at = crate::fill_tool::fountain_handles(&f, bb)
            .into_iter()
            .find(|(h, _)| *h == end)
            .map(|(_, p)| p)
            .expect("the end node");
        assert!((at - Point::new(80.0, 10.0)).hypot() < 1e-6, "{at:?}");
    }
}
