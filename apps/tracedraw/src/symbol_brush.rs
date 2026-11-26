//! The Symbol Scatter panel's brush: with Brush on the drawing ticked, a
//! drag over a selected group (the copies a Symbol Scatter made, or any
//! group) changes the objects in it whose centres are under the nib:
//! Move carries them with the pointer, Gather pulls them in toward the
//! nib's centre, Grow makes them larger, Turn spins them, Tint takes
//! their colours toward the panel's colour and Fade makes them more
//! transparent. Alt (or Shift) held does the opposite for Gather (spread),
//! Grow (shrink), Turn (the other way) and Fade (more opaque). The nearer the
//! nib's centre, the more an object changes; the whole stroke is one
//! undo step. Nothing else about the pointer changes while the box is
//! not ticked.

use crate::app::App;
use egui::{Modifiers, PointerButton, Response};
use tracedraw_core::geometry::{Affine, Point, Vec2};
use tracedraw_core::{Color, Command, Fill, Shape, ShapeId, ShapeKind};

/// What a stroke of the symbol brush does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SymbolBrush {
    #[default]
    Move,
    Gather,
    Grow,
    Turn,
    Tint,
    Fade,
}

impl SymbolBrush {
    pub const ALL: [SymbolBrush; 6] = [
        SymbolBrush::Move,
        SymbolBrush::Gather,
        SymbolBrush::Grow,
        SymbolBrush::Turn,
        SymbolBrush::Tint,
        SymbolBrush::Fade,
    ];

    pub fn key(self) -> &'static str {
        match self {
            SymbolBrush::Move => "docker.scatter_brush_move",
            SymbolBrush::Gather => "docker.scatter_brush_gather",
            SymbolBrush::Grow => "docker.scatter_brush_grow",
            SymbolBrush::Turn => "docker.scatter_brush_turn",
            SymbolBrush::Tint => "docker.scatter_brush_tint",
            SymbolBrush::Fade => "docker.scatter_brush_fade",
        }
    }
}

/// How much an object `d` mm from the nib's centre takes of a stroke, 1
/// at the centre to 0 at the edge.
fn falloff(d: f64, radius: f64) -> f64 {
    if !(radius > 0.0) || d >= radius {
        return 0.0;
    }
    let t = 1.0 - d / radius;
    t * t
}

/// `c` taken `t` (0 to 1) of the way toward `to`.
fn mix(c: Color, to: Color, t: f64) -> Color {
    let (a, b) = (c.to_rgb8(), to.to_rgb8());
    let m = |i: usize| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t).round() as u8;
    Color::rgb8(m(0), m(1), m(2))
}

/// Every plain colour of `sh` (inside groups too) taken `t` toward `to`.
fn tint(sh: &mut Shape, to: Color, t: f64) {
    if let Fill::Solid(c) = sh.fill {
        sh.fill = Fill::Solid(mix(c, to, t));
    }
    if let Some(st) = &mut sh.stroke {
        st.color = mix(st.color, to, t);
    }
    if let ShapeKind::Group { children } = &mut sh.kind {
        for c in children {
            tint(c, to, t);
        }
    }
}

