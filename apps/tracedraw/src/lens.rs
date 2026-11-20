//! Lens docker state.

use tracedraw_core::{live::Lens, Color};

#[derive(Debug, Clone, PartialEq)]
pub struct LensSettings {
    pub kind: usize,
    pub rate: f64,
    pub color: Color,
    pub color_to: Color,
    pub amount: f64,
    pub frozen: bool,
    pub remove_face: bool,
    /// Bitmap Effects lens: the effects, in order.
    pub effects: Vec<tracedraw_core::BitmapEffect>,
    /// The Cut lens's spurs: length, thickness and how hollow (mm, mm,
    /// 1 to 6), and the width of the cut (mm).
    pub spur_length: f64,
    pub spur_width: f64,
    pub curve: f64,
    pub gap: f64,
    /// The Cut lens's shadow falls on its upper side.
    pub shadow_up: bool,
}

impl Default for LensSettings {
    fn default() -> Self {
        LensSettings {
            kind: 0,
            rate: 50.0,
            color: Color::WHITE,
            color_to: Color::BLACK,
            amount: 2.0,
            frozen: false,
            remove_face: false,
            effects: Vec::new(),
            spur_length: 3.0,
            spur_width: 3.0,
            curve: 2.0,
            gap: 0.3,
            shadow_up: false,
        }
    }
}

pub const LENS_KEYS: [&str; 14] = [
    "lens.none",
    "lens.brighten",
    "lens.color_add",
    "lens.color_limit",
    "lens.custom_color_map",
    "lens.fish_eye",
    "lens.heat_map",
    "lens.invert",
    "lens.magnify",
    "lens.tinted_grayscale",
    "lens.transparency",
    "lens.wireframe",
    "lens.bitmap_effects",
    "lens.cut",
];

impl crate::app::App {
    /// Add bitmap effect `id` (default settings) to the selection's
    /// Bitmap Effects lens, making one when there is none.
    pub fn add_lens_effect(&mut self, id: &str) {
        use tracedraw_core::live::Effect;
        let current = self.selected_shapes().first().and_then(|s| {
            s.effects.iter().find_map(|e| match e {
                Effect::Lens(Lens::Effects { effects }) => Some(effects.clone()),
                _ => None,
            })
        });
        let mut effects = current.unwrap_or_default();
        effects.push(crate::fx::new_effect(id));
        self.push_effect(Effect::Lens(Lens::Effects { effects }), true);
    }
}

impl crate::app::App {
    /// Lens docker Apply with Frozen: each selected object with a lens
    /// keeps what the lens shows now, as a picture placed inside it (a
    /// clip frame), and loses the lens, so it can be moved and keeps the
    /// look. Flattening the lens instead dropped it, and the object went
    /// back to its own fill (#3097). The picture is what lies beneath
    /// seen through the lens, without what lies above the object.
    pub fn freeze_lens(&mut self) {
        use tracedraw_core::live::Effect;
        use tracedraw_core::{Command, Fill, Shape, ShapeKind};
        const DPI: f64 = 300.0;
        let page = self.page;
        let layers: Vec<_> = self.page_layers().iter().map(|l| l.id).collect();
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            let doc = self.doc();
            // Objects on a layer only: one inside a group keeps its lens.
            let Ok((lid, i)) = doc.locate(id) else {
                continue;
            };
            let Some(s) = doc.find_shape(id).cloned() else {
                continue;
            };
            if !s.effects.iter().any(|e| matches!(e, Effect::Lens(_))) {
                continue;
            }
            let Some(li) = layers.iter().position(|l| *l == lid) else {
                continue;
            };
            // The page up to the object: the layers above it and the
            // objects above it on its layer left out. Its outline stays
            // on the frame, so the picture leaves it out.
            let mut tmp = doc.clone();
            for (k, l) in layers.iter().enumerate() {
                let Ok(layer) = tmp.layer_mut(*l) else {
                    continue;
                };
                if k > li {
                    layer.shapes.clear();
                } else if k == li {
                    layer.shapes.truncate(i + 1);
                    if let Some(me) = layer.shapes.last_mut() {
                        me.stroke = None;
                        me.extra_strokes.clear();
                    }
                }
            }
            let area = s.bounds();
            let Some(pm) = tracedraw_render::render_area_image(&tmp, page, area, DPI) else {
                continue;
            };
            let Ok(png) = pm.encode_png() else {
                continue;
            };
            let pic = self.engine.new_shape_id();
            let mut bitmap = Shape::new(
                pic,
                ShapeKind::Bitmap {
                    rect: area,
                    width_px: pm.width(),
                    height_px: pm.height(),
                    png: png.into(),
                    fx: None,
                },
            );
            bitmap.fill = Fill::None;
            bitmap.stroke = None;
            let effects: Vec<Effect> = s
                .effects
                .iter()
                .filter(|e| !matches!(e, Effect::Lens(_)))
                .cloned()
                .collect();
            cmds.extend([
                Command::SetEffectStack {
                    shape: id,
                    effects,
                    parked: s.parked_effects.clone(),
                },
                Command::SetFill {
                    shapes: vec![id],
                    fill: Fill::None,
                },
                Command::AddShape {
                    layer: lid,
                    shape: bitmap,
                },
                Command::PlaceInside {
                    contents: vec![pic],
                    frame: id,
                },
            ]);
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Freeze Lens", &cmds) {
            self.status = e.to_string();
        }
    }
}

