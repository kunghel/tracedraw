//! The Action Lines tool's settings and geometry (#1617): speed lines,
//! parallel or radial, with their count, how much their lengths vary,
//! the empty middle of radial lines, their width (thin outline lines, or
//! filled strokes of a width that varies, square, tapered to a point at
//! one end or at both), how unevenly they are spaced, their angle, radial
//! lines following an oval box, which side parallel lines start from, and
//! a seed that deals another set. The same settings and box always give
//! the same lines, so the live preview is the object made on release.
//! See docs/behavior/shape-tools.md, "Action Lines".

use tracedraw_core::geometry::{Affine, BezPath, Point, Rect, Vec2};

/// How a filled line ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Taper {
    /// The same width all along.
    None,
    /// A point at the start (radial: the middle) end, full width at the
    /// other: classic speed lines.
    #[default]
    Tip,
    /// A point at both ends, widest in the middle.
    Both,
}

impl Taper {
    pub const ALL: [Taper; 3] = [Taper::None, Taper::Tip, Taper::Both];
    pub fn key(self) -> &'static str {
        match self {
            Taper::None => "toolbar.action_taper_none",
            Taper::Tip => "toolbar.action_taper_tip",
            Taper::Both => "toolbar.action_taper_both",
        }
    }
}

/// Which side of the box parallel lines start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    /// They end at the right edge, starting at random distances (as
    /// before this setting).
    #[default]
    End,
    /// They start at the left edge.
    Start,
    /// Centred across the box.
    Centre,
}

