//! The pictures of the image editing mode's Adjust and Image menus
//! (Bitmaps > Edit Inside): small drawings on a 16 point square in the
//! row's ink, with a few fixed colours where the colour is the point
//! (red, green and blue for the colour corrections).

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};

const RED: Color32 = Color32::from_rgb(0xE0, 0x3C, 0x31);
const GREEN: Color32 = Color32::from_rgb(0x2E, 0xA0, 0x43);
const BLUE: Color32 = Color32::from_rgb(0x2F, 0x6F, 0xE0);

fn at(r: Rect, x: f32, y: f32) -> Pos2 {
    r.min + Vec2::new(x / 16.0 * r.width(), y / 16.0 * r.height())
}

fn sq(r: Rect, x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_max(at(r, x, y), at(r, x + w, y + h))
}

fn text(p: &Painter, r: Rect, s: &str, ink: Color32) {
    p.rect_stroke(r.shrink(0.5), 1.0, Stroke::new(1.0, ink), egui::StrokeKind::Inside);
    p.text(
        r.center(),
        Align2::CENTER_CENTER,
        s,
        FontId::monospace(if s.len() > 2 { 5.5 } else { 7.0 }),
        ink,
    );
}

fn curve(p: &Painter, r: Rect, pts: &[(f32, f32)], ink: Color32) {
    let line: Vec<Pos2> = pts.iter().map(|(x, y)| at(r, *x, *y)).collect();
    p.add(egui::Shape::line(line, Stroke::new(1.3, ink)));
}

