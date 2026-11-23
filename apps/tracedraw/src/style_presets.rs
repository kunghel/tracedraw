//! Style presets (#2160): the look of an object (fill, outline,
//! transparency, drop shadow and the effects that do not depend on its
//! shape: contour, bevel, block shadow, inner shadow, lens and
//! transparency) saved under a name in the settings, so it can be given
//! to any object of any drawing later. The Effects docker lists them,
//! each with a picture of the selected object wearing it (the live
//! preview), and applies one to the selection as one undo step. See
//! docs/behavior/effects-docker.md, Presets.

use serde::{Deserialize, Serialize};
use tracedraw_core::live::Effect;
use tracedraw_core::{
    Affine, Command, Fill, Rect, Shadow, Shape, ShapeId, ShapeKind, Size, Stroke,
};

/// At most this many presets are kept.
pub const MAX_PRESETS: usize = 200;

/// A saved look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylePreset {
    pub name: String,
    /// A number of its own, so its picture is kept while the list
    /// changes around it.
    #[serde(default)]
    pub uid: u64,
    pub fill: Fill,
    #[serde(default)]
    pub stroke: Option<Stroke>,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default)]
    pub shadow: Option<Shadow>,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

fn one() -> f64 {
    1.0
}

/// Effects that make sense on any object: the others (blends, envelopes,
/// perspective, extrusion, distortion, symmetry) hold points of the
/// object they were made on and stay with it.
pub fn portable(e: &Effect) -> bool {
    matches!(
        e,
        Effect::Contour { .. }
            | Effect::Bevel { .. }
            | Effect::BlockShadow { .. }
            | Effect::InnerShadow { .. }
            | Effect::OuterShadow { .. }
            | Effect::Glow { .. }
            | Effect::ColorOverlay { .. }
            | Effect::GradientOverlay { .. }
            | Effect::GaussianBlur { .. }
            | Effect::Halftone { .. }
            | Effect::FillOpacity { .. }
            | Effect::BlendIf { .. }
            | Effect::Lens(_)
            | Effect::Transparency { .. }
    )
}

impl StylePreset {
    /// The look of `s` under `name`.
    pub fn from_shape(name: &str, s: &Shape, uid: u64) -> Self {
        StylePreset {
            name: name.to_string(),
            uid,
            fill: s.fill.clone(),
            stroke: s.stroke.clone(),
            opacity: s.opacity,
            shadow: s.shadow,
            effects: s.effects.iter().filter(|e| portable(e)).cloned().collect(),
        }
    }

    /// The effects `s` gets: its own that stay with it, in their order,
    /// then the preset's.
    pub fn effects_for(&self, s: &Shape) -> Vec<Effect> {
        s.effects
            .iter()
            .filter(|e| !portable(e))
            .chain(self.effects.iter())
            .cloned()
            .collect()
    }

    /// `s` wearing the preset.
    pub fn dressed(&self, s: &Shape) -> Shape {
        let mut c = s.clone();
        c.effects = self.effects_for(s);
        c.fill = self.fill.clone();
        c.stroke = self.stroke.clone();
        c.opacity = self.opacity;
        c.shadow = self.shadow;
        c
    }

    /// The commands that give `shapes` the preset; empty for none.
    pub fn commands(&self, shapes: &[Shape]) -> Vec<Command> {
        if shapes.is_empty() {
            return Vec::new();
        }
        let ids: Vec<ShapeId> = shapes.iter().map(|s| s.id).collect();
        let mut cmds = vec![
            Command::SetFill {
                shapes: ids.clone(),
                fill: self.fill.clone(),
            },
            Command::SetStroke {
                shapes: ids.clone(),
                stroke: self.stroke.clone(),
            },
            Command::SetOpacity {
                shapes: ids.clone(),
                opacity: self.opacity,
            },
            Command::SetShadow {
                shapes: ids,
                shadow: self.shadow,
            },
        ];
        for s in shapes {
            let effects = self.effects_for(s);
            if effects != s.effects {
                cmds.push(Command::SetEffects {
                    shape: s.id,
                    effects,
                });
            }
        }
        cmds
    }
}

/// The name a new preset is offered: "`base` n" with the first n free.
pub fn free_name(presets: &[StylePreset], base: &str) -> String {
    (1..)
        .map(|n| format!("{base} {n}"))
        .find(|name| presets.iter().all(|p| p.name != *name))
        .unwrap_or_else(|| base.to_string())
}

/// A number no preset in `presets` has.
pub fn free_uid(presets: &[StylePreset]) -> u64 {
    presets.iter().map(|p| p.uid).max().unwrap_or(0) + 1
}

/// The object the pictures show a preset on when nothing (or a group) is
/// selected: a rounded rectangle.
pub fn sample_shape() -> Shape {
    Shape::new(
        ShapeId(1),
        ShapeKind::Rect {
            rect: Rect::new(0.0, 0.0, 30.0, 20.0),
            radius: 3.0,
            corners: None,
        },
    )
}

