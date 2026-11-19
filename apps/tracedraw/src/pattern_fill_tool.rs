//! Interactive Fill on a pattern fill (two-colour, vector or bitmap,
//! #3379): the tool edits the tiles instead of putting a gradient in
//! their place. The first tile shows on the canvas as a dashed frame
//! with three nodes: its corner (a drag moves the tiles), the end of its
//! bottom side (a drag turns the tiles and scales them, keeping their
//! proportions) and the end of its left side (a drag changes their
//! height only). A drag across the object elsewhere lays the tiles from
//! where it began, turned along it and as wide as it is long.

use crate::app::App;
use tracedraw_core::{
    geometry::{Affine, Point, Size, Vec2},
    style::Pattern,
    Command, Fill, ShapeId,
};

/// Which node of the first tile a drag holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternHandle {
    /// The tile's corner: the tiles move.
    Origin,
    /// The end of the tile's bottom side: the tiles turn and scale.
    Width,
    /// The end of the tile's left side: the tiles change height.
    Height,
}

/// The space the placement is in, on the page: the object's when the
/// tiles transform with it, the page's otherwise.
fn frame(p: &Pattern, object: Affine) -> Affine {
    if p.placement().transform_with_object {
        object
    } else {
        Affine::IDENTITY
    }
}

/// The first tile's corners on the page: its corner, the end of its
/// bottom side and the end of its left side.
pub fn pattern_handles(p: &Pattern, object: Affine) -> [(PatternHandle, Point); 3] {
    let m = frame(p, object) * p.placement().sanitized().place();
    let t = p.tile_size();
    [
        (PatternHandle::Origin, m * Point::ZERO),
        (PatternHandle::Width, m * Point::new(t.width, 0.0)),
        (PatternHandle::Height, m * Point::new(0.0, t.height)),
    ]
}

/// The first tile's outline on the page, corner to corner.
pub fn pattern_tile_outline(p: &Pattern, object: Affine) -> [Point; 4] {
    let m = frame(p, object) * p.placement().sanitized().place();
    let t = p.tile_size();
    [
        m * Point::ZERO,
        m * Point::new(t.width, 0.0),
        m * Point::new(t.width, t.height),
        m * Point::new(0.0, t.height),
    ]
}

/// The pattern with node `h` of its first tile at page point `at`.
pub fn moved_pattern(p: &Pattern, object: Affine, h: PatternHandle, at: Point) -> Pattern {
    let f = frame(p, object);
    if f.determinant().abs() < 1e-12 {
        return p.clone();
    }
    let q = f.inverse() * at;
    if !(q.x.is_finite() && q.y.is_finite()) {
        return p.clone();
    }
    let mut pl = p.placement().sanitized();
    let tile = p.tile_size();
    let min = Pattern::MIN_TILE_MM;
    match h {
        PatternHandle::Origin => {
            pl.offset_x = q.x;
            pl.offset_y = q.y;
        }
        PatternHandle::Width => {
            let v = q - Point::new(pl.offset_x, pl.offset_y);
            let w = v.hypot();
            if w < min {
                return p.clone();
            }
            pl.rotation = v.y.atan2(v.x).to_degrees();
            let k = w / tile.width;
            pl.size = Some(Size::new(w, (tile.height * k).max(min)));
        }
        PatternHandle::Height => {
            let v = q - Point::new(pl.offset_x, pl.offset_y);
            let a = pl.rotation.to_radians();
            let n = Vec2::new(-a.sin(), a.cos());
            let h = v.dot(n).max(min);
            pl.size = Some(Size::new(tile.width, h));
        }
    }
    p.clone().with_placement(pl.sanitized())
}

impl App {
    /// The selected object's pattern fill with the Interactive Fill tool:
    /// its id, the pattern and the object's transform.
    pub fn selected_pattern(&self) -> Option<(ShapeId, Pattern, Affine)> {
        if self.tool != crate::tools::Tool::InteractiveFill {
            return None;
        }
        let s = self.selected_shapes().into_iter().next()?;
        match &s.fill {
            Fill::Pattern(p) => Some((s.id, p.clone(), s.transform)),
            _ => None,
        }
    }

