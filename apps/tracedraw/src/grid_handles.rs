//! The handles of a Grid effect (Layer effects > Grid) on the canvas.
//!
//! With the Pick tool and one object with a Grid selected, a round
//! handle on the left edge of the second column drags the gap between
//! columns, one on the top edge of the second row the gap between rows,
//! and a square past the corner of the last cell drags out more or fewer
//! columns and rows. A gap near nothing takes nothing, so the cells touch.
//! Each drag is one undo step; the object stays as it is.

use crate::app::{App, Drag};
use crate::tools::Tool;
use tracedraw_core::geometry::{Point, Rect, Vec2};
use tracedraw_core::live::{self, Effect};
use tracedraw_core::{Command, Shape, ShapeId};

/// How far from a handle, in screen pixels, a press takes it.
const REACH_PX: f64 = 7.0;
/// How far past the last cell's corner, in screen pixels, the count
/// handle sits (clear of the selection's own corner handle).
pub const COUNT_OFFSET_PX: f64 = 14.0;
/// The undo label of a drag.
const LABEL: &str = "Edit Grid";

/// A handle of a Grid effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridHandle {
    /// The gap between columns.
    GapX,
    /// The gap between rows.
    GapY,
    /// How many columns and rows.
    Count,
}

/// The cell of a Grid kept for the drawing's revision it was found at.
pub type GridCellCache = std::cell::RefCell<Option<(u64, ShapeId, Rect)>>;

/// The first Grid effect of `s`.
fn grid_of(s: &Shape) -> Option<Effect> {
    s.effects
        .iter()
        .find(|e| matches!(e, Effect::Grid { .. }))
        .cloned()
}

/// The handles of grid `e` whose cells are `cell`, at `zoom`, in page
/// space.
pub fn handles(e: &Effect, cell: Rect, zoom: f64) -> Vec<(GridHandle, Point)> {
    let (cols, rows) = live::grid_size(e);
    let mut out = Vec::new();
    if cols > 1 {
        let o = live::grid_offset(e, cell, 1, 0);
        out.push((
            GridHandle::GapX,
            Point::new(cell.x0 + o.x, cell.center().y + o.y),
        ));
    }
    if rows > 1 {
        let o = live::grid_offset(e, cell, 0, 1);
        out.push((
            GridHandle::GapY,
            Point::new(cell.center().x + o.x, cell.y1 + o.y),
        ));
    }
    let o = live::grid_offset(e, cell, cols - 1, rows - 1);
    let k = COUNT_OFFSET_PX / zoom.max(1e-6);
    out.push((
        GridHandle::Count,
        Point::new(cell.x1 + o.x + k, cell.y0 + o.y - k),
    ));
    out
}

/// Grid `start` with its handle `h` dragged by `d` (page space), for
/// cells `cell`; `snap` is how near nothing a gap becomes nothing.
pub fn dragged(start: &Effect, cell: Rect, h: GridHandle, d: Vec2, snap: f64) -> Effect {
    let mut e = start.clone();
    let Effect::Grid {
        columns,
        rows,
        gap_x,
        gap_y,
        ..
    } = &mut e
    else {
        return e;
    };
    let near_none = |v: f64| if v.abs() <= snap { 0.0 } else { v };
    // A gap never closes a cell up past nothing.
    match h {
        GridHandle::GapX => *gap_x = near_none(*gap_x + d.x).max(-cell.width() * 0.99),
        GridHandle::GapY => *gap_y = near_none(*gap_y - d.y).max(-cell.height() * 0.99),
        GridHandle::Count => {
            let (c0, r0) = live::grid_size(start);
            let step_x = cell.width() + *gap_x;
            let step_y = cell.height() + *gap_y;
            if step_x > 1e-9 {
                let c = (f64::from(c0) + d.x / step_x).round();
                *columns = c.clamp(1.0, f64::from(live::MAX_GRID_SIDE)) as u32;
            }
            if step_y > 1e-9 {
                let r = (f64::from(r0) - d.y / step_y).round();
                *rows = r.clamp(1.0, f64::from(live::MAX_GRID_SIDE)) as u32;
            }
        }
    }
    e
}

impl App {
    /// The selected object with a Grid effect while the Pick tool shows
    /// its handles: its id, the box of its cells and the effect.
    pub fn grid_under_edit(&self) -> Option<(ShapeId, Rect, Effect)> {
        if self.tool != Tool::Pick || self.rotate_mode || self.selection.len() != 1 {
            return None;
        }
        let id = self.selection[0];
        let s = self.doc().shape_on_page(id)?;
        let e = grid_of(&s)?;
        let rev = self.engine.revision();
        if let Some((r, i, cell)) = *self.grid_cell.borrow() {
            if r == rev && i == id {
                return Some((id, cell, e));
            }
        }
        // Found once per change of the drawing, not every frame.
        let cell = live::grid_cell(&s)?;
        *self.grid_cell.borrow_mut() = Some((rev, id, cell));
        Some((id, cell, e))
    }

    /// The handles of the grid being shown (page space).
    pub fn grid_handles(&self) -> Vec<(GridHandle, Point)> {
        match self.grid_under_edit() {
            Some((_, cell, e)) => handles(&e, cell, f64::from(self.view.zoom)),
            None => Vec::new(),
        }
    }

