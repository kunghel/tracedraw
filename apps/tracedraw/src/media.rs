//! Brush Strokes modes: Preset (tapered strokes), Brush (textured
//! strokes), Sprayer (objects along the path), Calligraphic, Expression
//! (pressure-like variable width), Tiles (a small picture repeated along
//! the path, bent to follow it) and Merge (a round stroke that becomes
//! one filled object with the same-coloured objects it touches). Each
//! mode turns a freehand polyline into geometry; "Apply to curve" does
//! the same to an existing curve.

use crate::app::App;
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::Point,
    style::{Stroke, WidthNode},
    Color, Command, Fill,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaMode {
    Preset,
    Brush,
    Sprayer,
    #[default]
    Calligraphic,
    Expression,
    Tiles,
    Merge,
}

impl MediaMode {
    pub const ALL: [MediaMode; 7] = [
        MediaMode::Preset,
        MediaMode::Brush,
        MediaMode::Sprayer,
        MediaMode::Calligraphic,
        MediaMode::Expression,
        MediaMode::Tiles,
        MediaMode::Merge,
    ];
    pub fn key(self) -> &'static str {
        match self {
            MediaMode::Preset => "media.preset",
            MediaMode::Brush => "media.brush",
            MediaMode::Sprayer => "media.sprayer",
            MediaMode::Calligraphic => "media.calligraphic",
            MediaMode::Expression => "media.expression",
            MediaMode::Tiles => "media.tiles",
            MediaMode::Merge => "media.merge",
        }
    }
}

/// The Tiles mode's pictures, in the order of `tracedraw_core::tiles`.
pub const TILE_KEYS: [&str; 6] = [
    "media.tiles_chain",
    "media.tiles_rope",
    "media.tiles_zigzag",
    "media.tiles_dash_dot",
    "media.tiles_waves",
    "media.tiles_beads",
];

pub const BRUSH_KEYS: [&str; 6] = [
    "media.taper_both",
    "media.taper_end",
    "media.taper_start",
    "media.wave",
    "media.bulge",
    "media.flat",
];

pub const SPRAY_KEYS: [&str; 5] = [
    "media.spray_circles",
    "media.spray_stars",
    "media.spray_squares",
    "media.spray_hearts",
    "media.spray_leaves",
];

/// Width profile along the stroke for the preset and brush modes: t in 0..1.
/// How thick a calligraphic nib is across, as a share of its width.
const CALLIGRAPHIC_STRETCH: f64 = 0.1;
/// Width nodes a preset stroke gets along its path: the width eases
/// between nodes, and each one more costs drawing time.
const PROFILE_NODES: usize = 8;
/// Width nodes an expression stroke's speed is sampled at.
const EXPRESSION_NODES: usize = 16;

fn profile(preset: usize, t: f64) -> f64 {
    match preset {
        0 => (t * std::f64::consts::PI).sin(),
        1 => 1.0 - t,
        2 => t,
        3 => 0.5 + 0.5 * (t * std::f64::consts::TAU * 2.0).sin().abs(),
        4 => 0.3 + 0.7 * (t * std::f64::consts::PI).sin(),
        _ => 1.0,
    }
    .max(0.02)
}

impl App {
    /// Geometry for a stroke in the current media mode. Returns shapes to
    /// add (several for the sprayer).
    pub fn media_shapes(&mut self, points: &[Point]) -> Vec<Shape> {
        self.media_shapes_pen(points, None)
    }