/// One step of the brush `how` on `children` (in the group's space): the
/// nib at `p` of `radius`, moved by `delta`, `motion` mm of pointer
/// travel, at `strength` (0 to 1), `swap` for the opposite. Whether any
/// object changed.
#[allow(clippy::too_many_arguments)]
pub fn brush_children(
    children: &mut [Shape],
    how: SymbolBrush,
    p: Point,
    delta: Vec2,
    radius: f64,
    strength: f64,
    swap: bool,
    color: Color,
) -> bool {
    let motion = delta.hypot();
    if motion <= 1e-9 || !(radius > 0.0) {
        return false;
    }
    let dose = motion / radius * strength.clamp(0.0, 1.0);
    let mut changed = false;
    for c in children.iter_mut() {
        let centre = c.bounds().center();
        let w = falloff((centre - p).hypot(), radius);
        if w <= 0.0 {
            continue;
        }
        let about = |m: Affine| {
            Affine::translate(centre.to_vec2()) * m * Affine::translate(-centre.to_vec2())
        };
        let t = match how {
            SymbolBrush::Move => Some(Affine::translate(delta * (w * strength))),
            SymbolBrush::Gather => {
                let k = (w * dose).min(0.5) * if swap { -1.0 } else { 1.0 };
                Some(Affine::translate((p - centre) * k))
            }
            SymbolBrush::Grow => {
                let k = 1.0 + (w * dose).min(0.5);
                Some(about(Affine::scale(if swap { 1.0 / k } else { k })))
            }
            SymbolBrush::Turn => {
                let a = (w * dose).min(0.5) * if swap { -1.0 } else { 1.0 };
                Some(about(Affine::rotate(a)))
            }
            SymbolBrush::Tint => {
                tint(c, color, (w * dose).min(1.0));
                None
            }
            SymbolBrush::Fade => {
                let k = (w * dose).min(1.0) * if swap { 1.0 } else { -1.0 };
                c.opacity = (c.opacity + k).clamp(0.05, 1.0);
                None
            }
        };
        if let Some(t) = t {
            c.carry_fills(t, Affine::IDENTITY);
            c.transform_by(t);
        }
        changed = true;
    }
    changed
}

impl App {
    /// The groups the symbol brush works on: the selected ones, while
    /// Brush on the drawing is ticked.
    pub fn symbol_brush_targets(&self) -> Vec<ShapeId> {
        if !self.symbol_scatter.brush_on {
            return Vec::new();
        }
        self.selected_shapes()
            .into_iter()
            .filter(|s| matches!(s.kind, ShapeKind::Group { .. }) && !s.locked)
            .map(|s| s.id)
            .collect()
    }

    /// The symbol brush's pointer input; false when it is off or nothing
    /// it can work on is selected, so the tool has the pointer.
    pub fn symbol_brush_input(&mut self, response: &Response, p: Point, mods: Modifiers) -> bool {
        if self.symbol_brush_targets().is_empty() {
            return false;
        }
        if response.drag_started_by(PointerButton::Primary) {
            self.symbol_brush_last = Some(p);
            self.symbol_brush_stepped = false;
            return true;
        }
        if response.dragged_by(PointerButton::Primary) {
            if let Some(last) = self.symbol_brush_last {
                self.symbol_brush_step(last, p, mods.alt || mods.shift);
            }
            self.symbol_brush_last = Some(p);
            return true;
        }
        if response.drag_stopped() {
            self.symbol_brush_last = None;
            self.symbol_brush_stepped = false;
            return true;
        }
        // A click still picks.
        false
    }

