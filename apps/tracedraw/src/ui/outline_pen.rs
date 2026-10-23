//! The Outline Pen dialog (F12, the Outline Pen tool, #357): every part of
//! the selected objects' outlines in one window, laid out in groups:
//! General (colour, width and its unit, style, mitre limit, dashes,
//! corners, line caps, position), Arrowheads, Calligraphy (stretch, angle,
//! a picture of the nib), Behind fill and Scale with object, Variable
//! Outline (the width nodes along the outline) and Overprint outline. OK
//! applies what was changed, in one undo step; with nothing selected it
//! sets the outline new objects get. See docs/behavior/outline-pen.md.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use egui::{Color32, Pos2, Rect, Sense, Stroke as EStroke, Ui, Vec2};
use tracedraw_core::patch::{StrokeField, StrokeMix};
use tracedraw_core::{LineCap, LineJoin, OutlinePosition, Stroke, WidthNode};

/// What the dialog keeps while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlinePenState {
    /// The outline as it was (the first object's, or the default).
    pub old: Option<Stroke>,
    /// The outline being edited.
    pub stroke: Stroke,
    /// The objects have an outline (unticked: OK takes it away).
    pub has: bool,
    /// How the selection's outlines differ.
    pub mix: StrokeMix,
    /// Fields set in the dialog (they apply even when they equal the
    /// value shown).
    pub touched: Vec<StrokeField>,
    /// Overprint outline as shown (`None`: the objects differ) and as
    /// it was.
    pub overprint: Option<bool>,
    pub overprint_old: Option<bool>,
    /// The width node picked in the Variable Outline group.
    pub node: Option<usize>,
    /// Nothing is selected: the dialog edits the default outline.
    pub defaults: bool,
}

/// Outline widths the width list offers, in points.
const WIDTHS_PT: [f64; 13] = [
    0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0, 24.0,
];

/// The dash styles the style list offers before the custom ones.
fn preset_dashes() -> [Vec<f64>; 4] {
    [
        vec![],
        vec![4.0, 2.0],
        vec![1.0, 1.0],
        vec![6.0, 2.0, 1.0, 2.0],
    ]
}

impl App {
    /// Open the Outline Pen dialog on the selection's outline (or the
    /// default outline when nothing is selected).
    pub fn open_outline_pen(&mut self) {
        let targets = self.paint_targets();
        let defaults = targets.is_empty();
        let old = match targets.first() {
            Some(s) => s.stroke.clone(),
            None => self.default_stroke(),
        };
        let mix = if defaults {
            StrokeMix::default()
        } else {
            self.selection_stroke_mix()
        };
        let overprint_old = if defaults {
            Some(false)
        } else {
            let on = targets.iter().filter(|s| s.overprint_outline).count();
            if on == 0 {
                Some(false)
            } else if on == targets.len() {
                Some(true)
            } else {
                None
            }
        };
        let lib = crate::ui::line_style::OutlineLib::of(self);
        let stroke = old.clone().unwrap_or(Stroke {
            color: lib.black,
            ..Stroke::default()
        });
        self.dialog = crate::ui::dialogs::Dialog::OutlinePen(Box::new(OutlinePenState {
            has: old.is_some(),
            old,
            stroke,
            mix,
            touched: Vec::new(),
            overprint: overprint_old,
            overprint_old,
            node: None,
            defaults,
        }));
    }

    /// OK in the Outline Pen dialog.
    pub fn apply_outline_pen(&mut self, st: &OutlinePenState) {
        let new = st.has.then(|| st.stroke.clone());
        if st.defaults {
            if new != st.old {
                self.request_default_change(crate::defaults::DefaultChange::Outline(new));
            }
            return;
        }
        let before = self.engine.last_step_id();
        self.patch_selection_stroke(&st.old, &new, &st.touched);
        if let Some(on) = st.overprint.filter(|_| st.overprint != st.overprint_old) {
            let ids: Vec<_> = self.paint_targets().iter().map(|s| s.id).collect();
            let cmd = tracedraw_core::Command::SetOverprint {
                shapes: ids,
                fill: None,
                outline: Some(on),
            };
            // One step with the outline change when there was one.
            if self.engine.last_step_id() != before {
                let _ = self.engine.amend(&cmd);
            } else {
                self.run(cmd);
            }
        }
    }
}

