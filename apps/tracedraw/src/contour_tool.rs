//! The Contour tool and the settings its bar and the Contour panel share
//! (`Effect::Contour`, drawn by `tracedraw_core::live`).
//!
//! A drag from an object draws a live contour: away from the object's
//! middle past its edge gives an outside contour, toward the middle an
//! inside one; the distance from the start makes `steps * offset` (the
//! bar's steps kept, the offset following). One undo step per drag. The
//! bar and the panel show the selected contour and edit the selected
//! contours live.
//!
//! An object with a contour shows two handles on the canvas while the
//! tool is active (#1265): from the middle of its right edge a line runs
//! out (Outside) or in (Inside, To centre) to the offset handle at the
//! last step, which sets the distance; the steps handle on that line at
//! the first step's distance sets the number of steps, the last step
//! staying where it is. Each drag is drawn live and is one undo step.

use crate::app::App;
use serde::{Deserialize, Serialize};
use tracedraw_core::{
    geometry::Point, live::Effect, shaping::Join, Color, Command, ShapeId, ShapeKind,
};

/// How near (screen pixels) a press takes a contour handle.
pub const HANDLE_PX: f64 = 9.0;

/// A handle of the selected contour on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContourHandle {
    /// At the last step: the distance (`steps * offset`, or the offset
    /// to centre).
    Offset,
    /// At the first step's distance: the number of steps.
    Steps,
}

/// Where the selected contour's handles are, in page millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContourHandles {
    pub id: ShapeId,
    /// The middle of the object's right edge, where the line starts.
    pub anchor: Point,
    /// The offset handle.
    pub end: Point,
    /// The steps handle (none to centre, where the steps are as many as
    /// fit).
    pub steps: Option<Point>,
    /// 1 when the steps go out (right), -1 when they go in (left).
    pub dir: f64,
}

impl ContourHandles {
    /// The handles of contour settings `s` on an object with page bounds
    /// `b`.
    pub fn of(id: ShapeId, s: &ContourSettings, b: tracedraw_core::geometry::Rect) -> Self {
        let s = s.clamped();
        let anchor = Point::new(b.x1, (b.y0 + b.y1) / 2.0);
        let dir = if s.direction == ContourDirection::Outside {
            1.0
        } else {
            -1.0
        };
        let n = f64::from(s.steps.max(1));
        let (total, steps) = match s.direction {
            ContourDirection::ToCenter => (s.offset, None),
            _ => (
                n * s.offset,
                Some(Point::new(anchor.x + dir * s.offset, anchor.y)),
            ),
        };
        ContourHandles {
            id,
            anchor,
            end: Point::new(anchor.x + dir * total, anchor.y),
            steps,
            dir,
        }
    }

    /// The settings `s` with handle `h` dragged to `p`: the offset handle
    /// sets the distance from the edge (the steps kept), the steps handle
    /// the number of steps that fit in that distance (the last step kept).
    pub fn dragged(&self, s: &ContourSettings, h: ContourHandle, p: Point) -> ContourSettings {
        let mut s = s.clamped();
        // Along the line, from the edge, at least a hundredth of a
        // millimetre a step.
        let t = (p.x - self.anchor.x) * self.dir;
        let n = f64::from(s.steps.max(1));
        match (h, s.direction) {
            (ContourHandle::Offset, ContourDirection::ToCenter) => s.offset = t.max(0.01),
            (ContourHandle::Offset, _) => s.offset = (t / n).max(0.01),
            (ContourHandle::Steps, ContourDirection::ToCenter) => {}
            (ContourHandle::Steps, _) => {
                let total = n * s.offset;
                let steps = (total / t.max(1e-6)).round().clamp(1.0, 999.0) as u32;
                s.steps = steps;
                s.offset = total / f64::from(steps);
            }
        }
        s.clamped()
    }
}

/// Where the steps go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContourDirection {
    ToCenter,
    Inside,
    #[default]
    Outside,
}

