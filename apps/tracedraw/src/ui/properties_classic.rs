//! The Properties docker's classic layout: one section at a time, picked
//! by the row of tabs at the top, under a plain bold title, with the
//! outline's settings in short rows whose labels line up on the right:
//! the paint as a swatch with its editor in a popup, the width as a box
//! with a list of widths, the line style as a list of dash pictures,
//! then (behind a fold) the mitre limit, corners, line caps and
//! position, the arrowheads with their arrows, and the calligraphic
//! nib. The grouped layout stays the default; a button at the end of
//! the tab row switches between the two.

use crate::ui::btn_icons::IconUi;
use super::*;

/// The width of the right-aligned label column of the classic outline.
const LABEL_W: f32 = 78.0;

/// Where the Properties docker keeps, while it draws, whether it is in
/// the classic layout: the editors it shares with other panels (the
/// fill's colour, the row labels) look at it.
fn active_id() -> egui::Id {
    egui::Id::new("properties_classic_active")
}

/// Whether the Properties docker is drawing in the classic layout.
pub(super) fn active(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(active_id()).unwrap_or(false))
}

/// Set for the Properties docker's drawing, cleared after it.
pub(super) fn set_active(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(active_id(), on));
}

/// The colour picker laid open in the section, as the classic layout
/// shows a uniform fill: the colour area and hue strip, the model with
/// the eyedropper, the model's sliders, the hex code and the colour's
/// description. True when the colour changed.
pub(super) fn inline_color(ui: &mut Ui, c: &mut Color) -> bool {
    use crate::ui::color_picker as cp;
    let id = ui.id().with("classic_inline_color");
    let mut changed = false;
    // The hue kept apart, so a grey keeps the hue it was given; a colour
    // changed elsewhere (another object picked) starts from its own.
    let kept = ui.data(|d| d.get_temp::<(f32, Color)>(id.with("hue")));
    let mut hue = match kept {
        Some((h, was)) if was == *c => h,
        _ => cp::viewer_point(*c).0,
    };
    let model = cp::edit_model(*c);
    if let Some((h, s, b)) = crate::ui::color_docker::viewer(ui, hue, *c) {
        hue = h;
        *c = cp::from_viewer(h, s, b, model);
        changed = true;
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        right_label(ui, &tr("docker.model"), LABEL_W);
        changed |= cp::model_list(ui, id.with("model"), c);
        changed |= cp::eyedropper(ui, id, c);
    });
    let model = cp::edit_model(*c);
    let mut m = c.convert_to(model);
    if !cp::shows_hex(ui.ctx(), id.with("model"), *c)
        && crate::ui::color_docker::sliders(ui, &mut m)
    {
        *c = m;
        changed = true;
    }
    if cp::hex_field(ui, id, c) {
        hue = cp::viewer_point(*c).0;
        changed = true;
    }
    ui.label(
        egui::RichText::new(crate::app::color_description(*c))
            .color(Tokens::text_dim())
            .size(11.0),
    );
    ui.data_mut(|d| d.insert_temp(id.with("hue"), (hue, *c)));
    changed
}

/// A label right-aligned in a column `w` wide, so its control starts
/// at the same place on every row.
pub(super) fn right_label(ui: &mut Ui, text: &str, w: f32) {
    let text = label_text(text);
    ui.allocate_ui_with_layout(
        egui::vec2(w, 22.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.label(text);
        },
    );
}

/// A row label as shown: without the colon some have, then one colon.
fn label_text(text: &str) -> String {
    format!("{}:", text.trim_end_matches([':', '\u{ff1a}']))
}

/// The width of the label column for `labels`: at least [`LABEL_W`],
/// and wide enough for the longest one, so none runs off the left.
pub(super) fn label_column(ui: &Ui, labels: &[String]) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    labels
        .iter()
        .map(|t| {
            ui.painter()
                .layout_no_wrap(label_text(t), font.clone(), Tokens::text())
                .size()
                .x
                + 2.0
        })
        .fold(LABEL_W, f32::max)
}

/// The section a classic Properties docker shows: the one last picked if
/// the selection has it, else the first one.
pub(super) fn shown_tab<'a>(app: &App, sections: &[&'a str]) -> Option<&'a str> {
    let want = app.settings.properties_tab.as_str();
    sections
        .iter()
        .find(|s| **s == want)
        .or_else(|| sections.first())
        .copied()
}

