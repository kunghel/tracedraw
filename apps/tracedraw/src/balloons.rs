//! Speech balloons (#2179): the Speech Balloon tool draws a balloon for
//! comics in one drag, its body and its tail welded into one curve. The
//! balloon keeps its style, its body's box, the tip of its tail, the
//! tail's width and its bend in the object's data (`balloon`), so the
//! tail can be dragged to point at someone else (the diamond at its tip,
//! with the tool) and the style, width and bend changed on the property
//! bar at any time; the curve is made again from them.
//!
//! Styles: Speech (an ellipse), Thought (a cloud with bubbles toward the
//! tip instead of a tail), Whisper (Speech with a dashed outline), Shout
//! (a spiky burst) and Caption (a rounded box).

use crate::app::App;
use serde::{Deserialize, Serialize};
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::{ellipse_path, rect_path, Affine, BezPath, Point, Rect, Vec2},
    shaping::{overlay, Op},
    Color, Command, Fill, ShapeId, Stroke,
};

/// The object data key holding a balloon.
pub const DATA_KEY: &str = "balloon";

/// The dashes of a Whisper balloon's outline.
const WHISPER_DASH: [f64; 2] = [4.0, 2.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BalloonStyle {
    #[default]
    Speech,
    Thought,
    Whisper,
    Shout,
    Caption,
}

impl BalloonStyle {
    pub const ALL: [BalloonStyle; 5] = [
        BalloonStyle::Speech,
        BalloonStyle::Thought,
        BalloonStyle::Whisper,
        BalloonStyle::Shout,
        BalloonStyle::Caption,
    ];

    pub fn key(self) -> &'static str {
        match self {
            BalloonStyle::Speech => "balloon.speech",
            BalloonStyle::Thought => "balloon.thought",
            BalloonStyle::Whisper => "balloon.whisper",
            BalloonStyle::Shout => "balloon.shout",
            BalloonStyle::Caption => "balloon.caption",
        }
    }
}

/// A balloon, in the object's own space (before its transform).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Balloon {
    pub style: BalloonStyle,
    /// The body's box: x0, y0, x1, y1.
    pub body: [f64; 4],
    /// Where the tail points.
    pub tip: [f64; 2],
    /// The tail's width where it leaves the body, percent of the body's
    /// smaller side (5 to 100).
    pub width: f64,
    /// How far the tail bends to one side (-100 to 100, 0 straight).
    pub bend: f64,
}

fn finite(v: f64, lo: f64, hi: f64, d: f64) -> f64 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        d
    }
}

impl Balloon {
    /// A balloon whose body fills `body`, its tail pointing down and to
    /// the left, half the body's height below it.
    pub fn new(body: Rect, style: BalloonStyle, width: f64, bend: f64) -> Balloon {
        let tip = Point::new(body.x0 + body.width() * 0.25, body.y0 - body.height() * 0.5);
        Balloon {
            style,
            body: [body.x0, body.y0, body.x1, body.y1],
            tip: [tip.x, tip.y],
            width,
            bend,
        }
        .clamped()
    }

    pub fn body_rect(&self) -> Rect {
        Rect::new(self.body[0], self.body[1], self.body[2], self.body[3])
    }

    pub fn tip(&self) -> Point {
        Point::new(self.tip[0], self.tip[1])
    }

    /// The numbers kept in range.
    pub fn clamped(&self) -> Balloon {
        let r = self.body_rect();
        let ok = [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite());
        let body = if ok {
            let r = Rect::from_points(r.origin(), Point::new(r.x1, r.y1));
            [r.x0, r.y0, r.x1, r.y1]
        } else {
            [0.0, 0.0, 10.0, 10.0]
        };
        let tip = if self.tip.iter().all(|v| v.is_finite()) {
            self.tip
        } else {
            [body[0], body[1]]
        };
        Balloon {
            style: self.style,
            body,
            tip,
            width: finite(self.width, 5.0, 100.0, 30.0),
            bend: finite(self.bend, -100.0, 100.0, 0.0),
        }
    }

