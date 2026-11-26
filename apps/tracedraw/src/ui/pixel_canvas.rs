//! What the image editing mode's picture window shows over the picture
//! (Bitmaps > Edit Inside): the right-click menu (the brush's size and
//! options for the brushes, the selection's commands for the rest,
//! #4391 #4393 #4394), the box the Text tool types into (#4370) and the
//! small box a selection command asks its pixels in.

use crate::app::App;
use crate::i18n::tr;
use crate::pixel_edit::{CloneSample, PixelTool};
use crate::theme::Tokens;
use crate::ui::menus::{check, item, sep, sub};
use crate::ui::pixel_cmds::PixelCmd;
use crate::ui::tips::RichTip;
use egui::{Color32, Pos2, Rect, Response, Ui, Vec2};

/// The extras over the picture window `area`, whose response is `resp`.
pub fn extras(app: &mut App, ui: &mut Ui, resp: &Response, area: Rect) {
    let zooming = app.pixel_edit.as_ref().is_some_and(|pe| pe.tool == PixelTool::Zoom);
    if !zooming {
        let at = resp.interact_pointer_pos();
        egui::Popup::context_menu(resp)
            .id(resp.id.with("pixel_canvas_menu"))
            .style(crate::ui::menus::menu_popup_style)
            .show(|ui| {
                crate::ui::menus::body(ui, |ui| canvas_menu(app, ui, at));
            });
    }
    text_box(app, ui, area);
    ask_box(app, ui.ctx());
}

/// A tool that paints with a round brush: its right-click menu sets the
/// brush.
fn brushes(t: PixelTool) -> bool {
    t.is_brush()
        || t.is_liquify()
        || matches!(t, PixelTool::MaskBrush | PixelTool::Remove | PixelTool::Sprayer | PixelTool::Light)
}

/// The right-click menu over the picture.
fn canvas_menu(app: &mut App, ui: &mut Ui, at: Option<Pos2>) {
    let Some(tool) = app.pixel_edit.as_ref().map(|pe| pe.tool) else {
        return;
    };
    if brushes(tool) {
        if let Some(pe) = app.pixel_edit.as_mut() {
            brush_menu(ui, pe);
        }
        let has = app.pixel_edit.as_ref().is_some_and(|pe| pe.pi.sel.is_some());
        if has {
            sep(ui);
            cmd(app, ui, PixelCmd::SelectNone, at);
            cmd(app, ui, PixelCmd::InvertSelection, at);
        }
        return;
    }
    selection_menu(app, ui, at);
}

fn cmd(app: &mut App, ui: &mut Ui, c: PixelCmd, at: Option<Pos2>) {
    let label = if c.asks() {
        format!("{}\u{2026}", tr(c.key()))
    } else {
        tr(c.key())
    };
    if crate::ui::menus::item_label_keyed(ui, c.key(), &label, c.shortcut(), c.enabled(app)) {
        c.ask_or_run(app, at);
        ui.close();
    }
}

/// The selection's commands (#4391 #4393).
fn selection_menu(app: &mut App, ui: &mut Ui, at: Option<Pos2>) {
    cmd(app, ui, PixelCmd::SelectNone, at);
    cmd(app, ui, PixelCmd::SelectAll, at);
    cmd(app, ui, PixelCmd::InvertSelection, at);
    sep(ui);
    cmd(app, ui, PixelCmd::Feather, at);
    sub(ui, "pixel.sel.modify", |ui| {
        cmd(app, ui, PixelCmd::Border, at);
        cmd(app, ui, PixelCmd::Smooth, at);
        cmd(app, ui, PixelCmd::Expand, at);
        cmd(app, ui, PixelCmd::Contract, at);
    });
    let transform = app.pixel_edit.as_ref().is_some_and(|pe| pe.pi.sel.is_some());
    if item(ui, "pixel.sel.transform", "V", transform) {
        if let Some(pe) = app.pixel_edit.as_mut() {
            crate::ui::pixel_mode::choose_tool(pe, PixelTool::Move);
        }
    }
    sep(ui);
    cmd(app, ui, PixelCmd::CopyToLayer, at);
    cmd(app, ui, PixelCmd::CutToLayer, at);
    cmd(app, ui, PixelCmd::Cut, at);
    cmd(app, ui, PixelCmd::Copy, at);
    cmd(app, ui, PixelCmd::Paste, at);
    sep(ui);
    cmd(app, ui, PixelCmd::Fill, at);
    cmd(app, ui, PixelCmd::StrokeSelection, at);
    cmd(app, ui, PixelCmd::ContentFill, at);
    cmd(app, ui, PixelCmd::Delete, at);
    sep(ui);
    cmd(app, ui, PixelCmd::Crop, at);
    cmd(app, ui, PixelCmd::SaveSelection, at);
    let quick = app
        .pixel_edit
        .as_ref()
        .is_some_and(|pe| pe.target == crate::pixel_edit_app::MaskTarget::QuickMask);
    if check(ui, PixelCmd::QuickMask.key(), PixelCmd::QuickMask.shortcut(), quick) {
        PixelCmd::QuickMask.run(app);
    }
}

