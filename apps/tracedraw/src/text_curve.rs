//! Curve Text: a text bent round a circle, over an arch, along a bowl or
//! along a wave from the text's property bar, with no path to draw. The
//! text stays a text fitted to a path of its own (`TextOnPath::preset`
//! names the curve), so it stays editable and Fit Text to Path's settings
//! still apply. A circle text dropped on a round object takes its centre
//! and fits inside its edge. See docs/behavior/text.md (Curve Text).

use crate::app::App;
use tracedraw_core::geometry::{Affine, Arc, BezPath, Point, Shape as _, Vec2};
use tracedraw_core::{Command, CurvePreset, Shape, ShapeId, ShapeKind, TextAlign, TextOnPath};

/// The key naming curve `k` (`None`: straight).
pub fn preset_key(k: Option<CurvePreset>) -> &'static str {
    match k {
        None => "text_curve.straight",
        Some(CurvePreset::Circle) => "text_curve.circle",
        Some(CurvePreset::ArcUp) => "text_curve.arc_up",
        Some(CurvePreset::ArcDown) => "text_curve.arc_down",
        Some(CurvePreset::Wave) => "text_curve.wave",
    }
}

/// How much of a circle an arch or a bowl takes, radians.
const ARC_SWEEP: f64 = std::f64::consts::TAU / 3.0;

/// The baseline curve `k` takes for a text `w` wide in letters `size`
/// high (both mm), in the text's own space, with the middle of the text
/// at the origin; and whether it is closed. A circle is `radius` round
/// when given.
pub fn preset_path(k: CurvePreset, w: f64, size: f64, radius: Option<f64>) -> (BezPath, bool) {
    let w = w.max(size).max(1.0);
    let arc = |center: Point, r: f64, start: f64, sweep: f64| {
        let mut p = BezPath::new();
        let a = Arc::new(center, Vec2::new(r, r), start, sweep, 0.0);
        p.move_to(a.center + Vec2::from_angle(start) * r);
        a.to_cubic_beziers(0.01, |p1, p2, q| p.curve_to(p1, p2, q));
        p
    };
    let half_pi = std::f64::consts::FRAC_PI_2;
    match k {
        // The text's space is y up: the letters stand to the left of the
        // way the curve runs.
        CurvePreset::Circle => {
            // The text runs most of the way round, a little gap left
            // where its ends meet; it starts at the top going clockwise,
            // so centred text sits over the top, its letters outward.
            let r = radius
                .filter(|r| r.is_finite() && *r > 0.0)
                .unwrap_or(w / 0.85 / std::f64::consts::TAU);
            let mut p = arc(Point::new(0.0, -r), r, half_pi, -std::f64::consts::TAU);
            p.close_path();
            (p, true)
        }
        CurvePreset::ArcUp => {
            // Over the top of a circle below, left to right (clockwise).
            let r = w * 1.15 / ARC_SWEEP;
            (
                arc(
                    Point::new(0.0, -r),
                    r,
                    half_pi + ARC_SWEEP / 2.0,
                    -ARC_SWEEP,
                ),
                false,
            )
        }
        CurvePreset::ArcDown => {
            // Along the bottom of a circle above, left to right.
            let r = w * 1.15 / ARC_SWEEP;
            (
                arc(Point::new(0.0, r), r, -half_pi - ARC_SWEEP / 2.0, ARC_SWEEP),
                false,
            )
        }
        CurvePreset::Wave => {
            // Two waves across the text, as high as half the letters.
            let len = w * 1.05;
            let amp = size * 0.5;
            let n = 96;
            let mut p = BezPath::new();
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let q = Point::new(
                    -len / 2.0 + len * t,
                    -amp * (t * 2.0 * std::f64::consts::TAU).sin(),
                );
                if i == 0 {
                    p.move_to(q);
                } else {
                    p.line_to(q);
                }
            }
            (p, false)
        }
    }
}

