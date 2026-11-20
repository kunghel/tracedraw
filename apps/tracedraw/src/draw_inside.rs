//! Draw Inside (#3407): a button on the property bar of the tools that
//! draw. With one closed shape selected it starts editing the shape's
//! contents, so what the tool draws next goes inside the shape, clipped
//! by it, as Place Inside Frame would put it there; pressed again (or
//! Finish Editing on the clip frame bar) it ends. Draw Behind, beside
//! it, puts what the tool draws behind the selected objects instead of
//! on top.

use crate::app::App;
use crate::i18n::tr;
use crate::tools::Tool;
use egui::Ui;
use tracedraw_core::{document::ShapeKind, Command, Shape};

/// The tools whose property bar has Draw Inside: the ones that draw
/// shapes and lines.
pub fn draws_shapes(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Freehand
            | Tool::Bezier
            | Tool::Pen
            | Tool::Polyline
            | Tool::TwoPointLine
            | Tool::BSpline
            | Tool::ThreePointCurve
            | Tool::BrushStrokes
            | Tool::PainterlyBrush
            | Tool::Sketch
            | Tool::ShapeRecognition
            | Tool::Rectangle
            | Tool::ThreePointRectangle
            | Tool::Ellipse
            | Tool::ThreePointEllipse
            | Tool::Polygon
            | Tool::Star
            | Tool::Spiral
            | Tool::CommonShapes
    )
}

/// Whether `s` is a closed shape that can hold what is drawn inside it:
/// a rectangle, a polygon or star, an ellipse or pie (not an open arc),
/// a closed curve, or a clip frame already.
pub fn is_closed_shape(s: &Shape) -> bool {
    match &s.kind {
        ShapeKind::Rect { .. } | ShapeKind::Polygon { .. } | ShapeKind::ClipFrame { .. } => true,
        ShapeKind::Ellipse { arc, .. } => arc.is_none_or(|a| a.pie),
        ShapeKind::Path { closed, .. } => *closed,
        _ => false,
    }
}