impl ContourDirection {
    pub const ALL: [ContourDirection; 3] = [
        ContourDirection::ToCenter,
        ContourDirection::Inside,
        ContourDirection::Outside,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ContourDirection::ToCenter => "docker.to_center",
            ContourDirection::Inside => "docker.inside",
            ContourDirection::Outside => "docker.outside",
        }
    }
}

pub fn join_key(j: Join) -> &'static str {
    match j {
        Join::Miter => "contour.corner_miter",
        Join::Round => "contour.corner_round",
        Join::Bevel => "contour.corner_bevel",
    }
}

pub fn blend_key(b: u8) -> &'static str {
    match b {
        1 => "docker.clockwise",
        2 => "docker.counterclockwise",
        _ => "docker.direct",
    }
}

/// The Contour tool's and panel's values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContourSettings {
    pub direction: ContourDirection,
    pub steps: u32,
    /// Between steps, mm.
    pub offset: f64,
    pub corners: Join,
    /// 0 direct, 1 clockwise, 2 counterclockwise through the hues.
    pub color_blend: u8,
    /// The last step's fill and outline colours.
    pub fill_to: Color,
    pub outline_to: Color,
    /// -1 to 1.
    pub accel_objects: f64,
    pub accel_colors: f64,
}

impl Default for ContourSettings {
    fn default() -> Self {
        ContourSettings {
            direction: ContourDirection::Outside,
            steps: 3,
            offset: 2.0,
            corners: Join::Miter,
            color_blend: 0,
            fill_to: Color::WHITE,
            outline_to: Color::BLACK,
            accel_objects: 0.0,
            accel_colors: 0.0,
        }
    }
}

fn fin(v: f64, lo: f64, hi: f64, or: f64) -> f64 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        or
    }
}

impl ContourSettings {
    pub fn clamped(self) -> Self {
        let d = ContourSettings::default();
        ContourSettings {
            steps: self.steps.clamp(1, 999),
            offset: fin(self.offset, 0.001, 1000.0, d.offset),
            color_blend: self.color_blend.min(2),
            accel_objects: fin(self.accel_objects, -1.0, 1.0, 0.0),
            accel_colors: fin(self.accel_colors, -1.0, 1.0, 0.0),
            ..self
        }
    }

    pub fn effect(&self) -> Effect {
        let s = self.clamped();
        Effect::Contour {
            steps: s.steps,
            offset: s.offset,
            outside: s.direction == ContourDirection::Outside,
            to_center: s.direction == ContourDirection::ToCenter,
            fill_to: s.fill_to,
            outline_to: Some(s.outline_to),
            color_blend: s.color_blend,
            corners: Some(s.corners),
            accel_objects: s.accel_objects,
            accel_colors: s.accel_colors,
        }
    }

    /// Take the values of contour `e`.
    pub fn take(&mut self, e: &Effect) {
        let Effect::Contour {
            steps,
            offset,
            outside,
            to_center,
            fill_to,
            outline_to,
            color_blend,
            corners,
            accel_objects,
            accel_colors,
        } = e
        else {
            return;
        };
        self.direction = if *to_center {
            ContourDirection::ToCenter
        } else if *outside {
            ContourDirection::Outside
        } else {
            ContourDirection::Inside
        };
        self.steps = *steps;
        self.offset = *offset;
        self.corners = corners.unwrap_or(Join::Round);
        self.color_blend = *color_blend;
        self.fill_to = *fill_to;
        self.outline_to = outline_to.unwrap_or(*fill_to);
        self.accel_objects = *accel_objects;
        self.accel_colors = *accel_colors;
        *self = self.clamped();
    }
}

/// A drag of the Contour tool under way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContourDrag {
    pub id: ShapeId,
    pub start: Point,
}

fn is_contour(e: &Effect) -> bool {
    matches!(e, Effect::Contour { .. })
}