    /// `media_shapes` for a stroke drawn with a pen: `pen` is its
    /// pressure along `points`, which sets the width (#850).
    pub fn media_shapes_pen(
        &mut self,
        points: &[Point],
        pen: Option<&crate::pen::PressureProfile>,
    ) -> Vec<Shape> {
        let pts =
            tracedraw_core::geometry::simplify(points, 0.1 + self.media_smoothing / 100.0 * 1.5);
        if pts.len() < 2 {
            return Vec::new();
        }
        let color = match &self.default_stroke() {
            Some(st) => st.color,
            None => Color::BLACK,
        };
        // A stroke stays the path drawn, with the brush as its live
        // outline: its nodes edit like any curve's and the brush follows
        // (#829).
        if let Some(stroke) = self.media_stroke_pen(&pts, color, pen) {
            let id = self.engine.new_shape_id();
            let path = tracedraw_core::geometry::smooth_path(&pts, false);
            let mut s = Shape::new(
                id,
                ShapeKind::Path {
                    path,
                    closed: false,
                },
            );
            s.fill = Fill::None;
            s.stroke = Some(stroke);
            s.name = Some(self.media_mode_name().into());
            return vec![s];
        }
        // The sprayer and the tiles: the path drawn, with its objects
        // along it as a live effect, so they follow it when it is edited
        // and break apart into a group (#3435).
        let effect = match self.media_mode {
            MediaMode::Sprayer => self.spray_effect(color),
            MediaMode::Tiles => self.tiles_effect(color),
            _ => return Vec::new(),
        };
        let id = self.engine.new_shape_id();
        let path = tracedraw_core::geometry::smooth_path(&pts, false);
        let mut s = Shape::new(
            id,
            ShapeKind::Path {
                path,
                closed: false,
            },
        );
        s.fill = Fill::None;
        s.stroke = None;
        s.name = Some(self.media_mode_name().into());
        s.effects.push(effect);
        vec![s]
    }

    /// The tiles effect of the Brush Strokes settings in `color`: the
    /// picture chosen, as wide across as the width.
    pub fn tiles_effect(&self, color: Color) -> tracedraw_core::live::Effect {
        tracedraw_core::live::Effect::Tiles {
            preset: self.media_preset.min(TILE_KEYS.len() - 1) as u8,
            width: self.media_width,
            color,
        }
    }

    /// Brush Strokes in Merge mode: the stroke along `points` as a round
    /// nib of the width draws it, as a filled object in the outline
    /// colour (black with none), made one with every object of the
    /// active layer it overlaps that is filled with that same colour (as
    /// it shows on screen) and
    /// has no outline and no effects (unlocked, shown). One undo step;
    /// the merged object is selected and stands where the bottom one
    /// of those it took in stood. `None` when nothing was drawn.
    pub fn merge_brush_stroke(&mut self, points: &[Point]) -> Option<tracedraw_core::ShapeId> {
        use tracedraw_core::geometry::Shape as _;
        use tracedraw_core::shaping::{self, FillRule, Op};
        let layer = self.active_layer()?;
        let pts =
            tracedraw_core::geometry::simplify(points, 0.1 + self.media_smoothing / 100.0 * 1.5);
        let pts = if pts.is_empty() { points.to_vec() } else { pts };
        let band = shaping::stroke_band(&pts, self.media_width, false);
        if band.elements().is_empty() {
            return None;
        }
        let color = self.default_stroke().map_or(Color::BLACK, |st| st.color);
        let fill = Fill::Solid(color);
        let mut merged = band;
        let mut taken: Vec<tracedraw_core::ShapeId> = Vec::new();
        let mut at: Option<usize> = None;
        let shapes: Vec<Shape> = self
            .doc()
            .page(self.page)
            .ok()
            .and_then(|p| p.layers.iter().find(|l| l.id == layer))
            .map(|l| l.shapes.clone())
            .unwrap_or_default();
        for (i, s) in shapes.iter().enumerate() {
            // The same colour as it shows (a black in CMYK and one in
            // RGB are one).
            let same = matches!(&s.fill, Fill::Solid(c) if c.to_rgb8() == color.to_rgb8());
            let alike = same
                && s.stroke.is_none()
                && s.effects.is_empty()
                && s.visible
                && !s.locked
                && matches!(
                    s.kind,
                    ShapeKind::Path { .. }
                        | ShapeKind::Rect { .. }
                        | ShapeKind::Ellipse { .. }
                        | ShapeKind::Polygon { .. }
                );
            if !alike {
                continue;
            }
            let p = s.page_path();
            let (a, b) = (p.bounding_box(), merged.bounding_box());
            if a.x0 > b.x1 || b.x0 > a.x1 || a.y0 > b.y1 || b.y0 > a.y1 {
                continue;
            }
            let meet = shaping::overlay(&merged, &p, Op::Intersect);
            if shaping::filled_area(&meet, FillRule::EvenOdd) < 1e-6 {
                continue;
            }
            merged = shaping::overlay(&merged, &p, Op::Weld);
            taken.push(s.id);
            at.get_or_insert(i);
        }
        let path = shaping::fit_polygon_curves(&merged, 0.02);
        let id = self.engine.new_shape_id();
        let mut s = Shape::new(id, ShapeKind::Path { path, closed: true });
        s.fill = fill;
        s.stroke = None;
        s.name = Some(self.media_mode_name().into());
        let mut cmds = Vec::new();
        if !taken.is_empty() {
            cmds.push(Command::DeleteShapes {
                shapes: taken.clone(),
            });
        }
        cmds.push(Command::AddShape { layer, shape: s });
        // In the place of the bottom object it took in.
        if let Some(index) = at {
            cmds.push(Command::Reorder {
                shape: id,
                layer,
                index,
            });
        }
        self.engine.run_batch("Brush Strokes", &cmds).ok()?;
        self.selection = vec![id];
        Some(id)
    }