/// A picture of `shape` wearing `preset`, `px` pixels on its long side
/// at most, on a transparent background (premultiplied RGBA), with its
/// pixel size.
pub fn picture(preset: &StylePreset, shape: &Shape, px: u32) -> Option<(Vec<u8>, u32, u32)> {
    let s = preset.dressed(shape);
    let mut b = s.visual_bounds();
    if let Some(sh) = &s.shadow {
        let grow = sh.blur.abs();
        b = b.union(Rect::new(
            b.x0 + sh.offset.x - grow,
            b.y0 + sh.offset.y - grow,
            b.x1 + sh.offset.x + grow,
            b.y1 + sh.offset.y + grow,
        ));
    }
    let pad = (b.width().max(b.height()) * 0.06).max(0.3);
    let b = b.inflate(pad, pad);
    let longest = b.width().max(b.height());
    if !(longest.is_finite() && longest > 0.0) {
        return None;
    }
    let px = px.clamp(8, 512) as f64;
    let zoom = px / longest;
    let w = (b.width() * zoom).ceil().clamp(1.0, px) as u32;
    let h = (b.height() * zoom).ceil().clamp(1.0, px) as u32;
    let mut tmp = tracedraw_core::Document::new("preset", Size::new(b.width(), b.height()));
    let page = tmp.pages.first()?.id;
    let layer = tmp.pages.first()?.layers.first()?.id;
    let mut c = s;
    c.transform_by(Affine::translate((-b.x0, -b.y0)));
    tmp.layer_mut(layer).ok()?.shapes.push(c);
    let pm = tracedraw_render::render_page(
        &tmp,
        page,
        &tracedraw_render::RenderOptions {
            width: w,
            height: h,
            view: tracedraw_render::ViewTransform {
                zoom,
                origin_x: 0.0,
                origin_y: h as f64,
                stretch_y: 1.0,
            },
            preview: None,
            wireframe: false,
            anti_alias: true,
            draft: true,
            focus: None,
            ..tracedraw_render::RenderOptions::default()
        },
    )?;
    Some((pm.data().to_vec(), w, h))
}

impl crate::app::App {
    /// Save the look of the first selected object as a preset named
    /// `name`; false when nothing is selected or the list is full.
    pub fn save_style_preset(&mut self, name: &str) -> bool {
        let Some(s) = self.selected_shapes().into_iter().next() else {
            return false;
        };
        let list = &mut self.settings.style_presets;
        if list.len() >= MAX_PRESETS {
            return false;
        }
        let name = name.trim();
        let name = if name.is_empty() {
            free_name(list, &crate::i18n::tr("effects.preset_default_name"))
        } else {
            name.to_string()
        };
        let uid = free_uid(list);
        list.push(StylePreset::from_shape(&name, &s, uid));
        self.settings.save();
        true
    }

    /// Give the selection preset `index`, as one undo step.
    pub fn apply_style_preset(&mut self, index: usize) {
        let Some(p) = self.settings.style_presets.get(index).cloned() else {
            return;
        };
        self.apply_preset(&p);
    }