impl Anchor {
    pub const ALL: [Anchor; 3] = [Anchor::End, Anchor::Start, Anchor::Centre];
    pub fn key(self) -> &'static str {
        match self {
            Anchor::End => "toolbar.action_anchor_end",
            Anchor::Start => "toolbar.action_anchor_start",
            Anchor::Centre => "toolbar.action_anchor_centre",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ActionLines {
    /// 2 to 500.
    pub count: u32,
    pub radial: bool,
    /// How much shorter than the longest a line may come out, 0 to 100 %.
    pub length_var: f64,
    /// Radial: the empty middle, as a share of the radius, 0 to 90 %.
    pub inner: f64,
    /// Line width, mm; 0 draws thin lines with the default outline.
    pub width: f64,
    /// How much thinner than `width` a line may come out, 0 to 100 %.
    pub width_var: f64,
    pub taper: Taper,
    /// How far a line may stray from even spacing, 0 to 100 % of a step.
    pub spacing_var: f64,
    /// Degrees: parallel lines turned about the box's middle; radial
    /// lines' first direction.
    pub angle: f64,
    /// Radial lines reach the box's ellipse instead of the circle in it.
    pub oval: bool,
    pub anchor: Anchor,
    /// Another number deals other random lengths, widths and spacing.
    pub seed: u32,
}

impl Default for ActionLines {
    fn default() -> Self {
        ActionLines {
            count: 12,
            radial: false,
            length_var: 70.0,
            inner: 30.0,
            width: 0.0,
            width_var: 50.0,
            taper: Taper::Tip,
            spacing_var: 0.0,
            angle: 0.0,
            oval: false,
            anchor: Anchor::End,
            seed: 0,
        }
    }
}

impl ActionLines {
    /// Filled strokes rather than outline lines.
    pub fn filled(&self) -> bool {
        self.width > 0.0
    }

    /// The lines across `rect`: open segments, or closed outlines of the
    /// filled strokes (see [`ActionLines::filled`]).
    pub fn path(&self, rect: Rect) -> BezPath {
        let n = self.count.clamp(2, 500) as usize;
        let lv = (self.length_var / 100.0).clamp(0.0, 1.0);
        let sv = (self.spacing_var / 100.0).clamp(0.0, 1.0);
        let wv = (self.width_var / 100.0).clamp(0.0, 1.0);
        let mut seed: u32 = 0x9E37_79B9 ^ n as u32 ^ self.seed.wrapping_mul(0x85EB_CA6B);
        if seed == 0 {
            seed = 1;
        }
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % 1000) as f64 / 1000.0
        };
        let mut segs: Vec<(Point, Point)> = Vec::with_capacity(n);
        let c = rect.center();
        if self.radial {
            let (rx, ry) = if self.oval {
                (rect.width() / 2.0, rect.height() / 2.0)
            } else {
                let r = rect.width().min(rect.height()) / 2.0;
                (r, r)
            };
            let inner = (self.inner / 100.0).clamp(0.0, 0.9);
            let start = self.angle.to_radians();
            for i in 0..n {
                let jitter = (rnd() - 0.5) * sv;
                let a = start + std::f64::consts::TAU * (i as f64 + jitter) / n as f64;
                let at = |f: f64| c + Vec2::new(a.cos() * rx * f, a.sin() * ry * f);
                let f1 = 1.0 - 0.25 * lv * rnd();
                let f0 = (inner + (f1 - inner) * 0.6 * lv * rnd()).min(f1 - 0.02);
                segs.push((at(f0.max(0.0)), at(f1)));
            }
        } else {
            let step = rect.height() / n as f64;
            for i in 0..n {
                let y = rect.y0 + step * (i as f64 + 0.5 + (rnd() - 0.5) * sv);
                let len = rect.width() * (1.0 - lv * rnd());
                let (x0, x1) = match self.anchor {
                    Anchor::End => (rect.x1 - len, rect.x1),
                    Anchor::Start => (rect.x0, rect.x0 + len),
                    Anchor::Centre => (c.x - len / 2.0, c.x + len / 2.0),
                };
                // The thin end points away from where they gather.
                let (a, b) = match self.anchor {
                    Anchor::Start => (Point::new(x1, y), Point::new(x0, y)),
                    _ => (Point::new(x0, y), Point::new(x1, y)),
                };
                segs.push((a, b));
            }
            if self.angle.abs() > 1e-9 {
                let t = Affine::rotate_about(self.angle.to_radians(), c);
                for s in &mut segs {
                    *s = (t * s.0, t * s.1);
                }
            }
        }
        let mut path = BezPath::new();
        for (a, b) in segs {
            if !self.filled() {
                path.move_to(a);
                path.line_to(b);
                continue;
            }
            let w = self.width * (1.0 - wv * rnd());
            let d = b - a;
            let len = d.hypot();
            if len < 1e-9 {
                continue;
            }
            let half = Vec2::new(-d.y, d.x) / len * (w / 2.0);
            match self.taper {
                Taper::None => {
                    path.move_to(a + half);
                    path.line_to(b + half);
                    path.line_to(b - half);
                    path.line_to(a - half);
                }
                Taper::Tip => {
                    path.move_to(a);
                    path.line_to(b + half);
                    path.line_to(b - half);
                }
                Taper::Both => {
                    let m = a.midpoint(b);
                    path.move_to(a);
                    path.line_to(m + half);
                    path.line_to(b);
                    path.line_to(m - half);
                }
            }
            path.close_path();
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{PathEl, Shape as _};

    fn r() -> Rect {
        Rect::new(0.0, 0.0, 100.0, 60.0)
    }

    fn segments(p: &BezPath) -> Vec<(Point, Point)> {
        let mut out = Vec::new();
        let mut at = Point::ZERO;
        for el in p.elements() {
            match *el {
                PathEl::MoveTo(q) => at = q,
                PathEl::LineTo(q) => out.push((at, q)),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn the_same_settings_give_the_same_lines_and_a_seed_others() {
        let a = ActionLines::default();
        assert_eq!(a.path(r()), a.path(r()));
        let b = ActionLines { seed: 7, ..a };
        assert_ne!(a.path(r()), b.path(r()));
        let radial = ActionLines { radial: true, ..a };
        assert_eq!(radial.path(r()).elements().len(), 24, "12 open segments");
    }

    #[test]
    fn lengths_vary_as_set_and_parallel_lines_keep_their_side() {
        let even = ActionLines {
            length_var: 0.0,
            ..Default::default()
        };
        for (a, b) in segments(&even.path(r())) {
            assert!((a.x - 0.0).abs() < 1e-9 && (b.x - 100.0).abs() < 1e-9);
        }
        let start = ActionLines {
            anchor: Anchor::Start,
            ..Default::default()
        };
        for (a, b) in segments(&start.path(r())) {
            assert!(
                (b.x - 0.0).abs() < 1e-9 && a.x <= 100.0 + 1e-9,
                "{a:?} {b:?}"
            );
        }
        let centre = ActionLines {
            anchor: Anchor::Centre,
            ..Default::default()
        };
        for (a, b) in segments(&centre.path(r())) {
            assert!(((a.x + b.x) / 2.0 - 50.0).abs() < 1e-9);
        }
    }

    #[test]
    fn radial_lines_leave_the_middle_empty_and_follow_an_oval() {
        let a = ActionLines {
            radial: true,
            inner: 50.0,
            ..Default::default()
        };
        let c = r().center();
        for (p, q) in segments(&a.path(r())) {
            assert!(
                (p - c).hypot() >= 15.0 - 1e-6,
                "inner half of the 30 mm radius"
            );
            assert!((q - c).hypot() <= 30.0 + 1e-6);
        }
        let oval = ActionLines {
            radial: true,
            oval: true,
            length_var: 0.0,
            angle: 0.0,
            ..Default::default()
        };
        let ends: Vec<Point> = segments(&oval.path(r())).into_iter().map(|s| s.1).collect();
        assert!(
            (ends[0].x - 100.0).abs() < 1e-6,
            "reaches the box's right edge"
        );
        assert!(ends.iter().all(|p| (((p.x - 50.0) / 50.0).powi(2)
            + ((p.y - 30.0) / 30.0).powi(2)
            - 1.0)
            .abs()
            < 1e-6));
    }

    #[test]
    fn wide_lines_are_filled_strokes_tapered_as_chosen() {
        let count = |t: Taper| {
            let a = ActionLines {
                width: 2.0,
                width_var: 0.0,
                taper: t,
                ..Default::default()
            };
            let p = a.path(r());
            let closes = p
                .elements()
                .iter()
                .filter(|e| matches!(e, PathEl::ClosePath))
                .count();
            (closes, p.elements().len(), p.area().abs())
        };
        let (c, els, area_tip) = count(Taper::Tip);
        assert_eq!((c, els), (12, 12 * 4), "a triangle each");
        let (_, els, area_none) = count(Taper::None);
        assert_eq!(els, 12 * 5);
        let (_, els, area_both) = count(Taper::Both);
        assert_eq!(els, 12 * 5);
        assert!(
            area_tip > 0.0 && (area_none - 2.0 * area_tip).abs() < 1e-6,
            "{area_none} {area_tip}"
        );
        assert!((area_both - area_tip).abs() < 1e-6, "{area_both}");
    }

    #[test]
    fn an_angle_turns_parallel_lines_and_spacing_can_stray() {
        let a = ActionLines {
            angle: 90.0,
            length_var: 0.0,
            ..Default::default()
        };
        for (p, q) in segments(&a.path(r())) {
            assert!((p.x - q.x).abs() < 1e-6, "vertical");
        }
        let even = ActionLines::default();
        let loose = ActionLines {
            spacing_var: 100.0,
            ..even
        };
        let ys = |a: &ActionLines| {
            segments(&a.path(r()))
                .iter()
                .map(|s| s.0.y)
                .collect::<Vec<_>>()
        };
        assert_eq!(ys(&even)[0], 2.5);
        assert_ne!(ys(&even), ys(&loose));
        assert!(ys(&loose).iter().all(|y| *y >= 0.0 && *y <= 60.0));
    }
}
