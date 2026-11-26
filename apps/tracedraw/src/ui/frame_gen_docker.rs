//! The Frame Generator docker (Effects > Frame Generator): the frame's
//! style with a preview, the corner, margin, band width, line width and
//! repeat, the side ornaments, the colour, and a full-width Apply.

use crate::app::App;
use crate::frame_gen::{frame, Corner, FrameSettings, Style};
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::docker_form::{indented, row, section};
use crate::ui::tips::RichTip;
use egui::{Color32, Sense, Stroke, Ui, Vec2};
use tracedraw_core::geometry::{Affine, PathEl, Rect};

/// The frame drawn small round an empty page.
fn preview(ui: &mut Ui, s: &FrameSettings) {
    // 150 by 100 points, or smaller in proportion in a narrow panel
    // (#4323).
    let w = crate::ui::docker_form::fit_w(ui, 150.0);
    let k = f64::from(w / 150.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, w * 100.0 / 150.0), Sense::hover());
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 0.0, Color32::WHITE);
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Inside,
    );
    // A 150 by 100 mm page with the frame 6 mm in, shown `k` points per
    // mm.
    let page = Rect::new(0.0, 0.0, 150.0, 100.0);
    let sample = FrameSettings {
        margin: 6.0,
        ..s.clone()
    };
    let outer = page.inflate(-sample.margin, -sample.margin);
    let m = Affine::translate((r.left() as f64, r.bottom() as f64))
        * Affine::scale_non_uniform(k, -k);
    let c = s.color.to_rgb8();
    let color = Color32::from_rgb(c[0], c[1], c[2]);
    for pc in frame(outer, &sample) {
        let mut runs: Vec<Vec<egui::Pos2>> = Vec::new();
        let mut closed = false;
        tracedraw_core::geometry::flatten(&(m * pc.path.clone()), 0.3, &mut |el| match el {
            PathEl::MoveTo(q) => runs.push(vec![egui::pos2(q.x as f32, q.y as f32)]),
            PathEl::LineTo(q) => {
                if let Some(run) = runs.last_mut() {
                    run.push(egui::pos2(q.x as f32, q.y as f32));
                }
            }
            PathEl::ClosePath => closed = true,
            _ => {}
        });
        for run in runs {
            if run.len() < 2 {
                continue;
            }
            if pc.width == 0.0 {
                if run.len() > 2 {
                    painter.add(egui::Shape::convex_polygon(run, color, Stroke::NONE));
                }
            } else {
                let w = (pc.width as f32 * k as f32).max(0.6);
                if closed {
                    painter.add(egui::Shape::closed_line(run, Stroke::new(w, color)));
                } else {
                    painter.add(egui::Shape::line(run, Stroke::new(w, color)));
                }
            }
        }
    }
}

pub fn frame_generator(app: &mut App, ui: &mut Ui) {
    crate::ui::docker_form::fit_sliders(ui);
    let s = &mut app.frame_gen;
    row(ui, &tr("docker.frame_style"), |ui| {
        crate::ui::keynav::ComboBox::from_id_salt("frame_gen_style")
            .selected_text(tr(s.style.key()))
            .width(crate::ui::docker_form::fit_w(ui, 190.0))
            .truncate()
            .show_ui(ui, |ui| {
                for st in Style::ALL {
                    ui.selectable_value(&mut s.style, st, tr(st.key()));
                }
            })
            .response
            .rich_tip(crate::ui::tips::keyed("docker.frame_style"));
    });
    indented(ui, |ui| preview(ui, s));
    if !matches!(
        s.style,
        Style::ArtDeco | Style::GreekKey | Style::Waves | Style::Guilloche
    ) {
        row(ui, &tr("docker.frame_corner"), |ui| {
            crate::ui::keynav::ComboBox::from_id_salt("frame_gen_corner")
                .selected_text(tr(s.corner.key()))
                .width(crate::ui::docker_form::fit_w(ui, 190.0))
                .truncate()
                .show_ui(ui, |ui| {
                    for c in Corner::ALL {
                        ui.selectable_value(&mut s.corner, c, tr(c.key()));
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("docker.frame_corner"));
        });
    }
    section(ui, None);
    let mm = |ui: &mut Ui, v: &mut f64, lo: f64, hi: f64, key: &str| {
        ui.add(
            crate::ui::field::NumField::new(v)
                .range(lo..=hi)
                .speed(0.1)
                .max_decimals(2)
                .suffix(" mm"),
        )
        .rich_tip(crate::ui::tips::keyed(key));
    };
    row(ui, &tr("docker.frame_margin"), |ui| {
        mm(ui, &mut s.margin, -500.0, 500.0, "docker.frame_margin")
    });
    row(ui, &tr("docker.frame_width"), |ui| {
        mm(ui, &mut s.width, 0.5, 200.0, "docker.frame_width")
    });
    row(ui, &tr("docker.frame_line"), |ui| {
        mm(ui, &mut s.line, 0.05, 10.0, "docker.frame_line")
    });
    if matches!(
        s.style,
        Style::Beads | Style::GreekKey | Style::Scallops | Style::Waves | Style::Guilloche
    ) {
        row(ui, &tr("docker.frame_repeat"), |ui| {
            mm(ui, &mut s.repeat, 1.0, 200.0, "docker.frame_repeat")
        });
    }
    if s.style == Style::Guilloche {
        row(ui, &tr("docker.frame_strands"), |ui| {
            ui.add(crate::ui::field::NumField::new(&mut s.strands).range(2..=60))
                .rich_tip(crate::ui::tips::keyed("docker.frame_strands"));
        });
    }
    indented(ui, |ui| {
        ui.checkbox(&mut s.side_ornaments, tr("docker.frame_side_ornaments"))
            .rich_tip(crate::ui::tips::keyed("docker.frame_side_ornaments"));
    });
    row(ui, &tr("docker.frame_color"), |ui| {
        let mut c = s.color.to_rgb8();
        if ui
            .color_edit_button_srgb(&mut c)
            .rich_tip(crate::ui::tips::keyed("docker.frame_color"))
            .changed()
        {
            s.color = tracedraw_core::Color::rgb8(c[0], c[1], c[2]);
        }
    });
    let tip = crate::ui::tips::pair("docker.apply", "docker.frame_apply_desc");
    if crate::ui::docker_form::wide_button(ui, "docker.apply", true, &tip) {
        let s = app.frame_gen.clone();
        app.apply_frame(&s);
    }
    if crate::ui::docker_form::reset_button(ui) {
        app.frame_gen.reset();
    }
}