    /// The sprayer effect of the Brush Strokes settings in `color`.
    pub fn spray_effect(&self, color: Color) -> tracedraw_core::live::Effect {
        tracedraw_core::live::Effect::Spray {
            preset: self.media_preset.min(u8::MAX as usize) as u8,
            size: self.media_width,
            spacing: self.media_spacing.max(0.5),
            color,
        }
    }

    /// The live outline the current mode draws along `pts` in `color`:
    /// a calligraphic nib, or widths along the path from the preset's
    /// profile (with a jittered edge for Brush, the drawing speed for
    /// Expression). `None` for the sprayer.
    pub fn media_stroke(&self, pts: &[Point], color: Color) -> Option<Stroke> {
        self.media_stroke_pen(pts, color, None)
    }

    /// `media_stroke` for a stroke drawn with a pen (#850): its pressure
    /// along the drag scales the width of the Preset, Brush and
    /// Expression modes, from `1 - pressure setting` of it at no pressure
    /// (half, by default) to the full width at full pressure; in
    /// Expression mode it takes the place of the drawing speed. The
    /// calligraphic nib and the sprayer keep their size.
    pub fn media_stroke_pen(
        &self,
        pts: &[Point],
        color: Color,
        pen: Option<&crate::pen::PressureProfile>,
    ) -> Option<Stroke> {
        let width = self.media_width;
        let mut st = Stroke::new(color, width);
        st.cap = tracedraw_core::style::LineCap::Butt;
        let preset = self.media_preset;
        let pen_floor = 1.0 - self.media_pressure.clamp(0.0, 1.0);
        let widths = |n: usize, w_at: &dyn Fn(f64) -> f64| -> Vec<WidthNode> {
            if let Some(pen) = pen {
                return pen.width_nodes(n, width, pen_floor, w_at);
            }
            (0..=n)
                .map(|i| {
                    let t = i as f64 / n as f64;
                    WidthNode::symmetric(t, width * w_at(t).max(0.0))
                })
                .collect()
        };
        match self.media_mode {
            MediaMode::Calligraphic => {
                st.stretch = CALLIGRAPHIC_STRETCH;
                st.nib_angle = self.media_angle;
            }
            MediaMode::Preset => st.widths = widths(PROFILE_NODES, &|t| profile(preset, t)),
            MediaMode::Expression if pen.is_some() => {
                // A real pen: its pressure is the width.
                st.widths = widths(EXPRESSION_NODES, &|_| 1.0);
            }
            MediaMode::Brush => {
                // Nodes at the crests and troughs of the jittered edge: the
                // width eases between them, so they draw the whole wave.
                let pen_scale = |t: f64| match pen {
                    Some(pen) => pen_floor + (1.0 - pen_floor) * pen.at(t, 0.5 / 40.0),
                    None => 1.0,
                };
                let jitter =
                    |t: f64| profile(preset, t) * (0.85 + 0.15 * (t * 40.0).sin()) * pen_scale(t);
                let mut at = vec![0.0];
                at.extend(
                    (0..)
                        .map(|k| {
                            (std::f64::consts::FRAC_PI_2 + k as f64 * std::f64::consts::PI) / 40.0
                        })
                        .take_while(|t| *t < 1.0),
                );
                at.push(1.0);
                st.widths = at
                    .into_iter()
                    .map(|t| WidthNode::symmetric(t, width * jitter(t).max(0.0)))
                    .collect();
            }
            MediaMode::Expression => {
                // Pressure: speed between samples stands in for pen
                // pressure (slow = wide), scaled by the docker's setting.
                let mut cum = vec![0.0];
                let mut dists = vec![0.0];
                for w in pts.windows(2) {
                    let d = (w[1] - w[0]).hypot();
                    dists.push(d);
                    cum.push(cum.last().copied().unwrap_or(0.0) + d);
                }
                let total = cum.last().copied().unwrap_or(0.0).max(1e-9);
                let max = dists.iter().cloned().fold(0.0, f64::max).max(1e-9);
                let pressure = self.media_pressure;
                let at_point = |t: f64| {
                    let i = cum.partition_point(|c| *c < t * total).min(dists.len() - 1);
                    ((1.0 - dists[i] / max) * 0.7 + 0.3) * (0.5 + 0.5 * pressure)
                };
                st.widths = widths(EXPRESSION_NODES, &at_point);
            }
            MediaMode::Sprayer | MediaMode::Tiles | MediaMode::Merge => return None,
        }
        Some(st)
    }