fn with_contour(effects: &[Effect], c: Option<Effect>) -> Vec<Effect> {
    let mut out = Vec::with_capacity(effects.len() + 1);
    let mut placed = false;
    for e in effects {
        if is_contour(e) {
            if !placed {
                out.extend(c.clone());
                placed = true;
            }
        } else {
            out.push(e.clone());
        }
    }
    if !placed {
        out.extend(c);
    }
    out
}

impl App {
    /// The first selected object's contour.
    pub fn selected_contour(&self) -> Option<Effect> {
        self.selected_shapes()
            .into_iter()
            .find_map(|s| s.effects.iter().find(|e| is_contour(e)).cloned())
    }

    /// The handles of the selected object's contour, with the Contour
    /// tool and one object selected that has a contour.
    pub fn contour_handles(&self) -> Option<ContourHandles> {
        if self.tool != crate::tools::Tool::Contour || self.selection.len() != 1 {
            return None;
        }
        let id = *self.selection.first()?;
        let s = self.doc().shape_on_page(id)?;
        let e = s.effects.iter().find(|e| is_contour(e))?;
        let mut st = self.contour;
        st.take(e);
        let b = s.bounds();
        if ![b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()) {
            return None;
        }
        Some(ContourHandles::of(id, &st, b))
    }

    /// The contour handle under page point `p` (the steps handle wins
    /// over the offset handle when they meet).
    pub fn contour_handle_at(&self, p: Point) -> Option<ContourHandle> {
        let h = self.contour_handles()?;
        let tol = HANDLE_PX / f64::from(self.view.zoom.max(1e-6));
        if h.steps.is_some_and(|q| (q - p).hypot() <= tol) {
            return Some(ContourHandle::Steps);
        }
        ((h.end - p).hypot() <= tol).then_some(ContourHandle::Offset)
    }

    /// Handle `handle` dragged to `p`: the contour follows live. The first
    /// move of a drag (`begun` false) is a new undo step, the next ones
    /// change it, so a drag is one step.
    pub fn drag_contour_handle(&mut self, handle: ContourHandle, p: Point, begun: bool) {
        let Some(h) = self.contour_handles() else {
            return;
        };
        let s = self.contour_handles_settings(h.id);
        let next = h.dragged(&s, handle, p);
        if next == s && begun {
            return;
        }
        if !begun {
            self.effect_edit_step = None;
        }
        self.contour = next;
        self.apply_contour_settings();
    }

    /// The settings of object `id`'s contour.
    fn contour_handles_settings(&self, id: ShapeId) -> ContourSettings {
        let mut st = self.contour;
        if let Some(e) = self
            .doc()
            .find_shape(id)
            .and_then(|s| s.effects.iter().find(|e| is_contour(e)).cloned())
        {
            st.take(&e);
        }
        st
    }

    /// The bar and the panel show the selected contour.
    pub fn sync_contour_settings(&mut self) {
        if let Some(e) = self.selected_contour() {
            let mut s = self.contour;
            s.take(&e);
            self.contour = s;
        }
    }