/// Draw the picture of the adjustment `id` (an effect id).
pub fn adjust(p: &Painter, r: Rect, id: &str, ink: Color32) {
    let line = Stroke::new(1.2, ink);
    match id {
        "auto_adjust" => {
            curve(p, r, &[(2.0, 14.0), (10.0, 6.0)], ink);
            for (x, y) in [(12.0, 3.0), (14.0, 7.0), (8.0, 2.0)] {
                p.circle_filled(at(r, x, y), 1.2, GREEN);
            }
        }
        "image_adjustments" => {
            for (k, x) in [5.0, 11.0, 7.0].iter().enumerate() {
                let y = 3.5 + k as f32 * 4.5;
                p.line_segment([at(r, 1.5, y), at(r, 14.5, y)], line);
                p.circle_filled(at(r, *x, y), 2.0, [RED, GREEN, BLUE][k]);
            }
        }
        "contrast_enhancement" => {
            p.rect_filled(sq(r, 1.5, 3.0, 13.0, 5.0), 0.0, ink);
            p.rect_stroke(sq(r, 1.5, 3.0, 13.0, 10.0), 0.0, line, egui::StrokeKind::Inside);
        }
        "local_equalization" => {
            for (k, a) in [0.2_f32, 0.9, 0.6, 0.35].iter().enumerate() {
                let (x, y) = ((k % 2) as f32 * 6.5 + 1.5, (k / 2) as f32 * 6.5 + 1.5);
                p.rect_filled(sq(r, x, y, 6.0, 6.0), 0.0, ink.gamma_multiply(*a));
            }
        }
        "target_balance" => {
            p.circle_stroke(at(r, 8.0, 8.0), r.width() * 0.4, line);
            p.circle_stroke(at(r, 8.0, 8.0), r.width() * 0.2, line);
            p.circle_filled(at(r, 8.0, 8.0), 1.5, RED);
        }
        "tone_curve" => {
            p.rect_stroke(r.shrink(1.0), 0.0, Stroke::new(1.0, ink), egui::StrokeKind::Inside);
            curve(p, r, &[(2.0, 14.0), (5.0, 12.0), (8.0, 8.0), (11.0, 4.0), (14.0, 2.0)], ink);
        }
        "brightness_contrast_intensity" => {
            p.circle_stroke(at(r, 8.0, 8.0), r.width() * 0.2, line);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (s, c) = a.sin_cos();
                p.line_segment(
                    [at(r, 8.0 + 4.8 * c, 8.0 + 4.8 * s), at(r, 8.0 + 7.0 * c, 8.0 + 7.0 * s)],
                    line,
                );
            }
        }
        "gamma" => {
            p.rect_stroke(r.shrink(1.0), 0.0, Stroke::new(1.0, ink), egui::StrokeKind::Inside);
            curve(p, r, &[(4.0, 3.0), (6.0, 9.0), (8.0, 13.0), (10.0, 9.0), (12.0, 3.0)], ink);
        }
        "hue_saturation_lightness" => {
            p.circle_filled(at(r, 8.0, 5.5), r.width() * 0.22, RED);
            p.circle_filled(at(r, 5.0, 10.5), r.width() * 0.22, GREEN);
            p.circle_filled(at(r, 11.0, 10.5), r.width() * 0.22, BLUE);
        }
        "desaturate" => {
            p.rect_filled(sq(r, 1.5, 1.5, 13.0, 13.0), 0.0, ink.gamma_multiply(0.35));
            p.line_segment([at(r, 1.5, 14.5), at(r, 14.5, 1.5)], line);
            p.rect_stroke(sq(r, 1.5, 1.5, 13.0, 13.0), 0.0, line, egui::StrokeKind::Inside);
        }
        "selective_color" => {
            curve(p, r, &[(13.0, 3.0), (5.0, 11.0)], ink);
            p.circle_filled(at(r, 4.0, 12.0), 2.2, RED);
            p.circle_filled(at(r, 12.0, 12.0), 1.8, GREEN);
            p.circle_filled(at(r, 3.0, 4.0), 1.8, BLUE);
        }
        "replace_colors" => {
            p.rect_filled(sq(r, 1.0, 1.0, 7.0, 7.0), 1.0, RED);
            p.rect_filled(sq(r, 8.0, 8.0, 7.0, 7.0), 1.0, BLUE);
            curve(p, r, &[(10.0, 2.5), (13.0, 2.5), (13.0, 6.0)], ink);
            curve(p, r, &[(6.0, 13.5), (3.0, 13.5), (3.0, 10.0)], ink);
        }
        "channel_mixer" => {
            for (k, y) in [4.0, 9.0, 6.5].iter().enumerate() {
                let x = 3.5 + k as f32 * 4.5;
                p.line_segment([at(r, x, 1.5), at(r, x, 14.5)], line);
                p.rect_filled(sq(r, x - 1.8, *y, 3.6, 2.5), 0.5, [RED, GREEN, BLUE][k]);
            }
        }
        "color_balance" => {
            for (k, h) in [9.0, 12.0, 6.0].iter().enumerate() {
                let x = 2.0 + k as f32 * 4.5;
                p.rect_filled(sq(r, x, 15.0 - h, 3.5, *h), 0.0, [RED, GREEN, BLUE][k]);
            }
        }
        "invert_colors" => {
            p.rect_stroke(sq(r, 1.5, 1.5, 13.0, 13.0), 0.0, line, egui::StrokeKind::Inside);
            p.rect_filled(sq(r, 8.0, 1.5, 6.5, 13.0), 0.0, ink);
            p.circle_stroke(at(r, 8.0, 8.0), r.width() * 0.22, line);
        }
        "posterize" => {
            curve(
                p,
                r,
                &[(1.5, 14.5), (1.5, 11.0), (5.5, 11.0), (5.5, 7.0), (9.5, 7.0), (9.5, 3.0), (14.5, 3.0)],
                ink,
            );
        }
        "deinterlace" => {
            for k in 0..5 {
                let y = 2.5 + k as f32 * 2.8;
                let s = if k % 2 == 0 { line } else { Stroke::new(1.2, ink.gamma_multiply(0.4)) };
                p.line_segment([at(r, 1.5, y), at(r, 14.5, y)], s);
            }
        }
        _ => text(p, r, "fx", ink),
    }
}