/// The button at the end of the section row that switches between the
/// grouped layout and the classic one; a picture of a panel with tabs.
pub(super) fn layout_button(app: &mut App, ui: &mut Ui) {
    let on = app.settings.properties_classic;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("properties_layout_button"), rect));
    crate::theme::paint_icon_button(ui.painter(), rect, resp.hovered(), on);
    let r = egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 14.0));
    let ink = egui::Stroke::new(1.2, Tokens::icon());
    let p = ui.painter();
    p.rect_stroke(r, 1.0, ink, egui::StrokeKind::Inside);
    // Three tabs along the top, the first one drawn as picked.
    let tab_w = r.width() / 3.0;
    p.rect_filled(
        egui::Rect::from_min_size(r.min, egui::vec2(tab_w, 4.0)),
        0.0,
        Tokens::icon(),
    );
    p.line_segment(
        [
            egui::pos2(r.left(), r.top() + 4.0),
            egui::pos2(r.right(), r.top() + 4.0),
        ],
        ink,
    );
    for i in 1..3 {
        let x = r.left() + tab_w * i as f32;
        p.line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.top() + 4.0)], ink);
    }
    // Rows of settings with their labels.
    for i in 0..3 {
        let y = r.top() + 7.0 + i as f32 * 2.5;
        p.line_segment(
            [egui::pos2(r.left() + 3.0, y), egui::pos2(r.left() + 6.0, y)],
            ink,
        );
        p.line_segment(
            [
                egui::pos2(r.left() + 8.0, y),
                egui::pos2(r.right() - 3.0, y),
            ],
            ink,
        );
    }
    if resp
        .rich_tip(crate::ui::tips::pair(
            "docker.classic_layout",
            "docker.classic_layout_desc",
        ))
        .clicked()
    {
        app.settings.properties_classic = !on;
        app.settings.save();
    }
}

/// A classic section's title: the section's name in bold, as a page
/// title rather than a bar that folds.
pub(super) fn title(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(text).font(crate::theme::bold(13.0)));
    ui.add_space(2.0);
}

/// A swatch button of the outline's paint: its colour, or a red slash
/// for none; `mixed` draws it empty.
fn paint_swatch(ui: &mut Ui, paint: Option<&Fill>, mixed: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(58.0, 24.0), egui::Sense::click());
    crate::theme::paint_icon_button(ui.painter(), rect, resp.hovered(), false);
    let p = ui.painter();
    let sw = egui::Rect::from_min_max(
        rect.left_top() + egui::vec2(4.0, 4.0),
        egui::pos2(rect.right() - 16.0, rect.bottom() - 4.0),
    );
    match paint.filter(|_| !mixed) {
        Some(f) if !matches!(f, Fill::None) => {
            let c = crate::app::fill_preview_color(f).unwrap_or(egui::Color32::GRAY);
            p.rect_filled(sw, 0.0, c);
        }
        Some(_) => {
            p.rect_filled(sw, 0.0, egui::Color32::WHITE);
            p.line_segment(
                [sw.left_bottom(), sw.right_top()],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(220, 40, 40)),
            );
        }
        None => {}
    }
    p.rect_stroke(
        sw,
        0.0,
        egui::Stroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Outside,
    );
    drop_arrow(p, egui::pos2(rect.right() - 8.0, rect.center().y));
    resp
}

/// The small down arrow of a button that opens a list.
fn drop_arrow(p: &egui::Painter, c: egui::Pos2) {
    p.add(egui::epaint::PathShape::convex_polygon(
        vec![
            c + egui::vec2(-3.5, -2.0),
            c + egui::vec2(3.5, -2.0),
            c + egui::vec2(0.0, 2.0),
        ],
        Tokens::icon(),
        egui::Stroke::NONE,
    ));
}