impl LensSettings {
    pub fn to_lens(&self) -> Option<Lens> {
        Some(match self.kind {
            1 => Lens::Brighten { rate: self.rate },
            2 => Lens::ColorAdd {
                color: self.color,
                rate: self.rate,
            },
            3 => Lens::ColorLimit {
                color: self.color,
                rate: self.rate,
            },
            4 => Lens::CustomColorMap {
                from: self.color_to,
                to: self.color,
                direction: 0,
            },
            5 => Lens::FishEye {
                rate: self.rate * 10.0,
            },
            6 => Lens::HeatMap {
                rotation: self.rate,
            },
            7 => Lens::Invert,
            8 => Lens::Magnify {
                amount: self.amount,
            },
            9 => Lens::TintedGrayscale { color: self.color },
            // The panel's Rate is how see-through the lens is, as in other
            // drawing programs (#2800); the lens keeps the colour's share.
            10 => Lens::Transparency {
                rate: 100.0 - self.rate.clamp(0.0, 100.0),
                color: self.color,
            },
            11 => Lens::Wireframe {
                outline: self.color_to,
                fill: self.color,
            },
            12 => Lens::Effects {
                effects: self.effects.clone(),
            },
            // Cut: Rate is how dark the shadow starts, Color its colour.
            13 => Lens::Cut {
                spur_length: self.spur_length,
                spur_width: self.spur_width,
                curve: self.curve,
                gap: self.gap,
                shadow: self.rate.clamp(0.0, 100.0),
                shadow_color: self.color,
                shadow_up: self.shadow_up,
            },
            _ => return None,
        })
    }

