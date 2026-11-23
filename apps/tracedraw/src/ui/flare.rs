//! Object > Insert > Light Flare: a flare of light drawn with vector
//! objects (a glowing halo, a bright core, rays, a ring and a trail of
//! faint discs along one direction), made as one group the size of the
//! settings, in the middle of the page. Each part is an ordinary object
//! with a see-through gradient, so it can be recoloured, moved or taken
//! apart like any other.

use crate::app::App;
use crate::i18n::tr;
use crate::ui::dialogs::{ok_cancel, window};
use crate::ui::field::NumField;
use crate::ui::tips::RichTip;
use egui::Context;
use tracedraw_core::document::{Shape, ShapeKind};
use tracedraw_core::geometry::{BezPath, Circle, Point, Shape as _, Vec2};
use tracedraw_core::style::{Fill, Fountain, FountainKind, Stop};
use tracedraw_core::{Color, Command, ShapeId};

/// The dialog's settings, kept for the next flare.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flare {
    /// Diameter of the halo, mm.
    pub halo: f64,
    /// Number of rays (0: none).
    pub rays: u32,
    /// Length of the longest rays from the middle, mm.
    pub ray_length: f64,
    /// A thin ring round the halo.
    pub ring: bool,
    /// Number of faint discs in the trail (0: none).
    pub discs: u32,
    /// How far the trail reaches from the middle, mm.
    pub distance: f64,
    /// Direction of the trail, degrees counterclockwise from the right.
    pub angle: f64,
    pub color: Color,
}

impl Default for Flare {
    fn default() -> Self {
        Flare {
            halo: 40.0,
            rays: 12,
            ray_length: 60.0,
            ring: true,
            discs: 5,
            distance: 90.0,
            angle: -35.0,
            color: Color::rgb8(255, 170, 60),
        }
    }
}

/// A radial gradient from `inner` at the middle to `outer` at the edge,
/// the opacity going from `a` to `b`.
fn glow(inner: Color, outer: Color, a: f64, b: f64) -> Fill {
    let mut f = Fountain::two(FountainKind::Radial, inner, outer, 0.0);
    f.stops = vec![
        Stop {
            opacity: a as f32,
            ..Stop::new(0.0, inner)
        },
        Stop {
            opacity: b as f32,
            ..Stop::new(1.0, outer)
        },
    ];
    Fill::Fountain(f)
}

fn disc(c: Point, r: f64) -> BezPath {
    Circle::new(c, r.max(0.01)).to_path(0.01)
}

/// A number from 0 to 1, the next of the sequence kept in `s`.
fn next(s: &mut u64) -> f64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    (*s >> 11) as f64 / (1u64 << 53) as f64
}

impl Flare {
    /// The parts of the flare with its middle at `c`, from the bottom up,
    /// as (path, fill) pairs.
    pub fn parts(&self, c: Point) -> Vec<(BezPath, Fill)> {
        let white = Color::WHITE;
        let r = (self.halo / 2.0).max(0.5);
        let mut out = vec![(disc(c, r), glow(white, self.color, 0.9, 0.0))];
        if self.ring {
            let mut ring = disc(c, r * 1.6);
            let mut inner = disc(c, r * 1.52);
            inner = inner.reverse_subpaths();
            ring.extend(inner.elements().iter().copied());
            out.push((ring, glow(self.color, self.color, 0.15, 0.35)));
        }
        let mut seed = 0x9E37_79B9_7F4A_7C15 ^ u64::from(self.rays) ^ (u64::from(self.discs) << 32);
        let dir = Vec2::new(self.angle.to_radians().cos(), self.angle.to_radians().sin());
        for k in 0..self.discs {
            let along = (k + 1) as f64 / self.discs as f64;
            let at = c + dir * self.distance * (0.25 + 0.75 * along);
            let size = r * (0.12 + 0.35 * next(&mut seed));
            out.push((
                disc(at, size),
                glow(self.color, self.color, 0.08, 0.3 + 0.2 * next(&mut seed)),
            ));
        }
        if self.rays > 0 {
            let n = self.rays as usize;
            let mut star = BezPath::new();
            let step = std::f64::consts::TAU / n as f64;
            let base = (r * 0.06).max(0.2);
            for i in 0..n {
                let a = i as f64 * step + 0.3 * step * (next(&mut seed) - 0.5);
                let len = self.ray_length.max(r) * (0.55 + 0.45 * next(&mut seed));
                let tip = c + Vec2::new(a.cos(), a.sin()) * len;
                let side = |da: f64| c + Vec2::new((a + da).cos(), (a + da).sin()) * base;
                let first = side(-step / 2.0);
                if i == 0 {
                    star.move_to(first);
                } else {
                    star.line_to(first);
                }
                star.line_to(tip);
                star.line_to(side(step / 2.0));
            }
            star.close_path();
            out.push((star, glow(white, self.color, 0.85, 0.0)));
        }
        out.push((disc(c, r * 0.18), glow(white, white, 1.0, 0.0)));
        out
    }
}

impl App {
    pub fn open_flare_dialog(&mut self) {
        self.dialog = crate::ui::dialogs::Dialog::Flare(self.last_flare);
    }

