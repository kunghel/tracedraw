//! Colour points of a gradient fill (the Points kind): their nodes on the
//! canvas (numbered as mesh nodes are, from
//! [`crate::mesh_tool::MESH_NODE_BASE`]), dragged to move them, a palette
//! colour for the chosen one, a double click inside the object to add one
//! and Delete to take the chosen one away; the chosen one's spread (how
//! far its colour reaches, #3947) on the property bar and as a ring with
//! a handle round it on the canvas. See docs/behavior/gradient-fill.md.

use crate::app::App;
use crate::mesh_tool::MESH_NODE_BASE;
use tracedraw_core::geometry::{Point, Shape as _};
use tracedraw_core::style::{Fountain, FountainKind, Stop};
use tracedraw_core::{Color, Command, Fill, Shape, ShapeId};

/// The gradient of `s` when it is a colour points one.
fn points_fill(s: &Shape) -> Option<&Fountain> {
    match &s.fill {
        Fill::Fountain(f) if f.kind == FountainKind::Points => Some(f),
        _ => None,
    }
}

/// The effect node number of the spread handle of colour point `i`
/// (#3947): past the mesh handles' numbers, so a press on it keeps the
/// point chosen as a mesh handle keeps its node.
pub const SPREAD_HANDLE_BASE: usize = 500_000_000;

/// The colour point whose spread handle effect node `index` is.
pub fn spread_handle_of(index: usize) -> Option<usize> {
    index.checked_sub(SPREAD_HANDLE_BASE)
}

/// The radius on the page of the ring of a colour point with spread
/// `spread` in an object with box `b`: 15 % of the box's diagonal at
/// spread 1.
pub fn spread_radius(b: tracedraw_core::geometry::Rect, spread: f64) -> f64 {
    0.15 * b.width().hypot(b.height()) * spread
}

/// Colour point `i` of `s` on the page, its spread ring's radius and the
/// spread handle (on the ring, to the right of the point).
pub fn spread_ring(s: &Shape, i: usize) -> Option<(Point, f64, Point)> {
    let f = points_fill(s)?;
    if i >= f.stops.len() {
        return None;
    }
    let b = s.page_path().bounding_box();
    let at = Fountain::rel_to_page(b, *f.point_places().get(i)?);
    let r = spread_radius(b, f.point_spread(i));
    Some((at, r, Point::new(at.x + r, at.y)))
}

/// The spreads of all the points of `f`, one for each stop.
fn spreads_of(f: &Fountain) -> Vec<f64> {
    (0..f.stops.len()).map(|i| f.point_spread(i)).collect()
}

/// The page-space places of the colour points of `s`, with their node
/// numbers; none for another fill.
pub fn point_nodes(s: &Shape) -> Vec<(usize, Point)> {
    let Some(f) = points_fill(s) else {
        return Vec::new();
    };
    let b = s.page_path().bounding_box();
    f.point_places()
        .into_iter()
        .enumerate()
        .map(|(k, r)| (MESH_NODE_BASE + k, Fountain::rel_to_page(b, r)))
        .collect()
}

/// The stops spread evenly, first to last, so they stay in order (the
/// points kind places them by their points, not by position).
fn respread(stops: &mut [Stop]) {
    let n = stops.len();
    for (i, s) in stops.iter_mut().enumerate() {
        s.pos = if n > 1 {
            i as f64 / (n - 1) as f64
        } else {
            0.0
        };
    }
}

impl App {
    /// A selected object has a colour points gradient.
    pub fn has_color_points(&self) -> bool {
        self.selection
            .iter()
            .any(|id| self.doc().find_shape(*id).and_then(points_fill).is_some())
    }

    /// The chosen colour point of a selected object: (object, point).
    pub fn selected_color_point(&self) -> Option<(ShapeId, usize)> {
        let (id, k) = self.selected_effect_node?;
        let i = k.checked_sub(MESH_NODE_BASE)?;
        if !self.selection.contains(&id) || crate::mesh_tool::is_mesh_handle(k) {
            return None;
        }
        let f = points_fill(self.doc().find_shape(id)?)?;
        (i < f.stops.len()).then_some((id, i))
    }

    /// Set the colour points of object `id` to `f`, as one undo step
    /// named `label`.
    fn set_points_fill(&mut self, id: ShapeId, f: Fountain, label: &'static str) -> bool {
        let cmds = [Command::SetFill {
            shapes: vec![id],
            fill: Fill::Fountain(f),
        }];
        match self.engine.run_batch(label, &cmds) {
            Ok(()) => true,
            Err(e) => {
                self.status = e.to_string();
                false
            }
        }
    }