fn percent(ui: &mut Ui, key: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    let mut p = (*v * 100.0).round();
    if ui
        .add(egui::Slider::new(&mut p, range).suffix(" %").text(tr(key)))
        .changed()
    {
        *v = p / 100.0;
    }
}

/// The brush's size and look, and the clone brush's source (#4394).
fn brush_menu(ui: &mut Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    let t = pe.tool;
    ui.set_min_width(260.0);
    ui.label(egui::RichText::new(crate::ui::pixel_mode::named(t)).strong());
    let o = &mut pe.opts;
    ui.add(
        egui::Slider::new(&mut o.size, 1.0..=1000.0)
            .logarithmic(true)
            .suffix(" px")
            .text(tr("pixel.opt.size")),
    );
    if t != PixelTool::Sprayer {
        percent(ui, "pixel.opt.hardness", &mut o.hardness, 0.0..=100.0);
    }
    if t.is_brush() {
        percent(ui, "pixel.opt.opacity", &mut o.opacity, 0.0..=100.0);
        if t != PixelTool::Smudge {
            percent(ui, "pixel.opt.flow", &mut o.flow, 1.0..=100.0);
        }
        ui.add(
            egui::Slider::new(&mut o.angle, 0.0..=180.0)
                .suffix("\u{b0}")
                .text(tr("pixel.opt.angle")),
        );
        percent(ui, "pixel.opt.roundness", &mut o.roundness, 5.0..=100.0);
    }
    if t.is_liquify() {
        percent(ui, "pixel.opt.strength", &mut o.flow, 1.0..=100.0);
    }
    if matches!(t, PixelTool::Clone | PixelTool::Heal) {
        ui.separator();
        clone_source_options(ui, o);
    }
}

/// The clone and heal brushes' source: turned, scaled, mirrored, what it
/// is taken from, and whether it follows the brush (#4394). Shown in the
/// property bar and the right-click menu.
pub fn clone_source_options(ui: &mut Ui, o: &mut crate::pixel_edit::BrushOpts) {
    ui.add(
        egui::Slider::new(&mut o.clone_angle, -180.0..=180.0)
            .suffix("\u{b0}")
            .text(tr("pixel.opt.clone_angle")),
    )
    .rich_tip(tr("pixel.opt.clone_angle_tip"));
    percent(ui, "pixel.opt.clone_scale", &mut o.clone_scale, 10.0..=400.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut o.clone_flip_h, tr("pixel.opt.clone_flip_h"))
            .rich_tip(tr("pixel.opt.clone_flip_tip"));
        ui.checkbox(&mut o.clone_flip_v, tr("pixel.opt.clone_flip_v"))
            .rich_tip(tr("pixel.opt.clone_flip_tip"));
    });
    egui::ComboBox::from_id_salt("pixel_clone_sample")
        .selected_text(format!("{}: {}", tr("pixel.opt.sample"), tr(o.clone_sample.key())))
        .show_ui(ui, |ui| {
            for m in CloneSample::ALL {
                ui.selectable_value(&mut o.clone_sample, m, tr(m.key()));
            }
        })
        .response
        .rich_tip(tr("pixel.opt.sample_tip"));
    ui.checkbox(&mut o.clone_aligned, tr("pixel.opt.clone_aligned"))
        .rich_tip(tr("pixel.opt.clone_aligned_tip"));
}