/// The letter size of a text, mm (its first span's).
fn text_size_mm(kind: &ShapeKind) -> f64 {
    match kind {
        ShapeKind::Text { spans, .. } => spans
            .first()
            .map(|s| s.size_pt * 25.4 / 72.0)
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(5.0),
        _ => 5.0,
    }
}

/// The curve of a text bent by Curve Text.
pub fn curve_of(s: &Shape) -> Option<CurvePreset> {
    match &s.kind {
        ShapeKind::Text {
            on_path: Some(p), ..
        } => p.preset,
        _ => None,
    }
}

/// `s` (an artistic text) bent to `k`, or straight with `None`; a circle
/// `radius` round when given. `None` for other objects.
fn curved(s: &Shape, k: Option<CurvePreset>, radius: Option<f64>) -> Option<ShapeKind> {
    let ShapeKind::Text {
        frame: None,
        on_path,
        ..
    } = &s.kind
    else {
        return None;
    };
    // Straight: the text's own width, unbent.
    let mut flat = s.clone();
    if let ShapeKind::Text { on_path, .. } = &mut flat.kind {
        *on_path = None;
    }
    let w = flat.local_path().bounding_box().width();
    let size = text_size_mm(&s.kind);
    let mut kind = flat.kind;
    let ShapeKind::Text {
        on_path: out,
        align,
        ..
    } = &mut kind
    else {
        return None;
    };
    match k {
        None => {
            // A path of the text's own (Fit Text to Path) stays; only a
            // Curve Text curve goes.
            if on_path.as_ref().is_some_and(|p| p.preset.is_none()) {
                return None;
            }
        }
        Some(k) => {
            let (path, closed) = preset_path(k, w, size, radius);
            let (a, offset) = crate::ops2::placement_on(&path, closed, TextAlign::Center);
            *align = a;
            // Keep the settings of an earlier curve (distance, mirrors).
            let keep = on_path.clone().unwrap_or_default();
            *out = Some(TextOnPath {
                path,
                offset,
                preset: Some(k),
                ..keep
            });
        }
    }
    Some(kind)
}

impl App {
    /// The curve of the selected texts, when they share one; `None` also
    /// for straight ones. The outer `None`: no artistic text selected.
    pub fn text_curve(&self) -> Option<Option<CurvePreset>> {
        let texts: Vec<Shape> = self
            .selected_shapes()
            .into_iter()
            .filter(|s| matches!(s.kind, ShapeKind::Text { frame: None, .. }))
            .collect();
        let first = curve_of(texts.first()?);
        Some(first.filter(|_| texts.iter().all(|s| curve_of(s) == first)))
    }

    /// Curve Text: the selected artistic texts bent to `k` (straight
    /// with `None`), each keeping its middle where it was; one undo step.
    pub fn set_text_curve(&mut self, k: Option<CurvePreset>) -> bool {
        let mut cmds = Vec::new();
        for s in self.selected_shapes() {
            if curve_of(&s) == k && k.is_some() {
                continue;
            }
            let Some(kind) = curved(&s, k, None) else {
                continue;
            };
            cmds.extend(keep_middle(&s, kind));
        }
        self.run_curve_cmds("Curve Text", &cmds)
    }

    fn run_curve_cmds(&mut self, label: &'static str, cmds: &[Command]) -> bool {
        if cmds.is_empty() {
            return false;
        }
        match self.engine.run_batch(label, cmds) {
            Ok(()) => true,
            Err(e) => {
                self.status = e.to_string();
                false
            }
        }
    }