/// The dialog's window. True when it is to close.
pub fn dialog(app: &mut App, ctx: &egui::Context, st: &mut OutlinePenState, close: &mut bool) {
    let lib = crate::ui::line_style::OutlineLib::of(app);
    let mut ok = false;
    let mut help = false;
    crate::ui::dialogs::window(ctx, tr("outline_pen.title")).show(ctx, |ui| {
        ui.set_min_width(640.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(330.0);
                group(ui, &tr("outline_pen.general"), |ui| general(ui, st, &lib));
            });
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.set_width(290.0);
                group(ui, &tr("outline_pen.arrowheads"), |ui| {
                    arrowheads(ui, st, &lib)
                });
                group(ui, &tr("outline_pen.calligraphy"), |ui| calligraphy(ui, st));
            });
        });
        group(ui, &tr("outline_pen.variable_outline"), |ui| {
            variable(ui, st, &lib)
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            help = ui
                .add_sized([27.0, 27.0], egui::Button::new("?"))
                .on_hover_text(tr("dialog.help"))
                .clicked();
            if !st.defaults {
                let mut on = st.overprint.unwrap_or(false);
                if ui
                    .add(
                        egui::Checkbox::new(&mut on, tr("outline_pen.overprint"))
                            .indeterminate(st.overprint.is_none()),
                    )
                    .changed()
                {
                    st.overprint = Some(on);
                }
            }
            ok = crate::ui::dialogs::ok_cancel(ui, close);
        });
    });
    if help {
        app.open_dialog_help();
    }
    if ok {
        app.apply_outline_pen(st);
    }
}

/// A titled group with a light frame around it.
fn group(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    crate::ui::chrome::section(ui, title);
    egui::Frame::new()
        .stroke(EStroke::new(1.0, Tokens::border()))
        .inner_margin(egui::Margin::same(8))
        .corner_radius(3.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui)
        });
}

/// Mark `field` as set and the outline as present.
fn touch(st: &mut OutlinePenState, field: StrokeField) {
    if !st.touched.contains(&field) {
        st.touched.push(field);
    }
    st.has = true;
}

fn row_label(ui: &mut Ui, key: &str) {
    ui.add_sized(
        [86.0, 20.0],
        egui::Label::new(egui::RichText::new(tr(key)).color(Tokens::text())),
    );
}

