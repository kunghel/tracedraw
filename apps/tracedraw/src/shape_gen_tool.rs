//! The Shape Generator tool: drag a box on the page and the panel's
//! pattern is drawn in it, following the pointer while the box is
//! dragged. The pattern and its options are those of the Shape Generator
//! panel, which Options on the tool's property bar opens.

use crate::app::App;
use crate::shape_gen::{generate, GenKind, Generated, ShapeGenSettings};
use tracedraw_core::geometry::{Affine, BezPath, Point, Rect, Shape as _};

/// The most curves the tool's preview draws while the box is dragged
/// (releasing the button makes them all).
const PREVIEW_CURVES: usize = 6000;

/// A generation slower than this (seconds) is not run again on every
/// pointer move: between runs the last drawing is stretched to the box.
const SLOW: f64 = 0.03;

/// The least time (seconds) between two runs of a slow generator while
/// the box is dragged.
const SLOW_EVERY: f64 = 0.2;

/// The preview kept between frames of one drag.
#[derive(Debug, Clone)]
pub struct ToolPreview {
    /// What the curves were made from (with the box's size).
    settings: ShapeGenSettings,
    /// The curves as one path, the box's bottom left at the origin.
    path: BezPath,
    /// The box the curves belong to, in the path's space.
    frame: Rect,
    /// How long the generation took, seconds.
    cost: f64,
    made: web_time::Instant,
}

/// The settings the tool draws a `w` x `h` mm box with: the panel's, the
/// box's size, and no picked selection (the pattern fills the dragged
/// box, not the outline picked in the panel). Rings and rosettes, whose
/// radii are in mm, are sized to the box.
pub fn tool_settings(panel: &ShapeGenSettings, w: f64, h: f64) -> ShapeGenSettings {
    let mut s = panel.clone();
    if s.kind.uses_box() {
        s.width = w;
        s.height = h;
    }
    s.boundary = None;
    s.source_center = None;
    s.vor_points.clear();
    let mut s = s.sanitized();
    if s.kind == GenKind::Rings {
        // Rings have their radii in mm: the outer ring and its waves
        // are brought to the box's shorter side.
        let reach = s.min_radius.max(s.max_radius) + s.ring_amplitude.abs();
        if reach > 1e-9 {
            let k = w.min(h) / 2.0 / reach;
            s.min_radius *= k;
            s.max_radius *= k;
            s.ring_amplitude *= k;
        }
    }
    if s.kind == GenKind::Guilloche {
        // A rosette's radii are in mm too.
        let reach = s.guil_inner.max(s.guil_outer) + s.guil_amp.abs() + s.guil_amp2.abs();
        if reach > 1e-9 {
            let k = w.min(h) / 2.0 / reach;
            s.guil_inner *= k;
            s.guil_outer *= k;
            s.guil_amp *= k;
            s.guil_amp2 *= k;
        }
    }
    s
}

/// The box a generation belongs to: its frame, else its curves' bounds.
fn anchor(g: &Generated) -> Option<Rect> {
    g.frame.or_else(|| g.bounds())
}

/// Every curve of `g` (the first [`PREVIEW_CURVES`]) as one path.
fn merged(g: &Generated) -> BezPath {
    let mut out = BezPath::new();
    for item in g.items.iter().take(PREVIEW_CURVES) {
        out.extend(item.path.elements().iter().copied());
    }
    out
}

impl App {
    /// What a drag of the Shape Generator tool over `rect` (page mm)
    /// would draw, as outlines in page space. A fast generator is run
    /// for every size the box takes; a slow one a few times a second,
    /// its last drawing stretched to the box in between.
    pub fn shape_gen_tool_preview(&self, rect: Rect) -> BezPath {
        if rect.width() < 0.05 || rect.height() < 0.05 {
            return BezPath::new();
        }
        let s = tool_settings(&self.shape_gen, rect.width(), rect.height());
        let mut cache = self.shape_gen_tool_cache.borrow_mut();
        let fresh = match cache.as_ref() {
            Some(c) if c.settings == s => true,
            Some(c) => {
                // The same pattern in another size: a slow one waits.
                let mut sized = c.settings.clone();
                sized.width = s.width;
                sized.height = s.height;
                sized == s && c.cost > SLOW && c.made.elapsed().as_secs_f64() < SLOW_EVERY
            }
            None => false,
        };
        if !fresh {
            let t = web_time::Instant::now();
            let g = generate(&s);
            let frame = anchor(&g).unwrap_or(Rect::new(0.0, 0.0, s.width, s.height));
            *cache = Some(ToolPreview {
                path: merged(&g),
                frame,
                settings: s.clone(),
                cost: t.elapsed().as_secs_f64(),
                made: web_time::Instant::now(),
            });
        }
        let Some(c) = cache.as_ref() else {
            return BezPath::new();
        };
        let f = c.frame;
        if !s.kind.uses_box() || f.width() < 1e-9 || f.height() < 1e-9 {
            // Its own size: centred in the box.
            return Affine::translate(rect.center() - f.center()) * c.path.clone();
        }
        // The drawing's box onto the dragged one (the same size, or a
        // slow drawing stretched until its next run).
        let (kx, ky) = if c.settings == s {
            (1.0, 1.0)
        } else {
            (
                s.width / c.settings.width.max(1e-9),
                s.height / c.settings.height.max(1e-9),
            )
        };
        let fit = Affine::translate(rect.center().to_vec2())
            * Affine::scale_non_uniform(kx, ky)
            * Affine::translate(-f.center().to_vec2());
        fit * c.path.clone()
    }