    /// A palette colour with a colour point chosen: the point takes it.
    /// False when none is chosen.
    pub fn color_color_point(&mut self, c: Color) -> bool {
        let Some((id, i)) = self.selected_color_point() else {
            return false;
        };
        let Some(mut f) = self.doc().find_shape(id).and_then(points_fill).cloned() else {
            return false;
        };
        f.stops[i].color = c;
        self.set_points_fill(id, f, "Colour Point")
    }

    /// A palette colour dropped at `at` (screen) on a colour point of a
    /// selected object: that point takes it and becomes the chosen one,
    /// with no need to choose it first (#4244). False when no point lies
    /// there.
    pub fn drop_on_color_point(&mut self, c: Color, at: egui::Pos2) -> bool {
        /// How near a point's node a drop counts as on it, points.
        const NODE_PX: f32 = 8.0;
        let mut best: Option<(ShapeId, usize, f32)> = None;
        for id in self.selection.clone() {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            for (k, p) in point_nodes(s) {
                let d = (self.view.to_screen(p) - at).length();
                if d <= NODE_PX && best.is_none_or(|(_, _, b)| d < b) {
                    best = Some((id, k, d));
                }
            }
        }
        let Some((id, k, _)) = best else {
            return false;
        };
        let Some(mut f) = self.doc().find_shape(id).and_then(points_fill).cloned() else {
            return false;
        };
        let Some(stop) = k.checked_sub(MESH_NODE_BASE).and_then(|i| f.stops.get_mut(i)) else {
            return false;
        };
        stop.color = c;
        self.selected_effect_node = Some((id, k));
        self.set_points_fill(id, f, "Colour Point")
    }

    /// The spread of the chosen colour point, when one is chosen.
    pub fn color_point_spread(&self) -> Option<f64> {
        let (id, i) = self.selected_color_point()?;
        Some(points_fill(self.doc().find_shape(id)?)?.point_spread(i))
    }