fn general(ui: &mut Ui, st: &mut OutlinePenState, lib: &crate::ui::line_style::OutlineLib) {
    let mixed = |st: &OutlinePenState, f: StrokeField| st.mix.has(f) && !st.touched.contains(&f);
    // Colour.
    ui.horizontal(|ui| {
        row_label(ui, "outline_pen.color");
        let mut c = st.stroke.color;
        if crate::ui::color_picker::color_button(ui, &mut c) {
            st.stroke.color = c;
            st.stroke.fill = None;
            touch(st, StrokeField::Paint);
        }
    });
    // Width: a list with None, hairline and common widths, a box for any
    // width, and the unit.
    ui.horizontal(|ui| {
        row_label(ui, "outline_pen.width");
        let u = lib.units;
        let shown = if !st.has {
            tr("outline_pen.none")
        } else if mixed(st, StrokeField::Width) {
            String::new()
        } else if st.stroke.width <= Stroke::HAIRLINE + 1e-9 {
            tr("docker.hairline")
        } else {
            format!("{:.3}", u.from_mm(st.stroke.width))
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        };
        egui::ComboBox::from_id_salt("outline_pen_width")
            .selected_text(shown)
            .width(110.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(!st.has, tr("outline_pen.none"))
                    .clicked()
                {
                    st.has = false;
                }
                if ui.selectable_label(false, tr("docker.hairline")).clicked() {
                    st.stroke.width = Stroke::HAIRLINE;
                    touch(st, StrokeField::Width);
                }
                for pt in WIDTHS_PT {
                    let mm = pt * 25.4 / 72.0;
                    let v = u.from_mm(mm);
                    let label = format!("{v:.3}")
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string();
                    if ui.selectable_label(false, label).clicked() {
                        st.stroke.width = mm;
                        touch(st, StrokeField::Width);
                    }
                }
            });
        let mut v = u.from_mm(st.stroke.width);
        if ui
            .add(
                crate::ui::field::NumField::new(&mut v)
                    .range(u.from_mm(0.001)..=u.from_mm(1000.0))
                    .unit_mm(u.mm())
                    .max_decimals(4)
                    .width(60.0)
                    .mixed(mixed(st, StrokeField::Width) || !st.has),
            )
            .changed()
        {
            st.stroke.width = u.to_mm(v).clamp(0.001, 1000.0);
            touch(st, StrokeField::Width);
        }
        if let Some(nu) = crate::ui::propbar::outline_unit_combo(ui, "outline_pen_units", u) {
            crate::ui::line_style::request_units(ui, nu);
        }
    });
    // Style: the dash patterns drawn, then Edit Line Style.
    ui.horizontal(|ui| {
        row_label(ui, "outline_pen.style");
        let dashes: Vec<Vec<f64>> = preset_dashes()
            .into_iter()
            .chain(lib.dashes.iter().cloned())
            .collect();
        let resp = egui::ComboBox::from_id_salt("outline_pen_style")
            .selected_text("")
            .width(170.0)
            .show_ui(ui, |ui| {
                for d in &dashes {
                    let r = crate::ui::line_style::preview(ui, d, 150.0);
                    if st.stroke.dash == *d {
                        ui.painter().rect_stroke(
                            r.rect.expand(2.0),
                            0.0,
                            EStroke::new(1.0, Tokens::ACCENT),
                            egui::StrokeKind::Outside,
                        );
                    }
                    if r.clicked() {
                        st.stroke.dash = d.clone();
                        touch(st, StrokeField::Dash);
                    }
                }
            });
        // The style in use, drawn over the closed list.
        if !mixed(st, StrokeField::Dash) {
            let r = resp.response.rect.shrink2(Vec2::new(8.0, 0.0));
            let r = Rect::from_min_max(r.min, Pos2::new(r.max.x - 18.0, r.max.y));
            dash_line(ui.painter(), r, &st.stroke.dash);
        }
        if ui
            .button("\u{2026}")
            .on_hover_text(tr("docker.edit_line_style"))
            .clicked()
        {
            crate::ui::line_style::request(ui);
        }
    });
    // Mitre limit.
    ui.horizontal(|ui| {
        row_label(ui, "docker.miter_limit");
        let limit_mixed = mixed(st, StrokeField::MiterLimit);
        if ui
            .add_enabled(
                st.stroke.join == LineJoin::Miter,
                crate::ui::field::NumField::new(&mut st.stroke.miter_limit)
                    .range(Stroke::MITER_LIMIT_RANGE)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix("°")
                    .width(70.0)
                    .mixed(limit_mixed),
            )
            .on_hover_text(tr("docker.miter_limit_tip"))
            .changed()
        {
            touch(st, StrokeField::MiterLimit);
        }
    });
    // Dashes: as they come, or fitted to the corners and ends.
    ui.horizontal(|ui| {
        row_label(ui, "outline_pen.dashes");
        let shown = !mixed(st, StrokeField::AlignDashes);
        for (align, pic, key) in [
            (false, Pic::DashDefault, "outline_pen.dashes_default"),
            (true, Pic::DashAlign, "docker.align_dashes"),
        ] {
            let on = shown && st.stroke.align_dashes == align;
            if pic_button(ui, pic, on, !st.stroke.dash.is_empty(), key) {
                st.stroke.align_dashes = align;
                touch(st, StrokeField::AlignDashes);
            }
        }
    });
    ui.horizontal(|ui| {
        row_label(ui, "docker.corners");
        let shown = !mixed(st, StrokeField::Join);
        for (j, pic, key) in [
            (LineJoin::Miter, Pic::JoinMiter, "docker.join_miter"),
            (LineJoin::Round, Pic::JoinRound, "docker.join_round"),
            (LineJoin::Bevel, Pic::JoinBevel, "docker.join_bevel"),
        ] {
            if pic_button(ui, pic, shown && st.stroke.join == j, true, key) {
                st.stroke.join = j;
                touch(st, StrokeField::Join);
            }
        }
    });
    ui.horizontal(|ui| {
        row_label(ui, "outline_pen.line_caps");
        let shown = !mixed(st, StrokeField::Cap);
        for (c, pic, key) in [
            (LineCap::Butt, Pic::CapButt, "docker.cap_butt"),
            (LineCap::Round, Pic::CapRound, "docker.cap_round"),
            (LineCap::Square, Pic::CapSquare, "docker.cap_square"),
        ] {
            if pic_button(ui, pic, shown && st.stroke.cap == c, true, key) {
                st.stroke.cap = c;
                touch(st, StrokeField::Cap);
            }
        }
    });
    ui.horizontal(|ui| {
        row_label(ui, "outline_pen.position");
        let shown = !mixed(st, StrokeField::Position);
        for (p, pic, key) in [
            (
                OutlinePosition::Outside,
                Pic::Outside,
                "docker.position_outside",
            ),
            (
                OutlinePosition::Center,
                Pic::Center,
                "docker.position_center",
            ),
            (
                OutlinePosition::Inside,
                Pic::Inside,
                "docker.position_inside",
            ),
        ] {
            if pic_button(ui, pic, shown && st.stroke.position == p, true, key) {
                st.stroke.position = p;
                touch(st, StrokeField::Position);
            }
        }
    });
}

