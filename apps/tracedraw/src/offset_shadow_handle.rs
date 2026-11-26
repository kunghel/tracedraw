//! The handle of the Block Shadow and Inner Shadow tools on the canvas
//! (#1280): with the tool active, the selected object's shadow shows a
//! line from the object's middle to a square at the middle plus the
//! shadow's offset. A drag that starts on the square moves the end by the
//! pointer's way from the press, so the shadow keeps what it had and
//! changes only as far as the pointer goes; Ctrl keeps that move
//! horizontal or vertical. Each drag is one undo step. A drag that starts
//! anywhere else on an object, the line included, casts a new shadow from
//! the press, as before.

use crate::app::{App, Drag};
use crate::tools::Tool;
use tracedraw_core::{
    geometry::{Point, Vec2},
    live::Effect,
    ShapeId,
};

/// The selected shadow's handle, page millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetHandle {
    pub id: ShapeId,
    /// The object's middle, where the line starts.
    pub mid: Point,
    /// The square: `mid + offset`.
    pub end: Point,
    pub offset: Vec2,
}

/// The offset of the shadow `tool` makes, when `e` is one.
fn offset_of(tool: Tool, e: &Effect) -> Option<Vec2> {
    match (tool, e) {
        (Tool::BlockShadow, Effect::BlockShadow { offset, .. })
        | (Tool::InnerShadow, Effect::InnerShadow { offset, .. }) => Some(*offset),
        _ => None,
    }
}

impl App {
    /// The handle of the first selected object with the active tool's
    /// shadow; none with another tool.
    pub fn offset_shadow_handle(&self) -> Option<OffsetHandle> {
        if !matches!(self.tool, Tool::BlockShadow | Tool::InnerShadow) {
            return None;
        }
        self.selection.iter().find_map(|id| {
            let s = self.doc().find_shape(*id)?;
            let offset = s.effects.iter().find_map(|e| offset_of(self.tool, e))?;
            let mid = self.doc().shape_on_page(*id)?.bounds().center();
            let end = mid + offset;
            (end.x.is_finite() && end.y.is_finite()).then_some(OffsetHandle {
                id: *id,
                mid,
                end,
                offset,
            })
        })
    }

    /// A press at `at` on the selected shadow's square takes it: the drag
    /// then moves the end from where it is. False elsewhere (the line runs
    /// over the object, where a drag casts a new shadow).
    pub fn begin_offset_shadow_handle(&mut self, at: Point) -> bool {
        let Some(h) = self.offset_shadow_handle() else {
            return false;
        };
        let tol = self
            .line_pick_tol()
            .max(crate::shadow_tool::HANDLE_PX / f64::from(self.view.zoom.max(1e-6)));
        if (h.end - at).hypot() > tol {
            return false;
        }
        if self.selection != vec![h.id] {
            self.select(vec![h.id]);
        }
        self.effect_edit_step = None;
        self.drag = Drag::OffsetShadow {
            press: at,
            from: h.offset,
        };
        true
    }

