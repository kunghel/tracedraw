//! File > Print Preview: the sheets exactly as File > Print sends them,
//! with the imposition, marks, mirror, invert and each separation plate,
//! and the print options at hand beside them (see
//! `docs/behavior/printing.md`, Print Preview).

use crate::app::App;
use crate::export::{plate_page, plates, print_sheets};
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::dialogs::PrintState;
use egui::Ui;
use tracedraw_core::Document;

/// The open preview: the settings it shows, the sheet and plate on screen
/// and what was last built from them.
pub struct PrintPreview {
    pub st: PrintState,
    /// Index of the sheet shown.
    pub sheet: usize,
    /// The inks shown while Print separations is on, one per plate of
    /// [`plates`] (all at first, #571).
    pub inks: Vec<bool>,
    /// Zoom over the fitted size (1 = the sheet fits the window).
    pub zoom: f32,
    /// A drag on the sheet moves the page information, not the artwork.
    pub move_info: bool,
    /// The margin line being dragged (0 left, 1 right, 2 top, 3 bottom).
    pub margin_drag: Option<usize>,
    built: Option<(String, Document)>,
    texture: Option<(String, egui::TextureHandle)>,
}

impl PrintPreview {
    pub fn new(st: PrintState) -> Self {
        PrintPreview {
            st,
            sheet: 0,
            inks: Vec::new(),
            zoom: 1.0,
            move_info: false,
            margin_drag: None,
            built: None,
            texture: None,
        }
    }
}

/// What the sheets depend on: every print setting but the preflight list.
fn settings_key(st: &PrintState) -> String {
    let mut st = st.clone();
    st.preflight = None;
    st.copies = 1;
    format!("{st:?}")
}

/// The sheets for the settings, built again only when they changed.
fn sheets<'a>(app: &App, pv: &'a mut PrintPreview) -> &'a Document {
    let key = settings_key(&pv.st);
    if pv.built.as_ref().map(|(k, _)| k != &key).unwrap_or(true) {
        pv.built = Some((key, print_sheets(app, &pv.st)));
        pv.texture = None;
    }
    // Just built when missing.
    &pv.built
        .get_or_insert_with(|| (String::new(), Document::default()))
        .1
}

/// The page shown: the sheet, or one plate of it.
pub fn shown_page(
    doc: &Document,
    sheet: usize,
    plate: usize,
    separations: bool,
) -> Option<tracedraw_core::document::Page> {
    let page = doc.pages.get(sheet)?;
    if !separations || plate == 0 {
        return Some(page.clone());
    }
    let p = plates(doc).get(plate - 1)?.0;
    Some(plate_page(page, p))
}

/// The page shown with the inks ticked: every ink, the sheet as it is; one
/// ink, its plate in grey (black = full ink); some, the colours those
/// inks print (#571).
pub fn shown_page_inks(
    doc: &Document,
    sheet: usize,
    inks: &[bool],
    separations: bool,
) -> Option<tracedraw_core::document::Page> {
    let page = doc.pages.get(sheet)?;
    let all = plates(doc);
    let on: Vec<usize> = (0..all.len())
        .filter(|i| inks.get(*i).copied().unwrap_or(true))
        .collect();
    if !separations || on.len() == all.len() {
        return Some(page.clone());
    }
    if let [one] = on.as_slice() {
        return shown_page(doc, sheet, one + 1, true);
    }
    let mut process = [false; 4];
    let mut spots = Vec::new();
    for i in on {
        match all[i].0 {
            crate::export::Plate::Process(k) if k < 4 => process[k] = true,
            crate::export::Plate::Spot(id) => spots.push(id),
            _ => {}
        }
    }
    Some(crate::export::inks_page(page, process, &spots))
}

