//! The Mockup docker (Effects > Mockup): the product photos as pictures
//! to pick from, a larger picture of the chosen one with where the
//! selected design will land, the design's size and place in the print
//! area, the shading, texture and ink, a full-width Apply, and the
//! approval JPEG's caption, width and Export.

use crate::app::App;
use crate::i18n::tr;
use crate::mockup::{self, Blend, MockupSettings, OWN_PHOTO, TEMPLATES};
use crate::theme::Tokens;
use crate::ui::docker_form::{indented, row, section};
use crate::ui::tips::RichTip;
use egui::{Color32, Sense, Stroke, TextureHandle, Ui, Vec2};
use std::collections::HashMap;
use std::sync::Arc;

/// The photos drawn small, by template id and width, made once.
type Thumbs = Arc<HashMap<(String, u32), TextureHandle>>;

fn thumb(ui: &Ui, id: &str, w: u32) -> Option<TextureHandle> {
    let key = egui::Id::new("mockup_thumbs");
    let have: Option<Thumbs> = ui.ctx().data(|d| d.get_temp(key));
    if let Some(t) = have.as_ref().and_then(|m| m.get(&(id.to_string(), w))) {
        return Some(t.clone());
    }
    let t = mockup::template(id)?;
    let img = mockup::photo(t, w);
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_raw(),
    );
    let tex = ui
        .ctx()
        .load_texture(format!("mockup_{id}_{w}"), color, egui::TextureOptions::LINEAR);
    let mut map = have.map(|m| (*m).clone()).unwrap_or_default();
    map.insert((id.to_string(), w), tex.clone());
    ui.ctx().data_mut(|d| d.insert_temp(key, Arc::new(map)));
    Some(tex)
}

/// One product to pick: its photo, framed when chosen.
fn tile(ui: &mut Ui, s: &mut MockupSettings, id: &str, name: &str, size: Vec2) {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(r);
    match thumb(ui, id, 160) {
        Some(tex) => {
            painter.image(
                tex.id(),
                r,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            // My own photo: a frame with a mountain and a sun.
            painter.rect_filled(r, 2.0, Tokens::field());
            let c = r.center();
            let ink = Tokens::text_dim();
            let s2 = r.height() * 0.22;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - s2 * 1.8, c.y + s2),
                    egui::pos2(c.x - s2 * 0.4, c.y - s2 * 0.6),
                    egui::pos2(c.x + s2 * 1.8, c.y + s2),
                ],
                ink,
                Stroke::NONE,
            ));
            painter.circle_filled(egui::pos2(c.x + s2 * 1.1, c.y - s2 * 0.9), s2 * 0.35, ink);
        }
    }
    let chosen = s.template == id;
    painter.rect_stroke(
        r,
        2.0,
        if chosen {
            Stroke::new(2.0, Tokens::selected_row_line())
        } else {
            Stroke::new(1.0, Tokens::border())
        },
        egui::StrokeKind::Inside,
    );
    if resp.rich_tip(name).clicked() {
        s.template = id.to_string();
    }
}