    /// The node of the first tile under a page point, within the pick
    /// distance.
    pub fn pattern_handle_at(&self, p: Point) -> Option<PatternHandle> {
        let (_, pat, object) = self.selected_pattern()?;
        let tol = self.line_pick_tol();
        pattern_handles(&pat, object)
            .into_iter()
            .filter(|(_, hp)| (*hp - p).hypot() <= tol)
            .min_by(|a, b| (a.1 - p).hypot().total_cmp(&(b.1 - p).hypot()))
            .map(|(h, _)| h)
    }

    /// Node `h` of the selected pattern's first tile dragged to `p`: one
    /// undo step "Fill" for the drag, the next moves joining it (`begun`).
    pub fn drag_pattern_handle(&mut self, h: PatternHandle, p: Point, begun: bool) {
        let Some((id, pat, object)) = self.selected_pattern() else {
            return;
        };
        let moved = moved_pattern(&pat, object, h, p);
        if moved == pat {
            return;
        }
        let cmd = Command::SetFill {
            shapes: vec![id],
            fill: Fill::Pattern(moved),
        };
        if begun && self.engine.undo_label() == Some("Fill") {
            let _ = self.engine.amend(&cmd);
        } else {
            self.run(cmd);
        }
    }

    /// A drag across an object with a pattern fill: the tiles start where
    /// it began, turned along it and as wide as it is long (their
    /// proportions kept). False when the object has no pattern fill.
    pub fn pattern_from_drag(&mut self, id: ShapeId, from: Point, to: Point) -> bool {
        let Some(s) = self.doc().shape_on_page(id) else {
            return false;
        };
        let Fill::Pattern(pat) = &s.fill else {
            return false;
        };
        let object = s.transform;
        let laid = moved_pattern(pat, object, PatternHandle::Origin, from);
        let laid = moved_pattern(&laid, object, PatternHandle::Width, to);
        self.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Pattern(laid),
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::style::{PatternPlacement, PatternTile};
    use tracedraw_core::{document::ShapeKind, geometry::Rect, Color};

    fn two_color(size: f64) -> Pattern {
        Pattern::TwoColor {
            tile: PatternTile::default(),
            front: Color::BLACK,
            back: Color::WHITE,
            size_mm: size,
            placement: PatternPlacement::default(),
        }
    }

    fn app_with_pattern(p: Pattern) -> (App, ShapeId) {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 100.0, 60.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.select(vec![id]);
        app.apply_fill(Fill::Pattern(p));
        app.tool = crate::tools::Tool::InteractiveFill;
        (app, id)
    }

    fn fill_of(app: &App, id: ShapeId) -> Fill {
        app.doc().find_shape(id).expect("shape").fill.clone()
    }

    fn near(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-6
    }

    /// The nodes sit on the first tile's corners, and each moves to
    /// where it is dragged: the corner moves the tiles, the bottom side's
    /// end turns and scales them (proportions kept), the left side's end
    /// changes their height only.
    #[test]
    fn each_node_lands_where_it_is_dragged() {
        let p = two_color(10.0);
        let hs = pattern_handles(&p, Affine::IDENTITY);
        assert!(near(hs[0].1, Point::new(0.0, 0.0)));
        assert!(near(hs[1].1, Point::new(10.0, 0.0)));
        assert!(near(hs[2].1, Point::new(0.0, 10.0)));
        let moved = moved_pattern(
            &p,
            Affine::IDENTITY,
            PatternHandle::Origin,
            Point::new(5.0, 7.0),
        );
        let hs = pattern_handles(&moved, Affine::IDENTITY);
        assert!(near(hs[0].1, Point::new(5.0, 7.0)));
        assert!(near(hs[1].1, Point::new(15.0, 7.0)));
        let turned = moved_pattern(
            &moved,
            Affine::IDENTITY,
            PatternHandle::Width,
            Point::new(5.0, 27.0),
        );
        let pl = turned.placement();
        assert!((pl.rotation - 90.0).abs() < 1e-9, "{}", pl.rotation);
        let t = turned.tile_size();
        assert!(
            (t.width - 20.0).abs() < 1e-9 && (t.height - 20.0).abs() < 1e-9,
            "{t:?}"
        );
        let hs = pattern_handles(&turned, Affine::IDENTITY);
        assert!(near(hs[1].1, Point::new(5.0, 27.0)), "{:?}", hs[1].1);
        let taller = moved_pattern(
            &turned,
            Affine::IDENTITY,
            PatternHandle::Height,
            Point::new(-25.0, 7.0),
        );
        let t = taller.tile_size();
        assert!(
            (t.width - 20.0).abs() < 1e-9 && (t.height - 30.0).abs() < 1e-9,
            "{t:?}"
        );
        // Dragged past the corner, the height stays positive.
        let flat = moved_pattern(
            &p,
            Affine::IDENTITY,
            PatternHandle::Height,
            Point::new(0.0, -5.0),
        );
        assert!(flat.tile_size().height > 0.0);
    }

    /// Tiles that transform with the object: the nodes follow the
    /// object's transform, and a node dragged lands where it is dropped.
    #[test]
    fn tiles_with_the_object_follow_its_transform() {
        let mut p = two_color(10.0);
        p.placement_mut().transform_with_object = true;
        let object = Affine::translate((50.0, 20.0)) * Affine::scale(2.0);
        let hs = pattern_handles(&p, object);
        assert!(near(hs[0].1, Point::new(50.0, 20.0)));
        assert!(near(hs[1].1, Point::new(70.0, 20.0)));
        let moved = moved_pattern(&p, object, PatternHandle::Width, Point::new(50.0, 60.0));
        let hs = pattern_handles(&moved, object);
        assert!(near(hs[1].1, Point::new(50.0, 60.0)), "{:?}", hs[1].1);
    }

    /// #3379: a drag across an object with a bitmap (or any pattern) fill
    /// with Interactive Fill keeps the pattern and lays its tiles along
    /// the drag; it no longer puts a gradient in its place.
    #[test]
    fn a_drag_on_a_pattern_fill_keeps_it() {
        let (mut app, id) = app_with_pattern(two_color(10.0));
        app.fill_from_drag(id, Point::new(10.0, 10.0), Point::new(10.0, 40.0));
        let Fill::Pattern(p) = fill_of(&app, id) else {
            panic!("the pattern was replaced: {:?}", fill_of(&app, id));
        };
        let hs = pattern_handles(&p, Affine::IDENTITY);
        assert!(near(hs[0].1, Point::new(10.0, 10.0)));
        assert!(near(hs[1].1, Point::new(10.0, 40.0)));
        assert_eq!(app.engine.undo_label(), Some("Fill"));
    }

    /// A drag on a node of the first tile moves it, in one undo step for
    /// the whole drag; the fill stays the pattern.
    #[test]
    fn a_drag_on_a_node_is_one_undo_step() {
        let (mut app, id) = app_with_pattern(two_color(10.0));
        let corner = Point::new(0.0, 0.0);
        assert_eq!(app.pattern_handle_at(corner), Some(PatternHandle::Origin));
        assert!(app.begin_fountain_drag(corner, corner));
        for x in 1..=5 {
            assert!(app.fountain_drag_to(Point::new(f64::from(x) * 4.0 + 3.0, 17.0), false));
        }
        let Fill::Pattern(p) = fill_of(&app, id) else {
            panic!("not a pattern");
        };
        let o = pattern_handles(&p, Affine::IDENTITY)[0].1;
        // Where the pointer let go (as it snaps).
        assert!((o - Point::new(23.0, 17.0)).hypot() < 3.0, "{o:?}");
        // One undo takes the whole drag back.
        app.undo();
        let Fill::Pattern(p) = fill_of(&app, id) else {
            panic!("not a pattern");
        };
        assert!(p.placement().is_default());
    }
}