/// The arrow picture in front of an arrowhead's list: pointing left for
/// the start, right for the end.
fn arrow_pic(ui: &mut Ui, end: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 22.0), egui::Sense::hover());
    let c = rect.center();
    let d = if end { 1.0 } else { -1.0 };
    let p = ui.painter();
    p.add(egui::epaint::PathShape::convex_polygon(
        vec![
            c + egui::vec2(8.0 * d, 0.0),
            c + egui::vec2(0.0, -7.0),
            c + egui::vec2(0.0, 7.0),
        ],
        Tokens::icon(),
        egui::Stroke::NONE,
    ));
    p.rect_filled(
        egui::Rect::from_two_pos(c + egui::vec2(0.0, -3.0), c + egui::vec2(-8.0 * d, 3.0)),
        0.0,
        Tokens::icon(),
    );
}

/// A small angle: two lines from a corner with an arc between them,
/// before the nib's angle box.
fn angle_pic(ui: &mut Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), egui::Sense::hover());
    let ink = egui::Stroke::new(1.2, Tokens::icon());
    let o = egui::pos2(rect.left() + 1.0, rect.bottom() - 4.0);
    let p = ui.painter();
    p.line_segment([o, o + egui::vec2(12.0, 0.0)], ink);
    p.line_segment([o, o + egui::vec2(9.0, -9.0)], ink);
    let arc: Vec<egui::Pos2> = (0..=6)
        .map(|i| {
            let a = -(i as f32) * std::f32::consts::FRAC_PI_4 / 6.0;
            o + egui::vec2(a.cos(), a.sin()) * 7.0
        })
        .collect();
    p.add(egui::Shape::line(arc, ink));
    resp
}

/// The dash patterns the line style list offers, with their names.
fn standard_dashes() -> Vec<(Vec<f64>, &'static str)> {
    vec![
        (vec![], "docker.dash_solid"),
        (vec![4.0, 2.0], "docker.dash_dashed"),
        (vec![1.0, 1.0], "docker.dash_dotted"),
        (vec![6.0, 2.0, 1.0, 2.0], "docker.dash_dash_dot"),
        (
            tracedraw_core::Stroke::ROUND_DOTS.to_vec(),
            "docker.dash_round_dots",
        ),
    ]
}

/// A width picked from the classic outline's list: `None` takes the
/// outline away; a width sets it, giving an object without an outline
/// one in `black`.
pub(super) fn pick_width(
    stroke: &mut Option<Stroke>,
    w: Option<f64>,
    black: tracedraw_core::Color,
) {
    match w {
        None => *stroke = None,
        Some(w) => {
            stroke
                .get_or_insert_with(|| Stroke {
                    color: black,
                    ..Stroke::default()
                })
                .width = w;
        }
    }
}

/// The id of the fold of the classic outline's less used settings.
fn more_id() -> egui::Id {
    egui::Id::new("properties_classic_outline_more")
}

