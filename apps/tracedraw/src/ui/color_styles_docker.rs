//! The Color Styles docker (#778): the styles as a list or as swatches,
//! loose ones first and then each harmony; styles picked to fill or
//! outline with, swap, merge, group into a harmony, change together on
//! the harmony wheel, shade into a gradient; and the harmony rules. The
//! work is in `color_styles.rs`; see `docs/behavior/color-styles.md`.

use crate::ui::btn_icons::IconUi;
use crate::app::App;
use crate::color_styles::{self as cs, Change};
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::tips::{keyed, pair, RichTip};
use egui::{Color32, Sense, Ui};
use tracedraw_core::{Color, ColorStyle, Command};

fn dim(ui: &mut Ui, text: String) {
    ui.label(
        egui::RichText::new(text)
            .color(Tokens::text_dim())
            .size(11.0),
    );
}

fn c32(c: Color) -> Color32 {
    crate::canvas::to_color32(c)
}

/// How the styles show (kept for the session).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum View {
    #[default]
    List,
    Swatches,
}

fn view_id() -> egui::Id {
    egui::Id::new("color_styles_view")
}

/// The adjust sliders' values since the pick last changed, and the
/// pick they belong to.
#[derive(Clone, Default)]
struct Adjust {
    pick: Vec<usize>,
    hue: f64,
    sat: f64,
    bright: f64,
    mark: Option<Option<u64>>,
}

fn adjust_id() -> egui::Id {
    egui::Id::new("color_styles_adjust")
}

pub fn show(app: &mut App, ui: &mut Ui) {
    // Objects dragged from the drawing and let go here become colour
    // styles (#2625): where the docker is, and a frame while they are
    // over it.
    let area = ui.max_rect().intersect(ui.clip_rect());
    app.color_styles_rect = Some(area);
    if app.transforming() && app.color_styles_drop_target() {
        ui.painter().rect_stroke(
            area.shrink(1.0),
            3.0,
            egui::Stroke::new(2.0, Tokens::SELECTION),
            egui::StrokeKind::Inside,
        );
    }
    let styles = app.doc().color_styles.clone();
    app.color_styles_sel.retain(|&i| i < styles.len());
    new_row(app, ui, &styles);
    if styles.is_empty() {
        dim(ui, tr("docker.cs_drop_hint"));
    } else {
        view_row(app, ui);
        let view = ui.data(|d| d.get_temp::<View>(view_id()).unwrap_or_default());
        match view {
            View::List => list(app, ui, &styles),
            View::Swatches => swatches(app, ui, &styles),
        }
        picked_actions(app, ui);
        adjust(app, ui);
    }
    gradient_row(app, ui);
    ui.separator();
    harmony_editor(app, ui);
}

/// New from selection, New color style, New from document and the menu.
fn new_row(app: &mut App, ui: &mut Ui, styles: &[ColorStyle]) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .ibutton("docker.new_from_selection")
            .rich_tip(pair(
                "docker.new_from_selection",
                "docker.cs_new_from_selection_desc",
            ))
            .clicked()
        {
            let mut st = styles.to_vec();
            for s in app.selected_shapes() {
                if let Some(c) = s.fill.preview_color() {
                    if !st.iter().any(|x| x.color == c) {
                        st.push(ColorStyle {
                            name: format!("Color {}", st.len() + 1),
                            color: c,
                            harmony: None,
                        });
                    }
                }
            }
            app.run(Command::SetColorStyles { styles: st });
        }
        if ui
            .ibutton("docker.new_color_style")
            .rich_tip(keyed("docker.new_color_style"))
            .clicked()
        {
            crate::ui::dockers2::add_color_style(app, app.mixer_color);
        }
        if ui
            .ibutton("docker.cs_from_document")
            .rich_tip(keyed("docker.cs_from_document"))
            .clicked()
        {
            let n = app.color_styles_from_document();
            app.status = trf("status.cs_added", &[("n", &n.to_string())]);
        }
        ui.menu_button(crate::ui::color_docker::MORE_ACTIONS, |ui| {
            more_menu(app, ui)
        })
        .response
        .rich_tip(keyed("docker.cs_more"));
    });
}