fn arrowheads(ui: &mut Ui, st: &mut OutlinePenState, lib: &crate::ui::line_style::OutlineLib) {
    for (field, id, key) in [
        (
            StrokeField::StartArrow,
            "outline_pen_start",
            "docker.arrow_start",
        ),
        (StrokeField::EndArrow, "outline_pen_end", "docker.arrow_end"),
    ] {
        let mixed = st.mix.has(field) && !st.touched.contains(&field);
        let a = if field == StrokeField::StartArrow {
            &mut st.stroke.start_arrow
        } else {
            &mut st.stroke.end_arrow
        };
        if crate::ui::dockers::arrow_combo(ui, id, &tr(key), a, &lib.arrows, mixed) {
            touch(st, field);
        }
    }
}

fn calligraphy(ui: &mut Ui, st: &mut OutlinePenState) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                row_label(ui, "outline_pen.stretch");
                let mut pct = st.stroke.stretch * 100.0;
                if ui
                    .add(
                        crate::ui::field::NumField::new(&mut pct)
                            .range(10.0..=100.0)
                            .speed(1.0)
                            .max_decimals(0)
                            .suffix(" %")
                            .width(60.0)
                            .mixed(
                                st.mix.has(StrokeField::Stretch)
                                    && !st.touched.contains(&StrokeField::Stretch),
                            ),
                    )
                    .changed()
                {
                    st.stroke.stretch = (pct / 100.0).clamp(0.1, 1.0);
                    touch(st, StrokeField::Stretch);
                }
            });
            ui.horizontal(|ui| {
                row_label(ui, "outline_pen.angle");
                if ui
                    .add(
                        crate::ui::field::NumField::new(&mut st.stroke.nib_angle)
                            .range(-90.0..=90.0)
                            .speed(1.0)
                            .max_decimals(1)
                            .suffix("°")
                            .width(60.0)
                            .mixed(
                                st.mix.has(StrokeField::NibAngle)
                                    && !st.touched.contains(&StrokeField::NibAngle),
                            ),
                    )
                    .changed()
                {
                    touch(st, StrokeField::NibAngle);
                }
            });
            if ui.button(tr("outline_pen.default")).clicked() {
                st.stroke.stretch = 1.0;
                st.stroke.nib_angle = 0.0;
                touch(st, StrokeField::Stretch);
                touch(st, StrokeField::NibAngle);
            }
        });
        // The nib: a square stretched and turned as set; a drag on it
        // turns and stretches it.
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::drag());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, Color32::WHITE);
        p.rect_stroke(
            rect,
            2.0,
            EStroke::new(1.0, Tokens::border()),
            egui::StrokeKind::Inside,
        );
        if let Some(at) = resp.interact_pointer_pos().filter(|_| resp.dragged()) {
            let d = at - rect.center();
            if d.length() > 2.0 {
                let a = (-d.y).atan2(d.x).to_degrees() as f64;
                let a = ((a + 90.0).rem_euclid(180.0)) - 90.0;
                st.stroke.nib_angle = a.round();
                st.stroke.stretch = f64::from(d.length() / 28.0).clamp(0.1, 1.0);
                touch(st, StrokeField::NibAngle);
                touch(st, StrokeField::Stretch);
            }
        }
        let corners = nib_corners(rect.center(), 24.0, st.stroke.stretch, st.stroke.nib_angle);
        p.add(egui::Shape::convex_polygon(
            corners.to_vec(),
            Color32::BLACK,
            EStroke::NONE,
        ));
    });
    for (field, key) in [
        (StrokeField::BehindFill, "docker.behind_fill"),
        (StrokeField::ScaleWithObject, "docker.scale_with_object"),
    ] {
        let mixed = st.mix.has(field) && !st.touched.contains(&field);
        let v = if field == StrokeField::BehindFill {
            &mut st.stroke.behind_fill
        } else {
            &mut st.stroke.scale_with_object
        };
        if ui
            .add(egui::Checkbox::new(v, tr(key)).indeterminate(mixed))
            .changed()
        {
            touch(st, field);
        }
    }
}