/// The names of the plates, translated for the process inks.
fn plate_names(doc: &Document) -> Vec<String> {
    let mut names = vec![tr("print.composite")];
    for (i, (_, name)) in plates(doc).into_iter().enumerate() {
        names.push(match i {
            0 => tr("fxp.cyan"),
            1 => tr("fxp.magenta"),
            2 => tr("fxp.yellow"),
            3 => tr("fxp.black"),
            _ => name,
        });
    }
    names
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(mut pv) = app.print_preview.take() else {
        return;
    };
    let mut close = false;
    let mut print = false;
    let ctx = ui.ctx().clone();

    egui::Panel::top("print_preview_bar")
        .frame(
            egui::Frame::new()
                .fill(Tokens::menu_bar())
                .inner_margin(egui::Margin::symmetric(8, 4)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(tr("dialog.print")).clicked() {
                    print = true;
                }
                ui.separator();
                let n = sheets(app, &mut pv).pages.len().max(1);
                pv.sheet = pv.sheet.min(n - 1);
                if ui
                    .add_enabled(pv.sheet > 0, egui::Button::new("<"))
                    .on_hover_text(tr("status.previous_page"))
                    .clicked()
                {
                    pv.sheet -= 1;
                }
                ui.label(trf(
                    "print.sheet_of",
                    &[
                        ("n", &(pv.sheet + 1).to_string()),
                        ("total", &n.to_string()),
                    ],
                ));
                if ui
                    .add_enabled(pv.sheet + 1 < n, egui::Button::new(">"))
                    .on_hover_text(tr("status.next_page"))
                    .clicked()
                {
                    pv.sheet += 1;
                }
                ui.separator();
                if pv.st.separations {
                    // Each ink with a box, always in view: one alone shows
                    // its plate, several print together (#571).
                    let names = plate_names(sheets(app, &mut pv));
                    let inks = names.len().saturating_sub(1);
                    pv.inks.resize(inks, true);
                    ui.label(tr("print.inks"));
                    for (i, name) in names.iter().skip(1).enumerate() {
                        ui.checkbox(&mut pv.inks[i], name.clone());
                    }
                    ui.separator();
                }
                ui.label(tr("print.zoom"));
                ui.add(
                    egui::Slider::new(&mut pv.zoom, 0.25..=4.0)
                        .logarithmic(true)
                        .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                );
                if ui.button(tr("print.fit")).clicked() {
                    pv.zoom = 1.0;
                }
                // Close at the bar's right end, where windows keep it
                // (#534).
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("dialog.close")).clicked() {
                        close = true;
                    }
                });
            });
        });

    egui::Panel::left("print_preview_options")
        .resizable(false)
        .exact_size(330.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let st = &mut pv.st;
                ui.checkbox(&mut st.all_pages, tr("dialog.all_pages"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut st.fit_to_page, tr("dialog.fit_to_page"));
                    if !st.fit_to_page {
                        ui.add(
                            crate::ui::field::NumField::new(&mut st.scale)
                                .range(1.0..=1000.0)
                                .suffix(" %"),
                        );
                    }
                });
                ui.separator();
                ui.strong(tr("dialog.prepress"));
                ui.checkbox(&mut st.separations, tr("dialog.separations"));
                ui.checkbox(&mut st.bleed, tr("dialog.print_bleed"));
                ui.checkbox(&mut st.crop_marks, tr("dialog.crop_marks"));
                ui.checkbox(&mut st.registration_marks, tr("dialog.registration_marks"));
                ui.checkbox(&mut st.color_bars, tr("print.color_bars"));
                ui.checkbox(&mut st.densitometer, tr("print.densitometer"));
                ui.checkbox(&mut st.page_info, tr("print.page_info_mark"));
                if st.page_info {
                    ui.horizontal(|ui| {
                        ui.add_space(18.0);
                        ui.checkbox(&mut pv.move_info, tr("print.move_info"));
                    });
                }
                let st = &mut pv.st;
                if st.info_offset != (0.0, 0.0)
                    && ui
                        .small_button(tr("print.info_reset"))
                        .on_hover_text(tr("print.info_drag_tip"))
                        .clicked()
                {
                    st.info_offset = (0.0, 0.0);
                }
                // The paper's margins kept clear: typed here, or their
                // dashed lines dragged on the sheet (#580, #582).
                ui.label(tr("print.margins"));
                ui.horizontal(|ui| {
                    for (i, key) in [
                        "print.margin_left",
                        "print.margin_right",
                        "print.margin_top",
                        "print.margin_bottom",
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.add(
                            crate::ui::field::NumField::new(&mut st.margins[i])
                                .range(0.0..=200.0)
                                .speed(0.5)
                                .suffix(" mm")
                                .width(62.0),
                        )
                        .on_hover_text(tr(key));
                    }
                });
                // Where the artwork sits on its page: a drag on the sheet
                // moves it too (#536).
                ui.horizontal(|ui| {
                    ui.label(tr("print.art_position"));
                    ui.add(
                        crate::ui::field::NumField::new(&mut st.art_offset.0)
                            .range(-1000.0..=1000.0)
                            .speed(0.5)
                            .prefix("x ")
                            .suffix(" mm")
                            .width(70.0),
                    );
                    ui.add(
                        crate::ui::field::NumField::new(&mut st.art_offset.1)
                            .range(-1000.0..=1000.0)
                            .speed(0.5)
                            .prefix("y ")
                            .suffix(" mm")
                            .width(70.0),
                    );
                    if ui
                        .add_enabled(
                            st.art_offset != (0.0, 0.0),
                            egui::Button::new(tr("print.art_reset")).small(),
                        )
                        .clicked()
                    {
                        st.art_offset = (0.0, 0.0);
                    }
                });
                ui.checkbox(&mut st.mirror, tr("dialog.mirror"));
                ui.checkbox(&mut st.invert, tr("dialog.invert"));
                ui.separator();
                crate::ui::dialogs::print_layout_fields(app, ui, st);
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(egui::Color32::from_gray(90)))
        .show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            let separations = pv.st.separations;
            let sheet = pv.sheet;
            let inks = pv.inks.clone();
            let Some(page) = shown_page_inks(sheets(app, &mut pv), sheet, &inks, separations)
            else {
                return;
            };
            let plate: String = inks.iter().map(|on| if *on { '1' } else { '0' }).collect();
            let margin = 24.0;
            let fit = ((area.width() - 2.0 * margin) as f64 / page.size.width.max(1e-6))
                .min((area.height() - 2.0 * margin) as f64 / page.size.height.max(1e-6))
                .max(0.01);
            // Keep the bitmap within what a texture holds.
            let zoom = (fit * pv.zoom as f64)
                .min(8000.0 / page.size.width.max(page.size.height).max(1e-6));
            let w = (page.size.width * zoom).ceil().max(1.0) as u32;
            let h = (page.size.height * zoom).ceil().max(1.0) as u32;
            let key = format!(
                "{}|{sheet}|{plate}|{w}x{h}",
                pv.built.as_ref().map(|(k, _)| k.as_str()).unwrap_or("")
            );
            if pv.texture.as_ref().map(|(k, _)| k != &key).unwrap_or(true) {
                if let Some(img) = render_sheet(&page, w, h, zoom) {
                    let tex = ctx.load_texture("print_preview", img, egui::TextureOptions::LINEAR);
                    pv.texture = Some((key, tex));
                }
            }
            let size = egui::vec2(w as f32, h as f32);
            // A drag moves the artwork, or the page information when
            // that is asked for (#388, #536).
            let info = pv.st.page_info && pv.move_info;
            let mut moved: Option<egui::Vec2> = None;
            egui::ScrollArea::both().show(ui, |ui| {
                let avail = ui
                    .available_size()
                    .max(size + egui::vec2(2.0 * margin, 2.0 * margin));
                let (r, resp) = ui.allocate_exact_size(avail, egui::Sense::drag());
                let resp = resp.on_hover_text(if info {
                    tr("print.info_drag_tip")
                } else {
                    tr("print.art_drag_tip")
                });
                let sheet_rect = egui::Rect::from_center_size(r.center(), size);
                // The margins' lines on the sheet (mm, y up).
                let k = zoom as f32;
                let m = pv.st.margins.map(|v| v.max(0.0) as f32);
                let (left, right) = (sheet_rect.left() + m[0] * k, sheet_rect.right() - m[1] * k);
                let (top, bottom) = (sheet_rect.top() + m[2] * k, sheet_rect.bottom() - m[3] * k);
                let near = |p: egui::Pos2| -> Option<usize> {
                    let inside_y = p.y >= top - 6.0 && p.y <= bottom + 6.0;
                    let inside_x = p.x >= left - 6.0 && p.x <= right + 6.0;
                    [
                        (inside_y && (p.x - left).abs() <= 6.0, 0),
                        (inside_y && (p.x - right).abs() <= 6.0, 1),
                        (inside_x && (p.y - top).abs() <= 6.0, 2),
                        (inside_x && (p.y - bottom).abs() <= 6.0, 3),
                    ]
                    .into_iter()
                    .find(|(hit, _)| *hit)
                    .map(|(_, i)| i)
                };
                let over = resp.hover_pos().and_then(near);
                if resp.drag_started() {
                    pv.margin_drag = resp.interact_pointer_pos().and_then(near);
                }
                let edge = pv.margin_drag.or(over);
                if resp.hovered() || pv.margin_drag.is_some() {
                    ui.ctx().set_cursor_icon(match edge {
                        Some(0 | 1) => egui::CursorIcon::ResizeHorizontal,
                        Some(_) => egui::CursorIcon::ResizeVertical,
                        None => egui::CursorIcon::Move,
                    });
                }
                if resp.dragged() {
                    let d = resp.drag_delta();
                    match pv.margin_drag {
                        Some(i) => {
                            // Inwards makes the margin larger.
                            let along = match i {
                                0 => d.x,
                                1 => -d.x,
                                2 => d.y,
                                _ => -d.y,
                            };
                            let v = &mut pv.st.margins[i];
                            *v = (*v + f64::from(along) / zoom.max(1e-6)).clamp(0.0, 200.0);
                        }
                        None => moved = Some(d),
                    }
                }
                if resp.drag_stopped() {
                    pv.margin_drag = None;
                }
                let p = ui.painter();
                p.rect_filled(
                    sheet_rect.translate(egui::vec2(4.0, 4.0)),
                    0.0,
                    egui::Color32::from_black_alpha(90),
                );
                if let Some((_, tex)) = &pv.texture {
                    p.image(
                        tex.id(),
                        sheet_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                // The margins dashed in blue, the one under the pointer or
                // dragged bolder.
                for (i, (a, b)) in [
                    (
                        egui::pos2(left, sheet_rect.top()),
                        egui::pos2(left, sheet_rect.bottom()),
                    ),
                    (
                        egui::pos2(right, sheet_rect.top()),
                        egui::pos2(right, sheet_rect.bottom()),
                    ),
                    (
                        egui::pos2(sheet_rect.left(), top),
                        egui::pos2(sheet_rect.right(), top),
                    ),
                    (
                        egui::pos2(sheet_rect.left(), bottom),
                        egui::pos2(sheet_rect.right(), bottom),
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let width = if edge == Some(i) { 2.0 } else { 1.0 };
                    p.extend(egui::Shape::dashed_line(
                        &[a, b],
                        egui::Stroke::new(width, Tokens::ACCENT),
                        5.0,
                        4.0,
                    ));
                }
            });
            if let Some(d) = moved {
                // Screen points over the sheet are `zoom` per millimetre;
                // the sheet's y runs up.
                let k = zoom.max(1e-6);
                let target = if info {
                    &mut pv.st.info_offset
                } else {
                    &mut pv.st.art_offset
                };
                target.0 += f64::from(d.x) / k;
                target.1 -= f64::from(d.y) / k;
            }
        });

    let (prev, next, esc) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::PageUp),
            i.key_pressed(egui::Key::PageDown),
            i.key_pressed(egui::Key::Escape),
        )
    });
    if prev {
        pv.sheet = pv.sheet.saturating_sub(1);
    }
    if next {
        pv.sheet += 1;
    }
    if esc {
        close = true;
    }
    if print {
        match crate::export::print(app, &pv.st) {
            Ok(msg) => app.status = msg,
            Err(e) => app.status = format!("{}: {e}", tr("dialog.print_failed")),
        }
        if app.settings.printing.remember_last {
            app.settings.printing.remember(&pv.st);
            app.settings.save();
        }
        close = true;
    }
    if !close {
        app.print_preview = Some(pv);
    }
}

