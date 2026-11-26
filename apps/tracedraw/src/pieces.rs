//! Combine Shapes commands that cut overlapping objects into pieces
//! (Break into Pieces, Merge Same Colors, Cut to Top Object, Piece
//! Outlines, Exclude Overlaps), and Combine Live, which keeps the
//! objects and draws the group as one combined curve (the `Combine`
//! live effect). See docs/help/en/lines-shapes/shaping.md.

use crate::app::App;
use tracedraw_core::geometry::{Affine, BezPath};
use tracedraw_core::shaping::{self, CombineMode, FillRule};
use tracedraw_core::{Color, Command, Fill, Shape, ShapeId, ShapeKind};

/// What a pieces command does with the objects it cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pieces {
    /// Every piece the objects cut each other into, in the paint of the
    /// top object over it.
    Break,
    /// What shows of each object, pieces of the same fill joined.
    MergeColors,
    /// The objects under the top one cut to its outline; it goes.
    CutToTop,
    /// The outline of every piece, as a line in the piece's colour.
    Outlines,
    /// One curve where an odd number of objects overlap.
    Exclude,
}

impl Pieces {
    pub const ALL: [Pieces; 5] = [
        Pieces::Break,
        Pieces::MergeColors,
        Pieces::CutToTop,
        Pieces::Outlines,
        Pieces::Exclude,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Pieces::Break => "menu.object.break_pieces",
            Pieces::MergeColors => "menu.object.merge_colors",
            Pieces::CutToTop => "menu.object.cut_to_top",
            Pieces::Outlines => "menu.object.piece_outlines",
            Pieces::Exclude => "menu.object.exclude",
        }
    }

    /// The undo step's name.
    fn label(self) -> &'static str {
        match self {
            Pieces::Break => "Break into Pieces",
            Pieces::MergeColors => "Merge Same Colors",
            Pieces::CutToTop => "Cut to Top Object",
            Pieces::Outlines => "Piece Outlines",
            Pieces::Exclude => "Exclude Overlaps",
        }
    }

    /// How many objects it needs selected.
    pub fn needs(self) -> usize {
        match self {
            Pieces::Outlines => 1,
            _ => 2,
        }
    }
}

/// The colour a piece's outline takes from its object: the fill's colour
/// (a gradient's first), or the outline's when it has no fill.
fn piece_color(s: &Shape) -> Color {
    match &s.fill {
        Fill::Solid(c) => *c,
        Fill::None => s.stroke.as_ref().map_or(Color::BLACK, |st| st.color),
        f => f.preview_color().unwrap_or(Color::BLACK),
    }
}

/// A piece on the page as an object painted like `src` (which lies on
/// the page through `to_page`), without an outline.
fn piece(id: ShapeId, area: BezPath, src: &Shape, to_page: Affine) -> Shape {
    let mut s = Shape::new(
        id,
        ShapeKind::Path {
            path: area.clone(),
            closed: true,
        },
    );
    s.fill = src.fill.for_result(to_page, &area);
    s.extra_fills = src.extra_fills.clone();
    s.opacity = src.opacity;
    s.fill_rule = FillRule::EvenOdd;
    s.stroke = None;
    s
}

impl App {
    /// Whether the pieces command `op` can run on the selection.
    pub fn pieces_ready(&self, op: Pieces) -> bool {
        self.selection.len() >= op.needs()
    }