/// The pictures of the Image menu's commands.
pub fn image(p: &Painter, r: Rect, which: &str, ink: Color32) {
    let line = Stroke::new(1.2, ink);
    let arrow = |p: &Painter, from: Pos2, to: Pos2| {
        p.line_segment([from, to], line);
        let d = (to - from).normalized() * 3.0;
        let n = Vec2::new(-d.y, d.x);
        p.line_segment([to, to - d + n], line);
        p.line_segment([to, to - d - n], line);
    };
    match which {
        "grayscale" => text(p, r, "GRY", ink),
        "black_white" => text(p, r, "BW", ink),
        "turn_right" | "turn_left" | "turn_180" => {
            p.rect_stroke(sq(r, 4.0, 5.0, 8.0, 9.0), 0.0, line, egui::StrokeKind::Inside);
            let (a, b) = if which == "turn_left" {
                (at(r, 12.0, 2.5), at(r, 3.0, 2.5))
            } else {
                (at(r, 4.0, 2.5), at(r, 13.0, 2.5))
            };
            arrow(p, a, b);
            if which == "turn_180" {
                p.text(r.center() + Vec2::new(0.0, 1.5), Align2::CENTER_CENTER, "2", FontId::monospace(6.0), ink);
            }
        }
        "flip_h" => {
            p.line_segment([at(r, 8.0, 1.0), at(r, 8.0, 15.0)], Stroke::new(1.0, ink.gamma_multiply(0.5)));
            arrow(p, at(r, 6.0, 8.0), at(r, 1.5, 8.0));
            arrow(p, at(r, 10.0, 8.0), at(r, 14.5, 8.0));
        }
        "flip_v" => {
            p.line_segment([at(r, 1.0, 8.0), at(r, 15.0, 8.0)], Stroke::new(1.0, ink.gamma_multiply(0.5)));
            arrow(p, at(r, 8.0, 6.0), at(r, 8.0, 1.5));
            arrow(p, at(r, 8.0, 10.0), at(r, 8.0, 14.5));
        }
        "auto_crop" => {
            crate::ui::icons::draw(p, r, crate::tools::Tool::Crop, ink);
            p.circle_filled(at(r, 13.5, 2.5), 1.4, GREEN);
        }
        "canvas_size" => {
            p.rect_stroke(sq(r, 1.0, 1.0, 14.0, 14.0), 0.0, Stroke::new(1.0, ink.gamma_multiply(0.5)), egui::StrokeKind::Inside);
            p.rect_filled(sq(r, 4.5, 4.5, 7.0, 7.0), 0.0, ink);
        }
        "smart_resize" => {
            p.rect_stroke(sq(r, 1.5, 4.0, 13.0, 8.0), 0.0, line, egui::StrokeKind::Inside);
            curve(p, r, &[(6.0, 4.0), (5.0, 8.0), (6.0, 12.0)], ink);
            curve(p, r, &[(10.0, 4.0), (11.0, 8.0), (10.0, 12.0)], ink);
        }
        "histogram" => {
            for (k, h) in [4.0_f32, 8.0, 12.0, 9.0, 6.0, 3.0].iter().enumerate() {
                let x = 1.5 + k as f32 * 2.2;
                p.rect_filled(sq(r, x, 15.0 - h, 1.8, *h), 0.0, ink);
            }
        }
        _ => text(p, r, "?", ink),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_picture_stays_in_its_square() {
        let ids = crate::fx::adjust::EFFECTS.iter().map(|e| e.id).chain([
            "grayscale",
            "black_white",
            "turn_right",
            "turn_left",
            "turn_180",
            "flip_h",
            "flip_v",
            "auto_crop",
            "canvas_size",
            "smart_resize",
            "histogram",
        ]);
        for id in ids {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(40.0))),
                ..Default::default()
            };
            let box_ = Rect::from_min_size(egui::pos2(4.0, 4.0), Vec2::splat(16.0));
            let mut out = ctx.run_ui(input, |ui| {
                adjust(ui.painter(), box_, id, Color32::BLACK);
                image(ui.painter(), box_, id, Color32::BLACK);
            });
            out.textures_delta.clear();
            assert!(!out.shapes.is_empty(), "{id}");
        }
    }
}