    /// The name a stroke of the current mode gets.
    fn media_mode_name(&self) -> &'static str {
        match self.media_mode {
            MediaMode::Calligraphic => "Calligraphic",
            MediaMode::Preset => "Preset stroke",
            MediaMode::Brush => "Brush stroke",
            MediaMode::Expression => "Expression stroke",
            MediaMode::Sprayer => "Sprayed objects",
            MediaMode::Tiles => "Tiles",
            MediaMode::Merge => "Merged stroke",
        }
    }

    /// Brush Strokes docker > Apply to curve: use the selected curves as
    /// the path of a stroke in the current mode.
    pub fn apply_media_to_selection(&mut self) {
        if self.active_layer().is_none() {
            return;
        }
        let curves: Vec<Shape> = self
            .selected_shapes()
            .into_iter()
            .filter(|s| matches!(s.kind, ShapeKind::Path { .. }))
            .collect();
        let mut cmds = Vec::new();
        let mut new_sel = Vec::new();
        for c in curves {
            // The curve keeps its nodes and takes the brush as its outline.
            let color = c.stroke.as_ref().map_or(Color::BLACK, |st| st.color);
            let pts = tracedraw_core::effects::resample(&c.page_path(), 120);
            if let Some(stroke) = self.media_stroke(&pts, color) {
                new_sel.push(c.id);
                cmds.push(Command::SetStroke {
                    shapes: vec![c.id],
                    stroke: Some(stroke),
                });
                continue;
            }
            // The sprayer and the tiles: the curve stays, with its
            // objects along it as a live effect (#3435), one in place of
            // any it had. Merge does nothing to a curve.
            let effect = match self.media_mode {
                MediaMode::Sprayer => self.spray_effect(color),
                MediaMode::Tiles => self.tiles_effect(color),
                _ => continue,
            };
            let mut effects = c.effects.clone();
            effects.retain(|e| {
                !matches!(
                    e,
                    tracedraw_core::live::Effect::Spray { .. }
                        | tracedraw_core::live::Effect::Tiles { .. }
                )
            });
            effects.push(effect);
            new_sel.push(c.id);
            cmds.push(Command::SetEffects {
                shape: c.id,
                effects,
            });
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Brush Strokes", &cmds);
            self.selection = new_sel;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::BezPath;
    use tracedraw_core::geometry::Shape as _;

    /// The texts the property bar draws.
    fn bar_texts(app: &mut App) -> Vec<String> {
        let ctx = crate::theme::ui_context();
        let mut texts = Vec::new();
        // Twice: the first frame only lays out the bar.
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(6000.0, 60.0),
                    )),
                    ..Default::default()
                },
                |ui| crate::ui::toolbar::property_bar(app, ui),
            );
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::epaint::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect();
        }
        texts
    }

    /// The Brush Strokes property bar has the mode list the hint speaks
    /// of, showing the mode in use, and the mode's own fields: the nib
    /// angle for Calligraphic, the shape and spacing for Sprayer, the
    /// shape for Preset. It held the calligraphic fields only (#1681).
    #[test]
    fn the_property_bar_lists_the_brush_modes() {
        use crate::i18n::tr;
        let mut app = App::headless();
        app.tool = crate::tools::Tool::BrushStrokes;
        let texts = bar_texts(&mut app);
        assert!(texts.contains(&tr("toolbar.brush_mode")), "{texts:?}");
        assert!(texts.contains(&tr("media.calligraphic")), "{texts:?}");
        assert!(texts.contains(&tr("docker.nib_angle")), "{texts:?}");
        app.media_mode = MediaMode::Sprayer;
        let texts = bar_texts(&mut app);
        assert!(texts.contains(&tr("media.sprayer")), "{texts:?}");
        assert!(texts.contains(&tr(SPRAY_KEYS[0])), "{texts:?}");
        assert!(texts.contains(&tr("docker.spacing")), "{texts:?}");
        assert!(!texts.contains(&tr("docker.nib_angle")), "{texts:?}");
        app.media_mode = MediaMode::Preset;
        app.media_preset = 2;
        let texts = bar_texts(&mut app);
        assert!(texts.contains(&tr(BRUSH_KEYS[2])), "{texts:?}");
        assert!(!texts.contains(&tr("docker.spacing")), "{texts:?}");
    }

    /// A brush stroke stays the path drawn, open and editable, with the
    /// brush as its live outline (#829): its nodes are few curves, and it
    /// is drawn as wide as the brush.
    #[test]
    fn a_brush_stroke_is_a_live_outline_on_its_path() {
        let mut app = App::headless();
        let pts: Vec<Point> = (0..=200)
            .map(|i| {
                let t = i as f64 / 200.0 * std::f64::consts::PI;
                Point::new(20.0 + 60.0 * t.cos(), 100.0 + 40.0 * t.sin())
            })
            .collect();
        app.media_width = 8.0;
        for mode in [
            MediaMode::Preset,
            MediaMode::Brush,
            MediaMode::Calligraphic,
            MediaMode::Expression,
        ] {
            app.media_mode = mode;
            let shapes = app.media_shapes(&pts);
            assert_eq!(shapes.len(), 1, "{mode:?}");
            let s = &shapes[0];
            let ShapeKind::Path { path, closed } = &s.kind else {
                panic!("{mode:?}: {:?}", s.kind);
            };
            assert!(!closed, "{mode:?}");
            assert!(
                path.elements().len() < 60,
                "{mode:?}: {}",
                path.elements().len()
            );
            let st = s.stroke.as_ref().expect("outline");
            match mode {
                MediaMode::Calligraphic => assert!(st.stretch < 0.5),
                _ => assert!(!st.widths.is_empty(), "{mode:?}"),
            }
            let band = tracedraw_core::shaping::outline_band(path, st, 1.0).expect("band");
            let reach = band.bounding_box().height() - path.bounding_box().height();
            assert!(reach > 1.0 && reach < 9.0, "{mode:?}: {reach}");
        }
        // The sprayer still sprays objects, along the path drawn.
        app.media_mode = MediaMode::Sprayer;
        let shapes = app.media_shapes(&pts);
        assert_eq!(shapes.len(), 1);
        let ev = tracedraw_core::live::evaluate(&shapes[0]);
        let ShapeKind::Group { children } = &ev.above[0].kind else {
            panic!("{:?}", ev.above);
        };
        assert!(children.len() > 1);
    }

    /// Tiles: the path drawn with its tiles along it as a live effect,
    /// each tile as wide across as the width; Apply to curve gives a
    /// curve the same, one in place of any it had.
    #[test]
    fn tiles_lie_along_the_stroke_as_a_live_effect() {
        let mut app = App::headless();
        let pts: Vec<Point> = (0..=100)
            .map(|i| Point::new(10.0 + i as f64, 50.0))
            .collect();
        app.media_width = 6.0;
        app.media_mode = MediaMode::Tiles;
        app.media_preset = 5;
        let shapes = app.media_shapes(&pts);
        assert_eq!(shapes.len(), 1);
        assert!(shapes[0].stroke.is_none());
        assert!(matches!(
            shapes[0].effects[..],
            [tracedraw_core::live::Effect::Tiles { preset: 5, .. }]
        ));
        let ev = tracedraw_core::live::evaluate(&shapes[0]);
        let ShapeKind::Group { children } = &ev.above[0].kind else {
            panic!("{:?}", ev.above);
        };
        // 100 mm of 6 mm beads.
        assert_eq!(children.len(), 17);
        let b = ev.above[0].bounds();
        assert!(b.height() > 4.0 && b.height() <= 6.0 + 1e-6, "{b:?}");
        // Apply to curve on a curve with a sprayer: tiles in its place.
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((50.0, 0.0));
        let id = app
            .new_shape(ShapeKind::Path {
                path: line,
                closed: false,
            })
            .expect("curve");
        app.selection = vec![id];
        app.media_mode = MediaMode::Sprayer;
        app.apply_media_to_selection();
        app.media_mode = MediaMode::Tiles;
        app.apply_media_to_selection();
        let fx = &app.doc().find_shape(id).expect("curve").effects;
        assert!(
            matches!(fx[..], [tracedraw_core::live::Effect::Tiles { .. }]),
            "{fx:?}"
        );
    }

    /// Merge: a stroke over two squares of its colour with no outline
    /// makes one object of the three, where the lower square stood; a red
    /// square and an outlined one stay as they were; one undo brings
    /// the squares back.
    #[test]
    fn a_merge_stroke_becomes_one_object_with_what_it_touches() {
        let mut app = App::headless();
        app.media_mode = MediaMode::Merge;
        app.media_width = 4.0;
        let sq = |app: &mut App, x: f64, fill: Fill, stroke: Option<Stroke>| {
            app.new_shape_styled(
                ShapeKind::Rect {
                    rect: tracedraw_core::geometry::Rect::new(x, 0.0, x + 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
                |s| {
                    s.fill = fill;
                    s.stroke = stroke;
                },
            )
            .expect("square")
        };
        // The outline colour new objects get, the stroke's colour.
        let ink = app.default_stroke().map_or(Color::BLACK, |st| st.color);
        let below = sq(&mut app, 100.0, Fill::Solid(ink), None);
        let a = sq(&mut app, 0.0, Fill::Solid(ink), None);
        let b = sq(&mut app, 30.0, Fill::Solid(ink), None);
        let red = sq(&mut app, 60.0, Fill::Solid(Color::rgb8(255, 0, 0)), None);
        let lined = sq(
            &mut app,
            80.0,
            Fill::Solid(ink),
            Some(Stroke::new(ink, 0.5)),
        );
        let pts: Vec<Point> = (0..=90).map(|i| Point::new(5.0 + i as f64, 5.0)).collect();
        let before = app.doc().clone();
        let id = app.merge_brush_stroke(&pts).expect("merged");
        assert_eq!(app.selection, vec![id]);
        for gone in [a, b] {
            assert!(app.doc().find_shape(gone).is_none());
        }
        for kept in [below, red, lined] {
            assert!(app.doc().find_shape(kept).is_some());
        }
        let m = app.doc().find_shape(id).expect("merged").clone();
        assert_eq!(m.fill, Fill::Solid(ink));
        assert!(m.stroke.is_none());
        let bb = m.bounds();
        assert!(bb.x0 < 0.5 && bb.y1 > 9.5, "{bb:?}");
        // One object: the band and both squares joined.
        let parts = m
            .local_path()
            .elements()
            .iter()
            .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
            .count();
        assert_eq!(parts, 1);
        // Where square a stood: just above the lower square.
        let layer = app.active_layer().expect("layer");
        let order: Vec<_> = app
            .doc()
            .page(app.page)
            .expect("page")
            .layers
            .iter()
            .find(|l| l.id == layer)
            .expect("layer")
            .shapes
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(order[..2], [below, id]);
        app.undo();
        assert_eq!(app.doc().pages, before.pages);
    }

    /// The sprayer leaves the path drawn with its objects along it as one
    /// live group: the path's nodes edited, the objects follow, and Break
    /// Apart leaves them grouped (#3435: the objects were left loose,
    /// with no line).
    #[test]
    fn sprayed_objects_follow_their_path_and_break_apart_as_a_group() {
        let mut app = App::headless();
        app.media_mode = MediaMode::Sprayer;
        app.media_spacing = 10.0;
        app.media_width = 4.0;
        let pts: Vec<Point> = (0..=40)
            .map(|i| Point::new(10.0 + i as f64 * 2.0, 50.0))
            .collect();
        let shapes = app.media_shapes(&pts);
        assert_eq!(shapes.len(), 1, "one path, not loose objects");
        let path = shapes[0].clone();
        assert!(matches!(path.kind, ShapeKind::Path { closed: false, .. }));
        let layer = app.active_layer().expect("layer");
        let id = path.id;
        app.engine
            .run(&Command::AddShape { layer, shape: path })
            .expect("add");
        let sprayed = |app: &App| {
            let s = app.doc().find_shape(id).expect("the path stays").clone();
            let ev = tracedraw_core::live::evaluate(&s);
            let g = ev.above.first().expect("the objects").clone();
            let ShapeKind::Group { children } = &g.kind else {
                panic!("{:?}", g.kind);
            };
            assert_eq!(children.len(), 9, "80 mm at 10 mm apart");
            g.page_path().bounding_box()
        };
        let before = sprayed(&app);
        assert!(before.y0 > 45.0 && before.y1 < 55.0, "{before:?}");
        // A click on an object picks the path that sprays it.
        assert_eq!(app.hit_test(Point::new(30.0, 50.0)), Some(id));
        // The path bent up at its end: the objects go with it.
        let mut bent = BezPath::new();
        bent.move_to(Point::new(10.0, 50.0));
        bent.line_to(Point::new(50.0, 50.0));
        bent.line_to(Point::new(50.0, 90.0));
        app.engine
            .run(&Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path {
                    path: bent,
                    closed: false,
                },
            })
            .expect("edit");
        let after = sprayed(&app);
        assert!(after.y1 > 85.0 && after.x1 < 55.0, "{after:?}");
        // Break Apart: the objects become a group of their own.
        app.select(vec![id]);
        app.break_effect_groups();
        let groups: Vec<_> = app.doc().pages[0]
            .layers
            .iter()
            .flat_map(|l| &l.shapes)
            .filter(|s| matches!(&s.kind, ShapeKind::Group { children } if children.len() == 9))
            .collect();
        assert_eq!(groups.len(), 1, "one group of sprayed objects");
    }

    /// Apply to curve with the sprayer keeps the curve and sprays along it.
    #[test]
    fn spraying_a_curve_keeps_the_curve() {
        let mut app = App::headless();
        let mut line = BezPath::new();
        line.move_to(Point::new(10.0, 10.0));
        line.line_to(Point::new(90.0, 10.0));
        let id = app
            .new_shape(ShapeKind::Path {
                path: line,
                closed: false,
            })
            .expect("curve");
        app.select(vec![id]);
        app.media_mode = MediaMode::Sprayer;
        app.apply_media_to_selection();
        let s = app.doc().find_shape(id).expect("the curve stays");
        assert!(s
            .effects
            .iter()
            .any(|e| matches!(e, tracedraw_core::live::Effect::Spray { .. })));
        assert_eq!(app.selection, vec![id]);
    }

    /// Apply to curve keeps the curve, with the brush as its outline.
    #[test]
    fn apply_to_curve_keeps_the_curve() {
        let mut app = App::headless();
        let mut line = BezPath::new();
        line.move_to(Point::new(10.0, 10.0));
        line.curve_to(
            Point::new(40.0, 0.0),
            Point::new(60.0, 40.0),
            Point::new(90.0, 20.0),
        );
        let id = app
            .new_shape(ShapeKind::Path {
                path: line.clone(),
                closed: false,
            })
            .expect("curve");
        app.select(vec![id]);
        app.media_mode = MediaMode::Preset;
        app.apply_media_to_selection();
        let s = app.doc().find_shape(id).expect("the curve stays");
        assert!(
            matches!(&s.kind, ShapeKind::Path { path, .. } if path.elements() == line.elements())
        );
        assert!(!s.stroke.as_ref().expect("outline").widths.is_empty());
        assert_eq!(app.selection, vec![id]);
    }
}