    /// A Pick tool press at `p` (page space) on a grid handle starts
    /// dragging it. True when the press was taken.
    pub fn grid_press(&mut self, p: Point) -> bool {
        let Some((id, _, start)) = self.grid_under_edit() else {
            return false;
        };
        let reach = REACH_PX / f64::from(self.view.zoom.max(1e-6));
        let Some((handle, _)) = self
            .grid_handles()
            .into_iter()
            .find(|(_, h)| (*h - p).hypot() <= reach)
        else {
            return false;
        };
        self.grid_step = None;
        self.drag = Drag::GridHandle {
            id,
            handle,
            from: p,
            start,
        };
        true
    }

    /// The grid handle dragged to `p`.
    pub fn grid_drag_to(&mut self, p: Point) {
        let Drag::GridHandle {
            id,
            handle,
            from,
            ref start,
        } = self.drag
        else {
            return;
        };
        let start = start.clone();
        let Some((_, cell, _)) = self.grid_under_edit() else {
            return;
        };
        let snap = REACH_PX / f64::from(self.view.zoom.max(1e-6));
        let e = dragged(&start, cell, handle, p - from, snap);
        self.set_grid_of(id, e);
    }

    /// Give the object `id` grid `grid`; the changes of one drag are one
    /// undo step.
    pub fn set_grid_of(&mut self, id: ShapeId, grid: Effect) {
        let Some(s) = self.doc().find_shape(id) else {
            return;
        };
        if grid_of(s).as_ref() == Some(&grid) {
            return;
        }
        let mut done = false;
        let effects: Vec<Effect> = s
            .effects
            .iter()
            .map(|e| match e {
                Effect::Grid { .. } if !done => {
                    done = true;
                    grid.clone()
                }
                other => other.clone(),
            })
            .collect();
        let cmd = Command::SetEffects { shape: id, effects };
        let r = if self.engine.undo_label() == Some(LABEL) && self.grid_step == Some(id) {
            self.engine.amend(&cmd)
        } else {
            self.engine.run_with_label(&cmd, LABEL)
        };
        match r {
            Ok(()) => self.grid_step = Some(id),
            Err(e) => self.status = e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::ShapeKind;

    fn grid(columns: u32, rows: u32, gap: f64) -> Effect {
        Effect::Grid {
            columns,
            rows,
            gap_x: gap,
            gap_y: gap,
            shift_x: 0.0,
            shift_y: 0.0,
        }
    }

    /// The handles sit on the second column, the second row and past the
    /// last corner; dragging them sets the gaps (nothing near nothing)
    /// and the count.
    #[test]
    fn handles_drag_gaps_and_count() {
        let cell = Rect::new(0.0, 0.0, 10.0, 10.0);
        let e = grid(3, 2, 2.0);
        let hs = handles(&e, cell, 1.0);
        assert_eq!(hs[0], (GridHandle::GapX, Point::new(12.0, 5.0)));
        assert_eq!(hs[1], (GridHandle::GapY, Point::new(5.0, -2.0)));
        assert_eq!(hs[2].0, GridHandle::Count);
        let gx = |e: &Effect| match e {
            Effect::Grid { gap_x, gap_y, .. } => (*gap_x, *gap_y),
            _ => (f64::NAN, f64::NAN),
        };
        assert_eq!(
            gx(&dragged(
                &e,
                cell,
                GridHandle::GapX,
                Vec2::new(3.0, 0.0),
                0.5
            ))
            .0,
            5.0
        );
        assert_eq!(
            gx(&dragged(
                &e,
                cell,
                GridHandle::GapX,
                Vec2::new(-1.8, 0.0),
                0.5
            ))
            .0,
            0.0
        );
        // Down widens the gap between rows (y up).
        assert_eq!(
            gx(&dragged(
                &e,
                cell,
                GridHandle::GapY,
                Vec2::new(0.0, -4.0),
                0.5
            ))
            .1,
            6.0
        );
        let more = dragged(&e, cell, GridHandle::Count, Vec2::new(24.0, -12.0), 0.5);
        assert_eq!(live::grid_size(&more), (5, 3));
        let fewer = dragged(&e, cell, GridHandle::Count, Vec2::new(-100.0, 100.0), 0.5);
        assert_eq!(live::grid_size(&fewer), (1, 1));
    }

    /// A drag on a handle in the app changes the grid of the selected
    /// object as one undo step.
    #[test]
    fn dragging_a_handle_is_one_step() {
        let mut app = App::headless();
        app.tool = Tool::Pick;
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        let mut effects = app.doc().find_shape(id).expect("shape").effects.clone();
        effects.push(grid(3, 2, 2.0));
        app.engine
            .run(&Command::SetEffects { shape: id, effects })
            .expect("grid");
        app.select(vec![id]);
        let (_, at) = app
            .grid_handles()
            .into_iter()
            .find(|(h, _)| *h == GridHandle::GapX)
            .expect("gap handle");
        let before = app.engine.undo_label().map(str::to_owned);
        assert!(app.grid_press(at));
        for k in 1..=4 {
            app.grid_drag_to(at + Vec2::new(f64::from(k) * 2.0, 0.0));
        }
        app.drag = Drag::None;
        let s = app.doc().find_shape(id).expect("shape");
        assert!(
            matches!(grid_of(s), Some(Effect::Grid { gap_x, .. }) if (gap_x - 10.0).abs() < 1e-9)
        );
        app.undo();
        let s = app.doc().find_shape(id).expect("shape");
        assert!(
            matches!(grid_of(s), Some(Effect::Grid { gap_x, .. }) if (gap_x - 2.0).abs() < 1e-9)
        );
        assert_eq!(app.engine.undo_label().map(str::to_owned), before);
    }
}