/// The corners of the nib picture: a square of half side `r` at `c`,
/// its height times `stretch`, turned by `angle` degrees.
pub fn nib_corners(c: Pos2, r: f32, stretch: f64, angle: f64) -> [Pos2; 4] {
    let (s, co) = (angle.to_radians() as f32).sin_cos();
    let h = r * stretch.clamp(0.1, 1.0) as f32;
    [(-r, -h), (r, -h), (r, h), (-r, h)].map(|(x, y)| {
        // Screen y runs down: a positive angle turns counter-clockwise.
        Pos2::new(c.x + x * co + y * s, c.y - x * s + y * co)
    })
}

/// The Variable Outline group: the width nodes as ticks along a bar (a
/// click on the bar adds one, on a tick picks it), the picked node's
/// position and its width on each side and in all.
fn variable(ui: &mut Ui, st: &mut OutlinePenState, lib: &crate::ui::line_style::OutlineLib) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(360.0, 34.0), Sense::click());
    let p = ui.painter();
    let y = rect.center().y;
    p.line_segment(
        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
        EStroke::new(2.0, Tokens::text_dim()),
    );
    for k in 0..=4 {
        let x = rect.left() + rect.width() * k as f32 / 4.0;
        p.line_segment(
            [Pos2::new(x, y - 9.0), Pos2::new(x, y + 9.0)],
            EStroke::new(1.0, Tokens::text_dim()),
        );
    }
    let x_of = |t: f64| rect.left() + rect.width() * t.clamp(0.0, 1.0) as f32;
    for (i, n) in st.stroke.widths.iter().enumerate() {
        let c = Pos2::new(x_of(n.at), y);
        let picked = st.node == Some(i);
        p.circle_filled(
            c,
            5.0,
            if picked {
                Tokens::ACCENT
            } else {
                Color32::WHITE
            },
        );
        p.circle_stroke(c, 5.0, EStroke::new(1.2, Tokens::ACCENT));
    }
    if resp.clicked() {
        if let Some(at) = resp.interact_pointer_pos() {
            let hit = st
                .stroke
                .widths
                .iter()
                .position(|n| (x_of(n.at) - at.x).abs() <= 6.0);
            match hit {
                Some(i) => st.node = Some(i),
                None => {
                    let t = f64::from((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let (l, r) =
                        tracedraw_core::style::width_at(st.stroke.width, &st.stroke.widths, t);
                    st.stroke.widths.push(WidthNode {
                        at: t,
                        left: l,
                        right: r,
                    });
                    st.stroke.widths.sort_by(|a, b| a.at.total_cmp(&b.at));
                    st.node = st.stroke.widths.iter().position(|n| n.at == t);
                    touch(st, StrokeField::Widths);
                }
            }
        }
    }
    let u = lib.units;
    let node = st.node.filter(|i| *i < st.stroke.widths.len());
    ui.horizontal(|ui| {
        let n = node.map(|i| st.stroke.widths[i]);
        let mut pos = n.map_or(0.0, |n| n.at * 100.0);
        let mut side1 = u.from_mm(n.map_or(0.0, |n| n.left));
        let mut side2 = u.from_mm(n.map_or(0.0, |n| n.right));
        ui.label(tr("outline_pen.node_position"));
        let a = ui.add_enabled(
            n.is_some(),
            crate::ui::field::NumField::new(&mut pos)
                .range(0.0..=100.0)
                .max_decimals(1)
                .suffix(" %")
                .width(56.0),
        );
        ui.label(tr("outline_pen.side1"));
        let b = ui.add_enabled(
            n.is_some(),
            crate::ui::field::NumField::new(&mut side1)
                .range(0.0..=u.from_mm(500.0))
                .max_decimals(3)
                .width(56.0),
        );
        ui.label(tr("outline_pen.side2"));
        let c = ui.add_enabled(
            n.is_some(),
            crate::ui::field::NumField::new(&mut side2)
                .range(0.0..=u.from_mm(500.0))
                .max_decimals(3)
                .width(56.0),
        );
        let mut total = side1 + side2;
        ui.label(tr("outline_pen.total"));
        let d = ui.add_enabled(
            n.is_some(),
            crate::ui::field::NumField::new(&mut total)
                .range(0.0..=u.from_mm(1000.0))
                .max_decimals(3)
                .width(56.0),
        );
        ui.label(u.short());
        if let Some(i) = node {
            let m = &mut st.stroke.widths[i];
            let mut changed = false;
            if a.changed() {
                m.at = pos / 100.0;
                changed = true;
            }
            if b.changed() {
                m.left = u.to_mm(side1);
                changed = true;
            }
            if c.changed() {
                m.right = u.to_mm(side2);
                changed = true;
            }
            if d.changed() {
                // The total shared as the two sides were.
                let old = m.left + m.right;
                let new = u.to_mm(total);
                if old > 1e-9 {
                    m.left *= new / old;
                    m.right *= new / old;
                } else {
                    m.left = new / 2.0;
                    m.right = new / 2.0;
                }
                changed = true;
            }
            if changed {
                touch(st, StrokeField::Widths);
            }
        }
        if ui
            .add_enabled(node.is_some(), egui::Button::new(tr("vo.delete_node")))
            .clicked()
        {
            if let Some(i) = node {
                st.stroke.widths.remove(i);
                st.node = None;
                touch(st, StrokeField::Widths);
            }
        }
    });
}

/// The pictures of the dialog's buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pic {
    DashDefault,
    DashAlign,
    JoinMiter,
    JoinRound,
    JoinBevel,
    CapButt,
    CapRound,
    CapSquare,
    Outside,
    Center,
    Inside,
}