    /// The balloon of `shape`, if it is one.
    pub fn of(shape: &Shape) -> Option<Balloon> {
        let text = shape.data.iter().find(|(k, _)| k == DATA_KEY)?.1.as_str();
        let b: Balloon = serde_json::from_str(text).ok()?;
        Some(b.clamped())
    }

    /// `data` with this balloon in it.
    pub fn in_data(&self, mut data: Vec<(String, String)>) -> Vec<(String, String)> {
        data.retain(|(k, _)| k != DATA_KEY);
        if let Ok(text) = serde_json::to_string(self) {
            data.push((DATA_KEY.to_string(), text));
        }
        data
    }

    /// The balloon's curve: the body and the tail welded (a Thought
    /// balloon's bubbles stay apart from the cloud).
    pub fn path(&self) -> BezPath {
        let r = self.body_rect();
        let body = body_path(self.style, r);
        if self.style == BalloonStyle::Thought {
            let mut out = body;
            for b in bubbles(r, self.tip()) {
                out.extend(b);
            }
            return out;
        }
        match tail_path(r, self.tip(), self.width, self.bend) {
            Some(tail) => {
                let welded = overlay(&body, &tail, Op::Weld);
                if welded.elements().is_empty() {
                    body
                } else {
                    welded
                }
            }
            None => body,
        }
    }
}

/// The body of a balloon of `style` filling `r`.
pub fn body_path(style: BalloonStyle, r: Rect) -> BezPath {
    match style {
        BalloonStyle::Speech | BalloonStyle::Whisper => ellipse_path(r),
        BalloonStyle::Caption => rect_path(r, r.width().min(r.height()) * 0.2),
        BalloonStyle::Shout => burst(r),
        BalloonStyle::Thought => cloud(r),
    }
}