/// The menu of the styles: delete unused, sort, convert, to the palette.
fn more_menu(app: &mut App, ui: &mut Ui) {
    let styles = app.doc().color_styles.clone();
    let picked = picked_or_all(app, &styles);
    let item = |ui: &mut Ui, key: &str, on: bool| {
        ui.add_enabled(on, crate::ui::btn_icons::Ib::new(key))
            .rich_tip(keyed(key))
            .clicked()
    };
    if item(ui, "docker.cs_delete_unused", !styles.is_empty()) {
        let gone = cs::unused(&styles, app.doc());
        app.color_styles_sel.clear();
        app.change_color_styles(cs::delete(&styles, &gone));
        app.status = trf("status.cs_deleted", &[("n", &gone.len().to_string())]);
        ui.close();
    }
    if item(ui, "docker.cs_sort_name", styles.len() > 1) {
        app.color_styles_sel.clear();
        app.run(Command::SetColorStyles {
            styles: cs::sort(&styles, false),
        });
        ui.close();
    }
    if item(ui, "docker.cs_sort_hue", styles.len() > 1) {
        app.color_styles_sel.clear();
        app.run(Command::SetColorStyles {
            styles: cs::sort(&styles, true),
        });
        ui.close();
    }
    ui.separator();
    if item(ui, "docker.cs_to_cmyk", !picked.is_empty()) {
        app.change_color_styles(cs::convert(&styles, &picked, "CMYK"));
        ui.close();
    }
    if item(ui, "docker.cs_to_rgb", !picked.is_empty()) {
        app.change_color_styles(cs::convert(&styles, &picked, "RGB"));
        ui.close();
    }
    if item(ui, "docker.cs_to_palette", !picked.is_empty()) {
        let mut colors = app.doc().palette.clone();
        for &i in &picked {
            let c = styles[i].color;
            if !colors.contains(&c) {
                colors.push(c);
            }
        }
        app.run(Command::SetDocumentPalette { colors });
        ui.close();
    }
}

/// The picked styles, or every style when none is picked.
fn picked_or_all(app: &App, styles: &[ColorStyle]) -> Vec<usize> {
    if app.color_styles_sel.is_empty() {
        (0..styles.len()).collect()
    } else {
        app.color_styles_sel.clone()
    }
}