    fn change_contours(&mut self, label: &'static str, next: Option<Effect>, add: bool) {
        let mut cmds = Vec::new();
        let mut ids = Vec::new();
        for s in self.selected_shapes() {
            let has = s.effects.iter().any(is_contour);
            if (!has && !add)
                || (add && has)
                || matches!(s.kind, ShapeKind::Bitmap { .. } | ShapeKind::Group { .. })
            {
                continue;
            }
            let effects = with_contour(&s.effects, next.clone());
            if effects == s.effects {
                continue;
            }
            ids.push(s.id);
            cmds.push(Command::SetEffects {
                shape: s.id,
                effects,
            });
        }
        if cmds.is_empty() {
            return;
        }
        let key = (self.engine.revision(), ids.clone(), label.to_string());
        if self.engine.undo_label() == Some(label) && self.effect_edit_step.as_ref() == Some(&key) {
            let _ = self.engine.undo();
        }
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
            return;
        }
        self.effect_edit_step = Some((self.engine.revision(), ids, label.to_string()));
    }

    /// The selected contours take the settings.
    pub fn apply_contour_settings(&mut self) {
        self.contour = self.contour.clamped();
        let e = self.contour.effect();
        self.change_contours("Contour", Some(e), false);
    }

    /// Apply: the settings' contour on the selected objects that have
    /// none.
    pub fn add_contour(&mut self) {
        self.contour = self.contour.clamped();
        let e = self.contour.effect();
        self.change_contours("Contour", Some(e), true);
    }

    /// Clear Contour.
    pub fn clear_contour(&mut self) {
        self.change_contours("Clear Contour", None, false);
    }

    /// Start a drag at `at` (page space) on an object.
    pub fn begin_contour(&mut self, at: Point) -> bool {
        let Some(id) = self.hit_test_inside(at) else {
            return false;
        };
        if self
            .doc()
            .find_shape(id)
            .is_none_or(|s| matches!(s.kind, ShapeKind::Bitmap { .. } | ShapeKind::Group { .. }))
        {
            return false;
        }
        self.select(vec![id]);
        self.sync_contour_settings();
        self.contour_drag = Some(ContourDrag { id, start: at });
        true
    }

    /// The pointer moved to `p` during a drag.
    pub fn drag_contour(&mut self, p: Point) {
        let Some(dd) = self.contour_drag else {
            return;
        };
        let Some(s) = self.doc().shape_on_page(dd.id) else {
            self.contour_drag = None;
            return;
        };
        let b = s.bounds();
        let c = b.center();
        // Away from the middle: outside; toward it: inside.
        let away = (p - c).hypot() > (dd.start - c).hypot();
        let d = (p - dd.start).hypot();
        if d < 1e-6 {
            return;
        }
        let mut st = self.contour;
        if st.direction != ContourDirection::ToCenter {
            st.direction = if away {
                ContourDirection::Outside
            } else {
                ContourDirection::Inside
            };
        }
        st.offset = (d / st.steps.max(1) as f64).max(0.01);
        self.contour = st.clamped();
        if self.selection != vec![dd.id] {
            self.select(vec![dd.id]);
        }
        let e = self.contour.effect();
        self.push_effect(e, true);
    }

    /// The Contour tool's clicks and drags.
    pub fn contour_input(&mut self, response: &egui::Response, p: Point) {
        use egui::PointerButton;
        if response.drag_started_by(PointerButton::Primary) {
            let origin = response
                .ctx
                .input(|i| i.pointer.press_origin())
                .map(|o| self.view.to_page(o))
                .unwrap_or(p);
            // A handle of the selected contour, else a new contour.
            match self.contour_handle_at(origin) {
                Some(h) => self.contour_handle_drag = Some((h, false)),
                None => {
                    self.begin_contour(origin);
                }
            }
        }
        if response.dragged_by(PointerButton::Primary) {
            if let Some((h, begun)) = self.contour_handle_drag {
                self.drag_contour_handle(h, p, begun);
                self.contour_handle_drag = Some((h, true));
            } else if self.contour_drag.is_some() {
                self.drag_contour(p);
            }
        }
        if response.drag_stopped() {
            self.contour_drag = None;
            self.contour_handle_drag = None;
        }
        if response.clicked_by(PointerButton::Primary) {
            match self.hit_test_inside(p) {
                Some(id) => self.select(vec![id]),
                None => self.select(Vec::new()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{geometry::Rect, Fill};

    fn square(app: &mut App) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("square");
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(Color::BLACK),
        });
        id
    }

    fn contour_of(app: &App, id: ShapeId) -> Option<Effect> {
        app.doc()
            .find_shape(id)?
            .effects
            .iter()
            .find(|e| is_contour(e))
            .cloned()
    }

    #[test]
    fn a_drag_draws_a_live_contour() {
        let mut app = App::headless();
        let id = square(&mut app);
        let steps = app.engine.history_labels().0.len();
        // From the edge outward by 6 mm: outside, 3 steps of 2 mm.
        assert!(app.begin_contour(Point::new(19.0, 10.0)));
        app.drag_contour(Point::new(22.0, 10.0));
        app.drag_contour(Point::new(25.0, 10.0));
        app.contour_drag = None;
        assert_eq!(app.engine.history_labels().0.len(), steps + 1);
        let Some(Effect::Contour {
            outside,
            offset,
            steps: n,
            corners,
            ..
        }) = contour_of(&app, id)
        else {
            panic!("a contour");
        };
        assert!(outside);
        assert_eq!(n, 3);
        assert!((offset - 2.0).abs() < 1e-9);
        assert_eq!(corners, Some(Join::Miter));
        // Toward the middle: inside.
        assert!(app.begin_contour(Point::new(19.0, 10.0)));
        app.drag_contour(Point::new(13.0, 10.0));
        app.contour_drag = None;
        assert!(matches!(
            contour_of(&app, id),
            Some(Effect::Contour { outside: false, .. })
        ));
        assert!(!app.begin_contour(Point::new(500.0, 500.0)));
    }

    /// Break Apart (Ctrl+K, the context menu) on a contour: Break Contour
    /// Group Apart. The object stays, without its contour, and the rings
    /// come out beside it, grouped when there are several (#424).
    #[test]
    fn break_apart_breaks_a_contour_group() {
        for (steps, children) in [(3u32, 3usize), (1, 0)] {
            let mut app = App::headless();
            let id = square(&mut app);
            app.select(vec![id]);
            app.contour.direction = ContourDirection::Inside;
            app.contour.steps = steps;
            app.add_contour();
            assert_eq!(
                app.effect_group_to_break(),
                Some("menu.object.break_contour_apart")
            );
            assert_eq!(app.break_apart_label(), "menu.object.break_contour_apart");
            app.break_apart();
            let s = app.doc().find_shape(id).expect("the object stays");
            assert!(s.effects.is_empty());
            assert_eq!(app.selection.len(), 2, "{:?}", app.selection);
            assert_eq!(app.selection[0], id);
            let other = app.doc().find_shape(app.selection[1]).expect("rings");
            match &other.kind {
                ShapeKind::Group { children: c } => assert_eq!(c.len(), children),
                _ => assert_eq!(children, 0, "one ring stays an object"),
            }
            assert_eq!(app.break_apart_label(), "menu.object.break_apart");
        }
    }

    /// The offset handle sets the distance, the steps handle the number
    /// of steps with the last step kept; each drag is one undo step and
    /// the contour follows every move (#1265).
    #[test]
    fn handles_drag_the_offset_and_the_steps() {
        let mut app = App::headless();
        let id = square(&mut app);
        app.tool = crate::tools::Tool::Contour;
        app.select(vec![id]);
        app.add_contour();
        // Outside, 3 steps of 2 mm: from (20, 10) to (26, 10), the steps
        // handle at (22, 10).
        let h = app.contour_handles().expect("handles");
        assert_eq!((h.anchor.x, h.anchor.y), (20.0, 10.0));
        assert_eq!((h.end.x, h.end.y), (26.0, 10.0));
        assert_eq!(h.steps.map(|q| (q.x, q.y)), Some((22.0, 10.0)));
        assert_eq!(
            app.contour_handle_at(Point::new(26.0, 10.2)),
            Some(ContourHandle::Offset)
        );
        assert_eq!(
            app.contour_handle_at(Point::new(22.0, 10.0)),
            Some(ContourHandle::Steps)
        );
        assert_eq!(app.contour_handle_at(Point::new(40.0, 40.0)), None);
        let steps = app.engine.history_labels().0.len();
        // The offset handle to 32 mm: 3 steps of 4 mm, live at each move.
        app.drag_contour_handle(ContourHandle::Offset, Point::new(29.0, 10.0), false);
        let Some(Effect::Contour { offset, .. }) = contour_of(&app, id) else {
            panic!("a contour");
        };
        assert!((offset - 3.0).abs() < 1e-9);
        app.drag_contour_handle(ContourHandle::Offset, Point::new(32.0, 9.0), true);
        let Some(Effect::Contour {
            offset, steps: n, ..
        }) = contour_of(&app, id)
        else {
            panic!("a contour");
        };
        assert!((offset - 4.0).abs() < 1e-9);
        assert_eq!(n, 3);
        assert_eq!(app.engine.history_labels().0.len(), steps + 1);
        // The steps handle 2 mm from the edge: 6 steps of 2 mm, the last
        // one still at 12 mm.
        app.drag_contour_handle(ContourHandle::Steps, Point::new(22.0, 10.0), false);
        let Some(Effect::Contour {
            offset, steps: n, ..
        }) = contour_of(&app, id)
        else {
            panic!("a contour");
        };
        assert_eq!(n, 6);
        assert!((offset - 2.0).abs() < 1e-9);
        assert_eq!(app.engine.history_labels().0.len(), steps + 2);
        // One undo takes the whole steps drag back.
        let _ = app.engine.undo();
        let Some(Effect::Contour {
            offset, steps: n, ..
        }) = contour_of(&app, id)
        else {
            panic!("a contour");
        };
        assert_eq!((n, offset), (3, 4.0));
        // Inside: the line runs into the object.
        app.sync_contour_settings();
        app.contour.direction = ContourDirection::Inside;
        app.apply_contour_settings();
        let h = app.contour_handles().expect("handles");
        assert_eq!((h.end.x, h.dir), (8.0, -1.0));
        app.drag_contour_handle(ContourHandle::Offset, Point::new(14.0, 10.0), false);
        let Some(Effect::Contour {
            offset, outside, ..
        }) = contour_of(&app, id)
        else {
            panic!("a contour");
        };
        assert!(!outside);
        assert!((offset - 2.0).abs() < 1e-9);
        // To centre: one handle, the offset itself.
        app.contour.direction = ContourDirection::ToCenter;
        app.apply_contour_settings();
        let h = app.contour_handles().expect("handles");
        assert!(h.steps.is_none());
        assert_eq!(h.end.x, 18.0);
        // No handles with another tool or without a contour.
        app.tool = crate::tools::Tool::Pick;
        assert!(app.contour_handles().is_none());
        app.tool = crate::tools::Tool::Contour;
        app.clear_contour();
        assert!(app.contour_handles().is_none());
    }

    #[test]
    fn the_bar_edits_the_selected_contour() {
        let mut app = App::headless();
        let id = square(&mut app);
        app.select(vec![id]);
        app.contour.direction = ContourDirection::ToCenter;
        app.add_contour();
        app.sync_contour_settings();
        assert_eq!(app.contour.direction, ContourDirection::ToCenter);
        app.contour.corners = Join::Round;
        app.contour.accel_colors = 0.5;
        app.apply_contour_settings();
        let Some(Effect::Contour {
            to_center,
            corners,
            accel_colors,
            ..
        }) = contour_of(&app, id)
        else {
            panic!("a contour");
        };
        assert!(to_center);
        assert_eq!(corners, Some(Join::Round));
        assert_eq!(accel_colors, 0.5);
        app.clear_contour();
        assert!(contour_of(&app, id).is_none());
        // Settings from an older contour and odd values.
        let mut s = ContourSettings::default();
        s.take(&Effect::Contour {
            steps: 5000,
            offset: f64::NAN,
            outside: false,
            to_center: false,
            fill_to: Color::BLACK,
            outline_to: None,
            color_blend: 9,
            corners: None,
            accel_objects: 3.0,
            accel_colors: 0.0,
        });
        assert_eq!(s.direction, ContourDirection::Inside);
        assert_eq!((s.steps, s.offset, s.color_blend), (999, 2.0, 2));
        assert_eq!((s.corners, s.accel_objects), (Join::Round, 1.0));
    }
}
