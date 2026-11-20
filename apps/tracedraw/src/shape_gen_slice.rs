//! Shape Generator slice grid: the selection cut into strips or a grid of
//! cells, each piece moved by a displacement (random, a wave, a ramp or
//! alternating), for sliced and shifted looks.
//!
//! Each piece is a clip frame: the cell's rectangle (no fill, no outline)
//! holding copies of the selected objects, so curves, text and bitmaps
//! are all cut the same way and stay editable (text stays text, a bitmap
//! keeps its pixels). The preview and Create draw the displaced cells.
//!
//! Model space: mm, y up.

use crate::app::App;
use crate::shape_gen::{polygon, Generated, Paint, Rng, ShapeGenSettings};
use tracedraw_core::geometry::{Affine, Point, Rect, Vec2};
use tracedraw_core::{Shape, ShapeId, ShapeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SliceLayout {
    #[default]
    Rows,
    Columns,
    Grid,
}

impl SliceLayout {
    pub const ALL: [SliceLayout; 3] = [SliceLayout::Rows, SliceLayout::Columns, SliceLayout::Grid];
    pub fn key(self) -> &'static str {
        match self {
            SliceLayout::Rows => "shapegen.slice_rows_layout",
            SliceLayout::Columns => "shapegen.slice_columns_layout",
            SliceLayout::Grid => "shapegen.slice_grid_layout",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlicePattern {
    #[default]
    Random,
    Wave,
    Ramp,
    Alternate,
}

impl SlicePattern {
    pub const ALL: [SlicePattern; 4] = [
        SlicePattern::Random,
        SlicePattern::Wave,
        SlicePattern::Ramp,
        SlicePattern::Alternate,
    ];
    pub fn key(self) -> &'static str {
        match self {
            SlicePattern::Random => "shapegen.slice_random",
            SlicePattern::Wave => "shapegen.slice_wave",
            SlicePattern::Ramp => "shapegen.slice_ramp",
            SlicePattern::Alternate => "shapegen.slice_alternate",
        }
    }
}

/// The cells over `b` and how far each moves: strips across (Rows), down
/// (Columns) or both (Grid), shrunk by half the gap on every side.
/// Rows move by their row, columns by their column; in a grid the
/// sideways move follows the row and the up-down move the column.
pub(crate) fn cells(s: &ShapeGenSettings, b: Rect) -> Vec<(Rect, Vec2)> {
    let (nx, ny) = match s.slice_layout {
        SliceLayout::Rows => (1, s.slice_rows.max(1)),
        SliceLayout::Columns => (s.slice_cols.max(1), 1),
        SliceLayout::Grid => (s.slice_cols.max(1), s.slice_rows.max(1)),
    };
    let (cw, ch) = (b.width() / nx as f64, b.height() / ny as f64);
    let half = (s.slice_gap / 2.0).min(cw * 0.45).min(ch * 0.45).max(0.0);
    let mut rng = Rng::new(s.seed, 0x511c);
    let mut out = Vec::with_capacity((nx * ny) as usize);
    // -1 to 1 along the strip index `k` of `n` (middle at `t`).
    let shape = |k: u32, n: u32, rng: &mut Rng| -> f64 {
        let t = (k as f64 + 0.5) / n as f64;
        match s.slice_pattern {
            SlicePattern::Random => rng.unit() * 2.0 - 1.0,
            SlicePattern::Wave => (std::f64::consts::TAU * s.slice_waves * t).sin(),
            SlicePattern::Ramp => 2.0 * t - 1.0,
            SlicePattern::Alternate => {
                if k % 2 == 0 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    };
    for j in 0..ny {
        for i in 0..nx {
            let r = Rect::new(
                b.x0 + i as f64 * cw + half,
                b.y0 + j as f64 * ch + half,
                b.x0 + (i + 1) as f64 * cw - half,
                b.y0 + (j + 1) as f64 * ch - half,
            );
            let (fx, fy) = match s.slice_layout {
                SliceLayout::Rows => {
                    let a = shape(j, ny, &mut rng);
                    let b = shape(j, ny, &mut rng);
                    (
                        a,
                        if s.slice_pattern == SlicePattern::Random {
                            b
                        } else {
                            a
                        },
                    )
                }
                SliceLayout::Columns => {
                    let a = shape(i, nx, &mut rng);
                    let b = shape(i, nx, &mut rng);
                    (
                        if s.slice_pattern == SlicePattern::Random {
                            b
                        } else {
                            a
                        },
                        a,
                    )
                }
                SliceLayout::Grid => (shape(j, ny, &mut rng), shape(i, nx, &mut rng)),
            };
            out.push((r, Vec2::new(s.slice_dx * fx, s.slice_dy * fy)));
        }
    }
    out
}

/// The displaced cells over the box, for the preview and Create.
pub(crate) fn slice_grid(s: &ShapeGenSettings, out: &mut Generated) {
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    for (r, off) in cells(s, rect) {
        if out.full() {
            out.cut = true;
            return;
        }
        let r = r + off;
        out.push(
            polygon(&[
                Point::new(r.x0, r.y0),
                Point::new(r.x1, r.y0),
                Point::new(r.x1, r.y1),
                Point::new(r.x0, r.y1),
            ]),
            true,
            Paint::Line,
        );
    }
}

impl App {
    /// Cut the selection into the panel's cells and move each piece:
    /// one group of clip frames in one undo step, the originals removed
    /// unless Keep Original is on.
    pub fn shape_gen_slice_selection(&mut self) -> Option<ShapeId> {
        let s = self.shape_gen.sanitized();
        let bounds = self.selection_bounds()?;
        let shapes = self.selected_shapes();
        if shapes.is_empty() || bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return None;
        }
        let layer = self.active_layer()?;
        let mut pieces = Vec::new();
        for (r, off) in cells(&s, bounds.inflate(1e-6, 1e-6)) {
            if r.width() <= 0.0 || r.height() <= 0.0 {
                continue;
            }
            let fid = self.engine.new_shape_id();
            let mut frame = Shape::new(
                fid,
                ShapeKind::Rect {
                    rect: r + off,
                    radius: 0.0,
                    corners: None,
                },
            );
            frame.stroke = None;
            let mut contents = Vec::with_capacity(shapes.len());
            for sh in &shapes {
                let nid = self.engine.new_shape_id();
                let mut c = crate::app::reid_pub(sh.clone(), nid, &mut self.engine);
                c.transform = Affine::translate(off) * c.transform;
                contents.push(c);
            }
            let pid = self.engine.new_shape_id();
            let mut piece = Shape::new(
                pid,
                ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents,
                },
            );
            piece.stroke = None;
            pieces.push(piece);
        }
        if pieces.is_empty() {
            return None;
        }
        let gid = self.engine.new_shape_id();
        let mut group = Shape::new(gid, ShapeKind::Group { children: pieces });
        group.stroke = None;
        group.name = Some(crate::i18n::tr("shapegen.kind_slice"));
        let mut cmds = Vec::new();
        if !s.slice_keep {
            cmds.push(tracedraw_core::Command::DeleteShapes {
                shapes: shapes.iter().map(|sh| sh.id).collect(),
            });
        }
        cmds.push(tracedraw_core::Command::AddShape {
            layer,
            shape: group,
        });
        if let Err(e) = self.engine.run_batch("Slice Selection", &cmds) {
            log::warn!("shape generator slice: {e}");
            return None;
        }
        self.select(vec![gid]);
        self.status = crate::i18n::tr("status.shape_generator_sliced");
        Some(gid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::{generate, GenKind};

    fn slice() -> ShapeGenSettings {
        ShapeGenSettings {
            kind: GenKind::SliceGrid,
            slice_layout: SliceLayout::Rows,
            slice_rows: 4,
            slice_pattern: SlicePattern::Alternate,
            slice_dx: 5.0,
            slice_dy: 0.0,
            ..ShapeGenSettings::default()
        }
    }

    #[test]
    fn cells_tile_the_box_and_move_by_the_pattern() {
        let b = Rect::new(0.0, 0.0, 100.0, 40.0);
        let c = cells(&slice(), b);
        assert_eq!(c.len(), 4);
        let area: f64 = c.iter().map(|(r, _)| r.area()).sum();
        assert!((area - 4000.0).abs() < 1e-9);
        // Alternate: +5, -5, +5, -5 sideways.
        let xs: Vec<f64> = c.iter().map(|(_, o)| o.x).collect();
        assert_eq!(xs, vec![5.0, -5.0, 5.0, -5.0]);
        // A ramp runs from one side to the other; a grid moves both ways.
        let g = cells(
            &ShapeGenSettings {
                slice_layout: SliceLayout::Grid,
                slice_cols: 3,
                slice_rows: 2,
                slice_pattern: SlicePattern::Ramp,
                slice_dy: 2.0,
                slice_gap: 1.0,
                ..slice()
            },
            b,
        );
        assert_eq!(g.len(), 6);
        assert!(g[0].1.x < 0.0 && g[5].1.x > 0.0);
        assert!(g[0].1.y < 0.0 && g[2].1.y > 0.0);
        // The gap shrinks every cell.
        assert!((g[0].0.width() - (100.0 / 3.0 - 1.0)).abs() < 1e-9);
        // The preview draws one outline per cell.
        assert_eq!(generate(&slice()).items.len(), 4);
    }

    #[test]
    fn slicing_puts_copies_of_the_selection_in_moved_frames_in_one_step() {
        let mut app = App::headless();
        let rect = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 100.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        let circle = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(10.0, 10.0, 30.0, 30.0),
                arc: None,
            })
            .expect("ellipse");
        app.select(vec![rect, circle]);
        app.shape_gen = slice();
        let before = app.engine.history_labels().0.len();
        let gid = app.shape_gen_slice_selection().expect("sliced");
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        assert!(app.doc().find_shape(rect).is_none());
        let g = app.doc().find_shape(gid).expect("group").clone();
        let ShapeKind::Group { children } = &g.kind else {
            panic!("not a group");
        };
        assert_eq!(children.len(), 4);
        for (k, piece) in children.iter().enumerate() {
            let ShapeKind::ClipFrame { frame, contents } = &piece.kind else {
                panic!("not a clip frame");
            };
            assert_eq!(contents.len(), 2);
            let dx = if k % 2 == 0 { 5.0 } else { -5.0 };
            let moved = contents[0].transform.translation();
            assert!((moved.x - dx).abs() < 1e-9, "{moved:?}");
            assert!(frame.stroke.is_none());
            // Every copy has its own id.
            assert_ne!(contents[0].id, rect);
        }
        app.undo();
        assert!(app.doc().find_shape(rect).is_some());
        // Keep Original leaves the selection in place.
        app.select(vec![rect, circle]);
        app.shape_gen.slice_keep = true;
        app.shape_gen_slice_selection().expect("sliced");
        assert!(app.doc().find_shape(rect).is_some());
    }
}