/// A spiky burst in `r`: points round the ellipse, every other one drawn
/// in, a little uneven as a shout is.
fn burst(r: Rect) -> BezPath {
    let n = 16;
    let c = r.center();
    let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
    let mut p = BezPath::new();
    for i in 0..n * 2 {
        let a = std::f64::consts::TAU * i as f64 / (n * 2) as f64;
        let k = if i % 2 == 0 {
            1.0 - ((i * 37 % 7) as f64) * 0.025
        } else {
            0.72 + ((i * 11 % 5) as f64) * 0.02
        };
        let q = Point::new(c.x + rx * k * a.cos(), c.y + ry * k * a.sin());
        if i == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p.close_path();
    p
}

/// A cloud in `r`: round bumps along an ellipse.
fn cloud(r: Rect) -> BezPath {
    let n = 11;
    let c = r.center();
    let bump = 0.12;
    let (ex, ey) = (r.width() * (0.5 - bump), r.height() * (0.5 - bump));
    let (bx, by) = (r.width() * bump * 2.2, r.height() * bump * 2.2);
    let at = |i: usize| {
        let a = std::f64::consts::TAU * i as f64 / n as f64;
        Point::new(c.x + ex * a.cos(), c.y + ey * a.sin())
    };
    let mut p = BezPath::new();
    p.move_to(at(0));
    for i in 0..n {
        let (a, b) = (at(i), at((i + 1) % n));
        let mid = std::f64::consts::TAU * (i as f64 + 0.5) / n as f64;
        let out = Vec2::new(mid.cos() * bx, mid.sin() * by);
        p.curve_to(a + out, b + out, b);
    }
    p.close_path();
    p
}

/// The point where the line from the middle of `r` to `toward` leaves
/// the ellipse in `r`.
fn rim(r: Rect, toward: Point) -> Point {
    let c = r.center();
    let d = toward - c;
    let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
    let k = ((d.x / rx).powi(2) + (d.y / ry).powi(2)).sqrt();
    if k <= 1e-12 || !k.is_finite() {
        c
    } else {
        c + d / k
    }
}

/// A Thought balloon's three bubbles, smaller toward the tip.
fn bubbles(r: Rect, tip: Point) -> Vec<BezPath> {
    let start = rim(r, tip);
    if (tip - r.center()).hypot() <= (start - r.center()).hypot() {
        return Vec::new();
    }
    let side = r.width().min(r.height());
    [(0.3, 0.16), (0.62, 0.11), (0.92, 0.07)]
        .iter()
        .map(|&(t, s)| {
            let c = start.lerp(tip, t);
            let rad = side * s / 2.0;
            ellipse_path(Rect::new(
                c.x - rad * 1.3,
                c.y - rad,
                c.x + rad * 1.3,
                c.y + rad,
            ))
        })
        .collect()
}

/// The tail: from a base across the body's middle (`width` percent of
/// its smaller side) to `tip`, its sides bowed by `bend`. None when the
/// tip lies inside the body.
fn tail_path(r: Rect, tip: Point, width: f64, bend: f64) -> Option<BezPath> {
    let c = r.center();
    let d = tip - c;
    let len = d.hypot();
    if !len.is_finite() || len <= (rim(r, tip) - c).hypot() + 1e-9 {
        return None;
    }
    let u = d / len;
    let n = Vec2::new(-u.y, u.x);
    let hw = r.width().min(r.height()) * width.clamp(5.0, 100.0) / 200.0;
    let (b1, b2) = (c + n * hw, c - n * hw);
    let k = n * (bend.clamp(-100.0, 100.0) / 100.0 * len * 0.35);
    let mut p = BezPath::new();
    p.move_to(b1);
    p.quad_to(b1.lerp(tip, 0.5) + k, tip);
    p.quad_to(b2.lerp(tip, 0.5) + k, b2);
    p.close_path();
    Some(p)
}

/// The outline a balloon of `style` gets from `base`: dashed for a
/// whisper, plain otherwise.
fn styled_stroke(base: Option<Stroke>, style: BalloonStyle) -> Stroke {
    let mut s = base.unwrap_or_else(|| Stroke::hairline(Color::BLACK));
    if style == BalloonStyle::Whisper {
        s.dash = WHISPER_DASH.to_vec();
    } else if s.dash == WHISPER_DASH {
        s.dash.clear();
    }
    s
}

/// A tail tip within this many screen pixels of the pointer takes a press.
const TIP_REACH: f32 = 8.0;

impl App {
    /// Speech Balloon tool: a balloon whose body fills `rect`, with the
    /// style, tail width and bend on the property bar.
    pub fn create_balloon(&mut self, rect: Rect) {
        let b = Balloon::new(
            rect,
            self.balloon_style,
            self.balloon_width,
            self.balloon_bend,
        );
        let stroke = styled_stroke(self.default_stroke(), b.style);
        let fill = match self.default_fill() {
            Fill::None => Fill::Solid(Color::WHITE),
            f => f,
        };
        let path = b.path();
        if let Some(id) = self.new_shape_styled(ShapeKind::Path { path, closed: true }, |s| {
            s.fill = fill;
            s.stroke = Some(stroke);
            s.data = b.in_data(std::mem::take(&mut s.data));
        }) {
            self.select(vec![id]);
        }
    }

    /// The selected balloons, with where their space sits on the page:
    /// those inside selected groups too, as a balloon grouped with its
    /// words is picked as the group (#3352).
    pub fn selected_balloons(&self) -> Vec<(ShapeId, Balloon, Affine)> {
        fn walk(s: &Shape, out: &mut Vec<(ShapeId, Balloon)>) {
            if let Some(b) = Balloon::of(s) {
                out.push((s.id, b));
            } else if let ShapeKind::Group { children } = &s.kind {
                children.iter().for_each(|c| walk(c, out));
            }
        }
        let mut found = Vec::new();
        for s in self.selected_shapes() {
            walk(&s, &mut found);
        }
        found
            .into_iter()
            .filter_map(|(id, b)| {
                let s = self.doc().find_shape(id)?;
                let parent = self.doc().parent_transform(id).unwrap_or(Affine::IDENTITY);
                Some((id, b, parent * s.transform))
            })
            .collect()
    }

    /// The bar's Add Text (discussion #3522): a paragraph text inside the
    /// first selected balloon, its frame the part of the body that holds
    /// text whole (the box inside the oval, the burst or the cloud),
    /// centred, the caret in it to type. False with no balloon selected.
    pub fn add_text_in_balloon(&mut self) -> bool {
        let Some((_, b, t)) = self.selected_balloons().into_iter().next() else {
            return false;
        };
        let r = b.body_rect();
        let k = match b.style {
            BalloonStyle::Speech | BalloonStyle::Whisper => 0.68,
            BalloonStyle::Caption => 0.84,
            BalloonStyle::Thought => 0.58,
            BalloonStyle::Shout => 0.5,
        };
        let inner = Rect::from_center_size(r.center(), (r.width() * k, r.height() * k));
        let frame = t.transform_rect_bbox(inner);
        if !(frame.width() > 1.0 && frame.height() > 1.0) {
            return false;
        }
        // Centred, without changing what new texts get.
        let align = std::mem::replace(
            &mut self.text_default_align,
            tracedraw_core::document::TextAlign::Center,
        );
        self.start_text(
            Point::new(frame.x0, frame.y0),
            Some(tracedraw_core::geometry::Size::new(
                frame.width(),
                frame.height(),
            )),
        );
        self.text_default_align = align;
        self.text_edit.is_some()
    }

    /// The commands that make shape `id` balloon `b`.
    fn balloon_commands(&self, id: ShapeId, b: &Balloon) -> Vec<Command> {
        let Some(s) = self.doc().find_shape(id) else {
            return Vec::new();
        };
        let mut cmds = vec![
            Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path {
                    path: b.path(),
                    closed: true,
                },
            },
            Command::SetObjectData {
                shape: id,
                data: b.in_data(s.data.clone()),
            },
        ];
        let stroke = styled_stroke(s.stroke.clone(), b.style);
        if s.stroke.as_ref() != Some(&stroke) {
            cmds.push(Command::SetStroke {
                shapes: vec![id],
                stroke: Some(stroke),
            });
        }
        cmds
    }

    /// The property bar's balloon settings changed by `f`: they go to the
    /// selected balloons (one undo step) and to the ones drawn next.
    pub fn set_balloon_settings(&mut self, f: impl Fn(&mut BalloonStyle, &mut f64, &mut f64)) {
        f(
            &mut self.balloon_style,
            &mut self.balloon_width,
            &mut self.balloon_bend,
        );
        let mut cmds = Vec::new();
        for (id, mut b, _) in self.selected_balloons() {
            f(&mut b.style, &mut b.width, &mut b.bend);
            cmds.extend(self.balloon_commands(id, &b.clamped()));
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Speech Balloon", &cmds);
        }
    }

    /// The tail tip of a selected balloon near `screen`.
    fn balloon_tip_at(&self, screen: egui::Pos2) -> Option<ShapeId> {
        self.selected_balloons()
            .into_iter()
            .find(|(_, b, t)| self.view.to_screen(*t * b.tip()).distance(screen) <= TIP_REACH)
            .map(|(id, _, _)| id)
    }

    /// The tail tip moved to `p` (page space) on balloon `id`; `first`
    /// makes a new undo step, later moves join it.
    fn move_balloon_tip(&mut self, id: ShapeId, p: Point, first: bool) {
        let Some((_, mut b, t)) = self
            .selected_balloons()
            .into_iter()
            .find(|(sid, _, _)| *sid == id)
        else {
            return;
        };
        if t.determinant().abs() < 1e-12 {
            return;
        }
        let local = t.inverse() * p;
        b.tip = [local.x, local.y];
        let cmds = self.balloon_commands(id, &b.clamped());
        let _ = if first {
            self.engine.run_batch("Speech Balloon", &cmds)
        } else {
            self.engine.amend_batch("Speech Balloon", &cmds)
        };
    }

    /// With the Speech Balloon tool: a drag from the diamond at a selected
    /// balloon's tail tip points the tail somewhere else. True when the
    /// press or drag was the tip's.
    pub fn balloon_tip_input(
        &mut self,
        response: &egui::Response,
        p: Point,
        screen: egui::Pos2,
    ) -> bool {
        use egui::PointerButton;
        if response.drag_started_by(PointerButton::Primary) {
            if let Some(id) = self.balloon_tip_at(screen) {
                self.balloon_tip_drag = Some(id);
                self.move_balloon_tip(id, p, true);
                return true;
            }
            return false;
        }
        let Some(id) = self.balloon_tip_drag else {
            return response.clicked_by(PointerButton::Primary)
                && self.balloon_tip_at(screen).is_some();
        };
        if response.dragged() {
            let now = response
                .hover_pos()
                .map(|s| self.view.to_page(s))
                .unwrap_or(p);
            self.move_balloon_tip(id, now, false);
        }
        if response.drag_stopped() || !response.ctx.input(|i| i.pointer.primary_down()) {
            self.balloon_tip_drag = None;
        }
        true
    }
}