/// The box the Text tool types into, under the text being typed (#4370).
fn text_box(app: &mut App, ui: &mut Ui, area: Rect) {
    let Some(pe) = app.pixel_edit.as_mut() else {
        return;
    };
    let Some(e) = pe.text_edit else {
        return;
    };
    let Some(t) = pe.pi.layers.get(e.layer).and_then(|l| l.text.clone()) else {
        pe.text_edit = None;
        return;
    };
    let ws = pe.ws;
    // Under the letters, or under the baseline before there are any.
    let (bx, by) = (t.at.0 - pe.pi.origin.0 as f32, t.at.1 - pe.pi.origin.1 as f32);
    let shown = pe.pi.layer_pixels(e.layer).and_then(crate::pixel_text::shown_box);
    let (x, y) = match shown {
        Some(b) => (b[0] as f32, b[3] as f32),
        None => (bx, by),
    };
    let mut pos = ws.to_screen(area, x, y) + Vec2::new(0.0, 8.0);
    pos.x = pos.x.clamp(area.min.x + 4.0, (area.max.x - 300.0).max(area.min.x + 4.0));
    pos.y = pos.y.clamp(area.min.y + 4.0, (area.max.y - 120.0).max(area.min.y + 4.0));
    let id = egui::Id::new(("pixel_text_box", pe.session, e.layer));
    let mut text = t.text.clone();
    let mut done = false;
    let ctx = ui.ctx().clone();
    egui::Area::new(id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(&ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(280.0);
                let focused = ui.memory(|m| m.has_focus(id));
                if focused
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter))
                {
                    done = true;
                }
                let r = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .id(id)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .hint_text(tr("pixel.text.type_here")),
                );
                if e.focus {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = true;
                }
                ui.label(
                    egui::RichText::new(tr("pixel.text.box_hint"))
                        .small()
                        .color(Tokens::text_dim()),
                );
            });
        });
    if let Some(ed) = pe.text_edit.as_mut() {
        ed.focus = false;
    }
    if text != t.text {
        pe.change_text(|t| t.text = text);
    }
    if done {
        pe.end_text();
    }
}

/// The small box a selection command asks its pixels (and the fill and
/// the stroke their colour) in; Enter or OK runs it.
fn ask_box(app: &mut App, ctx: &egui::Context) {
    let Some(mut ask) = app.pixel_edit.as_ref().and_then(|pe| pe.px_ask) else {
        return;
    };
    let mut ok = false;
    let mut cancel = false;
    let mut window = egui::Window::new(tr(ask.cmd.key()))
        .id(egui::Id::new("pixel_px_ask"))
        .collapsible(false)
        .resizable(false);
    if let Some(p) = ask.pos {
        window = window.default_pos(p + Vec2::new(12.0, 12.0));
    }
    window.show(ctx, |ui| {
        let fill = ask.cmd == PixelCmd::Fill;
        let stroke = ask.cmd == PixelCmd::StrokeSelection;
        if !fill {
            ui.horizontal(|ui| {
                ui.label(tr(if stroke { "pixel.ask.width" } else { "pixel.ask.radius" }));
                let r = ui.add(
                    egui::DragValue::new(&mut ask.px)
                        .range(if stroke { 1.0..=500.0 } else { 0.0..=500.0 })
                        .speed(0.25)
                        .max_decimals(1)
                        .suffix(" px"),
                );
                if ui.memory(|m| m.focused().is_none()) {
                    r.request_focus();
                }
            });
        }
        if fill || stroke {
            ui.horizontal(|ui| {
                ui.label(tr("pixel.opt.color"));
                let [r, g, b, a] = ask.color;
                let mut c = Color32::from_rgba_unmultiplied(r, g, b, a);
                if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::OnlyBlend)
                    .changed()
                {
                    ask.color = c.to_srgba_unmultiplied();
                }
            });
            let mut p = (ask.opacity * 100.0).round();
            if ui
                .add(egui::Slider::new(&mut p, 1.0..=100.0).suffix(" %").text(tr("pixel.opt.opacity")))
                .changed()
            {
                ask.opacity = p / 100.0;
            }
        }
        if stroke {
            ui.horizontal(|ui| {
                ui.label(tr("pixel.ask.position"));
                for at in crate::pixel_style::StrokeAt::ALL {
                    ui.radio_value(&mut ask.at, at, tr(at.key()));
                }
            });
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(tr("dialog.ok")).clicked() {
                ok = true;
            }
            if ui.button(tr("dialog.cancel")).clicked() {
                cancel = true;
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            ok = true;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
    });
    let Some(pe) = app.pixel_edit.as_mut() else {
        return;
    };
    if cancel {
        pe.px_ask = None;
        return;
    }
    if !ok {
        pe.px_ask = Some(ask);
        return;
    }
    pe.px_ask = None;
    run_asked(app, ask);
}

/// Run a command with what its box was given.
pub fn run_asked(app: &mut App, ask: crate::pixel_edit_app::PxAsk) {
    let Some(pe) = app.pixel_edit.as_mut() else {
        return;
    };
    match ask.cmd {
        PixelCmd::Feather => pe.feather_px = ask.px.round().clamp(0.0, 500.0) as u32,
        PixelCmd::Grow => pe.grow_px = ask.px.round().clamp(-500.0, 500.0) as i32,
        PixelCmd::Fill => {
            let (c, o) = (ask.color, ask.opacity);
            pe.apply(false, |pi| crate::pixel_edit::fill_selection(pi, c, o));
            return;
        }
        PixelCmd::StrokeSelection => {
            pe.ask_px.insert(ask.cmd, ask.px);
            pe.stroke_at = ask.at;
            let (c, o, w, at) = (ask.color, ask.opacity, ask.px, ask.at);
            pe.apply(false, |pi| crate::pixel_edit::stroke_selection(pi, c, w, at, o));
            return;
        }
        c => {
            pe.ask_px.insert(c, ask.px);
        }
    }
    ask.cmd.run(app);
}

/// A small square button with a drawn picture: on when `on`.
pub fn drawn_button(
    ui: &mut Ui,
    on: bool,
    tip: &str,
    draw: impl FnOnce(&egui::Painter, Rect, Color32),
) -> Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(Tokens::prop_button()), egui::Sense::click());
    crate::theme::paint_icon_button(ui.painter(), r, resp.hovered(), on);
    draw(ui.painter(), r.shrink(4.0), Tokens::icon());
    resp.rich_tip(tip.to_owned())
}