    /// The fill of object `id` with colour point `i` given `spread`
    /// (kept to its range), as a command; `None` when it has it already.
    pub fn spread_color_point(&self, id: ShapeId, i: usize, spread: f64) -> Option<Command> {
        let mut f = points_fill(self.doc().find_shape(id)?)?.clone();
        if i >= f.stops.len() || !spread.is_finite() {
            return None;
        }
        let v = spread.clamp(Fountain::MIN_SPREAD, Fountain::MAX_SPREAD);
        if (f.point_spread(i) - v).abs() < 1e-9 {
            return None;
        }
        f.spreads = spreads_of(&f);
        f.spreads[i] = v;
        Some(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Fountain(f),
        })
    }

    /// The property bar's Spread: the chosen colour point takes `spread`,
    /// one undo step. False when no point is chosen.
    pub fn set_color_point_spread(&mut self, spread: f64) -> bool {
        let Some((id, i)) = self.selected_color_point() else {
            return false;
        };
        if let Some(cmd) = self.spread_color_point(id, i, spread) {
            if let Err(e) = self.engine.run_batch("Colour Point Spread", &[cmd]) {
                self.status = e.to_string();
            }
        }
        true
    }

    /// The chosen colour point goes to the page point `to`.
    pub fn move_color_point(&mut self, id: ShapeId, i: usize, to: Point) -> Option<Command> {
        let s = self.doc().find_shape(id)?;
        let mut f = points_fill(s)?.clone();
        let b = s.page_path().bounding_box();
        f.points = f.point_places();
        *f.points.get_mut(i)? = Fountain::page_to_rel(b, to);
        Some(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Fountain(f),
        })
    }

    /// A double click at the page point `p` inside a selected object with
    /// colour points: a new point there, in the colour the fill has there,
    /// chosen. False when no such object is under it.
    pub fn add_color_point(&mut self, p: Point) -> bool {
        for s in self.selected_shapes() {
            let Some(f) = points_fill(&s) else {
                continue;
            };
            let b = s.page_path().bounding_box();
            if !b.contains(p) {
                continue;
            }
            let mut f = f.clone();
            let places = f.point_places();
            let (color, opacity) = f.points_sample(&places, b, p);
            f.points = places;
            let had_spreads = !f.spreads.is_empty();
            f.spreads = spreads_of(&f);
            let mut stop = Stop::new(1.0, color);
            stop.opacity = opacity;
            f.stops.push(stop);
            f.points.push(Fountain::page_to_rel(b, p));
            f.spreads.push(1.0);
            if !had_spreads {
                f.spreads.clear();
            }
            respread(&mut f.stops);
            let k = f.stops.len() - 1;
            if self.set_points_fill(s.id, f, "Add Colour Point") {
                self.selected_effect_node = Some((s.id, MESH_NODE_BASE + k));
            }
            return true;
        }
        false
    }

    /// Delete with a colour point chosen: it goes, while two stay. True
    /// when one was chosen.
    pub fn delete_color_point(&mut self) -> bool {
        let Some((id, i)) = self.selected_color_point() else {
            return false;
        };
        let Some(mut f) = self.doc().find_shape(id).and_then(points_fill).cloned() else {
            return false;
        };
        if f.stops.len() <= 2 {
            self.status = crate::i18n::tr("status.color_points_two");
            return true;
        }
        f.points = f.point_places();
        if !f.spreads.is_empty() {
            f.spreads = spreads_of(&f);
            f.spreads.remove(i);
        }
        f.stops.remove(i);
        f.points.remove(i);
        respread(&mut f.stops);
        if self.set_points_fill(id, f, "Delete Colour Point") {
            self.selected_effect_node = None;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;
    use tracedraw_core::ShapeKind;

    fn points_square(app: &mut App) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        let mut f = Fountain::two(FountainKind::Points, Color::BLACK, Color::WHITE, 0.0);
        f.points = vec![Point::new(-1.0, -1.0), Point::new(1.0, 1.0)];
        app.engine
            .run(&Command::SetFill {
                shapes: vec![id],
                fill: Fill::Fountain(f),
            })
            .expect("fill");
        app.select(vec![id]);
        id
    }

    fn fountain(app: &App, id: ShapeId) -> Fountain {
        match &app.doc().find_shape(id).expect("shape").fill {
            Fill::Fountain(f) => f.clone(),
            _ => panic!("a gradient"),
        }
    }

    /// A palette colour dropped on a colour point colours it and
    /// chooses it, with no point chosen before; dropped away from the
    /// points it goes to the object's fill as before (#4244).
    #[test]
    fn a_colour_dropped_on_a_colour_point_colours_it() {
        let mut app = App::headless();
        app.show_welcome = false;
        app.canvas_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        app.zoom_to_page();
        let id = points_square(&mut app);
        app.selected_effect_node = None;
        let s = app.doc().find_shape(id).expect("shape").clone();
        let (k, p) = point_nodes(&s)[1];
        let red = Color::rgb8(220, 20, 20);
        let at = app.view.to_screen(p) + egui::vec2(3.0, -2.0);
        assert!(app.drop_on_color_point(red, at));
        let f = fountain(&app, id);
        assert_eq!(f.stops[1].color, red);
        assert_eq!(f.stops[0].color, Color::BLACK);
        assert_eq!(app.selected_effect_node, Some((id, k)));
        // Far from both points: not taken here.
        let mid = app.view.to_screen(s.page_path().bounding_box().center());
        assert!(!app.drop_on_color_point(red, mid));
    }

    /// The points show as nodes at their places; a double click adds one
    /// in the colour there, chosen; a palette colour goes to it; a drag
    /// moves it; Delete takes it away but never below two.
    #[test]
    fn color_points_are_added_coloured_moved_and_deleted() {
        let mut app = App::headless();
        let id = points_square(&mut app);
        let s = app.doc().find_shape(id).expect("shape").clone();
        let nodes = point_nodes(&s);
        assert_eq!(nodes.len(), 2);
        let b = s.page_path().bounding_box();
        assert!(nodes[0].1.distance(Point::new(b.x0, b.y0)) < 1e-9);
        assert!(app.add_color_point(b.center()));
        let f = fountain(&app, id);
        assert_eq!(f.stops.len(), 3);
        let grey = f.stops[2].color.to_rgb8();
        assert!(grey[0] > 100 && grey[0] < 155, "{grey:?}");
        assert_eq!(app.selected_color_point(), Some((id, 2)));
        let red = Color::rgb8(255, 0, 0);
        assert!(app.color_color_point(red));
        assert_eq!(fountain(&app, id).stops[2].color, red);
        let cmd = app
            .move_color_point(id, 2, Point::new(b.x0 + 5.0, b.y0 + 5.0))
            .expect("move");
        app.engine.run(&cmd).expect("run");
        let f = fountain(&app, id);
        let at = Fountain::rel_to_page(b, f.points[2]);
        assert!(at.distance(Point::new(b.x0 + 5.0, b.y0 + 5.0)) < 1e-9);
        assert!(app.delete_color_point());
        assert_eq!(fountain(&app, id).stops.len(), 2);
        app.selected_effect_node = Some((id, MESH_NODE_BASE));
        assert!(app.delete_color_point());
        assert_eq!(fountain(&app, id).stops.len(), 2, "two stay");
    }

    /// #3947: the chosen point's spread is set from the bar (one undo
    /// step, kept to its range) and widens its colour's reach; its ring's
    /// handle sits on the ring; the spreads follow a point added or
    /// deleted.
    #[test]
    fn a_color_point_spread_widens_its_reach() {
        let mut app = App::headless();
        let id = points_square(&mut app);
        assert_eq!(app.color_point_spread(), None);
        app.selected_effect_node = Some((id, MESH_NODE_BASE));
        assert_eq!(app.color_point_spread(), Some(1.0));
        let s = app.doc().find_shape(id).expect("shape").clone();
        let b = s.page_path().bounding_box();
        // A third of the way from the black point: darker than half way
        // at spread 1, darker still with the black point spread to 3.
        let p = Point::new(b.x0 + b.width() / 3.0, b.y0 + b.height() / 3.0);
        let grey = |app: &App| {
            let f = fountain(app, id);
            f.points_sample(&f.point_places(), b, p).0.to_rgb8()[0]
        };
        let before = grey(&app);
        assert!(app.set_color_point_spread(3.0));
        assert_eq!(app.color_point_spread(), Some(3.0));
        assert!(grey(&app) < before, "{} {before}", grey(&app));
        assert_eq!(app.engine.undo_label(), Some("Colour Point Spread"));
        let (at, r, handle) = spread_ring(&app.doc().find_shape(id).expect("s").clone(), 0)
            .expect("ring");
        assert!((r - spread_radius(b, 3.0)).abs() < 1e-9);
        assert!((handle.distance(at) - r).abs() < 1e-9);
        assert!(app.set_color_point_spread(1e9));
        assert_eq!(app.color_point_spread(), Some(Fountain::MAX_SPREAD));
        app.undo();
        assert_eq!(app.color_point_spread(), Some(3.0));
        // A point added gets spread 1, the others keep theirs; deleting
        // one takes its spread along.
        assert!(app.add_color_point(b.center()));
        assert_eq!(fountain(&app, id).spreads, vec![3.0, 1.0, 1.0]);
        app.selected_effect_node = Some((id, MESH_NODE_BASE));
        assert!(app.delete_color_point());
        assert_eq!(fountain(&app, id).spreads, vec![1.0, 1.0]);
        app.undo();
        app.undo();
        app.undo();
        assert_eq!(fountain(&app, id).point_spread(0), 1.0);
    }

    /// #3947: the chosen point's ring handle is picked on the canvas and
    /// dragged, the spread following its distance from the point, the
    /// whole drag one undo step.
    #[test]
    fn the_spread_handle_is_dragged_on_the_canvas() {
        let mut app = App::headless();
        let id = points_square(&mut app);
        app.selected_effect_node = Some((id, MESH_NODE_BASE + 1));
        let s = app.doc().find_shape(id).expect("shape").clone();
        let b = s.page_path().bounding_box();
        let (at, _, handle) = spread_ring(&s, 1).expect("ring");
        let hit = app.effect_node_at(handle).expect("the handle");
        assert_eq!(hit, (id, SPREAD_HANDLE_BASE + 1));
        app.effect_node_drag = Some(hit);
        app.effect_node_grab = tracedraw_core::geometry::Vec2::ZERO;
        app.effect_node_stepped = false;
        let unit = spread_radius(b, 1.0);
        app.effect_node_drag_to(Point::new(at.x - 3.0 * unit, at.y));
        app.effect_node_drag_to(Point::new(at.x, at.y + 2.0 * unit));
        assert!((fountain(&app, id).point_spread(1) - 2.0).abs() < 1e-9);
        assert_eq!(app.selected_color_point(), Some((id, 1)), "still chosen");
        app.undo();
        assert_eq!(fountain(&app, id).point_spread(1), 1.0, "one step");
    }
}