    /// The pointer moved to `p` during a Block Shadow or Inner Shadow
    /// drag: a new shadow from the press, or the handle's end moved by the
    /// pointer's way (kept straight with `ctrl`).
    pub fn offset_shadow_drag_to(&mut self, p: Point, ctrl: bool) {
        let offset = match self.drag {
            Drag::ContourDrag { start } => p - start,
            Drag::OffsetShadow { press, from } => {
                from + crate::effect_line::constrain_move(p - press, ctrl)
            }
            _ => return,
        };
        match self.tool {
            Tool::BlockShadow => {
                let color = self.block_shadow_color;
                let gap = self.block_shadow_gap;
                let expand = self.block_shadow_expand;
                // A handle drag keeps the shadow's own colour, gap and
                // expansion.
                let (color, gap, expand) = self
                    .selected_shapes()
                    .first()
                    .and_then(|s| {
                        s.effects.iter().find_map(|e| match e {
                            Effect::BlockShadow {
                                color, gap, expand, ..
                            } if matches!(self.drag, Drag::OffsetShadow { .. }) => {
                                Some((*color, *gap, *expand))
                            }
                            _ => None,
                        })
                    })
                    .unwrap_or((color, gap, expand));
                self.push_effect(
                    Effect::BlockShadow {
                        offset,
                        color,
                        gap,
                        expand,
                    },
                    true,
                );
            }
            Tool::InnerShadow => {
                let st = self.selected_inner_shadow().unwrap_or(self.inner_shadow);
                self.push_effect(
                    Effect::InnerShadow {
                        offset,
                        color: st.color,
                        opacity: st.opacity,
                        blur: st.blur,
                        merge: st.merge,
                        spread: st.spread,
                    },
                    true,
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{geometry::Rect, Color, Command, Fill, ShapeKind};

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
            fill: Fill::Solid(Color::rgb8(220, 40, 40)),
        });
        id
    }

    fn block_offset(app: &App, id: ShapeId) -> Option<Vec2> {
        app.doc()
            .find_shape(id)?
            .effects
            .iter()
            .find_map(|e| offset_of(Tool::BlockShadow, e))
    }

    #[test]
    fn the_block_shadow_handle_moves_the_shadow_end() {
        let mut app = App::headless();
        app.tool = Tool::BlockShadow;
        let id = square(&mut app);
        app.select(vec![id]);
        // No shadow yet: no handle, a press does not take anything.
        assert!(app.offset_shadow_handle().is_none());
        assert!(!app.begin_offset_shadow_handle(Point::new(10.0, 10.0)));
        app.block_shadow_gap = 1.5;
        app.push_effect(
            Effect::BlockShadow {
                offset: Vec2::new(5.0, -5.0),
                color: Color::BLACK,
                gap: 0.5,
                expand: 0.0,
            },
            true,
        );
        let h = app.offset_shadow_handle().expect("handle");
        let mid = app.doc().shape_on_page(id).expect("page").bounds().center();
        assert!((h.mid - mid).hypot() < 1e-9);
        assert!((h.end - (mid + Vec2::new(5.0, -5.0))).hypot() < 1e-9);
        let steps = app.engine.history_labels().0.len();
        // The square: dragged by (3, 1), the end goes as far.
        assert!(app.begin_offset_shadow_handle(h.end + Vec2::new(0.1, 0.0)));
        app.offset_shadow_drag_to(h.end + Vec2::new(1.1, 0.5), false);
        app.offset_shadow_drag_to(h.end + Vec2::new(3.1, 1.0), false);
        app.drag = Drag::None;
        assert_eq!(app.engine.history_labels().0.len(), steps + 1);
        let off = block_offset(&app, id).expect("shadow");
        assert!((off - Vec2::new(8.0, -4.0)).hypot() < 1e-9, "{off:?}");
        // Its colour and gap stay, not the bar's defaults.
        let s = app.doc().find_shape(id).expect("square");
        assert!(s.effects.iter().any(|e| matches!(
            e,
            Effect::BlockShadow { color, gap, .. } if *color == Color::BLACK && *gap == 0.5
        )));
        // Kept straight with Ctrl.
        let h = app.offset_shadow_handle().expect("handle");
        assert!(app.begin_offset_shadow_handle(h.end));
        app.offset_shadow_drag_to(h.end + Vec2::new(4.0, 1.0), true);
        app.drag = Drag::None;
        let off = block_offset(&app, id).expect("shadow");
        assert!((off - Vec2::new(12.0, -4.0)).hypot() < 1e-9, "{off:?}");
        assert_eq!(app.engine.history_labels().0.len(), steps + 2);
        // The line and the rest of the object are left to new shadows.
        let h = app.offset_shadow_handle().expect("handle");
        assert!(!app.begin_offset_shadow_handle(h.mid + (h.end - h.mid) * 0.5));
        assert!(!app.begin_offset_shadow_handle(Point::new(2.0, 18.0)));
        // Another tool shows no handle.
        app.tool = Tool::Pick;
        assert!(app.offset_shadow_handle().is_none());
    }

    #[test]
    fn the_inner_shadow_handle_keeps_its_style() {
        let mut app = App::headless();
        app.tool = Tool::InnerShadow;
        let id = square(&mut app);
        app.select(vec![id]);
        app.push_effect(
            Effect::InnerShadow {
                offset: Vec2::new(2.0, 2.0),
                color: Color::rgb8(0, 0, 200),
                opacity: 0.4,
                blur: 1.0,
                merge: tracedraw_core::live::MergeMode::Multiply,
                spread: 0.0,
            },
            true,
        );
        let h = app.offset_shadow_handle().expect("handle");
        assert!(app.begin_offset_shadow_handle(h.end));
        app.offset_shadow_drag_to(h.end + Vec2::new(-1.0, 3.0), false);
        app.drag = Drag::None;
        let s = app.doc().find_shape(id).expect("square");
        let Some(Effect::InnerShadow {
            offset,
            color,
            opacity,
            ..
        }) = s
            .effects
            .iter()
            .find(|e| matches!(e, Effect::InnerShadow { .. }))
        else {
            panic!("an inner shadow");
        };
        assert!((*offset - Vec2::new(1.0, 5.0)).hypot() < 1e-9);
        assert_eq!(*color, Color::rgb8(0, 0, 200));
        assert_eq!(*opacity, 0.4);
    }
}