/// The sheet as a bitmap on white paper.
fn render_sheet(
    page: &tracedraw_core::document::Page,
    w: u32,
    h: u32,
    zoom: f64,
) -> Option<egui::ColorImage> {
    let mut doc = Document::new("sheet", page.size);
    doc.pages = vec![page.clone()];
    let opts = tracedraw_render::RenderOptions {
        width: w,
        height: h,
        view: tracedraw_render::ViewTransform {
            zoom,
            origin_x: 0.0,
            origin_y: page.size.height * zoom,
        },
        preview: None,
        wireframe: false,
        ..tracedraw_render::RenderOptions::default()
    };
    let pm = tracedraw_render::render_page(&doc, page.id, &opts)?;
    let mut white = tiny_skia::Pixmap::new(pm.width(), pm.height())?;
    white.fill(tiny_skia::Color::WHITE);
    white.draw_pixmap(
        0,
        0,
        pm.as_ref(),
        &Default::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    Some(egui::ColorImage::from_rgba_premultiplied(
        [white.width() as usize, white.height() as usize],
        white.data(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{Color, Fill, Shape, ShapeKind};

    #[test]
    fn plates_split_the_sheet_by_ink() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(Color::Cmyk {
            c: 1.0,
            m: 0.5,
            y: 0.0,
            k: 0.0,
        });
        doc.pages[0].layers[0].shapes.push(s);
        let grey = |plate| {
            let p = shown_page(&doc, 0, plate, true).expect("page");
            match p.layers[0].shapes[0].fill {
                Fill::Solid(Color::Gray { v }) => v,
                ref f => panic!("{f:?}"),
            }
        };
        assert!((grey(1) - 0.0).abs() < 1e-6, "cyan plate full");
        assert!((grey(2) - 0.5).abs() < 1e-6, "magenta plate half");
        assert!((grey(3) - 1.0).abs() < 1e-6, "yellow plate empty");
        // Composite, and separations off, show the sheet as it is.
        let p = shown_page(&doc, 0, 0, true).expect("page");
        assert!(matches!(
            p.layers[0].shapes[0].fill,
            Fill::Solid(Color::Cmyk { .. })
        ));
        let p = shown_page(&doc, 0, 2, false).expect("page");
        assert!(matches!(
            p.layers[0].shapes[0].fill,
            Fill::Solid(Color::Cmyk { .. })
        ));
        assert!(shown_page(&doc, 5, 0, false).is_none());
        // Ticked inks (#571): one alone is its plate in grey; cyan and
        // yellow together keep only those channels; every ink, the sheet.
        let fill = |inks: &[bool]| {
            shown_page_inks(&doc, 0, inks, true).expect("page").layers[0].shapes[0]
                .fill
                .clone()
        };
        assert!(
            matches!(fill(&[false, true, false, false]), Fill::Solid(Color::Gray { v }) if (v - 0.5).abs() < 1e-6)
        );
        match fill(&[true, false, true, false]) {
            Fill::Solid(Color::Cmyk { c, m, y, k }) => {
                assert!((c - 1.0).abs() < 1e-6 && m == 0.0 && y == 0.0 && k == 0.0)
            }
            f => panic!("{f:?}"),
        }
        assert_eq!(
            fill(&[true; 4]),
            Fill::Solid(Color::Cmyk {
                c: 1.0,
                m: 0.5,
                y: 0.0,
                k: 0.0
            })
        );
        assert_eq!(
            fill(&[]),
            Fill::Solid(Color::Cmyk {
                c: 1.0,
                m: 0.5,
                y: 0.0,
                k: 0.0
            }),
            "all at first"
        );
    }

    #[test]
    fn the_settings_key_ignores_copies_and_preflight() {
        let a = PrintState::default();
        let mut b = a.clone();
        b.copies = 4;
        assert_eq!(settings_key(&a), settings_key(&b));
        b.crop_marks = true;
        assert_ne!(settings_key(&a), settings_key(&b));
    }
}