    /// The round object under the middle of circle text `id`, near its
    /// centre: (its centre, its radius) on the page.
    fn round_object_under(&self, id: ShapeId, at: Point) -> Option<(Point, f64)> {
        self.page_layers()
            .into_iter()
            .rev()
            .filter(|l| l.visible && !l.locked)
            .flat_map(|l| l.shapes.iter().rev())
            .filter(|s| s.visible && s.id != id && !self.selection.contains(&s.id))
            .find_map(|s| {
                let b = s.page_path().bounding_box();
                let (w, h) = (b.width(), b.height());
                if w <= 1e-6 || h <= 1e-6 || (w / h - 1.0).abs() > 0.1 {
                    return None;
                }
                let round = matches!(s.kind, ShapeKind::Ellipse { arc: None, .. })
                    || (s.page_path().area().abs() / (w * h) - std::f64::consts::FRAC_PI_4).abs()
                        < 0.05;
                let r = (w + h) / 4.0;
                (round && (b.center() - at).hypot() <= r * 0.5).then_some((b.center(), r))
            })
    }

    /// After a move: a selected circle text let go over a round object
    /// (near its middle) takes the object's centre and runs round just
    /// inside its edge; one undo step of its own. True when one did.
    pub fn fit_curved_text_to_round(&mut self) -> bool {
        let mut cmds = Vec::new();
        for s in self.selected_shapes() {
            if curve_of(&s) != Some(CurvePreset::Circle) {
                continue;
            }
            let ShapeKind::Text {
                on_path: Some(op),
                origin,
                ..
            } = &s.kind
            else {
                continue;
            };
            // The middle of the circle the letters stand on (a path of
            // text is measured from the text's origin).
            let origin = *origin;
            let centre = s.transform * (op.path.bounding_box().center() + origin.to_vec2());
            let Some((c, r)) = self.round_object_under(s.id, centre) else {
                continue;
            };
            // The letters' tops a little inside the edge, in the text's
            // own measure (a scaled text keeps its look).
            let scale = s.transform.determinant().abs().sqrt().max(1e-9);
            let size = text_size_mm(&s.kind);
            let local_r = (r / scale - size * 1.15).max(size);
            let Some(kind) = curved(&s, Some(CurvePreset::Circle), Some(local_r)) else {
                continue;
            };
            let ShapeKind::Text {
                on_path: Some(np), ..
            } = &kind
            else {
                continue;
            };
            let now = s.transform * (np.path.bounding_box().center() + origin.to_vec2());
            cmds.push(Command::SetShapeKind { shape: s.id, kind });
            if (c - now).hypot() > 1e-9 {
                cmds.push(Command::TransformShapes {
                    shapes: vec![s.id],
                    transform: Affine::translate(c - now),
                });
            }
        }
        let done = self.run_curve_cmds("Fit Text to Shape", &cmds);
        if done {
            self.status = crate::i18n::tr("status.text_fitted_round");
        }
        done
    }
}