/// List or Swatches, and Pick all / Pick none.
fn view_row(app: &mut App, ui: &mut Ui) {
    let mut view = ui.data(|d| d.get_temp::<View>(view_id()).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.selectable_value(&mut view, View::List, tr("docker.cs_view_list"))
            .rich_tip(keyed("docker.cs_view_list"));
        ui.selectable_value(&mut view, View::Swatches, tr("docker.cs_view_swatches"))
            .rich_tip(keyed("docker.cs_view_swatches"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button(tr("docker.cs_pick_none"))
                .rich_tip(keyed("docker.cs_pick_none"))
                .clicked()
            {
                app.color_styles_sel.clear();
            }
            if ui
                .small_button(tr("docker.cs_pick_all"))
                .rich_tip(keyed("docker.cs_pick_all"))
                .clicked()
            {
                app.color_styles_sel = (0..app.doc().color_styles.len()).collect();
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(view_id(), view));
}

/// Click picks one style; Ctrl adds or takes one away; Shift picks the
/// run from the last one picked.
fn pick(app: &mut App, ui: &Ui, i: usize) {
    let m = ui.input(|i| i.modifiers);
    let sel = &mut app.color_styles_sel;
    if m.shift {
        if let Some(&from) = sel.last() {
            let (a, b) = (from.min(i), from.max(i));
            for k in a..=b {
                if !sel.contains(&k) {
                    sel.push(k);
                }
            }
            return;
        }
    }
    if m.command {
        if let Some(p) = sel.iter().position(|&k| k == i) {
            sel.remove(p);
        } else {
            sel.push(i);
        }
        return;
    }
    *sel = vec![i];
}

/// The groups the styles show in: the loose ones, then each harmony.
fn groups(styles: &[ColorStyle]) -> Vec<(Option<String>, Vec<usize>)> {
    let loose: Vec<usize> = styles
        .iter()
        .enumerate()
        .filter(|(_, s)| s.harmony.is_none())
        .map(|(i, _)| i)
        .collect();
    let mut out = Vec::new();
    if !loose.is_empty() {
        out.push((None, loose));
    }
    for h in cs::harmonies(styles) {
        let m = cs::members(styles, &h);
        out.push((Some(h), m));
    }
    out
}

/// A harmony's title: click picks all its styles.
fn harmony_title(app: &mut App, ui: &mut Ui, name: &str, members: &[usize]) {
    let all = !members.is_empty() && members.iter().all(|i| app.color_styles_sel.contains(i));
    let text = egui::RichText::new(format!("\u{23F7} {name}")).strong();
    if ui
        .selectable_label(all, text)
        .rich_tip(keyed("docker.cs_harmony"))
        .clicked()
    {
        if ui.input(|i| i.modifiers.command) {
            for &i in members {
                if !app.color_styles_sel.contains(&i) {
                    app.color_styles_sel.push(i);
                }
            }
        } else {
            app.color_styles_sel = members.to_vec();
        }
    }
}

/// A row a style: picked, colour, name, Fill, Outline, x.
fn list(app: &mut App, ui: &mut Ui, styles: &[ColorStyle]) {
    let mut st = styles.to_vec();
    let mut edited: Option<(usize, Color)> = None;
    let mut renamed = false;
    let mut remove = None;
    for (harmony, members) in groups(styles) {
        if let Some(h) = &harmony {
            harmony_title(app, ui, h, &members);
        }
        for i in members {
            ui.horizontal(|ui| {
                if harmony.is_some() {
                    ui.add_space(10.0);
                }
                let mut on = app.color_styles_sel.contains(&i);
                if ui
                    .checkbox(&mut on, "")
                    .rich_tip(keyed("docker.cs_pick"))
                    .clicked()
                {
                    if on {
                        app.color_styles_sel.push(i);
                    } else {
                        app.color_styles_sel.retain(|&k| k != i);
                    }
                }
                let mut c = st[i].color;
                if crate::ui::color_picker::color_button(ui, &mut c) {
                    edited = Some((i, c));
                }
                let field = egui::Id::new(("color_style_name", i));
                let (_, typed) = crate::ui::dockers2::kept_text(
                    ui,
                    field,
                    i,
                    &st[i].name.clone(),
                    |ui, text| {
                        ui.add(egui::TextEdit::singleline(text).desired_width(90.0))
                            .rich_tip(keyed("docker.cs_name"))
                    },
                );
                if let Some(name) = typed {
                    st[i].name = name;
                    renamed = true;
                }
                if ui
                    .small_button(tr("docker.fill"))
                    .rich_tip(pair("docker.fill", "docker.cs_fill_desc"))
                    .clicked()
                {
                    app.apply_color_style(i, false);
                }
                if ui
                    .small_button(tr("docker.outline"))
                    .rich_tip(pair("docker.outline", "docker.cs_outline_desc"))
                    .clicked()
                {
                    app.apply_color_style(i, true);
                }
                if ui
                    .small_button("x")
                    .rich_tip(keyed("docker.cs_delete"))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
    }
    if let Some((i, c)) = edited {
        let old = st[i].color;
        st[i].color = c;
        app.change_color_styles(Change {
            styles: st,
            pairs: vec![(old, c)],
        });
    } else if renamed {
        app.run(Command::SetColorStyles { styles: st });
    } else if let Some(i) = remove {
        app.color_styles_sel.clear();
        app.change_color_styles(cs::delete(styles, &[i]));
    }
}

/// The styles as swatches; a double click fills the selection.
fn swatches(app: &mut App, ui: &mut Ui, styles: &[ColorStyle]) {
    let size = egui::vec2(22.0, 22.0);
    for (harmony, members) in groups(styles) {
        if let Some(h) = &harmony {
            harmony_title(app, ui, h, &members);
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
            for i in members {
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                let p = ui.painter();
                p.rect_filled(rect, 3.0, c32(styles[i].color));
                let picked = app.color_styles_sel.contains(&i);
                let stroke = if picked {
                    egui::Stroke::new(2.0, Tokens::SELECTION)
                } else {
                    egui::Stroke::new(1.0, Tokens::control_border())
                };
                p.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
                let resp = resp.rich_tip_with(|| {
                    format!(
                        "{}\n{}\n{}",
                        styles[i].name,
                        styles[i].color.to_hex(),
                        tr("docker.cs_swatch_desc")
                    )
                });
                if resp.double_clicked() {
                    app.apply_color_style(i, false);
                } else if resp.clicked() {
                    pick(app, ui, i);
                }
            }
        });
    }
}

/// What the picked styles can do: fill, outline, swap, merge, group,
/// ungroup, select their objects, delete.
fn picked_actions(app: &mut App, ui: &mut Ui) {
    let sel = app.color_styles_sel.clone();
    if sel.is_empty() {
        dim(ui, tr("docker.cs_pick_hint"));
        return;
    }
    let styles = app.doc().color_styles.clone();
    dim(
        ui,
        trf("docker.cs_picked", &[("n", &sel.len().to_string())]),
    );
    ui.horizontal_wrapped(|ui| {
        let button = |ui: &mut Ui, key: &str, on: bool| {
            ui.add_enabled(on, crate::ui::btn_icons::Ib::new(key).small())
                .rich_tip_always(keyed(key))
                .clicked()
        };
        if button(ui, "docker.cs_fill_picked", true) {
            app.apply_color_style(sel[0], false);
        }
        if button(ui, "docker.cs_outline_picked", true) {
            app.apply_color_style(sel[0], true);
        }
        if button(ui, "docker.cs_swap", sel.len() == 2) {
            if let Some(c) = cs::swap(&styles, sel[0], sel[1]) {
                app.change_color_styles(c);
            }
        }
        if button(ui, "docker.cs_merge", sel.len() >= 2) {
            if let Some(c) = cs::merge(&styles, &sel) {
                app.color_styles_sel.clear();
                app.change_color_styles(c);
            }
        }
        if button(ui, "docker.cs_group", true) {
            let name = cs::fresh_name(&tr("docker.cs_harmony"), &cs::harmonies(&styles));
            let c = cs::group(&styles, &sel, &name);
            app.change_color_styles(c);
            app.color_styles_sel = cs::members(&app.doc().color_styles, &name);
        }
        let grouped = sel.iter().any(|&i| styles[i].harmony.is_some());
        if button(ui, "docker.cs_ungroup", grouped) {
            app.change_color_styles(cs::ungroup(&styles, &sel));
        }
        if button(ui, "docker.cs_select_objects", true) {
            let n = app.select_color_style_objects(&sel);
            app.status = trf("status.cs_selected", &[("n", &n.to_string())]);
        }
        if button(ui, "docker.cs_delete_picked", true) {
            app.color_styles_sel.clear();
            app.change_color_styles(cs::delete(&styles, &sel));
        }
    });
}

/// The picked styles changed together: the harmony wheel, the rule of
/// a harmony picked whole, and hue, saturation and brightness.
fn adjust(app: &mut App, ui: &mut Ui) {
    let sel = app.color_styles_sel.clone();
    if sel.is_empty() {
        return;
    }
    let styles = app.doc().color_styles.clone();
    let mut a = ui.data(|d| d.get_temp::<Adjust>(adjust_id()).unwrap_or_default());
    if a.pick != sel {
        a = Adjust {
            pick: sel.clone(),
            ..Default::default()
        };
    }
    ui.strong(tr("docker.cs_adjust"));
    ui.horizontal(|ui| {
        let turned = wheel(ui, &styles, &sel);
        ui.vertical(|ui| {
            // A harmony picked whole: its rule.
            let whole: Option<String> = styles[sel[0]].harmony.clone().filter(|h| {
                let m = cs::members(&styles, h);
                m.len() == sel.len() && m.iter().all(|i| sel.contains(i))
            });
            if let Some(h) = &whole {
                let mut rule = None;
                egui::ComboBox::from_id_salt("cs_rule")
                    .selected_text(tr("docker.cs_rule"))
                    .show_ui(ui, |ui| {
                        for r in crate::palette::Harmony::ALL {
                            if ui
                                .selectable_label(false, tr(r.key()))
                                .rich_tip_with(|| {
                                    format!("{}\n{}", tr(r.key()), tr(&format!("{}_desc", r.key())))
                                })
                                .clicked()
                            {
                                rule = Some(r);
                            }
                        }
                    })
                    .response
                    .rich_tip(keyed("docker.cs_rule"));
                if let Some(r) = rule {
                    app.change_color_styles(cs::set_rule(&styles, h, r));
                    app.color_styles_sel = cs::members(&app.doc().color_styles, h);
                }
            }
            let slider =
                |ui: &mut Ui, v: &mut f64, range: std::ops::RangeInclusive<f64>, key: &str| {
                    let before = *v;
                    let r = ui
                        .add(egui::Slider::new(v, range).text(tr(key)))
                        .rich_tip(keyed(key));
                    (r, *v - before)
                };
            let (rh, dh) = slider(ui, &mut a.hue, -180.0..=180.0, "docker.cs_hue");
            let (rs, ds) = slider(ui, &mut a.sat, -100.0..=100.0, "docker.cs_saturation");
            let (rb, db) = slider(ui, &mut a.bright, -100.0..=100.0, "docker.cs_brightness");
            let started = rh.drag_started() || rs.drag_started() || rb.drag_started();
            let stopped = rh.drag_stopped() || rs.drag_stopped() || rb.drag_stopped();
            let dh = dh + turned.map_or(0.0, |t| t.0);
            if turned.is_some_and(|t| t.1) || started {
                a.mark = Some(app.engine.begin_group());
            }
            if dh != 0.0 || ds != 0.0 || db != 0.0 {
                let c = cs::adjust(&styles, &sel, dh, ds / 100.0, db / 100.0);
                app.change_color_styles(c);
            }
            if turned.is_some_and(|t| t.2) || stopped {
                if let Some(mark) = a.mark.take() {
                    app.engine.end_group(mark, "Color Styles");
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(adjust_id(), a));
}

/// The hue wheel with a dot for each picked colour; dragging turns them
/// all round it. Returns the degrees turned this frame, and whether the
/// drag started or stopped.
fn wheel(ui: &mut Ui, styles: &[ColorStyle], sel: &[usize]) -> Option<(f64, bool, bool)> {
    let d = 96.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(d, d), Sense::drag());
    let resp = resp.rich_tip(keyed("docker.cs_wheel"));
    let p = ui.painter();
    let c = rect.center();
    let r = d * 0.5 - 2.0;
    // The ring of hues.
    let n = 72;
    for k in 0..n {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let at = |a: f32, rr: f32| c + egui::vec2(a.cos(), -a.sin()) * rr;
        let col = c32(Color::from_hsb(a0.to_degrees() as f64, 1.0, 1.0));
        p.add(egui::Shape::convex_polygon(
            vec![at(a0, r), at(a1, r), at(a1, r * 0.55), at(a0, r * 0.55)],
            col,
            egui::Stroke::NONE,
        ));
    }
    for &i in sel {
        let (h, s, _) = styles[i].color.to_hsb();
        let a = (h as f32).to_radians();
        let rr = r * (0.2 + 0.8 * s as f32);
        let at = c + egui::vec2(a.cos(), -a.sin()) * rr;
        p.line_segment([c, at], egui::Stroke::new(1.0, Tokens::text_dim()));
        p.circle_filled(at, 5.0, c32(styles[i].color));
        p.circle_stroke(at, 5.0, egui::Stroke::new(1.5, Color32::WHITE));
        p.circle_stroke(at, 6.5, egui::Stroke::new(1.0, Color32::BLACK));
    }
    if !resp.dragged() && !resp.drag_started() && !resp.drag_stopped() {
        return None;
    }
    let angle = |pos: egui::Pos2| {
        let v = pos - c;
        (-v.y).atan2(v.x).to_degrees() as f64
    };
    let turned = match ui.input(|i| i.pointer.interact_pos()) {
        Some(now) => {
            let before = now - resp.drag_delta();
            let mut t = angle(now) - angle(before);
            if t > 180.0 {
                t -= 360.0;
            } else if t < -180.0 {
                t += 360.0;
            }
            t
        }
        None => 0.0,
    };
    Some((turned, resp.drag_started(), resp.drag_stopped()))
}

/// New gradient: shades of the first picked style (or the Color panel's
/// colour) as a new harmony.
fn gradient_row(app: &mut App, ui: &mut Ui) {
    let id = egui::Id::new("color_styles_gradient");
    let (mut steps, mut spread) = ui.data(|d| d.get_temp::<(usize, f64)>(id).unwrap_or((5, 0.6)));
    ui.horizontal_wrapped(|ui| {
        let base = app
            .color_styles_sel
            .first()
            .and_then(|&i| app.doc().color_styles.get(i).map(|s| s.color))
            .unwrap_or(app.mixer_color);
        if ui
            .ibutton("docker.cs_new_gradient")
            .rich_tip(keyed("docker.cs_new_gradient"))
            .clicked()
        {
            let h = app.add_color_gradient(base, steps, spread);
            app.color_styles_sel = cs::members(&app.doc().color_styles, &h);
        }
        crate::ui::tips::tip_over(ui, keyed("docker.cs_steps"), |ui| {
            ui.label(tr("docker.cs_steps"));
            ui.add(egui::DragValue::new(&mut steps).range(2..=20));
        });
        crate::ui::tips::tip_over(ui, keyed("docker.cs_spread"), |ui| {
            ui.label(tr("docker.cs_spread"));
            let mut pct = spread * 100.0;
            ui.add(
                egui::DragValue::new(&mut pct)
                    .range(0.0..=100.0)
                    .suffix("%"),
            );
            spread = pct / 100.0;
        });
    });
    ui.data_mut(|d| d.insert_temp(id, (steps, spread)));
}

/// The harmony rules: a harmony of the base colour, as new styles.
fn harmony_editor(app: &mut App, ui: &mut Ui) {
    ui.strong(tr("docker.harmony_editor"));
    ui.horizontal_wrapped(|ui| {
        for h in crate::palette::Harmony::ALL {
            if ui
                .ibutton(h.key())
                .rich_tip_with(|| format!("{}\n{}", tr(h.key()), tr(&format!("{}_desc", h.key()))))
                .clicked()
            {
                let base = app.mixer_color;
                let mut st = app.doc().color_styles.clone();
                let taken = cs::harmonies(&st);
                let name = tr(h.key());
                let name = if taken.contains(&name) {
                    cs::fresh_name(&name, &taken)
                } else {
                    name
                };
                for (k, c) in h.colors(base).into_iter().enumerate() {
                    st.push(ColorStyle {
                        name: format!("{name} {}", k + 1),
                        color: c,
                        harmony: Some(name.clone()),
                    });
                }
                app.run(Command::SetColorStyles { styles: st });
                app.color_styles_sel = cs::members(&app.doc().color_styles, &name);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("docker.base_color"));
        let mut c = app.mixer_color;
        if crate::ui::color_picker::color_button(ui, &mut c) {
            app.mixer_color = c;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &egui::Context, app: &mut App) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| show(app, ui));
        out.textures_delta.clear();
    }

    /// The panel draws its views, a harmony picked whole (with its rule
    /// list and the wheel) and a loose pick, without changing anything.
    #[test]
    fn the_panel_draws_every_view_and_pick() {
        let mut app = App::headless();
        let style = |n: &str, c: Color, h: Option<&str>| ColorStyle {
            name: n.into(),
            color: c,
            harmony: h.map(|s| s.to_string()),
        };
        let styles = vec![
            style("A", Color::rgb8(200, 0, 0), None),
            style("B", Color::cmyk_pct(0.0, 50.0, 100.0, 0.0), Some("H")),
            style("C", Color::rgb8(0, 0, 200), Some("H")),
        ];
        app.run(Command::SetColorStyles {
            styles: styles.clone(),
        });
        let ctx = crate::theme::ui_context();
        for view in [View::List, View::Swatches] {
            ctx.data_mut(|d| d.insert_temp(view_id(), view));
            for pick in [vec![], vec![1, 2], vec![0, 2]] {
                app.color_styles_sel = pick.clone();
                frame(&ctx, &mut app);
                frame(&ctx, &mut app);
                assert_eq!(app.color_styles_sel, pick);
            }
        }
        assert_eq!(app.doc().color_styles, styles);
        assert_eq!(groups(&styles), vec![(None, vec![0]), (Some("H".into()), vec![1, 2])]);
    }
}