    /// The Shape Generator tool's box drag released over `rect`: the
    /// panel's pattern drawn to fill it, as one group (one undo step),
    /// selected. The panel takes the box's size, so Update and the
    /// panel's preview go on from what was drawn.
    pub fn create_shape_gen_in(&mut self, rect: Rect) -> Option<tracedraw_core::ShapeId> {
        let s = tool_settings(&self.shape_gen, rect.width(), rect.height());
        let g = generate(&s);
        self.shape_gen_tool_cache.borrow_mut().take();
        let id = self.shape_gen_add(&s, &g, rect.center(), None, "Shape Generator")?;
        if s.kind.uses_box() && self.shape_gen.source_center.is_none() {
            self.shape_gen.width = s.width;
            self.shape_gen.height = s.height;
        }
        Some(id)
    }

    /// Another seed for the tool's pattern (the property bar's New).
    pub fn shape_gen_new_seed(&mut self) {
        let s = &mut self.shape_gen;
        s.seed = (s.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 8) % 1_000_000;
    }
}

/// The tool's property bar: the generator, its seed and New, and Options
/// (the panel with every setting).
pub fn shape_gen_bar(app: &mut App, ui: &mut egui::Ui) {
    use crate::i18n::tr;
    use crate::ui::tips::{keyed, RichTip};
    crate::ui::toolbar::dim_labelled(ui, "shapegen.kind", |ui| {
        let kind = &mut app.shape_gen.kind;
        crate::ui::keynav::ComboBox::from_id_salt("shape_gen_tool_kind")
            .selected_text(tr(kind.key()))
            .width(170.0)
            .height(420.0)
            .show_ui(ui, |ui| {
                for k in GenKind::ALL {
                    ui.selectable_value(kind, k, tr(k.key()));
                }
            })
            .response
    });
    crate::ui::toolbar::dim_labelled(ui, "shapegen.seed", |ui| {
        ui.add(crate::ui::field::NumField::new(&mut app.shape_gen.seed).range(0..=999_999))
    });
    if ui
        .button(tr("shapegen.new_seed"))
        .rich_tip(keyed("shapegen.new_seed"))
        .clicked()
    {
        app.shape_gen_new_seed();
    }
    if ui
        .button(tr("toolbar.shape_gen_options"))
        .rich_tip(keyed("toolbar.shape_gen_options"))
        .clicked()
    {
        app.open_docker(crate::app::DockerTab::ShapeGenerator);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;

    fn bounds(p: &BezPath) -> Rect {
        p.bounding_box()
    }

    /// Dragging the tool draws the panel's pattern in the dragged box:
    /// one group in one undo step, selected, carrying its settings, and
    /// the panel takes the box's size.
    #[test]
    fn the_tool_draws_the_pattern_in_the_dragged_box() {
        let mut app = App::headless();
        app.tool = Tool::ShapeGenerator;
        app.shape_gen.kind = GenKind::Truchet;
        let (a, b) = (Point::new(20.0, 30.0), Point::new(100.0, 80.0));
        let depth = app.engine.history_labels().0.len();
        app.create_box_shape(a, b);
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        assert_eq!(app.selection.len(), 1);
        let made = app.selection_bounds().expect("bounds");
        let want = Rect::from_points(a, b);
        assert!((made.center() - want.center()).hypot() < 1.0, "{made:?}");
        assert!(made.width() <= want.width() + 1.0 && made.width() > want.width() * 0.8);
        assert!(made.height() <= want.height() + 1.0 && made.height() > want.height() * 0.8);
        let (_, saved) = app.shape_gen_selected().expect("generated");
        assert_eq!(saved.kind, GenKind::Truchet);
        assert!((saved.width - 80.0).abs() < 1e-9 && (saved.height - 50.0).abs() < 1e-9);
        assert!((app.shape_gen.width - 80.0).abs() < 1e-9);
        app.undo();
        assert!(app.selection_bounds().is_none());
    }

    /// While the box is dragged every generator shows its drawing inside
    /// the box (the cone template, which has its own size, centred on
    /// it), and what the drag makes is where the preview showed it.
    #[test]
    fn the_preview_of_every_generator_follows_the_box() {
        let mut app = App::headless();
        app.tool = Tool::ShapeGenerator;
        let (a, b) = (Point::new(10.0, 20.0), Point::new(70.0, 60.0));
        let rect = Rect::from_points(a, b);
        for kind in GenKind::ALL {
            app.shape_gen.kind = kind;
            let p = app.creation_preview(a, b, false).expect("a preview");
            if p.elements().is_empty() {
                continue;
            }
            let r = bounds(&p);
            assert!(
                (r.center() - rect.center()).hypot() < rect.width() * 0.5,
                "{kind:?}: {r:?}"
            );
            if kind.uses_box() {
                // Waves and warps may swing past the box's edge.
                let grown = rect.inflate(rect.width() * 0.5, rect.height() * 0.5);
                assert!(grown.union(r) == grown, "{kind:?}: {r:?} outside {rect:?}");
            }
        }
        // A picked outline in the panel does not move the tool's drawing.
        app.shape_gen.kind = GenKind::Voronoi;
        app.shape_gen.source_center = Some([500.0, 500.0]);
        let r = bounds(&app.creation_preview(a, b, false).expect("voronoi"));
        assert!((r.center() - rect.center()).hypot() < 10.0, "{r:?}");
        app.create_box_shape(a, b);
        let made = app.selection_bounds().expect("made");
        assert!((made.center() - rect.center()).hypot() < 10.0, "{made:?}");
        // The panel keeps its picked outline and its size.
        assert_eq!(app.shape_gen.source_center, Some([500.0, 500.0]));
    }
}