/// `s` given `kind`, moved so that the middle of its box stays put.
fn keep_middle(s: &Shape, kind: ShapeKind) -> Vec<Command> {
    let before = s.bounds().center();
    let mut after = s.clone();
    after.kind = kind.clone();
    let now = after.bounds().center();
    let mut cmds = vec![Command::SetShapeKind { shape: s.id, kind }];
    if (before - now).hypot() > 1e-9 && now.x.is_finite() && now.y.is_finite() {
        cmds.push(Command::TransformShapes {
            shapes: vec![s.id],
            transform: Affine::translate(before - now),
        });
    }
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;
    use tracedraw_core::TextSpan;

    fn text(app: &mut App, s: &str) -> ShapeId {
        tracedraw_text::install();
        let id = app
            .new_shape(ShapeKind::Text {
                spans: vec![TextSpan::new(s, "Arial", 24.0)],
                origin: Point::new(20.0, 100.0),
                frame: None,
                align: TextAlign::Left,
                para: Default::default(),
                on_path: None,
            })
            .expect("a text");
        app.select(vec![id]);
        id
    }

    fn shape(app: &App, id: ShapeId) -> Shape {
        app.doc().find_shape(id).expect("shape").clone()
    }

    /// Each curve bends the text in place and stays a text; straight
    /// again, it is as it was.
    #[test]
    fn curve_text_bends_and_straightens_in_place() {
        let mut app = App::headless();
        let id = text(&mut app, "SPACE EXPLORATION");
        let flat = shape(&app, id).bounds();
        assert_eq!(app.text_curve(), Some(None));
        for k in CurvePreset::ALL {
            assert!(app.set_text_curve(Some(k)), "{k:?}");
            let s = shape(&app, id);
            assert_eq!(curve_of(&s), Some(k));
            assert_eq!(app.text_curve(), Some(Some(k)));
            let b = s.bounds();
            assert!((b.center() - flat.center()).hypot() < 0.5, "{k:?} {b:?}");
            match k {
                // Round: about as tall as wide.
                CurvePreset::Circle => {
                    assert!((b.width() / b.height() - 1.0).abs() < 0.3, "{b:?}")
                }
                // Bent: taller than the straight text.
                _ => assert!(b.height() > flat.height() * 1.3, "{k:?} {b:?}"),
            }
        }
        // The arch reads over the top (the page is y up): its ends lie
        // below its middle.
        app.set_text_curve(Some(CurvePreset::ArcUp));
        let ShapeKind::Text {
            on_path: Some(op), ..
        } = &shape(&app, id).kind
        else {
            panic!("on a path");
        };
        let p0 = op.path.elements()[0].end_point().expect("start");
        assert!(p0.y < -1.0, "the arch's ends are lower: {p0:?}");
        // Its letters stand on it, over it: their tops above its top.
        let s = shape(&app, id);
        if let ShapeKind::Text { origin, .. } = &s.kind {
            let top = s.transform * (Point::new(0.0, op.path.bounding_box().y1) + origin.to_vec2());
            assert!(s.bounds().y1 > top.y + 3.0, "{:?} vs {top:?}", s.bounds());
        }
        assert!(app.set_text_curve(None));
        let b = shape(&app, id).bounds();
        assert!((b.width() - flat.width()).abs() < 0.5, "{b:?} vs {flat:?}");
        assert_eq!(curve_of(&shape(&app, id)), None);
    }

    /// A circle text let go on a round object takes its centre and runs
    /// just inside its edge.
    #[test]
    fn circle_text_fits_a_round_object_it_is_dropped_on() {
        let mut app = App::headless();
        let disc = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(100.0, 100.0, 180.0, 180.0),
                arc: None,
            })
            .expect("disc");
        let id = text(&mut app, "SPACE EXPLORATION DIVISION");
        app.set_text_curve(Some(CurvePreset::Circle));
        // Moved so that its middle is near the disc's.
        let c = shape(&app, id).bounds().center();
        app.engine
            .run(&Command::TransformShapes {
                shapes: vec![id],
                transform: Affine::translate(Point::new(143.0, 137.0) - c),
            })
            .expect("move");
        app.select(vec![id]);
        assert!(app.fit_curved_text_to_round());
        let s = shape(&app, id);
        let ShapeKind::Text {
            on_path: Some(op),
            origin,
            ..
        } = &s.kind
        else {
            panic!("on a path");
        };
        let c = s.transform * (op.path.bounding_box().center() + origin.to_vec2());
        assert!((c - Point::new(140.0, 140.0)).hypot() < 0.01, "{c:?}");
        // The letters inside the disc, their tops near its edge.
        let b = s.bounds();
        let disc_box = Rect::new(99.5, 99.5, 180.5, 180.5);
        assert!(
            disc_box.contains(Point::new(b.x0, b.y0)) && disc_box.contains(Point::new(b.x1, b.y1)),
            "{b:?}"
        );
        assert!(b.y1 > 172.0, "the tops near the edge: {b:?}");
        let _ = disc;
        // Far from any round object: nothing.
        app.engine
            .run(&Command::TransformShapes {
                shapes: vec![id],
                transform: Affine::translate((300.0, 0.0)),
            })
            .expect("move");
        assert!(!app.fit_curved_text_to_round());
    }
}