    /// Run a pieces command on the selected objects: the result is a new
    /// group in their place (one curve for Exclude Overlaps) and they
    /// go, in one undo step.
    pub fn pieces(&mut self, op: Pieces) {
        if !self.pieces_ready(op) {
            return;
        }
        let doc = self.doc();
        let mut shapes: Vec<Shape> = self
            .selection
            .iter()
            .filter_map(|id| doc.shape_on_page(*id))
            .filter(|s| !s.locked)
            .collect();
        shapes.sort_by_key(|s| self.z_key(s.id));
        if shapes.len() < op.needs() {
            return;
        }
        let areas: Vec<BezPath> = shapes.iter().map(crate::ops::shaping_area).collect();
        let Some(layer) = self.active_layer() else {
            return;
        };
        let mut made: Vec<Shape> = Vec::new();
        let mut gone: Vec<ShapeId> = shapes.iter().map(|s| s.id).collect();
        match op {
            Pieces::Break => {
                for (area, k) in shaping::divide_pieces(&areas) {
                    let id = self.engine.new_shape_id();
                    made.push(piece(id, area, &shapes[k], shapes[k].transform));
                }
            }
            Pieces::Outlines => {
                for (area, k) in shaping::divide_pieces(&areas) {
                    let id = self.engine.new_shape_id();
                    let mut s = Shape::new(
                        id,
                        ShapeKind::Path {
                            path: area,
                            closed: true,
                        },
                    );
                    s.fill = Fill::None;
                    s.stroke = Some(tracedraw_core::Stroke {
                        color: piece_color(&shapes[k]),
                        ..Default::default()
                    });
                    made.push(s);
                }
            }
            Pieces::MergeColors => {
                let shown = shaping::visible_parts(&areas);
                // Parts of the same fill joined, then split into the
                // pieces that touch.
                let mut by_fill: Vec<(usize, BezPath)> = Vec::new();
                for (k, part) in shown.into_iter().enumerate() {
                    if part.elements().is_empty() {
                        continue;
                    }
                    match by_fill
                        .iter_mut()
                        .find(|(j, _)| shapes[*j].fill == shapes[k].fill)
                    {
                        Some((_, acc)) => {
                            *acc = shaping::overlay(acc, &part, shaping::Op::Weld);
                        }
                        None => by_fill.push((k, part)),
                    }
                }
                for (k, area) in by_fill {
                    for p in shaping::overlay_parts(&area, &BezPath::new(), shaping::Op::Weld) {
                        let id = self.engine.new_shape_id();
                        made.push(piece(id, p, &shapes[k], shapes[k].transform));
                    }
                }
            }
            Pieces::CutToTop => {
                let (top, below) = areas.split_last().expect("two objects");
                let shown = shaping::visible_parts(below);
                for (k, part) in shown.into_iter().enumerate() {
                    if part.elements().is_empty() {
                        continue;
                    }
                    let cut = shaping::overlay(&part, top, shaping::Op::Intersect);
                    for p in shaping::overlay_parts(&cut, &BezPath::new(), shaping::Op::Weld) {
                        if shaping::filled_area(&p, FillRule::EvenOdd) <= 1e-4 {
                            continue;
                        }
                        let id = self.engine.new_shape_id();
                        made.push(piece(id, p, &shapes[k], shapes[k].transform));
                    }
                }
            }
            Pieces::Exclude => {
                let area = shaping::combine_areas(&areas, CombineMode::Exclude);
                let top = shapes.last().expect("objects");
                let id = self.engine.new_shape_id();
                let mut s = piece(id, area, top, top.transform);
                s.stroke = top.stroke.clone();
                made.push(s);
            }
        }
        if made.is_empty() {
            self.status = crate::i18n::tr("status.no_pieces");
            return;
        }
        let new_id = if made.len() == 1 {
            made[0].id
        } else {
            self.engine.new_shape_id()
        };
        let result = if made.len() == 1 {
            made.pop().expect("one")
        } else {
            Shape::new(new_id, ShapeKind::Group { children: made })
        };
        gone.dedup();
        let cmds = vec![
            Command::AddShape {
                layer,
                shape: result,
            },
            Command::DeleteShapes { shapes: gone },
        ];
        if let Err(e) = self.engine.run_batch(op.label(), &cmds) {
            self.status = e.to_string();
            return;
        }
        self.select(vec![new_id]);
    }

    /// Combine Live: the selected objects grouped (or the one selected
    /// group) and given a live Combine, welded at first; the mode is
    /// chosen in the Effects panel, which opens. The objects stay as
    /// they are inside the group.
    pub fn combine_live(&mut self) {
        self.combine_live_as(CombineMode::Weld);
    }

    /// Combine Live can act: two objects or more selected, or one group.
    pub fn combine_live_ready(&self) -> bool {
        self.selection.len() >= 2
            || (self.selection.len() == 1
                && self
                    .selected_shapes()
                    .first()
                    .is_some_and(|s| matches!(s.kind, ShapeKind::Group { .. })))
    }