impl App {
    /// The one selected shape Draw Inside would draw into, when it is a
    /// closed shape.
    pub fn draw_inside_target(&self) -> Option<Shape> {
        let shapes = self.selected_shapes();
        match shapes.as_slice() {
            [s] if is_closed_shape(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Draw Inside pressed: into the selected closed shape, or out of the
    /// contents being edited. With no closed shape selected the status
    /// line says one is needed.
    pub fn toggle_draw_inside(&mut self) {
        if self.editing_clip_contents() {
            self.finish_clip_frame_edit();
            return;
        }
        if self.clip_frame_edit.is_some() {
            return;
        }
        let Some(target) = self.draw_inside_target() else {
            self.status = tr("status.draw_inside_needs_closed");
            return;
        };
        if matches!(target.kind, ShapeKind::ClipFrame { .. }) {
            // A clip frame: its contents come out to be edited with what
            // is drawn.
            self.edit_clip_frame();
        } else {
            // A plain shape: everything on the page besides it stays
            // outside; what is drawn now goes in when the editing ends.
            let outside: std::collections::HashSet<_> = self
                .page_layers()
                .iter()
                .flat_map(|l| &l.shapes)
                .map(|s| s.id)
                .filter(|id| *id != target.id)
                .collect();
            self.clip_frame_edit = Some((target.id, outside));
            self.select(Vec::new());
        }
        self.status = tr("status.drawing_inside");
    }

    /// With Draw Behind on, the move that puts a shape a drawing tool has
    /// just added behind the selected objects of its layer (the lowest of
    /// them), or at the back of the layer when none of them is there;
    /// `None` when it stays on top. Not while a shape's contents are
    /// edited, where what is drawn goes inside.
    pub(crate) fn drawn_behind(&self, cmd: &Command) -> Option<Command> {
        let Command::AddShape { layer, shape } = cmd else {
            return None;
        };
        if !self.draw_behind || !draws_shapes(self.tool) || self.clip_frame_edit.is_some() {
            return None;
        }
        let shapes = &self.doc().layer(*layer).ok()?.shapes;
        let here = shapes.iter().position(|s| s.id == shape.id)?;
        let index = shapes
            .iter()
            .position(|s| s.id != shape.id && self.selection.contains(&s.id))
            .unwrap_or(0);
        (index < here).then_some(Command::Reorder {
            shape: shape.id,
            layer: *layer,
            index,
        })
    }
}

/// The Draw Inside button at the end of a drawing tool's property bar:
/// on while the contents of a shape are edited, greyed out with no
/// closed shape selected.
pub fn draw_inside_toggle(app: &mut App, ui: &mut Ui) {
    if !draws_shapes(app.tool) {
        return;
    }
    let on = app.editing_clip_contents();
    let usable = on || (app.clip_frame_edit.is_none() && app.draw_inside_target().is_some());
    ui.separator();
    let tip = if usable {
        crate::ui::tips::keyed("toolbar.draw_inside")
    } else {
        format!(
            "{}\n{}",
            crate::ui::tips::keyed("toolbar.draw_inside"),
            tr("status.draw_inside_needs_closed")
        )
    };
    let r = ui
        .add_enabled(
            usable,
            egui::Button::selectable(on, tr("toolbar.draw_inside")),
        )
        .on_hover_text(tip.clone())
        .on_disabled_hover_text(tip);
    if r.clicked() {
        app.toggle_draw_inside();
    }
    let tip = crate::ui::tips::keyed("toolbar.draw_behind");
    if ui
        .add(egui::Button::selectable(
            app.draw_behind,
            tr("toolbar.draw_behind"),
        ))
        .on_hover_text(tip)
        .clicked()
    {
        app.draw_behind = !app.draw_behind;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{BezPath, Point, Rect};

    fn square(app: &mut App) -> tracedraw_core::ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(0.0, 0.0, 50.0, 50.0),
            radius: 0.0,
            corners: None,
        })
        .expect("square")
    }

    /// With a closed shape selected, Draw Inside puts what the tool
    /// draws next inside the shape, clipped by it, when the editing
    /// ends; the second press ends it (#3407).
    #[test]
    fn what_is_drawn_inside_goes_into_the_shape() {
        let mut app = App::headless();
        let sq = square(&mut app);
        app.tool = Tool::Freehand;
        app.select(vec![sq]);
        app.toggle_draw_inside();
        assert!(app.editing_clip_contents());
        let mut line = BezPath::new();
        line.move_to(Point::new(10.0, 10.0));
        line.line_to(Point::new(80.0, 40.0));
        let drawn = app.add_drawn_line(line).expect("line");
        app.toggle_draw_inside();
        assert!(!app.editing_clip_contents());
        let s = app.doc().find_shape(sq).expect("the square stays").clone();
        let ShapeKind::ClipFrame { contents, .. } = &s.kind else {
            panic!("not a clip frame: {:?}", s.kind);
        };
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].id, drawn);
        // Drawn again into the clip frame: its contents grow.
        app.select(vec![sq]);
        app.toggle_draw_inside();
        assert!(app.editing_clip_contents());
        app.new_shape(ShapeKind::Ellipse {
            rect: Rect::new(5.0, 5.0, 20.0, 20.0),
            arc: None,
        })
        .expect("ellipse");
        app.toggle_draw_inside();
        let ShapeKind::ClipFrame { contents, .. } = &app.doc().find_shape(sq).expect("sq").kind
        else {
            panic!("not a clip frame");
        };
        assert_eq!(contents.len(), 2);
    }

    /// An open curve, or more than one object, takes nothing inside: the
    /// button is greyed out and pressing it only says why.
    #[test]
    fn draw_inside_needs_one_closed_shape() {
        let mut app = App::headless();
        let mut line = BezPath::new();
        line.move_to(Point::new(0.0, 0.0));
        line.line_to(Point::new(10.0, 10.0));
        let open = app
            .new_shape(ShapeKind::Path {
                path: line,
                closed: false,
            })
            .expect("line");
        let sq = square(&mut app);
        app.tool = Tool::Bezier;
        for sel in [vec![open], vec![open, sq], Vec::new()] {
            app.select(sel);
            assert!(app.draw_inside_target().is_none());
            app.toggle_draw_inside();
            assert!(!app.editing_clip_contents());
            assert_eq!(app.status, tr("status.draw_inside_needs_closed"));
        }
        assert!(draws_shapes(Tool::Freehand) && !draws_shapes(Tool::Pick));
    }

    fn order(app: &App) -> Vec<tracedraw_core::ShapeId> {
        let layer = app.active_layer().expect("layer");
        app.doc()
            .layer(layer)
            .expect("layer")
            .shapes
            .iter()
            .map(|s| s.id)
            .collect()
    }

    /// Draw Behind: what is drawn goes behind the selected object, or to
    /// the back of the layer with nothing selected; off, on top. One
    /// undo step takes the new shape away and leaves the order as it was.
    #[test]
    fn what_is_drawn_behind_goes_under_the_selection() {
        let mut app = App::headless();
        let a = square(&mut app);
        let b = square(&mut app);
        app.tool = Tool::Rectangle;
        app.draw_behind = true;
        app.select(vec![b]);
        let c = square(&mut app);
        assert_eq!(order(&app), vec![a, c, b]);
        app.select(Vec::new());
        let d = square(&mut app);
        assert_eq!(order(&app), vec![d, a, c, b]);
        app.undo();
        assert_eq!(order(&app), vec![a, c, b]);
        // Not a drawing tool, or Draw Behind off: on top, as before.
        app.tool = Tool::Pick;
        let e = square(&mut app);
        assert_eq!(order(&app), vec![a, c, b, e]);
        app.tool = Tool::Ellipse;
        app.draw_behind = false;
        let f = square(&mut app);
        assert_eq!(order(&app), vec![a, c, b, e, f]);
    }
}
