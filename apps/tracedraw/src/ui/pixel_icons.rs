//! The icons of the image editing mode's tools (Bitmaps > Edit Inside):
//! drawn on the 24 unit grid of the toolbox icons, in the same style
//! (see `icon_paint.rs` and docs/behavior/icons.md). Tools the drawing
//! has as well (eraser, smudge, eyedropper, zoom, pan) take its icon.

use crate::pixel_edit::PixelTool;
use crate::theme::Tokens;
use crate::tools::Tool;
use crate::ui::icon_paint::{rrect, Pen, LINE, THIN};
use egui::{Color32, Painter, Rect};

/// Draw `tool`'s icon in `r` with `color` as its ink.
pub fn draw(painter: &Painter, r: Rect, tool: PixelTool, color: Color32) {
    let drawing = match tool {
        PixelTool::Eraser => Some(Tool::Eraser),
        PixelTool::Smudge => Some(Tool::Smudge),
        PixelTool::Eyedropper => Some(Tool::ColorEyedropper),
        PixelTool::Zoom => Some(Tool::Zoom),
        PixelTool::Pan => Some(Tool::Pan),
        PixelTool::Liquify | PixelTool::Twirl => Some(Tool::Twirl),
        PixelTool::Smear => Some(Tool::Smear),
        PixelTool::LayerFade => Some(Tool::Transparency),
        _ => None,
    };
    if let Some(t) = drawing {
        crate::ui::icons::draw(painter, r, t, color);
        return;
    }
    let plain = color == Tokens::icon_off()
        || color.a() < 255
        || crate::theme::theme() == crate::theme::Theme::HighContrast;
    let acc = if plain {
        color
    } else {
        crate::ui::icons::accent()
    };
    let pen = Pen::new(painter, r, color, acc, Tokens::panel());
    let ink = color;
    let clear = Color32::TRANSPARENT;
    // A round brush on the diagonal, its tip at the lower left.
    let brush = |pen: &Pen| {
        pen.stroke("M20.5 3.5l-7 7", 2.6, ink);
        pen.fill(
            "M13.5 10.5c-2-.8-4.2.2-4.8 2.6-.4 1.6-.9 3.6-3.7 5.4 3.6.6 7.6-.6 8.9-3.4.6-1.4.6-3.4-.4-4.6z",
            ink,
            true,
        );
    };
    match tool {
        PixelTool::RectSelect => {
            pen.ghost("M4 5h16v14H4z", ink);
            pen.node(4.0, 5.0, false);
            pen.node(20.0, 19.0, true);
        }
        PixelTool::EllipseSelect => {
            pen.dashed(
                "M12 5a8.5 7 0 1 0 0 14a8.5 7 0 1 0 0-14z",
                LINE,
                ink,
                2.4,
                2.2,
            );
            pen.node(20.5, 12.0, true);
        }
        PixelTool::Lasso => {
            pen.ghost("M6 14c-3-4 0-10 7-10s9 5 6 8-8 3-11 2", ink);
            pen.line("M8 14c-1 2 0 4 2 5", acc);
            pen.node(10.0, 19.0, true);
        }
        PixelTool::MagicWand => {
            pen.stroke("M4 20L15 9", 2.6, ink);
            pen.line("M18 2.5v4M16 4.5h4", acc);
            pen.line("M20.5 9v3M19 10.5h3", acc);
            pen.line("M12 3v2.5M10.75 4.25h2.5", acc);
        }
        PixelTool::Brush => {
            brush(&pen);
            pen.line("M3 21.5h7", acc);
        }
        PixelTool::Clone => {
            // A stamp: the handle, the block and its print.
            pen.circle(12.0, 5.0, 2.6, pen.wash(), LINE, ink);
            pen.line("M10.5 7.5v4M13.5 7.5v4", ink);
            pen.body(&rrect(4.0, 11.5, 16.0, 5.0, 1.2));
            pen.stroke("M5 20.5h14", 2.4, acc);
        }
        PixelTool::Heal => {
            // A plaster across, its pad in the accent.
            let t = pen.rotated(-45.0, 12.0, 12.0);
            t.body(&rrect(2.5, 8.0, 19.0, 8.0, 4.0));
            t.fill(&rrect(9.0, 8.0, 6.0, 8.0, 0.5), acc, true);
        }
        PixelTool::Fill => {
            // A tipped bucket pouring a drop.
            let t = pen.rotated(-30.0, 11.0, 12.0);
            t.body("M5 8h12l-1.5 11h-9z");
            t.line("M7 8c0-4 8-4 8 0", ink);
            pen.fill("M19.5 13c1.5 2.3 2.3 3.6 2.3 4.6a2.3 2.3 0 0 1-4.6 0c0-1 .8-2.3 2.3-4.6z", acc, true);
        }
        PixelTool::Gradient => {
            for (i, a) in [0.15_f32, 0.35, 0.55, 0.75, 1.0].iter().enumerate() {
                let x = 4.0 + i as f32 * 3.2;
                pen.rect(x, 5.0, 3.2, 14.0, 0.0, Pen::alpha(acc, *a), 0.0, clear);
            }
            pen.rect(4.0, 5.0, 16.0, 14.0, 1.0, clear, LINE, ink);
        }
        PixelTool::Dodge => {
            // A lollipop: a ring on a stick.
            pen.circle(9.0, 9.0, 5.5, pen.wash(), LINE, ink);
            pen.stroke("M13 13l7.5 7.5", 2.4, ink);
            pen.line("M6.5 9a2.5 2.5 0 0 1 2.5-2.5", acc);
        }
        PixelTool::Burn => {
            // A hand closed to a small opening.
            pen.body("M4 10c0-3 3-5 6-5h3c3 0 6 2 6 5v4c0 3-3 5-6 5h-3c-3 0-6-2-6-5z");
            pen.circle(11.5, 12.0, 1.8, acc, 0.0, clear);
            pen.thin("M8 8.5h7", ink);
        }
        PixelTool::Sponge => {
            pen.body(&rrect(4.0, 6.0, 16.0, 12.0, 4.0));
            for (x, y) in [(8.0, 10.0), (12.5, 9.0), (16.0, 12.0), (9.5, 14.5), (14.0, 15.0)] {
                pen.circle(x, y, 1.1, acc, 0.0, clear);
            }
        }
        PixelTool::Blur => {
            // A drop.
            pen.body("M12 3c4 5.5 6 9 6 11.5a6 6 0 0 1-12 0C6 12 8 8.5 12 3z");
            pen.line("M9.5 15a2.5 2.5 0 0 0 2.5 2.5", acc);
        }
        PixelTool::Sharpen => {
            // A cone to a point.
            pen.body("M12 3l6 16H6z");
            pen.line("M9 14h6", acc);
        }
        PixelTool::RedEye => {
            pen.line("M2.5 12c3-5 6-7 9.5-7s6.5 2 9.5 7c-3 5-6 7-9.5 7s-6.5-2-9.5-7z", ink);
            pen.circle(12.0, 12.0, 3.6, Color32::from_rgb(0xD8, 0x30, 0x30), 0.0, clear);
            pen.circle(12.0, 12.0, 3.6, clear, THIN, ink);
        }
        PixelTool::Move => {
            pen.line("M12 3v18M3 12h18", ink);
            pen.arrow_head(12.0, 3.0, 0.0, -1.0, 3.0, acc);
            pen.arrow_head(12.0, 21.0, 0.0, 1.0, 3.0, acc);
            pen.arrow_head(3.0, 12.0, -1.0, 0.0, 3.0, acc);
            pen.arrow_head(21.0, 12.0, 1.0, 0.0, 3.0, acc);
        }
        PixelTool::Light => {
            // A sun: a disc and its rays.
            pen.circle(12.0, 12.0, 4.0, acc, 0.0, clear);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (s, c) = a.sin_cos();
                let d = format!(
                    "M{:.2} {:.2}L{:.2} {:.2}",
                    12.0 + 6.5 * c,
                    12.0 + 6.5 * s,
                    12.0 + 9.0 * c,
                    12.0 + 9.0 * s
                );
                pen.line(&d, ink);
            }
        }
        PixelTool::Warp => {
            pen.line("M4 6c5-3 11 3 16 0M4 12c5-3 11 3 16 0M4 18c5-3 11 3 16 0", ink);
            pen.line("M4 6v12M20 6v12", ink);
            pen.node(12.0, 12.0, true);
        }
        PixelTool::Restore => {
            // A brush with an arrow turning back.
            brush(&pen);
            pen.line("M3.5 9a6 6 0 0 1 6-6", acc);
            pen.arrow_head(3.5, 9.0, 0.0, 1.0, 2.8, acc);
        }
        PixelTool::Tint => {
            brush(&pen);
            pen.rect(2.5, 2.5, 7.0, 7.0, 1.5, acc, 0.0, acc);
        }
        PixelTool::ReplaceColor => {
            pen.rect(2.5, 3.0, 8.0, 8.0, 1.5, clear, LINE, ink);
            pen.rect(13.5, 13.0, 8.0, 8.0, 1.5, acc, 0.0, acc);
            pen.line("M14 5.5c3 0 5 2 5 4.5", ink);
            pen.arrow_head(19.0, 10.0, 0.0, 1.0, 2.6, ink);
            pen.line("M10 18.5c-3 0-5-2-5-4.5", ink);
            pen.arrow_head(5.0, 14.0, 0.0, -1.0, 2.6, ink);
        }
        PixelTool::Shape => {
            pen.rect(3.0, 3.0, 11.0, 11.0, 1.0, pen.wash(), LINE, ink);
            pen.circle(15.0, 15.0, 6.0, Pen::alpha(acc, 0.6), LINE, acc);
        }
        PixelTool::Remove => {
            // A brush with a spot fading out behind it.
            pen.ghost("M3 14a4 4 0 1 0 8 0a4 4 0 1 0-8 0", acc);
            brush(&pen);
        }
        PixelTool::FreehandMask => {
            pen.ghost("M4 18L6 5l8 3 6-3-2 14z", ink);
            pen.node(4.0, 18.0, false);
            pen.node(6.0, 5.0, false);
            pen.node(14.0, 8.0, false);
            pen.node(18.0, 19.0, true);
        }
        PixelTool::EdgeMask => {
            pen.ghost("M5 15c-3-4 0-11 7-11s9 5 6 8", ink);
            // A magnet: a U in the accent with its ends.
            pen.stroke("M11 13v4a4 4 0 0 0 8 0v-4", 2.4, acc);
            pen.line("M10 13h2.5M17.5 13h2.5", ink);
        }
        PixelTool::MaskBrush => {
            pen.ghost("M2.5 2.5h11v11h-11z", ink);
            brush(&pen);
        }
        PixelTool::BandMask => {
            pen.dashed("M3 15L15 3", LINE, ink, 2.4, 2.2);
            pen.dashed("M9 21L21 9", LINE, ink, 2.4, 2.2);
            pen.line("M8 8l8 8", acc);
            pen.arrow_head(8.0, 8.0, -1.0, -1.0, 2.6, acc);
            pen.arrow_head(16.0, 16.0, 1.0, 1.0, 2.6, acc);
        }
        PixelTool::Attract | PixelTool::Repel => {
            pen.dashed("M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z", LINE, ink, 2.4, 2.2);
            pen.circle(12.0, 12.0, 1.6, acc, 0.0, clear);
            let inward = tool == PixelTool::Attract;
            for (dx, dy) in [(0.0_f32, -1.0_f32), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)] {
                let (a, b) = (4.0, 8.0);
                let d = format!(
                    "M{:.1} {:.1}L{:.1} {:.1}",
                    12.0 + dx * a,
                    12.0 + dy * a,
                    12.0 + dx * b,
                    12.0 + dy * b
                );
                pen.line(&d, acc);
                if inward {
                    pen.arrow_head(12.0 + dx * a, 12.0 + dy * a, -dx, -dy, 2.4, acc);
                } else {
                    pen.arrow_head(12.0 + dx * b, 12.0 + dy * b, dx, dy, 2.4, acc);
                }
            }
        }
        PixelTool::Sprayer => {
            // A can and its spray.
            pen.body(&rrect(4.0, 9.0, 8.0, 12.0, 1.5));
            pen.rect(6.0, 6.0, 4.0, 3.0, 0.5, clear, LINE, ink);
            for (x, y, r) in [(15.0, 5.0, 1.2), (18.5, 7.5, 1.0), (16.0, 10.0, 1.3), (20.0, 12.0, 1.0), (17.5, 14.0, 1.1), (20.5, 4.0, 0.9)] {
                pen.circle(x, y, r, acc, 0.0, clear);
            }
        }
        PixelTool::ColorToClear | PixelTool::TransparencyBrush => {
            // A see-through checkerboard.
            for (k, (x, y)) in [(3.0, 3.0), (9.0, 9.0), (9.0, 3.0), (3.0, 9.0)].iter().enumerate() {
                let fill = if k < 2 { Pen::alpha(ink, 0.55) } else { clear };
                pen.rect(*x, *y, 6.0, 6.0, 0.0, fill, 0.0, clear);
            }
            pen.rect(3.0, 3.0, 12.0, 12.0, 0.5, clear, THIN, ink);
            if tool == PixelTool::TransparencyBrush {
                brush(&pen);
            } else {
                // A dropper on the colour.
                pen.stroke("M21 7l-8 8", 2.4, ink);
                pen.circle(13.0, 15.0, 1.8, acc, 0.0, clear);
                pen.line("M13 15l-4 6", acc);
            }
        }
        PixelTool::Eraser
        | PixelTool::Smudge
        | PixelTool::Eyedropper
        | PixelTool::Zoom
        | PixelTool::Pan
        | PixelTool::Liquify
        | PixelTool::Twirl
        | PixelTool::Smear
        | PixelTool::LayerFade => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_image_tool_draws_an_icon_in_its_box() {
        for t in PixelTool::ALL {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(40.0))),
                ..Default::default()
            };
            let box_ = Rect::from_min_size(egui::pos2(4.0, 4.0), egui::Vec2::splat(22.0));
            let mut out = ctx.run_ui(input, |ui| draw(ui.painter(), box_, t, Color32::BLACK));
            out.textures_delta.clear();
            assert!(!out.shapes.is_empty(), "{t:?} drew nothing");
            for s in out.shapes {
                let r = s.shape.visual_bounding_rect();
                assert!(box_.expand(2.0).contains_rect(r), "{t:?} leaves its box: {r:?}");
            }
        }
    }
}