/// A 28 x 24 button with a picture; true when clicked.
fn pic_button(ui: &mut Ui, pic: Pic, on: bool, enabled: bool, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(28.0, 24.0), Sense::click());
    let resp = resp.on_hover_text(tr(tip));
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 3.0, Tokens::ACCENT.gamma_multiply(0.35));
    } else if resp.hovered() && enabled {
        p.rect_filled(rect, 3.0, Tokens::tool_hover());
    }
    let ink = if enabled {
        Tokens::text()
    } else {
        Tokens::text_dim().gamma_multiply(0.5)
    };
    draw_pic(p, rect.shrink(4.0), pic, ink);
    enabled && resp.clicked()
}

fn draw_pic(p: &egui::Painter, r: Rect, pic: Pic, ink: Color32) {
    let thin = EStroke::new(1.0, ink);
    let thick = |w: f32| EStroke::new(w, ink);
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    let c = r.center();
    match pic {
        Pic::DashDefault | Pic::DashAlign => {
            // An L of dashes; aligned ones meet in the corner.
            let pts = [Pos2::new(l, b), Pos2::new(l, t), Pos2::new(rt, t)];
            let seg = |a: Pos2, z: Pos2, from: f32, to: f32| [a + (z - a) * from, a + (z - a) * to];
            for (a, z) in [(pts[0], pts[1]), (pts[1], pts[2])] {
                let runs: &[(f32, f32)] = if pic == Pic::DashAlign {
                    &[(0.0, 0.2), (0.4, 0.6), (0.8, 1.0)]
                } else {
                    &[(0.1, 0.35), (0.55, 0.8)]
                };
                for (f, g) in runs {
                    p.line_segment(seg(a, z, *f, *g), thick(2.0));
                }
            }
        }
        Pic::JoinMiter | Pic::JoinRound | Pic::JoinBevel => {
            // A thick corner, its outer edge as the join makes it.
            let w = 6.0;
            let inner = Pos2::new(l + w, t + w);
            let mut outer = vec![Pos2::new(l, b)];
            match pic {
                Pic::JoinMiter => outer.push(Pos2::new(l, t)),
                Pic::JoinBevel => {
                    outer.push(Pos2::new(l, t + w));
                    outer.push(Pos2::new(l + w, t));
                }
                _ => {
                    for k in 0..=6 {
                        let a = std::f32::consts::PI + std::f32::consts::FRAC_PI_2 * k as f32 / 6.0;
                        outer.push(Pos2::new(l + w + w * a.cos(), t + w + w * a.sin()));
                    }
                }
            }
            outer.push(Pos2::new(rt, t));
            let mut shape = outer.clone();
            shape.push(Pos2::new(rt, t + w));
            shape.push(inner);
            shape.push(Pos2::new(l + w, b));
            p.add(egui::Shape::closed_line(shape, thin));
            p.line_segment(
                [
                    Pos2::new(l + w / 2.0, b),
                    Pos2::new(l + w / 2.0, t + w / 2.0),
                ],
                EStroke::new(1.0, Tokens::ACCENT),
            );
            p.line_segment(
                [
                    Pos2::new(l + w / 2.0, t + w / 2.0),
                    Pos2::new(rt, t + w / 2.0),
                ],
                EStroke::new(1.0, Tokens::ACCENT),
            );
        }
        Pic::CapButt | Pic::CapRound | Pic::CapSquare => {
            // A thick line ending at the path's end (a thin upright).
            let end = c.x + 2.0;
            let h = 4.0;
            let reach = match pic {
                Pic::CapButt => end,
                _ => end + h,
            };
            let body =
                Rect::from_min_max(Pos2::new(l, c.y - h), Pos2::new(reach.min(end), c.y + h));
            p.rect_stroke(body, 0.0, thin, egui::StrokeKind::Middle);
            match pic {
                Pic::CapRound => {
                    let pts: Vec<Pos2> = (0..=8)
                        .map(|k| {
                            let a = -std::f32::consts::FRAC_PI_2
                                + std::f32::consts::PI * k as f32 / 8.0;
                            Pos2::new(end + h * a.cos(), c.y + h * a.sin())
                        })
                        .collect();
                    p.add(egui::Shape::line(pts, thin));
                }
                Pic::CapSquare => {
                    p.rect_stroke(
                        Rect::from_min_max(Pos2::new(end, c.y - h), Pos2::new(end + h, c.y + h)),
                        0.0,
                        thin,
                        egui::StrokeKind::Middle,
                    );
                }
                _ => {}
            }
            p.line_segment(
                [Pos2::new(l, c.y), Pos2::new(end, c.y)],
                EStroke::new(1.0, Tokens::ACCENT),
            );
            p.line_segment([Pos2::new(end, t), Pos2::new(end, b)], thin);
        }
        Pic::Outside | Pic::Center | Pic::Inside => {
            // The path (a thin line) and the outline's band against it.
            let band = 6.0;
            let y = c.y;
            let (y0, y1) = match pic {
                Pic::Outside => (y - band, y),
                Pic::Center => (y - band / 2.0, y + band / 2.0),
                _ => (y, y + band),
            };
            p.rect_filled(
                Rect::from_min_max(Pos2::new(l, y0), Pos2::new(rt, y1)),
                0.0,
                ink,
            );
            p.line_segment(
                [Pos2::new(l, y), Pos2::new(rt, y)],
                EStroke::new(1.0, Tokens::ACCENT),
            );
            // The object's inside, light, below the path.
            p.rect_stroke(
                Rect::from_min_max(Pos2::new(l, y), Pos2::new(rt, b)),
                0.0,
                EStroke::new(1.0, Tokens::text_dim()),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// A dash pattern drawn as a line across `r`.
fn dash_line(p: &egui::Painter, r: Rect, dash: &[f64]) {
    let y = r.center().y;
    let ink = EStroke::new(2.0, Tokens::text());
    if dash.is_empty() {
        p.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], ink);
        return;
    }
    let unit = 2.0f32;
    let mut x = r.left();
    let mut i = 0;
    while x < r.right() {
        let len = dash[i % dash.len()] as f32 * unit;
        if i % 2 == 0 {
            p.line_segment(
                [Pos2::new(x, y), Pos2::new((x + len).min(r.right()), y)],
                ink,
            );
        }
        x += len.max(0.5);
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect as PRect;
    use tracedraw_core::ShapeKind;

    fn rect(app: &mut App, x: f64) -> tracedraw_core::ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: PRect::new(x, 10.0, x + 30.0, 40.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    }

    /// The dialog opens on the selection's outline; OK sends only what was
    /// changed to every selected object, one undo step with overprint;
    /// with nothing selected it sets the default outline (#357).
    #[test]
    fn the_outline_pen_applies_what_was_changed() {
        let mut app = App::headless();
        let a = rect(&mut app, 10.0);
        let b = rect(&mut app, 60.0);
        app.select(vec![a]);
        app.apply_outline_color(Some(tracedraw_core::Color::rgb8(255, 0, 0)));
        app.select(vec![a, b]);
        app.open_outline_pen();
        let crate::ui::dialogs::Dialog::OutlinePen(mut st) =
            std::mem::replace(&mut app.dialog, crate::ui::dialogs::Dialog::None)
        else {
            panic!("the dialog");
        };
        assert!(st.has);
        assert!(st.mix.has(StrokeField::Paint));
        // Round corners and overprint; the colours stay each object's.
        st.stroke.join = LineJoin::Round;
        st.touched.push(StrokeField::Join);
        st.overprint = Some(true);
        let steps = app.engine.history_labels().0.len();
        app.apply_outline_pen(&st);
        assert_eq!(app.engine.history_labels().0.len(), steps + 1);
        let sa = app.doc().find_shape(a).cloned().expect("a");
        let sb = app.doc().find_shape(b).cloned().expect("b");
        assert_eq!(sa.stroke.as_ref().map(|s| s.join), Some(LineJoin::Round));
        assert_eq!(sb.stroke.as_ref().map(|s| s.join), Some(LineJoin::Round));
        assert_ne!(
            sa.stroke.as_ref().map(|s| s.color),
            sb.stroke.as_ref().map(|s| s.color)
        );
        assert!(sa.overprint_outline && sb.overprint_outline);
        // Nothing selected: the default outline.
        app.select(Vec::new());
        app.open_outline_pen();
        let crate::ui::dialogs::Dialog::OutlinePen(mut st) =
            std::mem::replace(&mut app.dialog, crate::ui::dialogs::Dialog::None)
        else {
            panic!("the dialog");
        };
        assert!(st.defaults);
        st.stroke.width = 2.0;
        st.has = true;
        app.apply_outline_pen(&st);
        assert!(
            matches!(app.dialog, crate::ui::dialogs::Dialog::ChangeDefaults(_))
                || app
                    .default_stroke()
                    .is_some_and(|s| (s.width - 2.0).abs() < 1e-9)
        );
    }

    #[test]
    fn the_nib_picture_stretches_and_turns() {
        let c = Pos2::new(50.0, 50.0);
        let flat = nib_corners(c, 20.0, 0.5, 0.0);
        assert!((flat[0].y - 40.0).abs() < 1e-4 && (flat[1].x - 70.0).abs() < 1e-4);
        let turned = nib_corners(c, 20.0, 1.0, 90.0);
        // A quarter turn: the right side's corner goes up.
        assert!((turned[1].x - 30.0).abs() < 1e-3 || (turned[1].y - 30.0).abs() < 1e-3);
    }
}