/// The chosen photo larger, with the outline the selected design will
/// take on it.
fn preview(ui: &mut Ui, app: &App) {
    let s = &app.mockup;
    let Some(t) = mockup::template(&s.template) else {
        return;
    };
    let w = ui.available_width().clamp(120.0, 260.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, w * 0.75), Sense::hover());
    let painter = ui.painter_at(r);
    if let Some(tex) = thumb(ui, t.id, 320) {
        painter.image(
            tex.id(),
            r,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    // The design's proportions: the selection's box, or a square.
    let doc = app.doc();
    let mut b: Option<tracedraw_core::geometry::Rect> = None;
    for id in &app.selection {
        if let Some(x) = doc.find_shape(*id).map(|s| s.bounds()) {
            b = Some(b.map_or(x, |a| a.union(x)));
        }
    }
    let d = b
        .filter(|b| b.height() > 1e-9 && app.selected_mockup().is_none())
        .map_or(1.0, |b| b.width() / b.height());
    let aspect = f64::from(t.aspect);
    let size = s.size.clamp(0.05, 1.0);
    let (uw, vh) = if d > aspect {
        (size, size * aspect / d)
    } else {
        (size * d / aspect, size)
    };
    let u0 = ((1.0 - uw) / 2.0 + s.offset.0).clamp(0.0, 1.0 - uw);
    let v0 = ((1.0 - vh) * f64::from(t.anchor) + s.offset.1).clamp(0.0, 1.0 - vh);
    let to = |u: f64, v: f64| {
        let (x, y) = t.surface.at(u as f32, v as f32);
        egui::pos2(
            r.left() + x / mockup_photos_w() * r.width(),
            r.top() + y / mockup_photos_h() * r.height(),
        )
    };
    let mut ring = Vec::new();
    let n = 12;
    for i in 0..n {
        ring.push(to(u0 + uw * f64::from(i) / f64::from(n), v0));
    }
    for i in 0..n {
        ring.push(to(u0 + uw, v0 + vh * f64::from(i) / f64::from(n)));
    }
    for i in 0..n {
        ring.push(to(u0 + uw * (1.0 - f64::from(i) / f64::from(n)), v0 + vh));
    }
    for i in 0..n {
        ring.push(to(u0, v0 + vh * (1.0 - f64::from(i) / f64::from(n))));
    }
    painter.add(egui::Shape::closed_line(ring.clone(), Stroke::new(2.5, Color32::from_black_alpha(90))));
    painter.add(egui::Shape::closed_line(ring, Stroke::new(1.2, Tokens::selected_row_line())));
}

fn mockup_photos_w() -> f32 {
    crate::mockup_photos::REF_W
}

fn mockup_photos_h() -> f32 {
    crate::mockup_photos::REF_H
}

/// A percentage field over a fraction.
fn percent(ui: &mut Ui, v: &mut f64, lo: f64, hi: f64, key: &str) {
    let mut p = *v * 100.0;
    if ui
        .add(
            crate::ui::field::NumField::new(&mut p)
                .range(lo..=hi)
                .speed(1.0)
                .max_decimals(0)
                .suffix(" %"),
        )
        .rich_tip(crate::ui::tips::keyed(key))
        .changed()
    {
        *v = p / 100.0;
    }
}

pub fn mockup(app: &mut App, ui: &mut Ui) {
    ui.spacing_mut().slider_width = 110.0;
    ui.label(egui::RichText::new(tr("docker.mockup_hint")).small().color(Tokens::text_dim()));
    section(ui, Some(&tr("docker.mockup_template")));
    // The products, as many across as fit.
    let gap = ui.spacing().item_spacing.x;
    let avail = ui.available_width();
    let across = ((avail + gap) / (64.0 + gap)).floor().clamp(2.0, 6.0) as usize;
    let tw = ((avail + gap) / across as f32 - gap).floor();
    let size = Vec2::new(tw, (tw * 0.75).round());
    let mut tiles: Vec<(&str, String)> = TEMPLATES.iter().map(|t| (t.id, tr(t.key))).collect();
    tiles.push((OWN_PHOTO, tr("mockup.own_photo")));
    for chunk in tiles.chunks(across) {
        ui.horizontal(|ui| {
            for (id, name) in chunk {
                tile(ui, &mut app.mockup, id, name, size);
            }
        });
    }
    let name = mockup::template(&app.mockup.template)
        .map_or_else(|| tr("mockup.own_photo"), |t| tr(t.key));
    ui.label(egui::RichText::new(name).strong())
        .rich_tip(crate::ui::tips::keyed("docker.mockup_template"));
    preview(ui, app);
    section(ui, None);
    let s = &mut app.mockup;
    row(ui, &tr("docker.mockup_size"), |ui| {
        percent(ui, &mut s.size, 10.0, 100.0, "docker.mockup_size")
    });
    row(ui, &tr("docker.mockup_across"), |ui| {
        percent(ui, &mut s.offset.0, -50.0, 50.0, "docker.mockup_across")
    });
    row(ui, &tr("docker.mockup_down"), |ui| {
        percent(ui, &mut s.offset.1, -50.0, 50.0, "docker.mockup_down")
    });
    row(ui, &tr("docker.mockup_shading"), |ui| {
        percent(ui, &mut s.shading, 0.0, 200.0, "docker.mockup_shading")
    });
    row(ui, &tr("docker.mockup_texture"), |ui| {
        percent(ui, &mut s.texture, 0.0, 300.0, "docker.mockup_texture")
    });
    row(ui, &tr("docker.mockup_blend"), |ui| {
        crate::ui::keynav::ComboBox::from_id_salt("mockup_blend")
            .selected_text(tr(s.blend.key()))
            .width(ui.available_width().clamp(120.0, 190.0))
            .show_ui(ui, |ui| {
                for b in Blend::ALL {
                    ui.selectable_value(&mut s.blend, b, tr(b.key()));
                }
            })
            .response
            .rich_tip(crate::ui::tips::keyed("docker.mockup_blend"));
    });
    let has = !app.selection.is_empty();
    let tip = crate::ui::tips::pair("docker.apply", "docker.mockup_apply_desc");
    if crate::ui::docker_form::wide_button(ui, "docker.apply", has, &tip) {
        let s = app.mockup.clone();
        app.apply_mockup(&s);
    }
    section(ui, None);
    let s = &mut app.mockup;
    row(ui, &tr("docker.mockup_caption"), |ui| {
        ui.add(egui::TextEdit::singleline(&mut s.caption).desired_width(ui.available_width().min(190.0)))
            .rich_tip(crate::ui::tips::keyed("docker.mockup_caption"));
    });
    row(ui, &tr("docker.mockup_width"), |ui| {
        ui.add(
            crate::ui::field::NumField::new(&mut s.jpeg_width)
                .range(200..=6000)
                .speed(10.0)
                .suffix(" px"),
        )
        .rich_tip(crate::ui::tips::keyed("docker.mockup_width"));
    });
    indented(ui, |_| {});
    let tip = crate::ui::tips::keyed("docker.mockup_export");
    if crate::ui::docker_form::wide_button(ui, "docker.mockup_export", has, &tip) {
        let s = app.mockup.clone();
        app.export_mockup_jpeg(&s);
    }
}