    pub fn from_lens(l: &Lens) -> Self {
        let mut s = LensSettings::default();
        match l {
            Lens::Brighten { rate } => {
                s.kind = 1;
                s.rate = *rate;
            }
            Lens::ColorAdd { color, rate } => {
                s.kind = 2;
                s.color = *color;
                s.rate = *rate;
            }
            Lens::ColorLimit { color, rate } => {
                s.kind = 3;
                s.color = *color;
                s.rate = *rate;
            }
            Lens::CustomColorMap { from, to, .. } => {
                s.kind = 4;
                s.color_to = *from;
                s.color = *to;
            }
            Lens::FishEye { rate } => {
                s.kind = 5;
                s.rate = rate / 10.0;
            }
            Lens::HeatMap { rotation } => {
                s.kind = 6;
                s.rate = *rotation;
            }
            Lens::Invert => s.kind = 7,
            Lens::Magnify { amount } => {
                s.kind = 8;
                s.amount = *amount;
            }
            Lens::TintedGrayscale { color } => {
                s.kind = 9;
                s.color = *color;
            }
            Lens::Transparency { rate, color } => {
                s.kind = 10;
                s.rate = 100.0 - rate.clamp(0.0, 100.0);
                s.color = *color;
            }
            Lens::Wireframe { outline, fill } => {
                s.kind = 11;
                s.color = *fill;
                s.color_to = *outline;
            }
            Lens::Effects { effects } => {
                s.kind = 12;
                s.effects = effects.clone();
            }
            Lens::Cut {
                spur_length,
                spur_width,
                curve,
                gap,
                shadow,
                shadow_color,
                shadow_up,
            } => {
                s.kind = 13;
                s.spur_length = *spur_length;
                s.spur_width = *spur_width;
                s.curve = *curve;
                s.gap = *gap;
                s.rate = *shadow;
                s.color = *shadow_color;
                s.shadow_up = *shadow_up;
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::live::Effect;

    /// #2800: the panel's Transparency rate is how see-through the lens
    /// is, Wireframe takes its outline colour, and both come back.
    #[test]
    fn transparency_rate_is_see_through_and_wireframe_keeps_its_outline() {
        let s = LensSettings {
            kind: 10,
            rate: 80.0,
            color: Color::rgb8(255, 0, 0),
            ..Default::default()
        };
        let lens = s.to_lens().expect("a lens");
        // 80 % see-through: white beneath stays mostly white.
        let c = lens.map_color([1.0, 1.0, 1.0]);
        assert!((c[1] - 0.8).abs() < 1e-4, "{c:?}");
        assert_eq!(LensSettings::from_lens(&lens), s);
        let s = LensSettings {
            kind: 11,
            color: Color::rgb8(255, 255, 0),
            color_to: Color::rgb8(0, 0, 255),
            ..Default::default()
        };
        let lens = s.to_lens().expect("a lens");
        assert_eq!(
            lens,
            Lens::Wireframe {
                outline: Color::rgb8(0, 0, 255),
                fill: Color::rgb8(255, 255, 0),
            }
        );
        assert_eq!(LensSettings::from_lens(&lens), s);
    }

    /// #3097: Apply with Frozen keeps what the lens shows, also once the
    /// object is moved off what was beneath it.
    #[test]
    fn a_frozen_lens_keeps_what_it_showed() {
        use tracedraw_core::{Color, Command, Fill, Rect, Shape, ShapeKind};
        let mut app = crate::app::App::headless();
        let layer = app.active_layer().expect("a layer");
        let rect = |app: &mut crate::app::App, r: Rect, fill: Color| {
            let id = app.engine.new_shape_id();
            let mut s = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: r,
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = Fill::Solid(fill);
            s.stroke = None;
            app.run(Command::AddShape { layer, shape: s });
            id
        };
        rect(
            &mut app,
            Rect::new(0.0, 0.0, 40.0, 40.0),
            Color::rgb8(0, 0, 255),
        );
        let lens = rect(
            &mut app,
            Rect::new(10.0, 10.0, 30.0, 30.0),
            Color::rgb8(0, 255, 0),
        );
        app.select(vec![lens]);
        app.push_effect(Effect::Lens(Lens::Invert), true);
        let px = |app: &crate::app::App, x: f64, y: f64| {
            let pm = tracedraw_render::render_area_image(
                app.doc(),
                app.page,
                Rect::new(x - 0.5, y - 0.5, x + 0.5, y + 0.5),
                25.4,
            )
            .expect("a picture");
            let c = pm.pixels()[0].demultiply();
            (c.red(), c.green(), c.blue())
        };
        let live = px(&app, 20.0, 20.0);
        assert!(live.0 > 200 && live.1 > 200 && live.2 < 50, "{live:?}");
        app.freeze_lens();
        let s = app.doc().find_shape(lens).expect("the object").clone();
        assert!(
            matches!(s.kind, ShapeKind::ClipFrame { .. }),
            "{:?}",
            s.kind
        );
        assert!(!s.effects.iter().any(|e| matches!(e, Effect::Lens(_))));
        assert_eq!(px(&app, 20.0, 20.0), live);
        // Moved off the blue square: the frozen look goes with it.
        app.run(Command::TransformShapes {
            shapes: vec![lens],
            transform: tracedraw_core::geometry::Affine::translate((60.0, 0.0)),
        });
        assert_eq!(px(&app, 80.0, 20.0), live);
    }

    #[test]
    fn effects_lens_settings_round_trip_and_effects_are_added_to_the_lens() {
        let mut s = LensSettings {
            kind: 12,
            ..Default::default()
        };
        s.effects.push(crate::fx::new_effect("invert_colors"));
        let lens = s.to_lens().expect("a lens");
        assert_eq!(LensSettings::from_lens(&lens), s);
        let mut app = crate::app::App::headless();
        let layer = app.active_layer().expect("a layer");
        let id = app.engine.new_shape_id();
        let shape = tracedraw_core::Shape::new(
            id,
            tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            },
        );
        app.run(tracedraw_core::Command::AddShape { layer, shape });
        app.select(vec![id]);
        app.add_lens_effect("gaussian_blur");
        app.add_lens_effect("invert_colors");
        let effects = app
            .doc()
            .find_shape(id)
            .map(|s| s.effects.clone())
            .unwrap_or_default();
        assert_eq!(effects.len(), 1, "one lens");
        match &effects[0] {
            Effect::Lens(Lens::Effects { effects }) => {
                let ids: Vec<&str> = effects.iter().map(|e| e.id.as_str()).collect();
                assert_eq!(ids, ["gaussian_blur", "invert_colors"]);
            }
            e => panic!("{e:?}"),
        }
        // The application's effects over raw pixels: invert.
        let mut px = vec![10, 20, 30, 255];
        crate::fx::lens_effects(&[crate::fx::new_effect("invert_colors")], 1, 1, &mut px);
        assert_eq!(px, vec![245, 235, 225, 255]);
    }
}