    /// Insert a flare made with `st` in the middle of the page, as one
    /// group, selected. One undo step.
    pub fn insert_flare(&mut self, st: &Flare) -> Option<ShapeId> {
        let layer = self.active_layer()?;
        self.last_flare = *st;
        let c = self.page_rect().center();
        let kids: Vec<Shape> = st
            .parts(c)
            .into_iter()
            .map(|(path, fill)| {
                let mut s = Shape::new(
                    self.engine.new_shape_id(),
                    ShapeKind::Path { path, closed: true },
                );
                s.fill = fill;
                s.stroke = None;
                s
            })
            .collect();
        let id = self.engine.new_shape_id();
        let mut group = Shape::new(id, ShapeKind::Group { children: kids });
        group.fill = Fill::None;
        group.stroke = None;
        group.name = Some(tr("flare.object_name"));
        self.run(Command::AddShape {
            layer,
            shape: group,
        });
        self.select(vec![id]);
        Some(id)
    }
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut Flare, close: &mut bool) {
    let units = app.units;
    window(ctx, tr("flare.title")).show(ctx, |ui| {
        ui.set_width(300.0);
        let length = |ui: &mut egui::Ui, v: &mut f64, key: &str| {
            let mut u = units.from_mm(*v);
            if ui
                .add(
                    NumField::new(&mut u)
                        .range(0.0..=10000.0)
                        .speed(0.5)
                        .suffix(format!(" {}", units.short())),
                )
                .rich_tip(crate::ui::tips::keyed(key))
                .changed()
            {
                *v = units.to_mm(u);
            }
        };
        egui::Grid::new("flare").num_columns(2).show(ui, |ui| {
            ui.label(tr("flare.halo"));
            length(ui, &mut st.halo, "flare.halo");
            ui.end_row();
            ui.label(tr("flare.rays"));
            let mut n = f64::from(st.rays);
            if ui
                .add(NumField::new(&mut n).range(0.0..=64.0).speed(0.2))
                .rich_tip(crate::ui::tips::keyed("flare.rays"))
                .changed()
            {
                st.rays = n.round().clamp(0.0, 64.0) as u32;
            }
            ui.end_row();
            ui.label(tr("flare.ray_length"));
            length(ui, &mut st.ray_length, "flare.ray_length");
            ui.end_row();
            ui.label(tr("flare.discs"));
            let mut n = f64::from(st.discs);
            if ui
                .add(NumField::new(&mut n).range(0.0..=20.0).speed(0.2))
                .rich_tip(crate::ui::tips::keyed("flare.discs"))
                .changed()
            {
                st.discs = n.round().clamp(0.0, 20.0) as u32;
            }
            ui.end_row();
            ui.label(tr("flare.distance"));
            length(ui, &mut st.distance, "flare.distance");
            ui.end_row();
            ui.label(tr("flare.angle"));
            ui.add(
                NumField::new(&mut st.angle)
                    .range(-360.0..=360.0)
                    .speed(1.0)
                    .suffix("°"),
            )
            .rich_tip(crate::ui::tips::keyed("flare.angle"));
            ui.end_row();
            ui.label(tr("flare.color"));
            crate::ui::color_picker::color_button(ui, &mut st.color);
            ui.end_row();
        });
        ui.checkbox(&mut st.ring, tr("flare.ring"))
            .rich_tip(crate::ui::tips::keyed("flare.ring"));
        ui.add_space(6.0);
        if ok_cancel(ui, close) {
            let st = *st;
            app.insert_flare(&st);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flare_is_one_group_of_see_through_parts_in_one_step() {
        let mut app = App::headless();
        let st = Flare::default();
        let id = app.insert_flare(&st).expect("flare");
        let g = app.doc().find_shape(id).expect("group").clone();
        let ShapeKind::Group { children } = &g.kind else {
            panic!("{:?}", g.kind);
        };
        // Halo, ring, five discs, rays, core.
        assert_eq!(children.len(), 9);
        // Centred on the page.
        let b = app.visual_bounds(&children[0]);
        let pc = app.page_rect().center();
        assert!((b.center() - pc).hypot() < 1e-6, "{b:?}");
        assert!((b.width() - st.halo).abs() < 0.1, "{b:?}");
        // Each part fades out.
        for k in children {
            let Fill::Fountain(f) = &k.fill else {
                panic!("{:?}", k.fill);
            };
            assert!(f.stops.iter().any(|s| s.opacity < 1.0));
        }
        assert_eq!(app.last_flare, st);
        app.undo();
        assert!(app.doc().find_shape(id).is_none());
    }

    #[test]
    fn the_trail_runs_along_the_angle_and_parts_can_be_left_out() {
        let st = Flare {
            rays: 0,
            ring: false,
            discs: 3,
            angle: 0.0,
            distance: 100.0,
            ..Default::default()
        };
        let parts = st.parts(Point::ZERO);
        // Halo, three discs, core.
        assert_eq!(parts.len(), 5);
        let last = parts[3].0.bounding_box().center();
        assert!(
            (last.x - 100.0).abs() < 1e-6 && last.y.abs() < 1e-6,
            "{last:?}"
        );
    }
}