    /// Give the selection the look `p` (a saved or a built-in preset), as
    /// one undo step.
    pub fn apply_preset(&mut self, p: &StylePreset) {
        let shapes = self.selected_shapes();
        let cmds = p.commands(&shapes);
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Apply Preset", &cmds) {
            self.status = e.to_string();
        }
    }

    /// Remove preset `index`.
    pub fn delete_style_preset(&mut self, index: usize) {
        if index < self.settings.style_presets.len() {
            self.settings.style_presets.remove(index);
            self.settings.save();
        }
    }

    /// The picture of preset `index` on the first selected object (the
    /// sample shape when nothing or a group is selected), `px` pixels on
    /// its long side; drawn again only when the drawing, the selection
    /// or the preset changes.
    pub fn style_preset_picture(
        &self,
        ctx: &egui::Context,
        index: usize,
        px: u32,
    ) -> Option<egui::TextureHandle> {
        let p = self.settings.style_presets.get(index)?;
        self.preset_picture(ctx, p, px)
    }

    /// The picture of `p` on the first selected object (the sample shape
    /// when nothing or a group is selected), as for a saved preset.
    pub fn preset_picture(
        &self,
        ctx: &egui::Context,
        p: &StylePreset,
        px: u32,
    ) -> Option<egui::TextureHandle> {
        let first = self
            .selected_shapes()
            .into_iter()
            .next()
            .filter(|s| !matches!(s.kind, ShapeKind::Group { .. }));
        let key = (
            p.uid,
            first.as_ref().map(|s| s.id),
            self.engine.revision(),
            px,
        );
        let id = egui::Id::new(("style_preset_picture", p.uid, px));
        if let Some((k, t)) =
            ctx.data(|d| d.get_temp::<((u64, Option<ShapeId>, u64, u32), egui::TextureHandle)>(id))
        {
            if k == key {
                return Some(t);
            }
        }
        let shape = first.unwrap_or_else(sample_shape);
        let (rgba, w, h) = picture(p, &shape, px)?;
        let image =
            egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], rgba.as_slice());
        let t = ctx.load_texture(
            format!("style_preset:{}", p.uid),
            image,
            egui::TextureOptions::LINEAR,
        );
        ctx.data_mut(|d| d.insert_temp(id, (key, t.clone())));
        Some(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Point, Vec2};
    use tracedraw_core::Color;

    fn shadow() -> Shadow {
        Shadow {
            offset: Vec2::new(1.5, -1.5),
            opacity: 0.6,
            blur: 1.0,
            color: Color::BLACK,
            ..Default::default()
        }
    }

    #[test]
    fn a_preset_keeps_the_look_and_leaves_the_shape_bound_effects() {
        let mut s = sample_shape();
        s.fill = Fill::Solid(Color::rgb8(200, 30, 30));
        s.opacity = 0.5;
        s.shadow = Some(shadow());
        let block = Effect::BlockShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::BLACK,
            gap: 0.0,
        };
        let persp = Effect::Perspective {
            corners: [Point::ZERO; 4],
        };
        s.effects = vec![persp.clone(), block.clone()];
        let p = StylePreset::from_shape("Red", &s, 7);
        assert_eq!(p.effects, vec![block.clone()]);
        assert_eq!(p.opacity, 0.5);
        // Another object keeps its own perspective and gets the block
        // shadow after it; its block shadow is replaced, not doubled.
        let mut t = sample_shape();
        t.effects = vec![
            Effect::BlockShadow {
                offset: Vec2::new(5.0, 5.0),
                color: Color::WHITE,
                gap: 1.0,
            },
            persp.clone(),
        ];
        assert_eq!(p.effects_for(&t), vec![persp, block]);
        let d = p.dressed(&t);
        assert_eq!(d.fill, s.fill);
        assert_eq!(d.shadow, s.shadow);
        // Settings round trip.
        let json = serde_json::to_string(&p).expect("json");
        let back: StylePreset = serde_json::from_str(&json).expect("back");
        assert_eq!(back, p);
        let old: StylePreset =
            serde_json::from_str(r#"{"name":"x","fill":{"fill":"none"}}"#).expect("older");
        assert_eq!(old.opacity, 1.0);
        assert!(old.effects.is_empty());
    }

    #[test]
    fn names_and_numbers_are_free() {
        let s = sample_shape();
        let a = StylePreset::from_shape("Preset 1", &s, 4);
        let list = vec![a];
        assert_eq!(free_name(&list, "Preset"), "Preset 2");
        assert_eq!(free_uid(&list), 5);
        assert_eq!(free_uid(&[]), 1);
    }

    #[test]
    fn the_picture_shows_the_preset() {
        let mut s = sample_shape();
        s.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        let p = StylePreset::from_shape("Blue", &s, 1);
        let (rgba, w, h) = picture(&p, &sample_shape(), 48).expect("picture");
        assert!(w <= 48 && h <= 48 && w > 0 && h > 0);
        // The middle pixel is the blue fill.
        let o = (((h / 2) * w + w / 2) * 4) as usize;
        assert!(rgba[o + 2] > 200 && rgba[o] < 40, "{:?}", &rgba[o..o + 4]);
    }

    #[test]
    fn presets_apply_to_the_selection_as_one_step_and_undo() {
        let mut app = crate::app::App::headless();
        let a = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("a");
        let b = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(20.0, 0.0, 30.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("b");
        app.select(vec![a]);
        app.apply_fill(Fill::Solid(Color::rgb8(10, 200, 10)));
        app.run(Command::SetShadow {
            shapes: vec![a],
            shadow: Some(shadow()),
        });
        let before = app.settings.style_presets.len();
        assert!(app.save_style_preset("Green"));
        let i = app.settings.style_presets.len() - 1;
        assert_eq!(app.settings.style_presets.len(), before + 1);
        let b_fill = app.doc().find_shape(b).expect("b").fill.clone();
        app.select(vec![b]);
        app.apply_style_preset(i);
        let shape_b = app.doc().find_shape(b).expect("b").clone();
        assert_eq!(shape_b.fill, Fill::Solid(Color::rgb8(10, 200, 10)));
        assert_eq!(shape_b.shadow, Some(shadow()));
        app.undo();
        let shape_b = app.doc().find_shape(b).expect("b").clone();
        assert_eq!(shape_b.fill, b_fill);
        assert_eq!(shape_b.shadow, None);
        // The docker draws the preset with its picture.
        let ctx = crate::theme::ui_context();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            crate::ui::effects_docker::effects_docker(&mut app, ui)
        });
        out.textures_delta.clear();
        assert!(app.style_preset_picture(&ctx, i, 32).is_some());
        app.delete_style_preset(i);
        assert_eq!(app.settings.style_presets.len(), before);
        // Nothing selected: nothing saved.
        app.select(vec![]);
        assert!(!app.save_style_preset("none"));
    }
}