/// A letter (B, I, U, S) as a button's picture: bold, slanted,
/// underlined or struck through.
pub fn letter_picture(p: &egui::Painter, r: Rect, ink: Color32, which: char) {
    let font = egui::FontId::proportional(r.height() * 0.95);
    let c = r.center();
    match which {
        'B' => {
            for dx in [-0.5, 0.0, 0.5] {
                p.text(c + Vec2::new(dx, 0.0), egui::Align2::CENTER_CENTER, "B", font.clone(), ink);
            }
        }
        'I' => {
            // A slanted stem with its serifs.
            let s = egui::Stroke::new(1.6, ink);
            p.line_segment([egui::pos2(c.x + 2.0, r.min.y + 1.0), egui::pos2(c.x - 2.0, r.max.y - 1.0)], s);
            p.line_segment([egui::pos2(c.x - 0.5, r.min.y + 1.0), egui::pos2(c.x + 4.5, r.min.y + 1.0)], s);
            p.line_segment([egui::pos2(c.x - 4.5, r.max.y - 1.0), egui::pos2(c.x + 0.5, r.max.y - 1.0)], s);
        }
        'U' => {
            p.text(c - Vec2::new(0.0, 1.5), egui::Align2::CENTER_CENTER, "U", font, ink);
            p.line_segment(
                [egui::pos2(r.min.x + 2.0, r.max.y - 0.5), egui::pos2(r.max.x - 2.0, r.max.y - 0.5)],
                egui::Stroke::new(1.2, ink),
            );
        }
        _ => {
            p.text(c, egui::Align2::CENTER_CENTER, "S", font, ink);
            p.line_segment(
                [egui::pos2(r.min.x + 1.0, c.y), egui::pos2(r.max.x - 1.0, c.y)],
                egui::Stroke::new(1.2, ink),
            );
        }
    }
}

/// Lines lined up left, in the middle or right, as a button's picture.
pub fn align_picture(p: &egui::Painter, r: Rect, ink: Color32, align: crate::pixel_text::Align) {
    use crate::pixel_text::Align;
    let s = egui::Stroke::new(1.4, ink);
    for (k, len) in [1.0f32, 0.6, 0.85, 0.5].iter().enumerate() {
        let y = r.min.y + 2.0 + k as f32 * (r.height() - 4.0) / 3.0;
        let w = r.width() * len;
        let x0 = match align {
            Align::Left => r.min.x,
            Align::Center => r.center().x - w / 2.0,
            Align::Right => r.max.x - w,
        };
        p.line_segment([egui::pos2(x0, y), egui::pos2(x0 + w, y)], s);
    }
}
