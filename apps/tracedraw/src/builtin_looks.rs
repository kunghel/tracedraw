//! Built-in looks offered above the saved Presets of the Effects docker:
//! polished metals (chrome, liquid metal, gold) and glass (liquid glass
//! that blurs what lies beneath, clear glass). Each is an ordinary
//! [`StylePreset`] (a gradient fill, an outline and portable effects) made
//! here rather than read from the settings, so it can be applied to any
//! object as one undo step and cannot be deleted. See
//! docs/behavior/effects-docker.md, Built-in looks.

use crate::style_presets::StylePreset;
use tracedraw_core::geometry::Vec2;
use tracedraw_core::live::{Effect, Lens, MergeMode};
use tracedraw_core::style::{Fountain, FountainKind, FountainRepeat};
use tracedraw_core::{BitmapEffect, Color, Fill, Stop, Stroke};

/// The i18n keys of the built-in looks, in the order they are listed.
pub const KEYS: [&str; 5] = [
    "effects.look_chrome",
    "effects.look_liquid_metal",
    "effects.look_gold",
    "effects.look_liquid_glass",
    "effects.look_clear_glass",
];

fn hex(v: u32) -> Color {
    Color::rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A linear gradient from `stops` (position, 0xRRGGBB) at `angle` degrees.
fn linear(stops: &[(f64, u32)], angle: f64) -> Fountain {
    let mut f = Fountain::two(FountainKind::Linear, Color::BLACK, Color::WHITE, angle);
    f.stops = stops.iter().map(|(p, c)| Stop::new(*p, hex(*c))).collect();
    f
}

/// A bevel lit from the upper left: `style` 0 soft edge, 1 emboss.
fn bevel(distance: f64, intensity: f64, style: u8, level: u8, directional: bool) -> Effect {
    Effect::Bevel {
        distance,
        light_angle: 135.0,
        intensity,
        style,
        shadow_color: Color::BLACK,
        light_color: Color::WHITE,
        level,
        emboss_color: None,
        directional,
    }
}

fn glow(inner: bool, color: Color, opacity: f64, blur: f64) -> Effect {
    Effect::Glow {
        inner,
        color,
        opacity,
        blur,
        merge: MergeMode::Screen,
    }
}

fn soft_shadow(opacity: f64, blur: f64) -> Effect {
    Effect::OuterShadow {
        offset: Vec2::new(0.8, -1.2),
        color: Color::BLACK,
        opacity,
        blur,
        merge: MergeMode::Multiply,
    }
}

/// Look `i` of [`KEYS`] under the name `name`.
fn look(i: usize, name: String) -> StylePreset {
    // Numbers of their own, far from the saved presets' 1, 2, 3...
    let uid = u64::MAX - i as u64;
    let preset = |fill: Fill, stroke: Option<Stroke>, effects: Vec<Effect>| StylePreset {
        name: name.clone(),
        uid,
        fill,
        stroke,
        opacity: 1.0,
        shadow: None,
        effects,
    };
    match i {
        // Chrome: the sky over a dark horizon over the ground, mirrored
        // in a polished surface, with a crisp edge.
        0 => preset(
            Fill::Fountain(linear(
                &[
                    (0.0, 0x4A3A28),
                    (0.38, 0xD8C8AE),
                    (0.47, 0x1C2126),
                    (0.52, 0x56687A),
                    (0.78, 0xC4D8EA),
                    (1.0, 0xFFFFFF),
                ],
                90.0,
            )),
            Some(Stroke::new(hex(0x2A2F35), 0.2)),
            vec![bevel(0.8, 0.6, 1, 4, true)],
        ),
        // Liquid metal: soft silver waves running there and back, a deep
        // rounded bevel and a light glow inside the edge.
        1 => {
            let mut f = linear(
                &[
                    (0.0, 0x5F656D),
                    (0.3, 0xE9ECEF),
                    (0.5, 0x8E959D),
                    (0.75, 0xFFFFFF),
                    (1.0, 0xA9B0B8),
                ],
                70.0,
            );
            f.repeat = FountainRepeat::Mirror;
            f.repeats = 2;
            f.smooth = true;
            preset(
                Fill::Fountain(f),
                None,
                vec![
                    bevel(1.6, 0.8, 0, 10, true),
                    glow(true, Color::WHITE, 0.5, 1.2),
                    soft_shadow(0.35, 1.5),
                ],
            )
        }
        // Gold: warm bands of light and shade, an embossed edge.
        2 => preset(
            Fill::Fountain(linear(
                &[
                    (0.0, 0x6B4A0E),
                    (0.25, 0xD9AE4A),
                    (0.45, 0xFFF0B5),
                    (0.6, 0xC9952B),
                    (0.8, 0xF5D77A),
                    (1.0, 0x6B4A0E),
                ],
                80.0,
            )),
            Some(Stroke::new(hex(0x5A3D08), 0.2)),
            vec![bevel(1.0, 0.7, 1, 6, true)],
        ),
        // Liquid glass: a pale pane that blurs what lies beneath, light
        // caught inside its edge, a thin bright rim and a soft shadow.
        3 => preset(
            Fill::Solid(Color::WHITE),
            Some(Stroke::new(Color::WHITE, 0.25)),
            vec![
                Effect::Lens(Lens::Effects {
                    effects: vec![BitmapEffect {
                        id: "gaussian_blur".to_string(),
                        params: [("radius".to_string(), 6.0)].into_iter().collect(),
                        visible: true,
                    }],
                }),
                Effect::FillOpacity { opacity: 0.18 },
                glow(true, Color::WHITE, 0.7, 1.5),
                soft_shadow(0.25, 2.0),
            ],
        ),
        // Clear glass: the same pane without the blur.
        _ => preset(
            Fill::Solid(hex(0xEAF4FF)),
            Some(Stroke::new(Color::WHITE, 0.25)),
            vec![
                Effect::FillOpacity { opacity: 0.12 },
                glow(true, Color::WHITE, 0.8, 1.0),
                soft_shadow(0.2, 1.5),
            ],
        ),
    }
}

/// Every built-in look, named in the interface language.
pub fn looks() -> Vec<StylePreset> {
    KEYS.iter()
        .enumerate()
        .map(|(i, k)| look(i, crate::i18n::tr(k)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style_presets::{picture, portable, sample_shape};

    #[test]
    fn every_look_is_portable_and_has_its_own_number() {
        let all = looks();
        assert_eq!(all.len(), KEYS.len());
        for (i, p) in all.iter().enumerate() {
            assert!(p.effects.iter().all(portable), "{}", p.name);
            assert!(!p.effects.is_empty(), "{}", p.name);
            assert!(all[i + 1..].iter().all(|q| q.uid != p.uid));
            assert!(p.uid > 1_000_000);
        }
        // The metals are gradients, the glass a see-through pane.
        assert!(matches!(all[0].fill, Fill::Fountain(_)));
        assert!(all[3]
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Lens(Lens::Effects { .. }))));
        assert!(all[3]
            .effects
            .iter()
            .any(|e| matches!(e, Effect::FillOpacity { opacity } if *opacity < 0.5)));
    }

    #[test]
    fn every_look_draws_a_picture() {
        for p in looks() {
            let (rgba, w, h) = picture(&p, &sample_shape(), 48).expect("picture");
            assert!(w > 0 && h > 0);
            assert!(rgba.chunks(4).any(|px| px[3] > 0), "{}", p.name);
        }
    }

    #[test]
    fn applying_a_look_is_one_step() {
        let mut app = crate::app::App::headless();
        let id = app
            .new_shape(tracedraw_core::ShapeKind::Ellipse {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 30.0, 20.0),
                arc: None,
            })
            .expect("ellipse");
        app.select(vec![id]);
        let depth = app.engine.history_labels().0.len();
        let chrome = looks().remove(0);
        app.apply_preset(&chrome);
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let s = app.doc().find_shape(id).expect("shape").clone();
        assert_eq!(s.fill, chrome.fill);
        assert_eq!(s.effects, chrome.effects);
        app.undo();
        assert!(app.doc().find_shape(id).expect("shape").effects.is_empty());
    }
}