    /// The nib goes from `last` to `p`: the objects under it change, the
    /// whole stroke one undo step.
    pub fn symbol_brush_step(&mut self, last: Point, p: Point, swap: bool) {
        let s = &self.symbol_scatter;
        let (how, radius, strength, color) = (
            s.brush,
            s.brush_radius,
            s.brush_strength / 100.0,
            s.one_color,
        );
        let mut cmds = Vec::new();
        for id in self.symbol_brush_targets() {
            let Some(g) = self.doc().find_shape(id) else {
                continue;
            };
            let ShapeKind::Group { children } = &g.kind else {
                continue;
            };
            // The nib in the group's own space.
            let back = (self.doc().parent_transform(id).unwrap_or(Affine::IDENTITY) * g.transform)
                .inverse();
            let (q, q0) = (back * p, back * last);
            let mut children = children.clone();
            if brush_children(&mut children, how, q, q - q0, radius, strength, swap, color) {
                cmds.push(Command::SetShapeKind {
                    shape: id,
                    kind: ShapeKind::Group { children },
                });
            }
        }
        if cmds.is_empty() {
            return;
        }
        let label = "Symbol Brush";
        // One undo step for the whole stroke: the commands set the groups
        // whole, so the step before is undone and this one stands for both.
        if self.symbol_brush_stepped && self.engine.undo_label() == Some(label) {
            let _ = self.engine.undo();
        }
        if self.engine.run_batch(label, &cmds).is_ok() {
            self.symbol_brush_stepped = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{BezPath, Rect};

    fn dot(x: f64, y: f64) -> Shape {
        let mut p = BezPath::new();
        p.move_to((x - 1.0, y - 1.0));
        p.line_to((x + 1.0, y - 1.0));
        p.line_to((x + 1.0, y + 1.0));
        p.line_to((x - 1.0, y + 1.0));
        p.close_path();
        let mut s = Shape::new(
            ShapeId(0),
            ShapeKind::Path {
                path: p,
                closed: true,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(0, 0, 0));
        s
    }

    /// Each brush changes only the objects under the nib, the nearer the
    /// more, and Alt does the opposite.
    #[test]
    fn the_symbol_brushes_change_what_is_under_the_nib() {
        let base = vec![dot(0.0, 0.0), dot(5.0, 0.0), dot(50.0, 0.0)];
        let centre = |s: &Shape| s.bounds().center();
        let size = |s: &Shape| s.bounds().width();
        let run = |how, swap| {
            let mut c = base.clone();
            let changed = brush_children(
                &mut c,
                how,
                Point::new(0.0, 0.0),
                Vec2::new(4.0, 0.0),
                10.0,
                1.0,
                swap,
                Color::rgb8(255, 0, 0),
            );
            assert!(changed);
            // The far one never changes.
            assert_eq!(c[2], base[2]);
            c
        };
        let m = run(SymbolBrush::Move, false);
        assert!((centre(&m[0]).x - 4.0).abs() < 1e-9);
        assert!(centre(&m[1]).x > 5.0 && centre(&m[1]).x < 9.0);
        let g = run(SymbolBrush::Gather, false);
        assert!(centre(&g[1]).x < 5.0);
        let s = run(SymbolBrush::Gather, true);
        assert!(centre(&s[1]).x > 5.0);
        let big = run(SymbolBrush::Grow, false);
        assert!(size(&big[0]) > 2.0 && size(&big[1]) > 2.0 && size(&big[0]) > size(&big[1]));
        let small = run(SymbolBrush::Grow, true);
        assert!(size(&small[0]) < 2.0);
        let t = run(SymbolBrush::Turn, false);
        assert!((size(&t[0]) - 2.0).abs() > 1e-6);
        assert!((centre(&t[0]).x).abs() < 1e-9);
        let red = run(SymbolBrush::Tint, false);
        let Fill::Solid(c) = red[0].fill else {
            panic!()
        };
        assert!(c.to_rgb8()[0] > 0);
        let f = run(SymbolBrush::Fade, false);
        assert!(f[0].opacity < 1.0 && f[1].opacity < 1.0 && f[0].opacity < f[1].opacity);
        // No move, no change.
        let mut c = base.clone();
        assert!(!brush_children(
            &mut c,
            SymbolBrush::Grow,
            Point::ZERO,
            Vec2::ZERO,
            10.0,
            1.0,
            false,
            Color::BLACK,
        ));
        let _ = Rect::ZERO;
    }

    /// With Brush on the drawing ticked, a stroke over the selected group
    /// moves its objects under the nib, one undo step for the stroke;
    /// unticked, nothing is taken.
    #[test]
    fn a_symbol_brush_stroke_is_one_undo_step() {
        let mut app = App::headless();
        let layer = app.active_layer().expect("layer");
        let gid = app.engine.new_shape_id();
        let mut kids = Vec::new();
        for x in [0.0, 5.0, 50.0] {
            let mut d = dot(x, 0.0);
            d.id = app.engine.new_shape_id();
            kids.push(d);
        }
        let g = Shape::new(gid, ShapeKind::Group { children: kids });
        app.run(Command::AddShape { layer, shape: g });
        app.selection = vec![gid];
        assert!(app.symbol_brush_targets().is_empty());
        app.symbol_scatter.brush_on = true;
        assert_eq!(app.symbol_brush_targets(), vec![gid]);
        let before = app.doc().clone();
        app.symbol_brush_stepped = false;
        for k in 0..5 {
            let a = Point::new(k as f64, 0.0);
            app.symbol_brush_step(a, a + Vec2::new(1.0, 0.0), false);
        }
        let ShapeKind::Group { children } = &app.doc().find_shape(gid).expect("group").kind else {
            panic!()
        };
        assert!(children[0].bounds().center().x > 1.0);
        assert_eq!(children[2].bounds().center().x, 50.0);
        app.undo();
        assert_eq!(app.doc().pages, before.pages);
    }
}