    /// Combine Live in `mode` (#4266): the selected objects grouped (or
    /// the one selected group) and drawn as one curve, welded, cut or
    /// intersected as `mode` says, while every object stays as it is
    /// inside the group and can still be moved and edited. Taking the
    /// effect away (Effects or Properties) gives the objects back.
    pub fn combine_live_as(&mut self, mode: CombineMode) {
        if !self.combine_live_ready() {
            return;
        }
        let sel = self.selected_shapes();
        let one_group = sel.len() == 1 && matches!(sel[0].kind, ShapeKind::Group { .. });
        if sel.len() >= 2 {
            self.group_selection();
        } else if !one_group {
            return;
        }
        self.push_effect(tracedraw_core::live::Effect::Combine { mode }, true);
        self.open_docker(crate::app::DockerTab::Effects);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    fn square(app: &mut App, x: f64, color: Color) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(x, 0.0, x + 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.engine
            .run(&Command::SetFill {
                shapes: vec![id],
                fill: Fill::Solid(color),
            })
            .expect("fill");
        id
    }

    fn children(app: &App) -> Vec<Shape> {
        let id = app.selection[0];
        match &app.doc().find_shape(id).expect("result").kind {
            ShapeKind::Group { children } => children.clone(),
            _ => vec![app.doc().find_shape(id).expect("result").clone()],
        }
    }

    fn area(s: &Shape) -> f64 {
        shaping::filled_area(&s.page_path(), FillRule::EvenOdd).round()
    }

    /// Two overlapping squares: Break into Pieces gives three pieces,
    /// the overlap in the top square's colour; Merge Same Colors with
    /// both squares one colour gives one; Cut to Top Object keeps the
    /// bottom square inside the top one; Piece Outlines gives coloured
    /// lines; Exclude Overlaps one curve with the overlap cut out. Each
    /// is one undo step and the squares go.
    #[test]
    fn overlapping_squares_cut_into_pieces() {
        let (green, blue) = (Color::rgb8(0, 160, 0), Color::rgb8(0, 0, 200));
        let run = |op: Pieces, second: Color| {
            let mut app = App::headless();
            let a = square(&mut app, 0.0, green);
            let b = square(&mut app, 5.0, second);
            app.select(vec![a, b]);
            let depth = app.engine.history_labels().0.len();
            app.pieces(op);
            assert_eq!(app.engine.history_labels().0.len(), depth + 1);
            assert!(app.doc().find_shape(a).is_none() && app.doc().find_shape(b).is_none());
            children(&app)
        };
        let pieces = run(Pieces::Break, blue);
        assert_eq!(pieces.len(), 3);
        let blue_area: f64 = pieces
            .iter()
            .filter(|s| s.fill == Fill::Solid(blue))
            .map(area)
            .sum();
        assert_eq!(blue_area, 100.0);
        let merged = run(Pieces::MergeColors, green);
        assert_eq!(merged.len(), 1);
        assert_eq!(area(&merged[0]), 150.0);
        let cut = run(Pieces::CutToTop, blue);
        assert_eq!(cut.len(), 1);
        assert_eq!(area(&cut[0]), 50.0);
        assert_eq!(cut[0].fill, Fill::Solid(green));
        let lines = run(Pieces::Outlines, blue);
        assert_eq!(lines.len(), 3);
        assert!(lines
            .iter()
            .all(|s| s.fill == Fill::None && s.stroke.is_some()));
        let ex = run(Pieces::Exclude, blue);
        assert_eq!(ex.len(), 1);
        assert_eq!(area(&ex[0]), 100.0);
    }

    /// Combine Live groups the squares and draws them as one curve; the
    /// squares stay inside the group.
    #[test]
    fn combine_live_keeps_the_objects() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, Color::BLACK);
        let b = square(&mut app, 5.0, Color::BLACK);
        app.select(vec![a, b]);
        app.combine_live();
        let g = app.doc().shape_on_page(app.selection[0]).expect("group");
        assert!(matches!(g.kind, ShapeKind::Group { .. }));
        assert!(app.doc().find_shape(a).is_some());
        let drawn = tracedraw_core::live::evaluate(&g).main;
        assert_eq!(
            shaping::filled_area(&drawn.local_path(), FillRule::EvenOdd).round(),
            150.0
        );
    }
}