/// How long a page of brush strokes takes to draw (#829): `cargo test
/// --release -p tracedraw -- --ignored frame_times_of_brush_strokes
/// --nocapture`.
#[cfg(test)]
#[test]
#[ignore]
fn frame_times_of_brush_strokes() {
    for mode in [
        MediaMode::Brush,
        MediaMode::Preset,
        MediaMode::Expression,
        MediaMode::Calligraphic,
    ] {
        let mut app = App::headless();
        app.media_mode = mode;
        app.media_width = 6.0;
        let layer = app.active_layer().expect("layer");
        let mut cmds = Vec::new();
        for k in 0..200 {
            let pts: Vec<Point> = (0..=150)
                .map(|i| {
                    let t = i as f64 / 150.0;
                    Point::new(
                        10.0 + 180.0 * t,
                        10.0 + (k as f64) * 1.4 + 6.0 * (t * 9.0).sin(),
                    )
                })
                .collect();
            for s in app.media_shapes(&pts) {
                cmds.push(Command::AddShape { layer, shape: s });
            }
        }
        app.engine.run_batch("strokes", &cmds).expect("add");
        let doc = app.doc().clone();
        let page = doc.pages[0].id;
        let opts = tracedraw_render::RenderOptions {
            width: 840,
            height: 1188,
            view: tracedraw_render::ViewTransform {
                zoom: 4.0,
                origin_x: 0.0,
                origin_y: 1188.0,
                stretch_y: 1.0,
            },
            ..Default::default()
        };
        for pass in 0..2 {
            let t = std::time::Instant::now();
            let mut n = 0;
            for s in doc.pages[0].layers.iter().flat_map(|l| &l.shapes) {
                if let Some(st) = &s.stroke {
                    n += tracedraw_core::shaping::outline_band(&s.page_path(), st, 1.0)
                        .map_or(0, |b| b.elements().len());
                }
            }
            eprintln!(
                "{mode:?} bands pass {pass}: {:?}, {n} elements",
                t.elapsed()
            );
        }
        let t = std::time::Instant::now();
        for _ in 0..3 {
            tracedraw_render::render_page(&doc, page, &opts);
        }
        eprintln!(
            "{mode:?}: 200 strokes drawn in {:?} a page",
            t.elapsed() / 3
        );
    }
}