/// The Outline section in the classic layout; the same edits as
/// [`outline_editor_mixed`], reported the same way.
pub(super) fn outline_editor(
    ui: &mut Ui,
    stroke: &mut Option<Stroke>,
    lib: &crate::ui::line_style::OutlineLib,
    mix: &StrokeMix,
    touched: &mut Vec<StrokeField>,
) -> bool {
    let mut changed = false;
    let u = lib.units;

    // The paint: a swatch whose popup holds every kind of paint.
    let paint_mixed = mix.presence || mix.has(StrokeField::Paint);
    let shown = stroke.as_ref().map(outline_paint_of);
    let swatch = ui
        .horizontal(|ui| {
            paint_swatch(ui, Some(shown.as_ref().unwrap_or(&Fill::None)), paint_mixed)
                .rich_tip(crate::ui::tips::keyed("docker.outline_color"))
        })
        .inner;
    let paint_popup = ui.id().with("classic_outline_paint");
    if swatch.clicked() {
        egui::Popup::toggle_id(ui.ctx(), paint_popup);
    }
    let ctx = ui.ctx().clone();
    egui::Popup::new(paint_popup, ctx, swatch.rect, ui.layer_id())
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(250.0);
            let mut paint = shown.clone().unwrap_or(Fill::None);
            let paint_mix = if paint_mixed {
                FillMix {
                    kind: false,
                    fields: vec![FillField::All],
                }
            } else {
                FillMix::default()
            };
            let mut paint_touched = Vec::new();
            let edited = ui
                .push_id("outline_paint", |ui| {
                    fill_editor_mixed(ui, &mut paint, &paint_mix, &mut paint_touched, None)
                })
                .inner;
            if edited {
                changed = true;
                touched.push(StrokeField::Paint);
                match stroke.as_mut() {
                    Some(s) => {
                        if set_outline_paint(s, paint) {
                            *stroke = None;
                        }
                    }
                    None if !matches!(paint, Fill::None) => {
                        let mut s = Stroke {
                            color: lib.black,
                            ..Stroke::default()
                        };
                        set_outline_paint(&mut s, paint);
                        *stroke = Some(s);
                    }
                    None => {}
                }
            }
            if let Some(s) = stroke.as_mut() {
                if matches!(s.fill.as_deref(), Some(Fill::Fountain(_)))
                    && ui
                        .add(
                            egui::Checkbox::new(&mut s.fill_along, tr("docker.outline_fill_along"))
                                .indeterminate(mix.has(StrokeField::Paint)),
                        )
                        .rich_tip(crate::ui::tips::keyed("docker.outline_fill_along"))
                        .changed()
                {
                    changed = true;
                    touched.push(StrokeField::Paint);
                }
            }
        });

    // The width: a box, the list of widths (None takes the outline away,
    // Hairline the thinnest line), and the unit.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let width_tip = crate::ui::tips::pair("toolbar.outline_width", "docker.outline_width_desc");
        let width_mixed = mix.presence || mix.has(StrokeField::Width);
        match stroke.as_mut() {
            Some(s) if s.width > Stroke::HAIRLINE + 1e-9 || width_mixed => {
                let mut v = u.from_mm(s.width);
                if ui
                    .add(
                        crate::ui::field::NumField::new(&mut v)
                            .range(u.from_mm(0.001)..=u.from_mm(1000.0))
                            .unit_mm(u.mm())
                            .max_decimals(3)
                            .width(80.0)
                            .mixed(width_mixed),
                    )
                    .rich_tip(width_tip.clone())
                    .changed()
                {
                    s.width = u.to_mm(v).clamp(0.001, 1000.0);
                    touched.push(StrokeField::Width);
                    changed = true;
                }
            }
            other => {
                let text = if other.is_some() {
                    tr("toolbar.hairline")
                } else {
                    tr("toolbar.none")
                };
                ui.add_sized(
                    egui::vec2(80.0, 22.0),
                    egui::Label::new(text).selectable(false),
                )
                .rich_tip(width_tip.clone());
            }
        }
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 22.0), egui::Sense::click());
        crate::theme::paint_icon_button(ui.painter(), rect, resp.hovered(), false);
        drop_arrow(ui.painter(), rect.center());
        let list = ui.id().with("classic_outline_widths");
        if resp.rich_tip(width_tip).clicked() {
            egui::Popup::toggle_id(ui.ctx(), list);
        }
        let ctx = ui.ctx().clone();
        egui::Popup::new(list, ctx, rect, ui.layer_id())
            .open_memory(None)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
            .show(|ui| {
                ui.set_min_width(110.0);
                let current = stroke.as_ref().map(|s| s.width);
                let mut pick: Option<Option<f64>> = None;
                if ui
                    .iselectable(!width_mixed && current.is_none(), "toolbar.none")
                    .clicked()
                {
                    pick = Some(None);
                }
                let hair = current.is_some_and(|w| w <= Stroke::HAIRLINE + 1e-9);
                if ui
                    .iselectable(!width_mixed && hair, "toolbar.hairline")
                    .clicked()
                {
                    pick = Some(Some(Stroke::HAIRLINE));
                }
                for w in crate::ui::propbar::outline_width_presets(u) {
                    let on = !width_mixed && current.is_some_and(|c| (c - w).abs() < 1e-6);
                    if ui
                        .selectable_label(on, crate::ui::propbar::outline_width_label(w, u))
                        .clicked()
                    {
                        pick = Some(Some(w));
                    }
                }
                if let Some(w) = pick {
                    pick_width(stroke, w, lib.black);
                    if w.is_some() {
                        touched.push(StrokeField::Width);
                    }
                    changed = true;
                }
            });
        if let Some(nu) = crate::ui::propbar::outline_unit_combo(ui, "outline_units_classic", u) {
            crate::ui::line_style::request_units(ui, nu);
        }
    });

    let Some(s) = stroke.as_mut() else {
        return changed;
    };
    let choice_tip = |n: &str| {
        format!(
            "{}\n{}. {}",
            tr(n),
            tr(&format!("{n}_tip")),
            tr(&format!("{n}_desc"))
        )
    };

    // The line style: a list of dash pictures, and its editor.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let dash_mixed = mix.has(StrokeField::Dash);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(160.0, 22.0), egui::Sense::click());
        crate::theme::paint_icon_button(ui.painter(), rect, resp.hovered(), false);
        if !dash_mixed {
            crate::ui::line_style::paint_dash(
                ui.painter(),
                egui::Rect::from_min_max(
                    rect.left_top() + egui::vec2(6.0, 6.0),
                    egui::pos2(rect.right() - 20.0, rect.bottom() - 6.0),
                ),
                &s.dash,
            );
        }
        drop_arrow(
            ui.painter(),
            egui::pos2(rect.right() - 9.0, rect.center().y),
        );
        let list = ui.id().with("classic_outline_dashes");
        if resp
            .rich_tip(crate::ui::tips::keyed("docker.dash"))
            .clicked()
        {
            egui::Popup::toggle_id(ui.ctx(), list);
        }
        let ctx = ui.ctx().clone();
        egui::Popup::new(list, ctx, rect, ui.layer_id())
            .open_memory(None)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
            .show(|ui| {
                let mut all: Vec<(Vec<f64>, String)> = standard_dashes()
                    .into_iter()
                    .map(|(d, n)| (d, tr(n)))
                    .collect();
                all.extend(
                    lib.dashes
                        .iter()
                        .map(|d| (d.clone(), tr("docker.dash_custom"))),
                );
                for (i, (d, name)) in all.into_iter().enumerate() {
                    let (r, resp) =
                        ui.allocate_exact_size(egui::vec2(160.0, 20.0), egui::Sense::click());
                    let on = !dash_mixed && s.dash == d;
                    crate::theme::paint_icon_button(ui.painter(), r, resp.hovered(), on);
                    crate::ui::line_style::paint_dash(
                        ui.painter(),
                        r.shrink2(egui::vec2(8.0, 6.0)),
                        &d,
                    );
                    if ui
                        .interact(r, ui.id().with(("dash_item", i)), egui::Sense::click())
                        .rich_tip(name)
                        .clicked()
                    {
                        if s.set_dash(&d) {
                            touched.push(StrokeField::Cap);
                        }
                        touched.push(StrokeField::Dash);
                        changed = true;
                    }
                }
            });
        if ui
            .small_button("•••")
            .rich_tip(crate::ui::tips::keyed("docker.edit_line_style"))
            .clicked()
        {
            crate::ui::line_style::request(ui);
        }
    });
    // How dashes meet at corners, as pictures.
    ui.horizontal(|ui| {
        use crate::ui::outline_pen::{pic_button_tipped, Pic};
        ui.spacing_mut().item_spacing.x = 2.0;
        let enabled = !s.dash.is_empty();
        let mixed = mix.has(StrokeField::AlignDashes);
        for (align, pic, key) in [
            (false, Pic::DashDefault, "outline_pen.dashes_default"),
            (true, Pic::DashAlign, "docker.align_dashes"),
        ] {
            let on = enabled && !mixed && s.align_dashes == align;
            if pic_button_tipped(ui, pic, on, enabled, &tr(key)) {
                s.align_dashes = align;
                touched.push(StrokeField::AlignDashes);
                changed = true;
            }
        }
    });

    // The fold: a triangle that shows or hides the rows below it.
    let mut more = ui.data_mut(|d| *d.get_persisted_mut_or(more_id(), true));
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::click());
    let c = rect.center();
    let pts = if more {
        vec![
            c + egui::vec2(-4.0, 2.0),
            c + egui::vec2(4.0, 2.0),
            c + egui::vec2(0.0, -2.5),
        ]
    } else {
        vec![
            c + egui::vec2(-4.0, -2.0),
            c + egui::vec2(4.0, -2.0),
            c + egui::vec2(0.0, 2.5),
        ]
    };
    let ink = if resp.hovered() {
        Tokens::text()
    } else {
        Tokens::icon()
    };
    ui.painter().add(egui::epaint::PathShape::convex_polygon(
        pts,
        ink,
        egui::Stroke::NONE,
    ));
    if resp
        .rich_tip(crate::ui::tips::keyed("docker.classic_more"))
        .clicked()
    {
        more = !more;
        ui.data_mut(|d| d.insert_persisted(more_id(), more));
    }
    if more {
        use crate::ui::outline_pen::{pic_button_tipped, Pic};
        let labels = [
            tr("docker.miter_limit"),
            tr("docker.corners"),
            tr("docker.caps"),
            tr("docker.outline_position"),
        ];
        let lw = label_column(ui, &labels);
        ui.add_enabled_ui(s.join == tracedraw_core::LineJoin::Miter, |ui| {
            ui.horizontal(|ui| {
                right_label(ui, &labels[0], lw);
                if ui
                    .add(
                        crate::ui::field::NumField::new(&mut s.miter_limit)
                            .range(Stroke::MITER_LIMIT_RANGE)
                            .speed(0.5)
                            .max_decimals(1)
                            .suffix("°")
                            .width(70.0)
                            .mixed(mix.has(StrokeField::MiterLimit)),
                    )
                    .rich_tip(tr("docker.miter_limit_tip"))
                    .changed()
                {
                    touched.push(StrokeField::MiterLimit);
                    changed = true;
                }
            });
        });
        ui.horizontal(|ui| {
            right_label(ui, &labels[1], lw);
            ui.spacing_mut().item_spacing.x = 2.0;
            let mixed = mix.has(StrokeField::Join);
            for (j, pic, n) in [
                (
                    tracedraw_core::LineJoin::Miter,
                    Pic::JoinMiter,
                    "docker.join_miter",
                ),
                (
                    tracedraw_core::LineJoin::Round,
                    Pic::JoinRound,
                    "docker.join_round",
                ),
                (
                    tracedraw_core::LineJoin::Bevel,
                    Pic::JoinBevel,
                    "docker.join_bevel",
                ),
            ] {
                if pic_button_tipped(ui, pic, !mixed && s.join == j, true, &choice_tip(n)) {
                    s.join = j;
                    touched.push(StrokeField::Join);
                    changed = true;
                }
            }
        });
        ui.horizontal(|ui| {
            right_label(ui, &labels[2], lw);
            ui.spacing_mut().item_spacing.x = 2.0;
            let mixed = mix.has(StrokeField::Cap);
            for (c, pic, n) in [
                (
                    tracedraw_core::LineCap::Butt,
                    Pic::CapButt,
                    "docker.cap_butt",
                ),
                (
                    tracedraw_core::LineCap::Round,
                    Pic::CapRound,
                    "docker.cap_round",
                ),
                (
                    tracedraw_core::LineCap::Square,
                    Pic::CapSquare,
                    "docker.cap_square",
                ),
            ] {
                if pic_button_tipped(ui, pic, !mixed && s.cap == c, true, &choice_tip(n)) {
                    s.cap = c;
                    touched.push(StrokeField::Cap);
                    changed = true;
                }
            }
        });
        ui.horizontal(|ui| {
            right_label(ui, &labels[3], lw);
            ui.spacing_mut().item_spacing.x = 2.0;
            for (pos, pic, n) in [
                (
                    tracedraw_core::OutlinePosition::Outside,
                    Pic::Outside,
                    "docker.position_outside",
                ),
                (
                    tracedraw_core::OutlinePosition::Center,
                    Pic::Center,
                    "docker.position_center",
                ),
                (
                    tracedraw_core::OutlinePosition::Inside,
                    Pic::Inside,
                    "docker.position_inside",
                ),
            ] {
                let on = !mix.has(StrokeField::Position) && s.position == pos;
                if pic_button_tipped(ui, pic, on, true, &choice_tip(n)) {
                    s.position = pos;
                    touched.push(StrokeField::Position);
                    changed = true;
                }
            }
        });
    }

    // The arrowheads: an arrow, the list, and the editor of each end.
    ui.separator();
    ui.label(tr("docker.group_arrowheads"));
    let custom = lib.arrows.as_slice();
    for (field, id, end) in [
        (StrokeField::StartArrow, "ClassicStart", false),
        (StrokeField::EndArrow, "ClassicEnd", true),
    ] {
        ui.horizontal(|ui| {
            arrow_pic(ui, end);
            let enabled = !end || !s.share_arrowheads;
            ui.add_enabled_ui(enabled, |ui| {
                let a = if end {
                    &mut s.end_arrow
                } else {
                    &mut s.start_arrow
                };
                if arrow_combo(ui, id, "", a, custom, mix.has(field)) {
                    if !end && s.share_arrowheads {
                        s.share_start_head();
                    }
                    touched.push(field);
                    changed = true;
                }
                if ui
                    .small_button("•••")
                    .rich_tip(crate::ui::tips::keyed("arrowhead_editor.edit"))
                    .clicked()
                {
                    crate::ui::arrowhead_editor::request(ui, end);
                }
            });
        });
    }
    ui.horizontal(|ui| {
        ui.add_space(22.0);
        if ui
            .add(
                egui::Checkbox::new(&mut s.share_arrowheads, tr("arrowhead_editor.share"))
                    .indeterminate(mix.has(StrokeField::ShareArrowheads)),
            )
            .rich_tip(crate::ui::tips::keyed("arrowhead_editor.share"))
            .changed()
        {
            if s.share_arrowheads {
                s.share_start_head();
            }
            touched.push(StrokeField::ShareArrowheads);
            changed = true;
        }
    });

    // The calligraphic nib: stretch and angle beside the nib itself,
    // dragged to turn and flatten it, and a button back to a round nib.
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(tr("outline_pen.calligraphy"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("\u{21ba}")
                .rich_tip(crate::ui::tips::keyed("docker.nib_reset"))
                .clicked()
            {
                s.stretch = 1.0;
                s.nib_angle = 0.0;
                touched.push(StrokeField::Stretch);
                touched.push(StrokeField::NibAngle);
                changed = true;
            }
        });
    });
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("\u{2194}").rich_tip(tr("docker.nib_stretch_tip"));
                let mut pct = s.stretch * 100.0;
                if ui
                    .add(
                        crate::ui::field::NumField::new(&mut pct)
                            .range(10.0..=100.0)
                            .speed(1.0)
                            .max_decimals(0)
                            .suffix(" %")
                            .width(70.0)
                            .mixed(mix.has(StrokeField::Stretch)),
                    )
                    .rich_tip(crate::ui::tips::pair(
                        "docker.nib_stretch_tip",
                        "docker.nib_stretch_desc",
                    ))
                    .changed()
                {
                    s.stretch = (pct / 100.0).clamp(0.1, 1.0);
                    touched.push(StrokeField::Stretch);
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                angle_pic(ui).rich_tip(tr("docker.nib_angle"));
                if ui
                    .add(
                        crate::ui::field::NumField::new(&mut s.nib_angle)
                            .range(-90.0..=90.0)
                            .speed(1.0)
                            .max_decimals(1)
                            .suffix("°")
                            .width(70.0)
                            .mixed(mix.has(StrokeField::NibAngle)),
                    )
                    .rich_tip(crate::ui::tips::pair(
                        "docker.nib_angle",
                        "docker.nib_angle_desc",
                    ))
                    .changed()
                {
                    touched.push(StrokeField::NibAngle);
                    changed = true;
                }
            });
        });
        ui.add_space(6.0);
        if let Some((stretch, angle)) =
            crate::ui::outline_pen::nib_picture(ui, s.stretch, s.nib_angle)
        {
            s.stretch = stretch;
            s.nib_angle = angle;
            touched.push(StrokeField::Stretch);
            touched.push(StrokeField::NibAngle);
            changed = true;
        }
    });
    for (field, key) in [
        (StrokeField::BehindFill, "docker.behind_fill"),
        (StrokeField::ScaleWithObject, "docker.scale_with_object"),
    ] {
        let v = if field == StrokeField::BehindFill {
            &mut s.behind_fill
        } else {
            &mut s.scale_with_object
        };
        if ui
            .add(egui::Checkbox::new(v, tr(key)).indeterminate(mix.has(field)))
            .rich_tip(crate::ui::tips::keyed(key))
            .changed()
        {
            touched.push(field);
            changed = true;
        }
    }
    changed
}