/// The diamonds at the tail tips of the selected balloons, with the
/// Speech Balloon tool.
pub fn draw_handles(app: &App, painter: &egui::Painter) {
    if app.tool != crate::tools::Tool::Balloon {
        return;
    }
    let blue = egui::Color32::from_rgb(40, 110, 220);
    for (_, b, t) in app.selected_balloons() {
        let c = app.view.to_screen(t * b.tip());
        let r = 5.0;
        let pts = vec![
            egui::pos2(c.x, c.y - r),
            egui::pos2(c.x + r, c.y),
            egui::pos2(c.x, c.y + r),
            egui::pos2(c.x - r, c.y),
        ];
        painter.add(egui::Shape::convex_polygon(
            pts,
            egui::Color32::WHITE,
            egui::Stroke::new(1.5, blue),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use tracedraw_core::geometry::Shape as _;

    #[test]
    fn every_style_makes_one_closed_curve_reaching_the_tip() {
        let body = Rect::new(0.0, 0.0, 40.0, 20.0);
        for style in BalloonStyle::ALL {
            let b = Balloon::new(body, style, 30.0, 0.0);
            let path = b.path();
            assert!(!path.elements().is_empty(), "{style:?}");
            let bb = path.bounding_box();
            // The tail (or the last bubble) reaches down toward the tip.
            assert!(bb.y0 < -6.0, "{style:?} {bb:?}");
            assert!(bb.x1 >= 39.0 && bb.y1 >= 19.0, "{style:?} {bb:?}");
        }
        // Speech: welded into a single outline, no separate tail.
        let speech = Balloon::new(body, BalloonStyle::Speech, 30.0, 0.0).path();
        let moves = speech
            .elements()
            .iter()
            .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
            .count();
        assert_eq!(moves, 1, "one outline");
        // A tip inside the body: no tail at all.
        let mut inside = Balloon::new(body, BalloonStyle::Speech, 30.0, 0.0);
        inside.tip = [20.0, 10.0];
        assert_eq!(
            inside.path().bounding_box(),
            ellipse_path(body).bounding_box()
        );
        // A bent tail bows to one side.
        let straight = Balloon::new(body, BalloonStyle::Speech, 30.0, 0.0).path();
        let bent = Balloon::new(body, BalloonStyle::Speech, 30.0, 100.0).path();
        assert!((straight.area().abs() - bent.area().abs()).abs() > 1.0);
    }

    #[test]
    fn a_balloon_is_drawn_its_tip_dragged_and_its_style_changed() {
        let mut app = App::headless();
        app.tool = Tool::Balloon;
        app.create_box_shape(Point::new(10.0, 50.0), Point::new(50.0, 70.0));
        let (id, b, t) = app.selected_balloons()[0];
        assert_eq!(b.style, BalloonStyle::Speech);
        let s = app.doc().find_shape(id).unwrap().clone();
        assert!(matches!(s.fill, Fill::Solid(_)), "filled: text goes on it");
        // The tip moves to a new place in one undo step.
        let target = Point::new(80.0, 20.0);
        app.move_balloon_tip(id, t * b.tip(), true);
        app.move_balloon_tip(id, Point::new(70.0, 30.0), false);
        app.move_balloon_tip(id, target, false);
        let (_, b2, t2) = app.selected_balloons()[0];
        assert!(((t2 * b2.tip()) - target).hypot() < 1e-6);
        let reach = app
            .doc()
            .find_shape(id)
            .unwrap()
            .local_path()
            .bounding_box();
        assert!(reach.x1 > 79.0 && reach.y0 < 21.0, "{reach:?}");
        app.undo();
        let (_, b3, _) = app.selected_balloons()[0];
        assert_eq!(b3.tip, b.tip, "one undo puts the tail back");
        // Whisper: the outline turns dashed; back to Speech, plain again.
        app.set_balloon_settings(|st, _, _| *st = BalloonStyle::Whisper);
        let s = app.doc().find_shape(id).unwrap();
        assert_eq!(s.stroke.as_ref().unwrap().dash, WHISPER_DASH.to_vec());
        assert_eq!(Balloon::of(s).unwrap().style, BalloonStyle::Whisper);
        app.set_balloon_settings(|st, _, _| *st = BalloonStyle::Speech);
        let s = app.doc().find_shape(id).unwrap();
        assert!(s.stroke.as_ref().unwrap().dash.is_empty());
        // Odd data is left alone.
        let mut odd = s.clone();
        odd.data = vec![(DATA_KEY.into(), "not json".into())];
        assert!(Balloon::of(&odd).is_none());
    }
    /// Discussion #3522: Add Text puts a paragraph text inside the
    /// selected balloon, its frame within the body, centred, with the
    /// caret in it; what new texts get stays as it was.
    #[test]
    fn add_text_types_inside_the_balloon() {
        let mut app = App::headless();
        app.tool = Tool::Balloon;
        assert!(!app.add_text_in_balloon(), "nothing selected");
        app.create_box_shape(Point::new(10.0, 50.0), Point::new(50.0, 70.0));
        let (id, b, t) = app.selected_balloons()[0];
        let body = t.transform_rect_bbox(b.body_rect());
        let align = app.text_default_align;
        assert!(app.add_text_in_balloon());
        let edit = app.text_edit.as_ref().expect("typing").shape;
        assert_ne!(edit, id);
        let s = app.doc().find_shape(edit).unwrap();
        let ShapeKind::Text {
            frame: Some(frame),
            origin,
            align: text_align,
            ..
        } = &s.kind
        else {
            panic!("a paragraph text: {:?}", s.kind);
        };
        assert_eq!(*text_align, tracedraw_core::document::TextAlign::Center);
        let r = Rect::from_origin_size(*origin, *frame);
        assert!(r.x0 > body.x0 && r.x1 < body.x1, "{r:?} in {body:?}");
        assert!(r.y0 > body.y0 && r.y1 < body.y1, "{r:?} in {body:?}");
        assert!((r.center() - body.center()).hypot() < 1e-6);
        assert_eq!(app.text_default_align, align, "new texts as before");
    }

    /// #3352: a balloon grouped with its words is picked as the group;
    /// a style chosen on the bar still turns the balloon in it, and the
    /// words are left as they are.
    #[test]
    fn a_style_reaches_a_balloon_grouped_with_its_words() {
        let mut app = App::headless();
        app.tool = Tool::Balloon;
        app.create_box_shape(Point::new(10.0, 50.0), Point::new(50.0, 70.0));
        let (id, before, _) = app.selected_balloons()[0];
        let words = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(20.0, 55.0, 40.0, 65.0),
                radius: 0.0,
                corners: None,
            })
            .expect("words stand-in");
        app.select(vec![id, words]);
        app.group_selection();
        assert_eq!(app.selection.len(), 1);
        assert_ne!(app.selection[0], id, "the group is selected");
        app.set_balloon_settings(|st, _, _| *st = BalloonStyle::Shout);
        let s = app.doc().find_shape(id).expect("balloon in the group");
        let after = Balloon::of(s).expect("still a balloon");
        assert_eq!(after.style, BalloonStyle::Shout);
        assert_eq!(after.body, before.body);
        assert_eq!(after.tip, before.tip);
        assert!(matches!(
            app.doc().find_shape(words).map(|w| &w.kind),
            Some(ShapeKind::Rect { .. })
        ));
        app.undo();
        let s = app.doc().find_shape(id).expect("balloon");
        assert_eq!(
            Balloon::of(s).unwrap().style,
            BalloonStyle::Speech,
            "one undo"
        );
    }
}
