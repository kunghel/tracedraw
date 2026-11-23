//! Colour points of a gradient fill (the Points kind): their nodes on the
//! canvas (numbered as mesh nodes are, from
//! [`crate::mesh_tool::MESH_NODE_BASE`]), dragged to move them, a palette
//! colour for the chosen one, a double click inside the object to add one
//! and Delete to take the chosen one away. See
//! docs/behavior/gradient-fill.md.

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
            let mut stop = Stop::new(1.0, color);
            stop.opacity = opacity;
            f.stops.push(stop);
            f.points.push(Fountain::page_to_rel(b, p));
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
}
