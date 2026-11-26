//! File > Print Preview: the sheets exactly as File > Print sends them,
//! with the imposition, marks, mirror, invert and each separation plate,
//! and the print options at hand beside them (see
//! `docs/behavior/printing.md`, Print Preview).

use crate::ui::btn_icons::IconUi;
use crate::app::App;
use crate::export::{plate_page, plates, print_sheets};
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::dialogs::PrintState;
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::Document;

/// The open preview: the settings it shows, the sheet and plate on screen
/// and what was last built from them.
pub struct PrintPreview {
    pub st: PrintState,
    /// How far the drawing runs past its pages (#4071), worked out for
    /// the drawing's revision it holds.
    pub overhang: Option<(u64, f64)>,
    /// Index of the sheet shown.
    pub sheet: usize,
    /// With Print separations on, the plate of the sheet shown: 0 the
    /// inks ticked together, else the plate at that place of [`plates`]
    /// plus one, as File > Print sends it on a page of its own (#2379).
    pub plate: usize,
    /// The inks shown while Print separations is on, one per plate of
    /// [`plates`] (all at first, #571).
    pub inks: Vec<bool>,
    /// One ink ticked alone shows in the colour it prints, as several
    /// do, instead of its plate in grey (#1652).
    pub ink_in_color: bool,
    /// Zoom over the fitted size (1 = the sheet fits the window).
    pub zoom: f32,
    /// A drag on the sheet moves the page information, not the artwork.
    pub move_info: bool,
    /// The margin line being dragged (0 left, 1 right, 2 top, 3 bottom).
    pub margin_drag: Option<usize>,
    /// Transform: handles round the page on the sheet that stretch it
    /// (#595), and the one being dragged (x and y sides, -1, 0 or 1).
    pub transform: bool,
    pub handle_drag: Option<(i8, i8)>,
    /// The place on the sheet chosen with Transform on: Rotation turns
    /// only its page, the same place on every sheet (#2587); `None` turns
    /// them all.
    pub chosen_cell: Option<usize>,
    /// Arrange pages: a page dragged onto another place of the sheet
    /// trades places with the page there, on every sheet.
    pub arrange: bool,
    /// The place the page being dragged comes from.
    arrange_from: Option<usize>,
    /// The pages on the sheet shown, on screen, with their places.
    shown_cells: Vec<(usize, egui::Rect)>,
    /// The width the options panel needs for its widest row, as last
    /// laid out (0 until a row did not fit, #862).
    options_need: f32,
    built: Option<(String, Document)>,
    texture: Option<(String, egui::TextureHandle)>,
    /// What a drag on the sheet does: the Pick tool's moves, zooming or
    /// panning.
    pub tool: PreviewTool,
    /// The sheet's middle off the view's middle, points (wheel zoom about
    /// the pointer, panning).
    pub pan: egui::Vec2,
    /// The zoom tool's box being dragged (screen corners).
    marquee: Option<(egui::Pos2, egui::Pos2)>,
    /// Rulers in millimetres along the view's top and left.
    pub rulers: bool,
    /// The strip of sheet thumbnails and the status line at the bottom.
    pub strip: bool,
    /// The options panel at the left (off: the sheet takes the window).
    pub options: bool,
    /// The view's rectangle and fitted zoom (points per mm) last frame,
    /// for the bar's zoom buttons.
    view_area: egui::Rect,
    fit_k: f64,
    /// The sharp render of the part of the sheet in view when zoomed in
    /// past the whole sheet's render: its key, texture and the sheet part
    /// it shows (mm, y up).
    detail: Option<(String, egui::TextureHandle, tracedraw_core::geometry::Rect)>,
    /// When the view last moved (seconds), so the sharp part waits for the
    /// view to rest instead of rendering every frame of a zoom or pan.
    view_moved: f64,
    /// The sheet thumbnails for the sheets built under the key.
    thumbs: (String, Vec<Option<egui::TextureHandle>>),
    /// The pointer over the sheet (mm from its top left), for the status
    /// line.
    pointer_mm: Option<(f64, f64)>,
}

/// The tool a drag on the sheet uses in Print Preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PreviewTool {
    /// Moves the artwork, the margins and Transform's handles.
    #[default]
    Pick,
    /// A click zooms in (out with the right button or Shift), a drag
    /// zooms to its box.
    Zoom,
    /// A drag pans the view.
    Pan,
}

/// The zoom range over the fitted sheet.
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 64.0;
/// The zoom of one wheel notch or zoom button step.
const ZOOM_STEP: f32 = 1.25;
/// The largest side of the whole sheet's render, pixels.
const BASE_MAX_PX: f64 = 4096.0;
/// Height of a sheet thumbnail in the strip, points.
const THUMB_H: f32 = 64.0;
/// Width of the rulers, points.
const RULER_W: f32 = 18.0;

impl PrintPreview {
    pub fn new(st: PrintState) -> Self {
        PrintPreview {
            st,
            overhang: None,
            sheet: 0,
            plate: 0,
            inks: Vec::new(),
            ink_in_color: false,
            zoom: 1.0,
            move_info: false,
            margin_drag: None,
            transform: false,
            handle_drag: None,
            chosen_cell: None,
            arrange: false,
            arrange_from: None,
            shown_cells: Vec::new(),
            options_need: 0.0,
            built: None,
            texture: None,
            tool: PreviewTool::Pick,
            pan: egui::Vec2::ZERO,
            marquee: None,
            rulers: true,
            strip: true,
            options: true,
            view_area: egui::Rect::NOTHING,
            fit_k: 1.0,
            detail: None,
            view_moved: 0.0,
            thumbs: (String::new(), Vec::new()),
            pointer_mm: None,
        }
    }
}

/// The turn Rotation sets: every page's, or the page's in the place
/// chosen on the sheet (#2587).
fn rotation_of(st: &mut PrintState, chosen: Option<usize>) -> &mut f64 {
    match chosen {
        Some(n) => {
            if st.cell_rotation.len() <= n {
                st.cell_rotation.resize(n + 1, 0.0);
            }
            &mut st.cell_rotation[n]
        }
        None => &mut st.art_rotation,
    }
}

/// The artwork's offset that puts the pages at anchor `a` (0 to 8,
/// across the rows from the top left) of the room on a sheet of `sheet`
/// inside `inset` (mm: left, right, top, bottom), from the pages' boxes
/// `cells` now drawn with offset `now`.
pub fn anchor_offset(
    cells: &[tracedraw_core::geometry::Rect],
    sheet: tracedraw_core::geometry::Size,
    inset: [f64; 4],
    a: u8,
    now: (f64, f64),
) -> (f64, f64) {
    let Some(c) = cells.iter().copied().reduce(|a, b| a.union(b)) else {
        return now;
    };
    let [l, r, t, b] = inset.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 });
    let (x0, x1) = (l, (sheet.width - r).max(l));
    let (y0, y1) = (b, (sheet.height - t).max(b));
    let dx = match a % 3 {
        0 => x0 - c.x0,
        1 => (x0 + x1) / 2.0 - (c.x0 + c.x1) / 2.0,
        _ => x1 - c.x1,
    };
    let dy = match a / 3 {
        0 => y1 - c.y1,
        1 => (y0 + y1) / 2.0 - (c.y0 + c.y1) / 2.0,
        _ => y0 - c.y0,
    };
    (now.0 + dx, now.1 + dy)
}

/// Nine buttons in a square, one for each anchor of the sheet's room;
/// the one clicked.
fn anchor_buttons(ui: &mut Ui) -> Option<u8> {
    const CELL: f32 = 13.0;
    let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(CELL * 3.0), egui::Sense::hover());
    ui.painter().rect_stroke(
        r.shrink(CELL / 2.0),
        0.0,
        egui::Stroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Middle,
    );
    let mut out = None;
    for i in 0..9u8 {
        let c = r.min + egui::vec2((i % 3) as f32 + 0.5, (i / 3) as f32 + 0.5) * CELL;
        let cell = egui::Rect::from_center_size(c, egui::Vec2::splat(CELL - 3.0));
        let resp = ui
            .interact(
                cell,
                ui.id().with(("print_anchor", i)),
                egui::Sense::click(),
            )
            .rich_tip(crate::ui::tips::keyed("print.art_anchor"));
        if resp.clicked() {
            out = Some(i);
        }
        ui.painter().rect(
            cell,
            1.0,
            if resp.hovered() {
                Tokens::ACCENT
            } else {
                Tokens::panel()
            },
            egui::Stroke::new(1.0, Tokens::text_dim()),
            egui::StrokeKind::Inside,
        );
    }
    out
}

/// The place on the sheet under a point (mm, y up): the page's clip
/// frame there, with its box.
fn cell_at(cells: &[(usize, tracedraw_core::geometry::Rect)], x: f64, y: f64) -> Option<usize> {
    cells
        .iter()
        .find(|(_, r)| x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1)
        .map(|(n, _)| *n)
}

/// The bleed limit field's tip: its name, without the label's colon, and
/// what it does.
fn bleed_limit_tip() -> String {
    format!(
        "{}\n{}",
        tr("print.bleed_limit").trim_end_matches([':', '\u{ff1a}', ' ', '\u{a0}']),
        tr("print.bleed_limit_desc")
    )
}

/// What the sheets depend on: every print setting but the preflight list.
fn settings_key(st: &PrintState) -> String {
    let mut st = st.clone();
    st.preflight = None;
    st.copies = 1;
    // Naming, deleting or choosing a layout preset changes no sheet.
    st.layout_preset.clear();
    st.layout_preset_typed = None;
    st.layout_preset_delete = None;
    st.layout_preset_request = None;
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

/// Whether the artwork of the pages on `sheet` (each page's clip frame
/// on it, the part of its contents it shows) reaches into the edge `m`
/// the printer cannot print on; marks outside the pages do not count
/// (#1924).
pub fn art_in_unprintable_edge(
    sheet: &tracedraw_core::document::Page,
    m: crate::printers::HardMargins,
) -> bool {
    use tracedraw_core::geometry::Rect;
    let (w, h) = (sheet.size.width, sheet.size.height);
    let printable = Rect::new(m.left, m.bottom, w - m.right, h - m.top);
    let eps = 1e-6;
    sheet.layers.iter().flat_map(|l| l.shapes.iter()).any(|s| {
        let tracedraw_core::ShapeKind::ClipFrame { frame, contents } = &s.kind else {
            return false;
        };
        let cell = s.transform.transform_rect_bbox(frame.bounds());
        let Some(art) = contents
            .iter()
            .map(|c| s.transform.transform_rect_bbox(c.bounds()))
            .reduce(|a, b| a.union(b))
        else {
            return false;
        };
        let shown = art.intersect(cell);
        shown.width() > eps
            && shown.height() > eps
            && (shown.x0 < printable.x0 - eps
                || shown.y0 < printable.y0 - eps
                || shown.x1 > printable.x1 + eps
                || shown.y1 > printable.y1 + eps)
    })
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
/// inks print (#571). With `one_in_color` one ink alone shows the colour
/// it prints too, cyan ink in cyan (#1652).
pub fn shown_page_inks(
    doc: &Document,
    sheet: usize,
    inks: &[bool],
    separations: bool,
    one_in_color: bool,
) -> Option<tracedraw_core::document::Page> {
    let page = doc.pages.get(sheet)?;
    let all = plates(doc);
    let on: Vec<usize> = (0..all.len())
        .filter(|i| inks.get(*i).copied().unwrap_or(true))
        .collect();
    if !separations || on.len() == all.len() {
        return Some(page.clone());
    }
    if let ([one], false) = (on.as_slice(), one_in_color) {
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
    Some(crate::export::inks_page(page, process, &spots, &doc.inks))
}

/// What the preview pages through: each sheet, and with Print
/// separations on each plate of it that prints after it (the inks
/// ticked), as File > Print sends one page per sheet and plate (#2379).
/// Each is a sheet's index and a plate as [`PrintPreview::plate`] has it.
pub fn preview_views(doc: &Document, inks: &[bool], separations: bool) -> Vec<(usize, usize)> {
    let sheets = doc.pages.len().max(1);
    let plates = if separations { plates(doc).len() } else { 0 };
    let mut views = Vec::new();
    for sheet in 0..sheets {
        views.push((sheet, 0));
        for i in 0..plates {
            if inks.get(i).copied().unwrap_or(true) {
                views.push((sheet, i + 1));
            }
        }
    }
    views
}

/// The page of a view of [`preview_views`]: the inks ticked together for
/// plate 0, else that plate alone, in grey as it prints or, with
/// `in_color`, in the colour of its ink.
pub fn view_page(
    doc: &Document,
    (sheet, plate): (usize, usize),
    inks: &[bool],
    separations: bool,
    in_color: bool,
) -> Option<tracedraw_core::document::Page> {
    if !separations || plate == 0 {
        return shown_page_inks(doc, sheet, inks, separations, in_color);
    }
    if in_color {
        let only: Vec<bool> = (0..plates(doc).len()).map(|i| i + 1 == plate).collect();
        return shown_page_inks(doc, sheet, &only, true, true);
    }
    shown_page(doc, sheet, plate, true)
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

/// The smallest and largest stretch of the page a Transform handle sets,
/// as a factor.
const STRETCH_MIN: f64 = 0.01;
const STRETCH_MAX: f64 = 10.0;

/// A Transform handle `(hx, hy)` (-1, 0 or 1 on each axis) dragged by
/// `(dx, dy)` mm (x right, y up) with the page's cell `c` on the sheet:
/// the side under the handle follows the pointer and the opposite side
/// stays put, so the stretch grows by the move and the middle by half of
/// it. A side handle stretches its axis alone; a corner handle keeps the
/// page's proportions, both axes by the one factor of the axis the
/// pointer moved most, so a drawing fitted by its corners is not
/// distorted (#1543).
pub fn drag_transform_handle(
    st: &mut PrintState,
    (hx, hy): (i8, i8),
    c: tracedraw_core::geometry::Rect,
    dx: f64,
    dy: f64,
) {
    if !(dx.is_finite() && dy.is_finite()) || c.width() <= 1e-6 || c.height() <= 1e-6 {
        return;
    }
    // The factor each axis asks for on its own.
    let kx = (c.width() + f64::from(hx) * dx).max(1.0) / c.width();
    let ky = (c.height() + f64::from(hy) * dy).max(1.0) / c.height();
    let (kx, ky) = match (hx, hy) {
        (0, _) => (1.0, ky),
        (_, 0) => (kx, 1.0),
        _ => {
            // A corner: one factor for both, kept within the range on
            // both axes so they stay in step.
            let k = if (kx - 1.0).abs() >= (ky - 1.0).abs() {
                kx
            } else {
                ky
            };
            let (sx, sy) = (st.art_stretch.0.max(1e-9), st.art_stretch.1.max(1e-9));
            let low = (STRETCH_MIN / sx).max(STRETCH_MIN / sy);
            let high = (STRETCH_MAX / sx).min(STRETCH_MAX / sy);
            let k = if low <= high { k.clamp(low, high) } else { 1.0 };
            (k, k)
        }
    };
    if hx != 0 {
        let before = st.art_stretch.0;
        st.art_stretch.0 = (before * kx).clamp(STRETCH_MIN, STRETCH_MAX);
        // The opposite side stays: the middle moves by half the growth.
        let grown = c.width() * (st.art_stretch.0 / before.max(1e-9) - 1.0);
        st.art_offset.0 += f64::from(hx) * grown / 2.0;
    }
    if hy != 0 {
        let before = st.art_stretch.1;
        st.art_stretch.1 = (before * ky).clamp(STRETCH_MIN, STRETCH_MAX);
        let grown = c.height() * (st.art_stretch.1 / before.max(1e-9) - 1.0);
        st.art_offset.1 += f64::from(hy) * grown / 2.0;
    }
}

/// Lays out `add` as one block of the top bar's wrapping rows: the block
/// asks for the width it took the frame before, so it moves whole to the
/// next row when it does not fit. A slider or a list with its label was
/// placed piece by piece, and the piece that did not fit ran past the
/// edge of a phone's screen (#2666).
fn together<R>(ui: &mut Ui, key: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let id = egui::Id::new(("print_preview_bar_block", key));
    let width = ui.data(|d| d.get_temp::<f32>(id)).unwrap_or(0.0);
    let size = egui::vec2(width, ui.spacing().interact_size.y);
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    let r = ui.allocate_ui_with_layout(size, layout, add);
    let took = r.response.rect.width();
    if (took - width).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(id, took));
        ui.ctx().request_repaint();
    }
    r.inner
}

/// The id of the options panel at the left of Print Preview.
const PREVIEW_OPTIONS: &str = "print_preview_options";
/// The options panel's width at first, its narrowest and the widest its
/// edge drags to (wider still when a row needs it), points.
const OPTIONS_DEFAULT: f32 = 360.0;
const OPTIONS_MIN: f32 = 300.0;
const OPTIONS_MAX: f32 = 720.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(mut pv) = app.print_preview.take() else {
        return;
    };
    crate::ui::dialogs::take_saved_sheet(app, &mut pv.st);
    // The printer list, once it has been read (#792).
    if pv.st.looking_for_printers && app.poll_printers(&mut pv.st) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
    }
    let mut close = false;
    let mut print = false;
    let mut print_one = false;
    let mut print_close = false;
    let ctx = ui.ctx().clone();

    egui::Panel::top("print_preview_bar")
        .frame(
            egui::Frame::new()
                .fill(Tokens::menu_bar())
                .inner_margin(egui::Margin::symmetric(8, 4)),
        )
        .show(ui, |ui| {
            // Close at the bar's right end, where windows keep it (#534),
            // laid out first so the rest wraps to the left of it: placed
            // after the rest, it went wherever the rows ended, past a
            // phone's screen when they ran wider than it (#2666).
            let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
            let close_first = egui::Layout::right_to_left(egui::Align::Min);
            ui.allocate_ui_with_layout(row, close_first, |ui| {
                if ui
                    .button(tr("dialog.close"))
                    .rich_tip(crate::ui::tips::keyed("dialog.close"))
                    .clicked()
                {
                    close = true;
                }
                // One row, wrapping onto a second in a narrow window.
                let wrapped = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
                let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
                ui.allocate_ui_with_layout(row, wrapped, |ui| {
                    // Whole labels go to the next row, never half of one.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    // Print leaves the preview open, so pages can go out
                    // one at a time; Print and Close closes it (#4072).
                    if ui
                        .ibutton("dialog.print")
                        .rich_tip(tr("print.print_keep_tip"))
                        .clicked()
                    {
                        print = true;
                    }
                    if ui
                        .ibutton("print.print_and_close")
                        .rich_tip(tr("print.print_and_close_tip"))
                        .clicked()
                    {
                        print = true;
                        print_close = true;
                    }
                    if ui
                        .ibutton("print.print_sheet")
                        .rich_tip(tr("print.print_sheet_tip"))
                        .clicked()
                    {
                        print_one = true;
                    }
                    ui.separator();
                    // With separations, each plate of a sheet is a page of
                    // its own after the sheet, as it prints (#2379).
                    let separations = pv.st.separations;
                    let inks = pv.inks.clone();
                    let doc = sheets(app, &mut pv);
                    let n = doc.pages.len().max(1);
                    let names = if separations {
                        plate_names(doc)
                    } else {
                        Vec::new()
                    };
                    let views = preview_views(doc, &inks, separations);
                    pv.sheet = pv.sheet.min(n - 1);
                    let at = views
                        .iter()
                        .position(|v| *v == (pv.sheet, pv.plate))
                        .or_else(|| views.iter().position(|v| v.0 == pv.sheet))
                        .unwrap_or(0);
                    (pv.sheet, pv.plate) = views.get(at).copied().unwrap_or((0, 0));
                    let mut go = None;
                    if ui
                        .add_enabled(at > 0, egui::Button::new("<"))
                        .rich_tip(tr("status.previous_page"))
                        .clicked()
                    {
                        go = Some(at - 1);
                    }
                    ui.label(trf(
                        "print.sheet_of",
                        &[
                            ("n", &(pv.sheet + 1).to_string()),
                            ("total", &n.to_string()),
                        ],
                    ));
                    if separations {
                        if let Some(name) = names.get(pv.plate) {
                            ui.strong(name);
                        }
                    }
                    if ui
                        .add_enabled(at + 1 < views.len(), egui::Button::new(">"))
                        .rich_tip(tr("status.next_page"))
                        .clicked()
                    {
                        go = Some(at + 1);
                    }
                    if let Some(v) = go.and_then(|i| views.get(i)) {
                        (pv.sheet, pv.plate) = *v;
                    }
                    ui.separator();
                    // Zoom: out, the percent of the sheet's real size,
                    // in, the fitted sheet, its width and the real size.
                    let real =
                        f64::from(crate::settings::zoom_points_per_inch(&app.settings)) / 25.4;
                    let area = pv.view_area;
                    together(ui, "zoom", |ui| {
                        let center = if area.is_positive() {
                            area.center()
                        } else {
                            egui::Pos2::ZERO
                        };
                        if ui
                            .button("\u{2212}")
                            .rich_tip(tr("menu.view.zoom_out"))
                            .clicked()
                        {
                            zoom_at(&mut pv, center, center, 1.0 / ZOOM_STEP);
                        }
                        let fit_k = pv.fit_k.max(1e-6);
                        let mut percent = (fit_k * f64::from(pv.zoom) / real.max(1e-6)) * 100.0;
                        let r = ui
                            .add(
                                crate::ui::field::NumField::new(&mut percent)
                                    .range(1.0..=10000.0)
                                    .max_decimals(0)
                                    .suffix(" %")
                                    .width(64.0),
                            )
                            .rich_tip(tr("print.zoom_tip"));
                        if r.changed() {
                            let want = (percent / 100.0 * real / fit_k) as f32;
                            let by = want / pv.zoom.max(1e-6);
                            zoom_at(&mut pv, center, center, by);
                        }
                        if ui.button("+").rich_tip(tr("menu.view.zoom_in")).clicked() {
                            zoom_at(&mut pv, center, center, ZOOM_STEP);
                        }
                        if ui
                            .ibutton("print.fit")
                            .rich_tip(tr("print.fit_tip"))
                            .clicked()
                        {
                            pv.zoom = 1.0;
                            pv.pan = egui::Vec2::ZERO;
                        }
                        if ui
                            .ibutton("print.fit_width")
                            .rich_tip(tr("print.fit_width_tip"))
                            .clicked()
                        {
                            fit_width(&mut pv, app);
                        }
                        if ui.button("1:1").rich_tip(tr("print.actual_tip")).clicked() {
                            let want = (real / fit_k) as f32;
                            pv.zoom = want.clamp(ZOOM_MIN, ZOOM_MAX);
                            pv.pan = egui::Vec2::ZERO;
                        }
                    });
                    ui.separator();
                    // What the view shows: the rulers, the strip of
                    // sheets and the options panel.
                    ui.imenu_button("print.view", |ui| {
                        ui.checkbox(&mut pv.rulers, tr("menu.view.rulers"));
                        ui.checkbox(&mut pv.strip, tr("print.show_strip"))
                            .rich_tip(tr("print.show_strip_tip"));
                        ui.checkbox(&mut pv.options, tr("print.show_options"))
                            .rich_tip(tr("print.show_options_tip"));
                    });
                    // Kept for the next Print Preview (#3603).
                    app.settings.print_preview_rulers = pv.rulers;
                    app.settings.print_preview_strip = pv.strip;
                    app.settings.print_preview_options = pv.options;
                    // The printer with its Properties and status, and the
                    // copies: in the bar, so the left panel keeps its room
                    // for the options (#792, #953).
                    ui.separator();
                    let st = &mut pv.st;
                    together(ui, "printer", |ui| {
                        ui.label(tr("print.printer"));
                        if let Some(p) = crate::ui::dialogs::destination_row(ui, st, 220.0) {
                            if let Err(e) = crate::printers::open_properties(&p) {
                                app.status = e;
                            }
                        }
                        let chosen = st.printers.iter().find(|p| {
                            st.destination == 0 && st.printer.as_deref() == Some(p.name.as_str())
                        });
                        if let Some(p) = chosen {
                            ui.label(
                                egui::RichText::new(tr(p.state.key()))
                                    .color(Tokens::text_dim())
                                    .size(11.0),
                            )
                            .rich_tip(tr("print.status"));
                        }
                    });
                    ui.separator();
                    together(ui, "copies", |ui| {
                        ui.label(tr("print.number_of_copies"));
                        ui.add(crate::ui::field::NumField::new(&mut st.copies).range(1..=999));
                        ui.add_enabled(
                            st.copies > 1,
                            egui::Checkbox::new(&mut st.collate, tr("print.collate")),
                        );
                    });
                });
            });
        });

    // The inks while Print separations is on (built sheets are kept, so
    // this is a lookup on most frames).
    let ink_names = if pv.st.separations {
        plate_names(sheets(app, &mut pv))
    } else {
        Vec::new()
    };
    pv.inks.resize(ink_names.len().saturating_sub(1), true);

    // The strip of sheets and the status line at the bottom: every sheet
    // small, the one shown marked, a click shows another.
    if pv.strip {
        strip_panel(app, ui, &mut pv);
    }

    // Wide enough for the longer languages; its edge drags wider still.
    // It is never narrower than its widest row as last laid out, up to
    // half the window, so no label is cut; past that what does not fit
    // scrolls sideways (#862).
    let page_count = app.doc().pages.len();
    let has_selection = !app.selection.is_empty();
    let widest = (ui.max_rect().width() * 0.5).max(OPTIONS_MIN);
    let need = pv.options_need.clamp(OPTIONS_MIN, widest);
    let mut options_open = pv.options;
    let panel = egui::Panel::left(PREVIEW_OPTIONS)
        .resizable(true)
        .default_size(OPTIONS_DEFAULT.max(need))
        .size_range(need..=OPTIONS_MAX.max(need))
        .show_collapsible(ui, &mut options_open, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                let st = &mut pv.st;
                // Two options to a row, so the panel has room for more
                // (#951): which pages print, then the marks.
                egui::Grid::new("print_preview_range")
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        if ui
                            .radio_value(&mut st.range, 0, tr("dialog.all_pages"))
                            .clicked()
                        {
                            st.all_pages = true;
                        }
                        ui.radio_value(&mut st.range, 1, tr("print.range_page"));
                        ui.end_row();
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut st.range, 2, tr("print.range_pages"));
                            ui.add_enabled(
                                st.range == 2,
                                egui::TextEdit::singleline(&mut st.pages)
                                    .hint_text(format!("1-{page_count}"))
                                    .desired_width(70.0),
                            );
                        });
                        ui.add_enabled_ui(has_selection, |ui| {
                            ui.radio_value(&mut st.range, 3, tr("print.range_selection"));
                        });
                        if st.range == 3 {
                            ui.checkbox(&mut st.selection_on_page, tr("print.selection_on_page"))
                                .rich_tip(crate::ui::tips::keyed("print.selection_on_page"));
                        }
                        ui.end_row();
                        ui.horizontal(|ui| {
                            if ui
                                .checkbox(&mut st.fit_to_page, tr("dialog.fit_to_page"))
                                .changed()
                                && st.fit_to_page
                            {
                                // Ticked: a page dragged or stretched
                                // off its place comes back onto the
                                // sheet, fitted (#1577).
                                st.art_offset = (0.0, 0.0);
                                st.art_stretch = (1.0, 1.0);
                                st.art_rotation = 0.0;
                            }
                            if !st.fit_to_page {
                                ui.add(
                                    crate::ui::field::NumField::new(&mut st.scale)
                                        .range(1.0..=1000.0)
                                        .suffix(" %"),
                                );
                            }
                        });
                        crate::ui::dialogs::parity_combo(ui, st, "print_preview_parity");
                        ui.end_row();
                    });
                // As the Print dialog keeps it: every range but the
                // current page prints more than one page.
                if st.range == 3 && !has_selection {
                    st.range = 0;
                }
                st.all_pages = st.range != 1;
                ui.separator();
                ui.strong(tr("dialog.prepress"));
                ui.checkbox(&mut st.separations, tr("dialog.separations"));
                if st.separations {
                    // Each ink with a box under it, always in view: one
                    // alone shows its plate, several print together
                    // (#571). The boxes stand side by side and wrap at
                    // the panel's edge (#1737).
                    ui.indent("print_preview_inks", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (on, name) in pv.inks.iter_mut().zip(ink_names.iter().skip(1)) {
                                ui.checkbox(on, name.clone());
                            }
                        });
                        // One ink alone: its plate in grey, as it
                        // prints, or the colour the ink gives (#1652).
                        ui.checkbox(&mut pv.ink_in_color, tr("print.ink_in_color"))
                            .rich_tip(tr("print.ink_in_color_tip"));
                        // Plates with no ink on a sheet left out of the
                        // print.
                        ui.checkbox(&mut pv.st.skip_empty_plates, tr("print.skip_empty_plates"))
                            .rich_tip(crate::ui::tips::keyed("print.skip_empty_plates"));
                    });
                }
                let rev = app.engine.revision();
                let overhang = match pv.overhang {
                    Some((r, v)) if r == rev => v,
                    _ => {
                        let v = crate::export::page_overhang(app.doc());
                        pv.overhang = Some((rev, v));
                        v
                    }
                };
                let st = &mut pv.st;
                egui::Grid::new("print_preview_marks")
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        // The bleed limit beside its box, the value the
                        // Print dialog's Bleed limit sets (#1653).
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut st.bleed, tr("dialog.print_bleed"));
                            ui.add_enabled(
                                st.bleed,
                                crate::ui::field::NumField::new(&mut st.bleed_limit)
                                    .range(0.0..=50.0)
                                    .speed(0.5)
                                    .suffix(" mm"),
                            )
                            .rich_tip(bleed_limit_tip());
                        });
                        ui.checkbox(&mut st.crop_marks, tr("dialog.crop_marks"));
                        ui.end_row();
                        // Objects run past the pages but Bleed is off: the
                        // print cuts them at the page edge, where the crop
                        // marks are, so the marks seem to sit on their
                        // edge (#4071). One click prints the bleed.
                        if !st.bleed && overhang > 0.05 {
                            let label = trf(
                                "print.use_bleed",
                                &[("n", &format!("{overhang:.1}"))],
                            );
                            if ui
                                .add(crate::ui::btn_icons::Ib::with_text("print.use_bleed_title", label))
                                .rich_tip(crate::ui::tips::pair("print.use_bleed_title", "print.use_bleed_desc"))
                                .clicked()
                            {
                                st.bleed = true;
                                st.bleed_limit = overhang;
                            }
                            ui.end_row();
                        }
                        ui.checkbox(&mut st.registration_marks, tr("dialog.registration_marks"));
                        ui.checkbox(&mut st.color_bars, tr("print.color_bars"));
                        ui.end_row();
                        ui.checkbox(&mut st.densitometer, tr("print.densitometer"));
                        ui.checkbox(&mut st.page_info, tr("print.page_info_mark"));
                        ui.end_row();
                        ui.checkbox(&mut st.mirror, tr("dialog.mirror"));
                        ui.checkbox(&mut st.invert, tr("dialog.invert"));
                        ui.end_row();
                        // Grayscale output and the sheets' order (#1657,
                        // #1394, #1571).
                        ui.add_enabled(
                            !st.separations,
                            egui::Checkbox::new(&mut st.grayscale, tr("print.grayscale")),
                        )
                        .rich_tip(tr("print.grayscale_tip"))
                        .rich_disabled_tip(tr("print.grayscale_tip"));
                        ui.checkbox(&mut st.reverse_order, tr("print.reverse_order"))
                            .rich_tip(tr("print.reverse_order_tip"));
                        ui.end_row();
                    });
                crate::ui::dialogs::crop_mark_fields(ui, st);
                if st.page_info {
                    ui.checkbox(&mut pv.move_info, tr("print.move_info"));
                }
                let st = &mut pv.st;
                if st.info_offset != (0.0, 0.0)
                    && ui
                        .small_button(tr("print.info_reset"))
                        .rich_tip(tr("print.info_drag_tip"))
                        .clicked()
                {
                    st.info_offset = (0.0, 0.0);
                }
                // Transform: the page stretched on the sheet by handles,
                // or by these percentages (#595).
                if ui
                    .checkbox(&mut pv.transform, tr("print.transform"))
                    .rich_tip(tr("print.transform_tip"))
                    .changed()
                    && pv.transform
                {
                    pv.arrange = false;
                }
                // Arrange pages: drag a page onto another place.
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut pv.arrange, tr("print.arrange"))
                        .rich_tip(crate::ui::tips::keyed("print.arrange"))
                        .changed()
                        && pv.arrange
                    {
                        pv.transform = false;
                    }
                    if !pv.st.cell_order.is_empty()
                        && ui
                            .small_button(tr("print.arrange_reset"))
                            .rich_tip(crate::ui::tips::keyed("print.arrange_reset"))
                            .clicked()
                    {
                        pv.st.cell_order.clear();
                    }
                });
                let chosen = pv.chosen_cell.filter(|_| pv.transform);
                let transform = pv.transform;
                let st = &mut pv.st;
                ui.horizontal(|ui| {
                    let mut x = st.art_stretch.0 * 100.0;
                    let mut y = st.art_stretch.1 * 100.0;
                    let rx = ui.add(
                        crate::ui::field::NumField::new(&mut x)
                            .range(1.0..=1000.0)
                            .speed(0.5)
                            .prefix("x ")
                            .suffix(" %")
                            .width(70.0),
                    );
                    let ry = ui.add(
                        crate::ui::field::NumField::new(&mut y)
                            .range(1.0..=1000.0)
                            .speed(0.5)
                            .prefix("y ")
                            .suffix(" %")
                            .width(70.0),
                    );
                    if rx.changed() || ry.changed() {
                        st.art_stretch = (x / 100.0, y / 100.0);
                    }
                    if ui
                        .add_enabled(
                            st.art_stretch != (1.0, 1.0),
                            crate::ui::btn_icons::Ib::new("print.art_reset").small(),
                        )
                        .clicked()
                    {
                        st.art_stretch = (1.0, 1.0);
                    }
                });
                // Rotation: the page turned about its middle, by a quick
                // 90/180/270 or any angle; every page, or only the one
                // chosen on the sheet with Transform on (#2587).
                ui.horizontal(|ui| {
                    ui.label(tr("print.rotation"))
                        .rich_tip(tr("print.rotation_pick_tip"));
                    let turn = rotation_of(st, chosen);
                    ui.add(
                        crate::ui::field::NumField::new(turn)
                            .range(-360.0..=360.0)
                            .speed(0.5)
                            .suffix(" \u{b0}")
                            .width(70.0),
                    );
                    for q in [90.0, 180.0, 270.0] {
                        if ui.small_button(format!("{q}\u{b0}")).clicked() {
                            let turn = rotation_of(st, chosen);
                            *turn = (*turn + q).rem_euclid(360.0);
                        }
                    }
                    let turned = *rotation_of(st, chosen) != 0.0;
                    if ui
                        .add_enabled(turned, crate::ui::btn_icons::Ib::new("print.art_reset").small())
                        .clicked()
                    {
                        *rotation_of(st, chosen) = 0.0;
                    }
                    if st.cell_rotation.iter().all(|t| *t == 0.0) {
                        st.cell_rotation.clear();
                    }
                });
                if transform {
                    ui.label(
                        egui::RichText::new(match chosen {
                            Some(n) => trf("print.rotation_one", &[("n", &(n + 1).to_string())]),
                            None => tr("print.rotation_all"),
                        })
                        .small()
                        .weak(),
                    )
                    .rich_tip(tr("print.rotation_pick_tip"));
                }
                // The pages on the sheet shown and the room they may take,
                // for the anchor buttons.
                let shown = pv.sheet;
                let room = sheets(app, &mut pv).pages.get(shown).map(|p| {
                    let boxes: Vec<_> = sheet_cells(p).into_iter().map(|(_, r)| r).collect();
                    (boxes, p.size)
                });
                let st = &mut pv.st;
                // Where the artwork sits on its page: a drag on the sheet
                // moves it too (#536).
                ui.horizontal(|ui| {
                    // The pages put at a corner, side or the middle of the
                    // paper inside its margins (and the marks' room).
                    if let Some(a) = anchor_buttons(ui) {
                        if let Some((boxes, size)) = &room {
                            let marks = st.crop_marks
                                || st.registration_marks
                                || st.color_bars
                                || st.densitometer
                                || st.page_info
                                || st.page_numbers;
                            let extra = if marks {
                                crate::imposition::MARK_MARGIN
                                    + if st.bleed {
                                        st.bleed_limit.clamp(0.0, 50.0)
                                    } else {
                                        0.0
                                    }
                            } else {
                                0.0
                            };
                            let inset = st.margins.map(|m| m.max(0.0) + extra);
                            st.art_offset = anchor_offset(boxes, *size, inset, a, st.art_offset);
                        }
                    }
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
                            crate::ui::btn_icons::Ib::new("print.art_reset").small(),
                        )
                        .clicked()
                    {
                        st.art_offset = (0.0, 0.0);
                    }
                });
                ui.separator();
                crate::ui::dialogs::print_layout_fields(app, ui, st);
            })
        });
    // A row wider than the panel (a longer language, a setting shown):
    // the panel takes its width from the next frame on, this frame not
    // shown, so nothing is drawn cut.
    pv.options = options_open;
    if let Some(panel) = &panel {
        let scroll = &panel.inner;
        let chrome = panel.response.rect.width() - scroll.inner_rect.width();
        let want = scroll.content_size.x + chrome;
        if scroll.content_size.x > scroll.inner_rect.width() + 0.5 && want > pv.options_need + 0.5 {
            pv.options_need = want;
            if want <= widest {
                ctx.request_discard("print preview options wider");
            }
        }
    }

    // The tools a drag on the sheet uses: Pick, Zoom and Pan.
    egui::Panel::left("print_preview_tools")
        .resizable(false)
        .exact_size(36.0)
        .frame(
            egui::Frame::new()
                .fill(Tokens::menu_bar())
                .inner_margin(egui::Margin::symmetric(4, 6)),
        )
        .show(ui, |ui| {
            ui.vertical(|ui| {
                for (tool, icon, tip) in [
                    (
                        PreviewTool::Pick,
                        crate::tools::Tool::Pick,
                        "print.tool_pick_tip",
                    ),
                    (
                        PreviewTool::Zoom,
                        crate::tools::Tool::Zoom,
                        "print.tool_zoom_tip",
                    ),
                    (
                        PreviewTool::Pan,
                        crate::tools::Tool::Pan,
                        "print.tool_pan_tip",
                    ),
                ] {
                    let on = pv.tool == tool;
                    let r =
                        crate::ui::before_after::icon_button(ui, &tr(tip), on, true, |p, r, c| {
                            crate::ui::icons::draw(p, r, icon, c)
                        });
                    if r.clicked() {
                        pv.tool = tool;
                    }
                }
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(egui::Color32::from_gray(90)))
        .show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            // The rulers take a band along the top and the left.
            let area = if pv.rulers {
                egui::Rect::from_min_max(full.min + egui::vec2(RULER_W, RULER_W), full.max)
            } else {
                full
            };
            let separations = pv.st.separations;
            let sheet = pv.sheet;
            let shown_plate = if separations { pv.plate } else { 0 };
            let inks = pv.inks.clone();
            let in_color = pv.ink_in_color;
            let Some(page) = view_page(
                sheets(app, &mut pv),
                (sheet, shown_plate),
                &inks,
                separations,
                in_color,
            ) else {
                return;
            };
            let mut plate: String = inks.iter().map(|on| if *on { '1' } else { '0' }).collect();
            plate.push_str(&format!("p{shown_plate}"));
            if in_color {
                plate.push('c');
            }
            // The edge the chosen printer cannot print on, as the system
            // reports it (#1924); none for a file.
            let hard = (pv.st.destination == 0)
                .then(|| app.printer_margins.get(pv.st.printer.as_deref()))
                .flatten()
                .map(|m| m.for_sheet(page.size.width, page.size.height));
            let cut = hard.is_some_and(|m| art_in_unprintable_edge(&page, m));
            let fit = fit_zoom(area, page.size);
            pv.view_area = area;
            pv.fit_k = fit;
            let built_key = pv
                .built
                .as_ref()
                .map(|(k, _)| k.clone())
                .unwrap_or_default();
            let ppp = f64::from(ctx.pixels_per_point());
            // The whole sheet is rendered once at about the fitted size
            // (sharper on a high-density screen), so a zoom or pan only
            // scales it; past that size the part in view is rendered
            // sharp once the view rests.
            let base_px = (fit * ppp * 1.5)
                .min(BASE_MAX_PX / page.size.width.max(page.size.height).max(1e-6))
                .max(0.01);
            let key = format!("{built_key}|{sheet}|{plate}|{base_px:.4}");
            if pv.texture.as_ref().map(|(k, _)| k != &key).unwrap_or(true) {
                let whole = tracedraw_core::geometry::Rect::new(
                    0.0,
                    0.0,
                    page.size.width,
                    page.size.height,
                );
                if let Some(img) = render_part(&page, whole, base_px) {
                    let tex = ctx.load_texture("print_preview", img, egui::TextureOptions::LINEAR);
                    pv.texture = Some((key.clone(), tex));
                }
                pv.detail = None;
            }

            let resp = ui.interact(
                area,
                egui::Id::new("print_preview_sheet"),
                egui::Sense::click_and_drag(),
            );
            let now = ctx.input(|i| i.time);
            // The wheel zooms about the pointer (with Ctrl when Options
            // say the wheel scrolls); a pinch zooms; otherwise it pans.
            if let Some(h) = resp.hover_pos() {
                let web = crate::ui::field::web_wheel();
                let (scroll, pinch, mods, notches) = ctx.input(|i| {
                    // Wheel notches from the raw events, as the drawing
                    // window counts them.
                    let notches: f32 = i
                        .events
                        .iter()
                        .map(|e| match e {
                            egui::Event::MouseWheel { unit, delta, .. } => {
                                delta.y / crate::ui::field::wheel_notch(*unit, web)
                            }
                            _ => 0.0,
                        })
                        .sum();
                    (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers, notches)
                });
                let wheel_zooms = if app.settings.wheel_zooms {
                    !mods.ctrl && !mods.shift
                } else {
                    mods.ctrl
                };
                if pinch != 1.0 {
                    zoom_at(&mut pv, h, area.center(), pinch);
                    pv.view_moved = now;
                } else if wheel_zooms {
                    if notches != 0.0 {
                        let step = app.settings.wheel_zoom_factor();
                        zoom_at(&mut pv, h, area.center(), step.powf(notches));
                        pv.view_moved = now;
                    }
                } else if scroll != egui::Vec2::ZERO {
                    if mods.shift {
                        pv.pan += egui::vec2(scroll.y, scroll.x);
                    } else {
                        pv.pan += scroll;
                    }
                    pv.view_moved = now;
                }
            }
            // Panning with the middle button, Space held or the Pan tool.
            let space = ctx.input(|i| i.key_down(egui::Key::Space));
            let panning = resp.dragged_by(egui::PointerButton::Middle)
                || (resp.dragged_by(egui::PointerButton::Primary)
                    && (space || pv.tool == PreviewTool::Pan));
            if panning {
                pv.pan += resp.drag_delta();
                pv.view_moved = now;
            }
            // The zoom tool: a click zooms in (out with the right button
            // or Shift), a drag zooms to its box.
            if pv.tool == PreviewTool::Zoom && !space {
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(p) = resp.interact_pointer_pos() {
                        pv.marquee = Some((p, p));
                    }
                }
                if let (Some((a, _)), Some(p)) = (pv.marquee, resp.interact_pointer_pos()) {
                    if resp.dragged_by(egui::PointerButton::Primary) {
                        pv.marquee = Some((a, p));
                    }
                }
                if resp.drag_stopped() {
                    if let Some((a, b)) = pv.marquee.take() {
                        zoom_to_box(&mut pv, egui::Rect::from_two_pos(a, b), area);
                        pv.view_moved = now;
                    }
                }
                let shift = ctx.input(|i| i.modifiers.shift);
                if let Some(p) = resp.interact_pointer_pos() {
                    if resp.clicked() {
                        let f = if shift { 0.5 } else { 2.0 };
                        zoom_at(&mut pv, p, area.center(), f);
                        pv.view_moved = now;
                    } else if resp.secondary_clicked() {
                        zoom_at(&mut pv, p, area.center(), 0.5);
                        pv.view_moved = now;
                    }
                }
                if resp.hovered() {
                    ctx.set_cursor_icon(if shift {
                        egui::CursorIcon::ZoomOut
                    } else {
                        egui::CursorIcon::ZoomIn
                    });
                }
            } else {
                pv.marquee = None;
            }
            if (pv.tool == PreviewTool::Pan || space) && (resp.hovered() || panning) {
                ctx.set_cursor_icon(if panning {
                    egui::CursorIcon::Grabbing
                } else {
                    egui::CursorIcon::Grab
                });
            }
            // The sheet on screen.
            let k = fit * f64::from(pv.zoom);
            let size = egui::vec2((page.size.width * k) as f32, (page.size.height * k) as f32);
            pv.pan = clamp_pan(pv.pan, area, size);
            let sheet_rect = egui::Rect::from_center_size(area.center() + pv.pan, size);
            let to_screen_now = |x: f64, y: f64| {
                egui::pos2(
                    sheet_rect.left() + (x * k) as f32,
                    sheet_rect.bottom() - (y * k) as f32,
                )
            };
            let to_sheet = |p: egui::Pos2| {
                (
                    f64::from(p.x - sheet_rect.left()) / k,
                    f64::from(sheet_rect.bottom() - p.y) / k,
                )
            };
            pv.pointer_mm = resp.hover_pos().map(|p| {
                let (x, y) = to_sheet(p);
                (x, page.size.height - y)
            });

            // The Pick tool: moves the artwork (or the page information),
            // the margins' lines and Transform's handles.
            let pick = pv.tool == PreviewTool::Pick && !space && !panning;
            let info = pv.st.page_info && pv.move_info;
            let mut moved: Option<egui::Vec2> = None;
            let zoom = k;
            let m = pv.st.margins.map(|v| v.max(0.0) as f32);
            let kf = k as f32;
            let (left, right) = (
                sheet_rect.left() + m[0] * kf,
                sheet_rect.right() - m[1] * kf,
            );
            let (top, bottom) = (
                sheet_rect.top() + m[2] * kf,
                sheet_rect.bottom() - m[3] * kf,
            );
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
            // Every page on the sheet with its place, so that a click
            // chooses the one Rotation turns (#2587); the handles go
            // round the chosen one, or the first.
            let cells = sheet_cells(&page);
            let several = cells.len() > 1;
            if !several {
                pv.chosen_cell = None;
            }
            let cell = pv
                .chosen_cell
                .and_then(|n| cells.iter().find(|(c, _)| *c == n))
                .or(cells.first())
                .map(|(_, r)| *r);
            let handles: Vec<((i8, i8), egui::Pos2)> = match (pv.transform, cell) {
                (true, Some(c)) => {
                    let mut v = Vec::new();
                    for hy in [-1i8, 0, 1] {
                        for hx in [-1i8, 0, 1] {
                            if hx == 0 && hy == 0 {
                                continue;
                            }
                            let x = match hx {
                                -1 => c.x0,
                                0 => (c.x0 + c.x1) / 2.0,
                                _ => c.x1,
                            };
                            let y = match hy {
                                -1 => c.y0,
                                0 => (c.y0 + c.y1) / 2.0,
                                _ => c.y1,
                            };
                            v.push(((hx, hy), to_screen_now(x, y)));
                        }
                    }
                    v
                }
                _ => Vec::new(),
            };
            let handle_at = |p: egui::Pos2| {
                handles
                    .iter()
                    .find(|(_, h)| (p - *h).length() <= 7.0)
                    .map(|(k, _)| *k)
            };
            let mut edge = None;
            let mut handle = None;
            // Arrange pages: a page dragged onto another place trades
            // places with the page there.
            let arranging = pick && pv.arrange && several;
            if !arranging {
                pv.arrange_from = None;
            }
            let mut arrange_to = None;
            if arranging {
                let at = |p: Option<egui::Pos2>| {
                    p.and_then(|p| {
                        let (x, y) = to_sheet(p);
                        cell_at(&cells, x, y)
                    })
                };
                let resp = resp.clone().rich_tip(tr("print.arrange_desc"));
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    // Where the button went down, before the drag began.
                    let origin = ctx.input(|i| i.pointer.press_origin());
                    pv.arrange_from = at(origin.or(resp.interact_pointer_pos()));
                }
                let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
                if pv.arrange_from.is_some() {
                    arrange_to = at(pointer);
                }
                if resp.hovered() || pv.arrange_from.is_some() {
                    ctx.set_cursor_icon(if pv.arrange_from.is_some() {
                        egui::CursorIcon::Grabbing
                    } else if at(pointer).is_some() {
                        egui::CursorIcon::Grab
                    } else {
                        egui::CursorIcon::Default
                    });
                }
                if resp.drag_stopped() {
                    if let (Some(a), Some(b)) = (pv.arrange_from.take(), arrange_to) {
                        if a != b {
                            crate::imposition::swap_places(&mut pv.st.cell_order, a, b);
                        }
                    }
                    arrange_to = None;
                }
            }
            if pick && !arranging {
                let resp = resp.clone().rich_tip(if info {
                    tr("print.info_drag_tip")
                } else {
                    tr("print.art_drag_tip")
                });
                let over_handle = resp.hover_pos().and_then(handle_at);
                let over = if over_handle.is_some() {
                    None
                } else {
                    resp.hover_pos().and_then(near)
                };
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    pv.handle_drag = resp.interact_pointer_pos().and_then(handle_at);
                    pv.margin_drag = if pv.handle_drag.is_some() {
                        None
                    } else {
                        resp.interact_pointer_pos().and_then(near)
                    };
                }
                edge = pv.margin_drag.or(over);
                handle = pv.handle_drag.or(over_handle);
                if resp.hovered() || pv.margin_drag.is_some() || pv.handle_drag.is_some() {
                    ctx.set_cursor_icon(match (handle, edge) {
                        (Some((0, _)), _) => egui::CursorIcon::ResizeVertical,
                        (Some((_, 0)), _) => egui::CursorIcon::ResizeHorizontal,
                        (Some((hx, hy)), _) if hx == hy => egui::CursorIcon::ResizeNeSw,
                        (Some(_), _) => egui::CursorIcon::ResizeNwSe,
                        (None, Some(0 | 1)) => egui::CursorIcon::ResizeHorizontal,
                        (None, Some(_)) => egui::CursorIcon::ResizeVertical,
                        (None, None) => egui::CursorIcon::Move,
                    });
                }
                let primary = resp.dragged_by(egui::PointerButton::Primary);
                if let (true, Some(handle), Some(c)) = (primary, pv.handle_drag, cell) {
                    let d = resp.drag_delta();
                    let (dx, dy) = (f64::from(d.x) / zoom, -f64::from(d.y) / zoom);
                    drag_transform_handle(&mut pv.st, handle, c, dx, dy);
                } else if primary {
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
                // With Transform on, a click on a page chooses it for
                // Rotation, again or off the pages turns them all.
                if pv.transform && several && resp.clicked() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        let (x, y) = to_sheet(p);
                        let hit = cell_at(&cells, x, y);
                        pv.chosen_cell = if hit == pv.chosen_cell { None } else { hit };
                    }
                }
            }
            if resp.drag_stopped() {
                pv.margin_drag = None;
                pv.handle_drag = None;
            }

            // The sharp part in view, once the view rests.
            let screen_px = k * ppp;
            let resting = now - pv.view_moved > 0.15 && !resp.dragged();
            if screen_px > base_px * 1.05 {
                let shown = sheet_rect.intersect(area);
                if shown.is_positive() {
                    let (x0, y1) = to_sheet(shown.left_top());
                    let (x1, y0) = to_sheet(shown.right_bottom());
                    // Whole millimetres round the part, so small pans
                    // keep the render.
                    let part = tracedraw_core::geometry::Rect::new(
                        x0.floor().max(0.0),
                        y0.floor().max(0.0),
                        x1.ceil().min(page.size.width),
                        y1.ceil().min(page.size.height),
                    );
                    let dkey = format!(
                        "{key}|{screen_px:.3}|{:.0},{:.0},{:.0},{:.0}",
                        part.x0, part.y0, part.x1, part.y1
                    );
                    let have = pv.detail.as_ref().is_some_and(|(k, _, _)| *k == dkey);
                    if !have {
                        if resting {
                            if let Some(img) = render_part(&page, part, screen_px) {
                                let tex = ctx.load_texture(
                                    "print_preview_detail",
                                    img,
                                    egui::TextureOptions::LINEAR,
                                );
                                pv.detail = Some((dkey, tex, part));
                            }
                        } else {
                            ctx.request_repaint_after(std::time::Duration::from_millis(160));
                        }
                    }
                }
            } else {
                pv.detail = None;
            }

            let p = ui.painter_at(full);
            p.rect_filled(
                sheet_rect.translate(egui::vec2(4.0, 4.0)),
                0.0,
                egui::Color32::from_black_alpha(90),
            );
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            if let Some((_, tex)) = &pv.texture {
                p.image(tex.id(), sheet_rect, uv, egui::Color32::WHITE);
            }
            if let Some((_, tex, part)) = &pv.detail {
                let r = egui::Rect::from_two_pos(
                    to_screen_now(part.x0, part.y1),
                    to_screen_now(part.x1, part.y0),
                );
                p.image(tex.id(), r, uv, egui::Color32::WHITE);
            }
            pv.shown_cells = cells
                .iter()
                .map(|(n, c)| {
                    let r = egui::Rect::from_two_pos(
                        to_screen_now(c.x0, c.y0),
                        to_screen_now(c.x1, c.y1),
                    );
                    (*n, r)
                })
                .collect();
            // The page dragged and the place it goes to.
            if arranging {
                for (n, r) in &pv.shown_cells {
                    if Some(*n) == pv.arrange_from {
                        p.rect_filled(*r, 0.0, Tokens::ACCENT.gamma_multiply(0.18));
                    }
                    if Some(*n) == arrange_to && arrange_to != pv.arrange_from {
                        p.rect_stroke(
                            *r,
                            0.0,
                            egui::Stroke::new(2.5, Tokens::ACCENT),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
            // The pages a click can choose, outlined faintly.
            if (pv.transform || arranging) && several {
                for (_, r) in &pv.shown_cells {
                    p.rect_stroke(
                        *r,
                        0.0,
                        egui::Stroke::new(1.0, Tokens::ACCENT.gamma_multiply(0.35)),
                        egui::StrokeKind::Middle,
                    );
                }
            }
            // Transform's frame and handles round the page.
            if let (true, Some(c)) = (pv.transform, cell) {
                let r =
                    egui::Rect::from_two_pos(to_screen_now(c.x0, c.y0), to_screen_now(c.x1, c.y1));
                p.rect_stroke(
                    r,
                    0.0,
                    egui::Stroke::new(1.0, Tokens::ACCENT),
                    egui::StrokeKind::Middle,
                );
                for (k, h) in &handles {
                    let hr = egui::Rect::from_center_size(*h, egui::vec2(8.0, 8.0));
                    p.rect_filled(
                        hr,
                        0.0,
                        if handle == Some(*k) {
                            Tokens::ACCENT
                        } else {
                            egui::Color32::WHITE
                        },
                    );
                    p.rect_stroke(
                        hr,
                        0.0,
                        egui::Stroke::new(1.0, Tokens::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                }
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
            // The printer's unprintable edge shaded, and a warning
            // when artwork reaches into it (#1924).
            if let Some(m) = hard {
                let kf = k as f32;
                let inner = egui::Rect::from_min_max(
                    egui::pos2(
                        sheet_rect.left() + m.left as f32 * kf,
                        sheet_rect.top() + m.top as f32 * kf,
                    ),
                    egui::pos2(
                        sheet_rect.right() - m.right as f32 * kf,
                        sheet_rect.bottom() - m.bottom as f32 * kf,
                    ),
                );
                let shade = egui::Color32::from_rgba_unmultiplied(110, 110, 110, 110);
                let s = sheet_rect;
                for band in [
                    egui::Rect::from_min_max(s.min, egui::pos2(s.max.x, inner.min.y)),
                    egui::Rect::from_min_max(egui::pos2(s.min.x, inner.max.y), s.max),
                    egui::Rect::from_min_max(
                        egui::pos2(s.min.x, inner.min.y),
                        egui::pos2(inner.min.x, inner.max.y),
                    ),
                    egui::Rect::from_min_max(
                        egui::pos2(inner.max.x, inner.min.y),
                        egui::pos2(s.max.x, inner.max.y),
                    ),
                ] {
                    if band.width() > 0.0 && band.height() > 0.0 {
                        p.rect_filled(band, 0.0, shade);
                    }
                }
                if cut {
                    p.text(
                        egui::pos2(area.left() + 8.0, area.top() + 4.0),
                        egui::Align2::LEFT_TOP,
                        tr("print.unprintable_warning"),
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_rgb(255, 210, 120),
                    );
                }
            }
            // The zoom tool's box.
            if let Some((a, b)) = pv.marquee {
                let r = egui::Rect::from_two_pos(a, b);
                p.rect_filled(r, 0.0, Tokens::ACCENT.gamma_multiply(0.12));
                p.rect_stroke(
                    r,
                    0.0,
                    egui::Stroke::new(1.0, Tokens::ACCENT),
                    egui::StrokeKind::Middle,
                );
            }
            if pv.rulers {
                paint_rulers(&p, full, area, sheet_rect, k, resp.hover_pos());
            }
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
    if prev || next {
        let separations = pv.st.separations;
        let inks = pv.inks.clone();
        let views = preview_views(sheets(app, &mut pv), &inks, separations);
        let at = views
            .iter()
            .position(|v| *v == (pv.sheet, pv.plate))
            .unwrap_or(0);
        let to = if prev {
            at.saturating_sub(1)
        } else {
            (at + 1).min(views.len().saturating_sub(1))
        };
        if let Some(v) = views.get(to) {
            (pv.sheet, pv.plate) = *v;
        }
    }
    if esc {
        close = true;
    }
    if print || print_one {
        // The inks unticked here do not print (#846).
        pv.st.plates_off = pv
            .inks
            .iter()
            .enumerate()
            .filter(|(_, on)| !**on)
            .map(|(i, _)| i)
            .collect();
        // Print this sheet: the sheet shown alone.
        let only = (print_one && !print).then_some(pv.sheet);
        match crate::export::print_sheet(app, &pv.st, only) {
            Ok(msg) => app.status = msg,
            Err(e) => app.status = format!("{}: {e}", tr("dialog.print_failed")),
        }
        if pv.st.destination == 0 {
            app.printer_last = pv.st.printer.clone();
        }
        if app.settings.printing.remember_last {
            app.settings.printing.remember(&pv.st);
            app.settings.save();
        }
        close |= print_close;
    }
    if !close {
        app.print_preview = Some(pv);
    }
}

/// Every page on a sheet with its place: the page's clip frame and its
/// box (mm, y up).
pub fn sheet_cells(
    page: &tracedraw_core::document::Page,
) -> Vec<(usize, tracedraw_core::geometry::Rect)> {
    page.layers
        .first()
        .map(|l| {
            l.shapes
                .iter()
                .filter_map(|s| match &s.kind {
                    tracedraw_core::ShapeKind::ClipFrame { frame, .. } => match &frame.kind {
                        // Only the pages' places: a tile's frame is none.
                        tracedraw_core::ShapeKind::Rect { rect, .. } => {
                            crate::imposition::cell_of(s)
                                .map(|n| (n, s.transform.transform_rect_bbox(*rect)))
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// How many pages File > Print sends for `sheets` sheets: every sheet, or
/// with separations each ink ticked of it, times the copies.
pub fn pages_to_print(sheets: usize, st: &PrintState, inks: &[bool]) -> usize {
    let per_sheet = if st.separations {
        inks.iter().filter(|on| **on).count()
    } else {
        1
    };
    sheets * per_sheet * st.copies.max(1) as usize
}

/// The frame round a sheet thumbnail: the sheet shown in accent. Drawn
/// inside the thumbnail, so the strip's edge does not cut off the first
/// sheet's frame (#3242).
fn thumb_frame(shown: bool) -> (egui::Stroke, egui::StrokeKind) {
    let stroke = if shown {
        egui::Stroke::new(2.0, Tokens::ACCENT)
    } else {
        egui::Stroke::new(1.0, Tokens::text_dim())
    };
    (stroke, egui::StrokeKind::Inside)
}

/// The strip of sheet thumbnails and the status line under it.
fn strip_panel(app: &App, ui: &mut Ui, pv: &mut PrintPreview) {
    let sizes: Vec<tracedraw_core::geometry::Size> =
        sheets(app, pv).pages.iter().map(|p| p.size).collect();
    let key = pv
        .built
        .as_ref()
        .map(|(k, _)| k.clone())
        .unwrap_or_default();
    if pv.thumbs.0 != key || pv.thumbs.1.len() != sizes.len() {
        pv.thumbs = (key, vec![None; sizes.len()]);
    }
    let ppp = ui.ctx().pixels_per_point();
    egui::Panel::bottom("print_preview_strip")
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(Tokens::menu_bar())
                .inner_margin(egui::Margin::symmetric(8, 4)),
        )
        .show(ui, |ui| {
            let mut pending = false;
            egui::ScrollArea::horizontal()
                .id_salt("print_preview_strip_scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // A few new thumbnails a frame, so a long job
                        // never stalls the window.
                        let mut made = 0;
                        for (i, size) in sizes.iter().enumerate() {
                            let aspect = (size.width / size.height.max(1e-6)) as f32;
                            let w = (THUMB_H * aspect).clamp(12.0, THUMB_H * 3.0);
                            let (r, resp) = ui.allocate_exact_size(
                                egui::vec2(w, THUMB_H + 14.0),
                                egui::Sense::click(),
                            );
                            let pic = egui::Rect::from_min_size(r.min, egui::vec2(w, THUMB_H));
                            if ui.is_rect_visible(r) && pv.thumbs.1[i].is_none() {
                                if made < 4 {
                                    made += 1;
                                    let page = sheets(app, pv).pages.get(i).cloned();
                                    let px = f64::from(THUMB_H * ppp) / size.height.max(1e-6);
                                    let whole = tracedraw_core::geometry::Rect::new(
                                        0.0,
                                        0.0,
                                        size.width,
                                        size.height,
                                    );
                                    if let Some(img) = page.and_then(|p| render_part(&p, whole, px))
                                    {
                                        pv.thumbs.1[i] = Some(ui.ctx().load_texture(
                                            format!("print_preview_thumb_{i}"),
                                            img,
                                            egui::TextureOptions::LINEAR,
                                        ));
                                    }
                                } else {
                                    pending = true;
                                }
                            }
                            let p = ui.painter();
                            p.rect_filled(pic, 0.0, egui::Color32::WHITE);
                            if let Some(tex) = &pv.thumbs.1[i] {
                                p.image(
                                    tex.id(),
                                    pic,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    egui::Color32::WHITE,
                                );
                            }
                            let shown = i == pv.sheet;
                            let (stroke, kind) = thumb_frame(shown);
                            p.rect_stroke(pic, 0.0, stroke, kind);
                            p.text(
                                egui::pos2(pic.center().x, pic.bottom() + 2.0),
                                egui::Align2::CENTER_TOP,
                                (i + 1).to_string(),
                                egui::FontId::proportional(11.0),
                                if shown {
                                    Tokens::ACCENT
                                } else {
                                    Tokens::text_dim()
                                },
                            );
                            let resp = resp.rich_tip(trf(
                                "print.sheet_of",
                                &[
                                    ("n", &(i + 1).to_string()),
                                    ("total", &sizes.len().to_string()),
                                ],
                            ));
                            if resp.clicked() {
                                pv.sheet = i;
                                pv.plate = 0;
                            }
                        }
                    });
                });
            if pending {
                ui.ctx().request_repaint();
            }
            // The sheet shown, what goes to the printer and the pointer.
            ui.horizontal_wrapped(|ui| {
                let dim = |t: String| egui::RichText::new(t).color(Tokens::text_dim()).size(11.0);
                if let Some(size) = sizes.get(pv.sheet) {
                    ui.label(dim(trf(
                        "print.status_sheet_size",
                        &[("w", &fmt_mm(size.width)), ("h", &fmt_mm(size.height))],
                    )));
                    ui.separator();
                }
                let shown = pv.sheet;
                let tiled = pv.st.tile;
                let (cells, name) = sheets(app, pv)
                    .pages
                    .get(shown)
                    .map(|p| (sheet_cells(p).len(), p.name.clone()))
                    .unwrap_or_default();
                // Tiled, the tile's name (sheet and tile: "1 A2").
                ui.label(dim(if tiled {
                    format!("{}: {name}", tr("print.tile"))
                } else {
                    trf("print.status_pages_on_sheet", &[("n", &cells.to_string())])
                }));
                ui.separator();
                ui.label(dim(if pv.st.separations {
                    let n = pv.inks.iter().filter(|on| **on).count();
                    trf("print.status_separations", &[("n", &n.to_string())])
                } else {
                    tr("print.composite")
                }));
                ui.separator();
                let total = pages_to_print(sizes.len(), &pv.st, &pv.inks);
                ui.label(dim(trf(
                    "print.status_to_print",
                    &[("n", &total.to_string())],
                )));
                if let Some((x, y)) = pv.pointer_mm {
                    ui.separator();
                    ui.label(dim(trf(
                        "print.status_pointer",
                        &[("x", &fmt_mm(x)), ("y", &fmt_mm(y))],
                    )));
                }
            });
        });
}

/// The rulers along the view's top and left, in millimetres from the
/// sheet's top left corner, with the pointer marked on them.
fn paint_rulers(
    p: &egui::Painter,
    full: egui::Rect,
    area: egui::Rect,
    sheet: egui::Rect,
    k: f64,
    pointer: Option<egui::Pos2>,
) {
    use crate::ui::rulers::{label, major_step, ticks, TickKind};
    let bg = Tokens::menu_bar();
    let ink = Tokens::text_dim();
    let top = egui::Rect::from_min_max(
        egui::pos2(area.left(), full.top()),
        egui::pos2(full.right(), area.top()),
    );
    let side = egui::Rect::from_min_max(
        egui::pos2(full.left(), area.top()),
        egui::pos2(area.left(), full.bottom()),
    );
    p.rect_filled(top, 0.0, bg);
    p.rect_filled(side, 0.0, bg);
    p.rect_filled(egui::Rect::from_min_max(full.min, area.min), 0.0, bg);
    let major = major_step(k);
    let font = egui::FontId::proportional(9.0);
    let length = |kind: TickKind| match kind {
        TickKind::Major => RULER_W * 0.6,
        TickKind::Half => RULER_W * 0.35,
        TickKind::Minor => RULER_W * 0.2,
    };
    // Along the top: x from the sheet's left edge.
    let v0 = f64::from(top.left() - sheet.left()) / k;
    let v1 = f64::from(top.right() - sheet.left()) / k;
    for (v, kind) in ticks(v0, v1, k, 10) {
        let x = sheet.left() + (v * k) as f32;
        if x < top.left() || x > top.right() {
            continue;
        }
        p.line_segment(
            [
                egui::pos2(x, top.bottom()),
                egui::pos2(x, top.bottom() - length(kind)),
            ],
            egui::Stroke::new(1.0, ink),
        );
        if kind == TickKind::Major {
            // A number that would run past the ruler's end is left out.
            let galley = p.layout_no_wrap(label(v, major), font.clone(), ink);
            if x + 2.0 + galley.size().x <= top.right() {
                p.galley(egui::pos2(x + 2.0, top.top()), galley, ink);
            }
        }
    }
    // Down the left: y from the sheet's top edge.
    let v0 = f64::from(side.top() - sheet.top()) / k;
    let v1 = f64::from(side.bottom() - sheet.top()) / k;
    for (v, kind) in ticks(v0, v1, k, 10) {
        let y = sheet.top() + (v * k) as f32;
        if y < side.top() || y > side.bottom() {
            continue;
        }
        p.line_segment(
            [
                egui::pos2(side.right(), y),
                egui::pos2(side.right() - length(kind), y),
            ],
            egui::Stroke::new(1.0, ink),
        );
        if kind == TickKind::Major {
            // Turned a quarter, reading upwards, as the drawing window's
            // left ruler has them.
            let galley = p.layout_no_wrap(label(v, major), font.clone(), ink);
            let at = egui::pos2(side.left() + 1.0, y - 2.0);
            if at.y - galley.size().x < side.top() {
                continue;
            }
            p.add(
                egui::epaint::TextShape::new(at, galley, ink)
                    .with_angle(-std::f32::consts::FRAC_PI_2),
            );
        }
    }
    if let Some(at) = pointer {
        let mark = egui::Stroke::new(1.0, Tokens::ACCENT);
        p.line_segment(
            [egui::pos2(at.x, top.top()), egui::pos2(at.x, top.bottom())],
            mark,
        );
        p.line_segment(
            [
                egui::pos2(side.left(), at.y),
                egui::pos2(side.right(), at.y),
            ],
            mark,
        );
    }
}

/// A length in millimetres with at most one decimal.
fn fmt_mm(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

/// Fits the sheet's width to the view, its top in view.
fn fit_width(pv: &mut PrintPreview, app: &App) {
    let area = pv.view_area;
    if !area.is_positive() {
        return;
    }
    let sheet = pv.sheet;
    let Some(size) = sheets(app, pv).pages.get(sheet).map(|p| p.size) else {
        return;
    };
    let fit = fit_zoom(area, size);
    let k = (f64::from(area.width()) - 48.0).max(10.0) / size.width.max(1e-6);
    pv.zoom = ((k / fit) as f32).clamp(ZOOM_MIN, ZOOM_MAX);
    let h = (size.height * fit * f64::from(pv.zoom)) as f32;
    pv.pan = egui::vec2(0.0, (h - area.height()) * 0.5 + 24.0);
}

/// Zooms the view by `factor` (within the zoom range) keeping the sheet
/// point under `at` where it is; `center` is the view's middle.
pub fn zoom_at(pv: &mut PrintPreview, at: egui::Pos2, center: egui::Pos2, factor: f32) {
    if !(factor.is_finite() && factor > 0.0) {
        return;
    }
    let old = pv.zoom;
    pv.zoom = (pv.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    let f = pv.zoom / old.max(1e-6);
    // The sheet's middle moves away from the point by the same factor.
    let mid = center + pv.pan;
    let mid = at + (mid - at) * f;
    pv.pan = mid - center;
}

/// The zoom (over the fitted sheet) and pan that show the sheet part
/// `want` (points on screen, as drawn now) filling the view `area`.
pub fn zoom_to_box(pv: &mut PrintPreview, want: egui::Rect, area: egui::Rect) {
    if want.width() < 4.0 || want.height() < 4.0 {
        return;
    }
    let f = (area.width() / want.width()).min(area.height() / want.height());
    let at = want.center();
    zoom_at(pv, at, area.center(), f);
    // The box's middle stayed under `at`; then it goes to the view's
    // middle.
    pv.pan += area.center() - at;
}

/// The view's fitted zoom: points per millimetre that fit the sheet in
/// `area`, with a margin round it.
pub fn fit_zoom(area: egui::Rect, sheet: tracedraw_core::geometry::Size) -> f64 {
    let margin = 24.0;
    ((area.width() - 2.0 * margin) as f64 / sheet.width.max(1e-6))
        .min((area.height() - 2.0 * margin) as f64 / sheet.height.max(1e-6))
        .max(0.01)
}

/// Keeps part of the sheet in view: the pan may take it no further than
/// leaving 40 points of it inside `area`.
fn clamp_pan(pan: egui::Vec2, area: egui::Rect, sheet: egui::Vec2) -> egui::Vec2 {
    let room = (area.size() + sheet) * 0.5 - egui::vec2(40.0, 40.0);
    let room = room.max(egui::Vec2::ZERO);
    egui::vec2(pan.x.clamp(-room.x, room.x), pan.y.clamp(-room.y, room.y))
}

/// The part `part` (mm, y up) of the sheet as a bitmap of `px_per_mm`
/// pixels per millimetre on white paper.
fn render_part(
    page: &tracedraw_core::document::Page,
    part: tracedraw_core::geometry::Rect,
    px_per_mm: f64,
) -> Option<egui::ColorImage> {
    let w = (part.width() * px_per_mm).round().max(1.0) as u32;
    let h = (part.height() * px_per_mm).round().max(1.0) as u32;
    let mut doc = Document::new("sheet", page.size);
    doc.pages = vec![page.clone()];
    let opts = tracedraw_render::RenderOptions {
        width: w,
        height: h,
        view: tracedraw_render::ViewTransform {
            zoom: px_per_mm,
            origin_x: -part.x0 * px_per_mm,
            origin_y: part.y1 * px_per_mm,
            stretch_y: 1.0,
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

    /// With Print separations on, the preview pages through each plate of
    /// a sheet after the sheet, as File > Print sends one page per plate:
    /// Cyan, Magenta, Yellow, Black and each spot ink, each in grey; an
    /// unticked ink has no page. Before, only the inks ticked together
    /// were shown, never a page per plate (#2379).
    #[test]
    fn separations_preview_a_page_per_plate() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        doc.inks.push(tracedraw_core::document::Ink {
            id: 7,
            name: "PANTONE 186 C".into(),
            cmyk: [0.0, 1.0, 0.8, 0.05],
        });
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
        doc.pages[0].layers[0].shapes.push(s.clone());
        s.id = doc.ids_mut().shape();
        s.fill = Fill::Solid(doc.inks[0].color(0.6));
        doc.pages[0].layers[0].shapes.push(s);
        let views = preview_views(&doc, &[true; 5], true);
        assert_eq!(views, vec![(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (0, 5)]);
        let grey = |plate, shape: usize| {
            let p = view_page(&doc, (0, plate), &[true; 5], true, false).expect("page");
            match p.layers[0].shapes[shape].fill {
                Fill::Solid(Color::Gray { v }) => v,
                ref f => panic!("{f:?}"),
            }
        };
        assert!(grey(1, 0).abs() < 1e-6, "cyan plate full");
        assert!((grey(2, 0) - 0.5).abs() < 1e-6, "magenta plate half");
        assert!((grey(4, 0) - 1.0).abs() < 1e-6, "black plate empty");
        assert!((grey(5, 1) - 0.4).abs() < 1e-6, "spot plate at its tint");
        assert!(
            (grey(5, 0) - 1.0).abs() < 1e-6,
            "process ink off the spot plate"
        );
        assert!(
            (grey(1, 1) - 1.0).abs() < 1e-6,
            "spot ink off the cyan plate"
        );
        // An unticked ink has no page; separations off, the sheets alone.
        let views = preview_views(&doc, &[true, false, true, true, true], true);
        assert!(!views.contains(&(0, 2)), "{views:?}");
        assert_eq!(preview_views(&doc, &[true; 5], false), vec![(0, 0)]);
        // A plate in the colour of its ink, with that option.
        let p = view_page(&doc, (0, 1), &[true; 5], true, true).expect("page");
        assert!(
            matches!(p.layers[0].shapes[0].fill, Fill::Solid(Color::Cmyk { c, m, .. }) if c == 1.0 && m == 0.0),
            "{:?}",
            p.layers[0].shapes[0].fill
        );
    }

    /// #1924: the canvas's Printable Area and Print Preview use the
    /// printer's hard margins as the system reports them, 5 mm only
    /// until then or when it does not say; the preview warns when the
    /// artwork reaches into the edge the printer cannot print.
    #[test]
    fn the_printable_area_is_the_printers_own() {
        use crate::printers::HardMargins;
        use tracedraw_core::geometry::Rect;
        let mut app = App::headless();
        let page = Rect::new(0.0, 0.0, 210.0, 297.0);
        app.settings.printing.printer = Some("Office Laser".into());
        let laser = HardMargins {
            left: 4.0,
            right: 6.0,
            top: 3.0,
            bottom: 12.0,
        };
        // Not read yet: the 5 mm guess.
        app.printer_margins.set(Some("Office Laser"), None);
        assert_eq!(
            crate::canvas::printable_rect(&app, page),
            Rect::new(5.0, 5.0, 205.0, 292.0)
        );
        app.printer_margins.set(Some("Office Laser"), Some(laser));
        assert_eq!(app.printable_margins(210.0, 297.0), (laser, true));
        assert_eq!(
            crate::canvas::printable_rect(&app, page),
            Rect::new(4.0, 12.0, 204.0, 294.0)
        );
        // A page on its side: the margins turned with it.
        let side = Rect::new(0.0, 0.0, 297.0, 210.0);
        assert_eq!(
            crate::canvas::printable_rect(&app, side),
            Rect::new(12.0, 6.0, 294.0, 206.0)
        );

        // Print Preview: a frame 2 mm inside the page reaches into the
        // 4 mm left edge; one 15 mm inside does not.
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(2.0, 20.0, 100.0, 100.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        let st = PrintState::default();
        let sheets = print_sheets(&app, &st);
        let sheet = &sheets.pages[0];
        assert!(art_in_unprintable_edge(sheet, laser));
        app.run(tracedraw_core::Command::TransformShapes {
            shapes: vec![id],
            transform: tracedraw_core::geometry::Affine::translate((13.0, 0.0)),
        });
        let sheets = print_sheets(&app, &st);
        assert!(!art_in_unprintable_edge(&sheets.pages[0], laser));
    }

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
            shown_page_inks(&doc, 0, inks, true, false)
                .expect("page")
                .layers[0]
                .shapes[0]
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

    /// Four squares of pure cyan, magenta, yellow and black, each with a
    /// black outline, as in the report (#1652). One ink ticked alone
    /// shows its plate in grey: the cyan square black, which read as
    /// black ink on the cyan plate. With "Show one ink in its color" the
    /// one ink shows the colour it prints, as several inks do: the cyan
    /// square cyan, the others and every outline paper white, and no
    /// black anywhere unless Black is the ink ticked.
    #[test]
    fn one_ink_alone_shows_in_its_colour_when_asked() {
        use tracedraw_core::{Color, Fill, Shape, ShapeKind};
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let inks = [
            (1.0, 0.0, 0.0, 0.0),
            (0.0, 1.0, 0.0, 0.0),
            (0.0, 0.0, 1.0, 0.0),
            (0.0, 0.0, 0.0, 1.0),
        ];
        for (i, (c, m, y, k)) in inks.into_iter().enumerate() {
            let id = doc.ids_mut().shape();
            let x = 10.0 + 20.0 * i as f64;
            let mut s = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: tracedraw_core::geometry::Rect::new(x, 10.0, x + 15.0, 25.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = Fill::Solid(Color::Cmyk { c, m, y, k });
            s.stroke = Some(tracedraw_core::Stroke {
                color: Color::Cmyk {
                    c: 0.0,
                    m: 0.0,
                    y: 0.0,
                    k: 1.0,
                },
                ..Default::default()
            });
            doc.pages[0].layers[0].shapes.push(s);
        }
        // What the squares and their outlines show as, in CMYK.
        let cmyk = |c: &Color| match c.convert_to("CMYK") {
            Color::Cmyk { c, m, y, k } => [c, m, y, k],
            other => panic!("{other:?}"),
        };
        let shown = |only: usize, in_color: bool| {
            let mut ticked = [false; 4];
            ticked[only] = true;
            let p = shown_page_inks(&doc, 0, &ticked, true, in_color).expect("page");
            p.layers[0]
                .shapes
                .iter()
                .map(|s| {
                    let fill = match &s.fill {
                        Fill::Solid(c) => cmyk(c),
                        f => panic!("{f:?}"),
                    };
                    (fill, cmyk(&s.stroke.as_ref().expect("outline").color))
                })
                .collect::<Vec<_>>()
        };
        let paper = [0.0f32; 4];
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.02);
        for only in 0..4 {
            // As a plate: grey, the ink's own square black.
            let plate = shown(only, false);
            for (i, (fill, line)) in plate.iter().enumerate() {
                let want = if i == only { 1.0 } else { 0.0 };
                assert!(
                    near(*fill, [0.0, 0.0, 0.0, want]),
                    "plate {only}, square {i}: {fill:?}"
                );
                let want = if only == 3 { 1.0 } else { 0.0 };
                assert!(near(*line, [0.0, 0.0, 0.0, want]), "plate {only}: {line:?}");
            }
            // In its colour: the ink alone, nothing of the others.
            let color = shown(only, true);
            let mut ink = paper;
            ink[only] = 1.0;
            for (i, (fill, line)) in color.iter().enumerate() {
                let want = if i == only { ink } else { paper };
                assert!(near(*fill, want), "ink {only}, square {i}: {fill:?}");
                let want = if only == 3 { ink } else { paper };
                assert!(near(*line, want), "ink {only}: {line:?}");
            }
        }
        // Several inks and every ink show the same either way.
        for ticked in [[true, false, false, true], [true; 4]] {
            let a = shown_page_inks(&doc, 0, &ticked, true, false).expect("page");
            let b = shown_page_inks(&doc, 0, &ticked, true, true).expect("page");
            for (x, y) in a.layers[0].shapes.iter().zip(b.layers[0].shapes.iter()) {
                assert_eq!(x.fill, y.fill);
            }
        }
    }

    /// Pictures split by ink in the preview too (#1659): one ink shows
    /// its plate in grey, some inks the colours they print, every ink
    /// the picture as it is.
    #[test]
    fn plates_split_pictures_by_ink() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let img =
            image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 100]).expect("pixels");
        let png: tracedraw_core::SharedBytes = crate::bitmap_fx::encode(&img).expect("png").into();
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Bitmap {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
                width_px: 2,
                height_px: 1,
                png: png.clone(),
                fx: None,
            },
        );
        s.fill = Fill::None;
        s.stroke = None;
        doc.pages[0].layers[0].shapes.push(s);
        let pixels = |inks: &[bool]| -> Vec<[u8; 4]> {
            let p = shown_page_inks(&doc, 0, inks, true, false).expect("page");
            let ShapeKind::Bitmap { png, .. } = &p.layers[0].shapes[0].kind else {
                panic!("a picture");
            };
            let img = crate::bitmap_fx::decode(png).expect("decodes");
            img.pixels().map(|p| p.0).collect()
        };
        // Yellow alone: the red pixel full ink, the blue one none; the
        // alpha stays.
        let yellow = pixels(&[false, false, true, false]);
        assert!(yellow[0][0] < 26 && yellow[0][3] == 255, "{yellow:?}");
        assert!(yellow[1][0] > 229 && yellow[1][3] == 100, "{yellow:?}");
        // Cyan and yellow: the red pixel prints yellow only.
        let some = pixels(&[true, false, true, false]);
        let [r, g, b, _] = some[0];
        assert!(r > 200 && g > 200 && b < 80, "{some:?}");
        // Every ink: the picture as it is.
        let all = shown_page_inks(&doc, 0, &[true; 4], true, false).expect("page");
        assert!(matches!(
            &all.layers[0].shapes[0].kind,
            ShapeKind::Bitmap { png: p, .. } if tracedraw_core::SharedBytes::ptr_eq(p, &png)
        ));
    }

    /// With Print separations on, a box for each ink stands under that
    /// option in the left panel, as the request's sketch has them; they
    /// were in the top bar, where many spot inks ran past the window
    /// (#571).
    #[test]
    fn the_ink_boxes_stand_under_print_separations() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let st = PrintState {
            separations: true,
            ..PrintState::default()
        };
        app.print_preview = Some(PrintPreview::new(st));
        let find = |shapes: &[egui::epaint::ClippedShape], text: &str| {
            shapes.iter().find_map(|c| match &c.shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == text => Some(t.pos),
                _ => None,
            })
        };
        let mut shapes = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| show(&mut app, ui),
            );
            out.textures_delta.clear();
            shapes = out.shapes;
        }
        assert_eq!(app.print_preview.as_ref().map(|p| p.inks.len()), Some(4));
        let option = find(&shapes, &tr("dialog.separations")).expect("the option");
        for ink in ["fxp.cyan", "fxp.magenta", "fxp.yellow", "fxp.black"] {
            let at = find(&shapes, &tr(ink)).expect(ink);
            assert!(at.x < 330.0, "{ink} in the left panel: {at:?}");
            assert!(at.y > option.y && at.x > option.x, "{ink} under it: {at:?}");
        }
        // The four process inks share a row, side by side, instead of a
        // line each (#1737).
        let cyan = find(&shapes, &tr("fxp.cyan")).expect("cyan");
        let mut x = cyan.x;
        for ink in ["fxp.magenta", "fxp.yellow", "fxp.black"] {
            let at = find(&shapes, &tr(ink)).expect(ink);
            assert!((at.y - cyan.y).abs() < 2.0, "{ink} beside cyan: {at:?}");
            assert!(at.x > x, "{ink} right of the one before: {at:?}");
            x = at.x;
        }
    }

    /// Print Preview shows the printer choice, the Properties button of
    /// the printer chosen and the copies in its top bar, beside Print
    /// (they were at the top of the left panel, #953), the printers
    /// coming in when the system has listed them; the Print button prints
    /// to the printer chosen there (#792).
    #[test]
    fn the_preview_offers_the_printer_and_the_copies() {
        use crate::printers::{Printer, PrinterState};
        fn list() -> Vec<Printer> {
            vec![
                Printer {
                    name: "Studio Laser".into(),
                    default: true,
                    state: PrinterState::Ready,
                    ..Default::default()
                },
                Printer {
                    name: "Plotter".into(),
                    ..Default::default()
                },
            ]
        }
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.printers_seen.clear();
        app.print_preview = Some(PrintPreview::new(PrintState {
            looking_for_printers: true,
            ..PrintState::default()
        }));
        app.fetch_printers(list);
        let texts = |shapes: &[egui::epaint::ClippedShape]| -> Vec<(String, egui::Pos2)> {
            fn walk(s: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
                match s {
                    egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos)),
                    egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            shapes.iter().for_each(|c| walk(&c.shape, &mut out));
            out
        };
        // A window wide enough for the whole bar, with the pictures of
        // its buttons, on one row.
        for _ in 0..200 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| show(&mut app, ui),
            );
            out.textures_delta.clear();
            if app
                .print_preview
                .as_ref()
                .is_some_and(|p| !p.st.looking_for_printers)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let st = &app.print_preview.as_ref().expect("preview").st;
        assert_eq!(st.printers.len(), 2, "the printers came in");
        assert_eq!(st.printer.as_deref(), Some("Studio Laser"), "the default");
        // One more frame to show them.
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| show(&mut app, ui),
        );
        out.textures_delta.clear();
        let t = texts(&out.shapes);
        let at = |label: &str| t.iter().find(|(s, _)| s == label).map(|(_, p)| *p);
        let printer = at("Studio Laser").expect("the printer chosen");
        let copies = at(&tr("print.number_of_copies")).expect("the copies");
        let all_pages = at(&tr("dialog.all_pages")).expect("the other options");
        let print = at(&tr("dialog.print")).expect("the Print button");
        // Print keeps the preview open; Print and Close beside it (#4072).
        let and_close = at(&tr("print.print_and_close")).expect("Print and Close");
        assert!((and_close.y - print.y).abs() < 8.0 && and_close.x > print.x);
        // In the top bar, after Print, the left panel starting with the
        // pages to print (#953).
        assert!((printer.y - print.y).abs() < 8.0, "{printer:?} {print:?}");
        assert!((copies.y - print.y).abs() < 8.0, "{copies:?} {print:?}");
        assert!(
            printer.x > print.x && copies.x > printer.x,
            "{print:?} {printer:?} {copies:?}"
        );
        assert!(all_pages.x < 330.0 && all_pages.y > printer.y + 15.0);
        assert!(at(&tr("print.collate")).is_some());
        assert!(at(&tr("print.printer")).is_some_and(|p| p.y < all_pages.y));
    }

    /// Transform's corner handles keep the page's proportions: both
    /// stretches grow by the one factor and the opposite corner stays
    /// put; a side handle stretches its axis alone. A corner stretched
    /// each axis by its own move, so a drawing fitted to the sheet by a
    /// corner came out distorted (#1543).
    #[test]
    fn a_corner_handle_keeps_the_page_s_proportions() {
        let cell = tracedraw_core::geometry::Rect::new(10.0, 10.0, 110.0, 160.0);
        let mut st = PrintState::default();
        // The top right corner 20 mm right and 5 mm up: the width's 20 %
        // wins and the height follows it.
        drag_transform_handle(&mut st, (1, 1), cell, 20.0, 5.0);
        assert!(
            (st.art_stretch.0 - 1.2).abs() < 1e-9,
            "{:?}",
            st.art_stretch
        );
        assert!(
            (st.art_stretch.1 - 1.2).abs() < 1e-9,
            "{:?}",
            st.art_stretch
        );
        // The bottom left corner stays where it was: the middle moves by
        // half the growth on each axis.
        assert!((st.art_offset.0 - 10.0).abs() < 1e-9, "{:?}", st.art_offset);
        assert!((st.art_offset.1 - 15.0).abs() < 1e-9, "{:?}", st.art_offset);
        // Inwards from the bottom left corner: both shrink alike.
        drag_transform_handle(&mut st, (-1, -1), cell, 10.0, 60.0);
        assert!(
            (st.art_stretch.0 - st.art_stretch.1).abs() < 1e-9,
            "{:?}",
            st.art_stretch
        );
        assert!(st.art_stretch.0 < 1.2, "{:?}", st.art_stretch);
        // A side handle: its axis alone, the other untouched.
        let mut st = PrintState::default();
        drag_transform_handle(&mut st, (-1, 0), cell, -30.0, 7.0);
        assert!(
            (st.art_stretch.0 - 1.3).abs() < 1e-9,
            "{:?}",
            st.art_stretch
        );
        assert_eq!(st.art_stretch.1, 1.0);
        assert!((st.art_offset.0 + 15.0).abs() < 1e-9, "{:?}", st.art_offset);
        assert_eq!(st.art_offset.1, 0.0);
        drag_transform_handle(&mut st, (0, 1), cell, -30.0, 15.0);
        assert!(
            (st.art_stretch.1 - 1.1).abs() < 1e-9,
            "{:?}",
            st.art_stretch
        );
        assert!(
            (st.art_stretch.0 - 1.3).abs() < 1e-9,
            "{:?}",
            st.art_stretch
        );
        // Nothing moves on a move that is not a number or an empty cell.
        let before = st.clone();
        drag_transform_handle(&mut st, (1, 1), cell, f64::NAN, 1.0);
        drag_transform_handle(
            &mut st,
            (1, 1),
            tracedraw_core::geometry::Rect::new(0.0, 0.0, 0.0, 10.0),
            1.0,
            1.0,
        );
        assert_eq!(st.art_stretch, before.art_stretch);
        assert_eq!(st.art_offset, before.art_offset);
    }

    /// Ticking Fit to page puts back a page dragged or stretched on the
    /// sheet: the artwork position and the stretch go back to nothing,
    /// so the page sits fitted on the sheet again. They stayed as they
    /// were, so Fit to page seemed to do nothing to a dragged page
    /// (#1577).
    #[test]
    fn fit_to_page_brings_a_dragged_page_back() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        app.print_preview(None);
        let size = egui::vec2(1400.0, 900.0);
        if let Some(pv) = app.print_preview.as_mut() {
            pv.st.looking_for_printers = false;
            pv.st.fit_to_page = false;
            // As a drag on the sheet and a corner handle leave it.
            pv.st.art_offset = (50.0, -40.0);
            pv.st.art_stretch = (1.5, 1.5);
        }
        let mut texts = Vec::new();
        for _ in 0..4 {
            texts = frame_texts(&mut app, &ctx, size);
        }
        let label = tr("dialog.fit_to_page");
        let at = texts
            .iter()
            .find(|(t, _)| *t == label)
            .map(|(_, r)| r.center())
            .expect("Fit to page shown");
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| show(app, ui),
            );
            out.textures_delta.clear();
        };
        run(&mut app, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            run(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        run(&mut app, Vec::new());
        let st = &app.print_preview.as_ref().expect("preview").st;
        assert!(st.fit_to_page, "the box was ticked");
        assert_eq!(st.art_offset, (0.0, 0.0), "the page is back in place");
        assert_eq!(st.art_stretch, (1.0, 1.0), "and at its size");
        // The Print dialog's Layout tab does the same when Fit to page is
        // chosen there.
        let mut st = PrintState {
            fit_to_page: false,
            art_offset: (12.0, 3.0),
            art_stretch: (2.0, 0.5),
            ..PrintState::default()
        };
        let mut shown = Vec::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        crate::ui::dialogs::print_tab_layout(&app, ui, &mut st);
                    });
                },
            );
            out.textures_delta.clear();
            shown = out.shapes;
        }
        let mut found = Vec::new();
        fn walk(s: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
            match s {
                egui::Shape::Text(t) => {
                    out.push((t.galley.text().to_string(), t.visual_bounding_rect()))
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        shown.iter().for_each(|c| walk(&c.shape, &mut found));
        let at = found
            .iter()
            .find(|(t, _)| *t == label)
            .map(|(_, r)| r.center())
            .expect("Fit to page in the Layout tab");
        let tab = |app: &App, st: &mut PrintState, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        crate::ui::dialogs::print_tab_layout(app, ui, st);
                    });
                },
            );
            out.textures_delta.clear();
        };
        tab(&app, &mut st, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            tab(
                &app,
                &mut st,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        tab(&app, &mut st, Vec::new());
        assert!(st.fit_to_page, "Fit to page chosen in the Layout tab");
        assert_eq!(st.art_offset, (0.0, 0.0));
        assert_eq!(st.art_stretch, (1.0, 1.0));
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

    /// A page smaller than the paper, repeated on it: every copy shows in
    /// the preview, not only the bottom row with white over the rest
    /// (#848).
    #[test]
    fn every_repeated_page_shows_in_the_preview() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(90.0, 50.0));
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 90.0, 50.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(230, 30, 30));
        s.stroke = None;
        doc.pages[0].layers[0].shapes.push(s);
        let o = crate::imposition::Options {
            layout: crate::imposition::Layout::Repeat { cols: 2, rows: 5 },
            sheet: Some(tracedraw_core::geometry::Size::new(210.0, 297.0)),
            ..Default::default()
        };
        let sheets = crate::imposition::impose(&doc, &o);
        let page = &sheets.pages[0];
        let plan = crate::imposition::plan(&[doc.pages[0].size], &o);
        let zoom = 3.0;
        let (w, h) = (630usize, 891usize);
        let whole =
            tracedraw_core::geometry::Rect::new(0.0, 0.0, page.size.width, page.size.height);
        let img = render_part(page, whole, zoom).expect("image");
        assert_eq!(img.size, [w, h]);
        assert_eq!(plan[0].cells.len(), 10);
        for c in &plan[0].cells {
            let m = c.trim.center();
            let (x, y) = ((m.x * zoom) as usize, h - (m.y * zoom) as usize);
            let px = img.pixels[y * w + x];
            assert!(px.r() > 200 && px.g() < 80, "cell at {m:?}: {px:?}");
        }
    }

    /// The options at the left of Print Preview come wider than before
    /// and their edge drags wider, so the longer languages are not cut
    /// off (#862).
    #[test]
    fn the_options_panel_is_wider_and_its_edge_drags() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        app.print_preview(None);
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| show(app, ui),
            );
            out.textures_delta.clear();
        };
        for _ in 0..3 {
            run(&mut app, Vec::new());
        }
        let id = egui::Id::new(PREVIEW_OPTIONS);
        let width = || {
            egui::containers::panel::PanelState::load(&ctx, id)
                .map(|s| s.outer_rect.width())
                .expect("the panel's state")
        };
        let start = width();
        assert!(start >= 355.0, "{start}");
        let right = egui::containers::panel::PanelState::load(&ctx, id)
            .map(|s| s.outer_rect.right())
            .expect("the panel's state");
        let edge = egui::pos2(right - 2.0, 450.0);
        let to = edge + egui::vec2(120.0, 0.0);
        run(&mut app, vec![egui::Event::PointerMoved(edge)]);
        run(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: edge,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        for k in 1..=6 {
            run(
                &mut app,
                vec![egui::Event::PointerMoved(
                    edge + (to - edge) * (k as f32 / 6.0),
                )],
            );
        }
        run(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        run(&mut app, Vec::new());
        assert!(width() > start + 80.0, "{start} -> {}", width());
        assert!(app.print_preview.is_some());
    }

    /// On a phone, upright and on its side, Close shows inside the screen
    /// with a destination name as long as Android's print service, and
    /// nothing of the top bar runs past the screen's right edge (#2666:
    /// the name widened the bar past the screen and Close went with it).
    #[test]
    fn close_stays_on_a_phone_screen() {
        for size in [egui::vec2(360.0, 800.0), egui::vec2(800.0, 360.0)] {
            let ctx = crate::theme::ui_context();
            let mut app = App::headless();
            app.show_welcome = false;
            app.print_preview(None);
            if let Some(pv) = app.print_preview.as_mut() {
                pv.st.looking_for_printers = false;
                pv.st.destination = 0;
                pv.st.printer =
                    Some("System printer (through the Android print service)".to_string());
            }
            let mut shapes = Vec::new();
            for _ in 0..6 {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| show(&mut app, ui),
                );
                out.textures_delta.clear();
                shapes = out.shapes;
            }
            fn walk(s: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
                match s {
                    egui::Shape::Text(t) => {
                        out.push((t.galley.text().to_string(), t.visual_bounding_rect()))
                    }
                    egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                    _ => {}
                }
            }
            let mut texts = Vec::new();
            for c in &shapes {
                walk(&c.shape, &mut texts);
            }
            let close = tr("dialog.close");
            let at = texts
                .iter()
                .find(|(t, _)| *t == close)
                .map(|(_, r)| *r)
                .expect("Close is drawn");
            assert!(
                at.right() <= size.x && at.bottom() <= size.y,
                "{size:?}: Close at {at:?}"
            );
            let bar =
                egui::containers::panel::PanelState::load(&ctx, egui::Id::new("print_preview_bar"))
                    .map(|s| s.outer_rect)
                    .expect("the bar's state");
            assert!(bar.right() <= size.x + 0.5, "{size:?}: bar {bar:?}");
            for (text, r) in &texts {
                if r.top() < bar.bottom() && !text.trim().is_empty() {
                    assert!(r.right() <= size.x + 1.0, "{size:?}: {text:?} at {r:?}");
                }
            }
        }
    }

    /// In every language, at the main window's default size, no label of
    /// the options panel is cut at its right: the panel takes the width of
    /// its widest row (Spanish in the report, #862), with the layout
    /// options that add rows (Repeat, a sheet size, page information) on.
    #[test]
    fn no_option_is_cut_off_in_any_language() {
        let size = crate::splash::DEFAULT_SIZE;
        for code in [
            "en", "es", "pt-BR", "fr", "de", "it", "nl", "pl", "cs", "ru", "tr", "id", "ar", "hi",
            "bn", "ja", "zh-CN", "zh-TW", "lo", "vi", "eo",
        ] {
            crate::i18n::with_thread_language(code, || {
                let ctx = crate::theme::ui_context();
                let mut app = App::headless();
                app.show_welcome = false;
                app.print_preview(None);
                if let Some(pv) = app.print_preview.as_mut() {
                    pv.st.looking_for_printers = false;
                    pv.st.layout_kind = 3;
                    pv.st.sheet = 1;
                    pv.st.page_info = true;
                    pv.st.copies = 2;
                }
                let mut shapes = Vec::new();
                for _ in 0..8 {
                    let mut out = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| show(&mut app, ui),
                    );
                    out.textures_delta.clear();
                    shapes = out.shapes;
                }
                let panel =
                    egui::containers::panel::PanelState::load(&ctx, egui::Id::new(PREVIEW_OPTIONS))
                        .map(|s| s.outer_rect)
                        .expect("the panel's state");
                fn walk(
                    s: &egui::Shape,
                    clip: egui::Rect,
                    out: &mut Vec<(String, egui::Rect, egui::Rect)>,
                ) {
                    match s {
                        egui::Shape::Text(t) => {
                            out.push((t.galley.text().to_string(), t.visual_bounding_rect(), clip))
                        }
                        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
                        _ => {}
                    }
                }
                let mut texts = Vec::new();
                for c in &shapes {
                    walk(&c.shape, c.clip_rect, &mut texts);
                }
                // The top bar with the printer and the copies (#953)
                // stays inside the window, wrapping when it must.
                let close = tr("dialog.close");
                for (text, r, _) in &texts {
                    if r.top() < 80.0 {
                        assert!(
                            r.right() <= size.x + 1.0,
                            "{code}: {text:?} past the window {r:?}"
                        );
                    }
                }
                assert!(
                    texts
                        .iter()
                        .any(|(t, r, _)| *t == close && r.right() <= size.x),
                    "{code}: Close shows"
                );
                // The panel's texts: those drawn within its clip.
                let ours: Vec<_> = texts
                    .iter()
                    .filter(|(_, r, clip)| {
                        clip.right() <= panel.right() + 1.0 && r.left() < panel.right()
                    })
                    .collect();
                assert!(ours.len() > 20, "{code}: the options drew ({})", ours.len());
                for (text, r, clip) in ours {
                    if text.trim().is_empty() || r.bottom() < clip.top() || r.top() > clip.bottom()
                    {
                        continue;
                    }
                    assert!(
                        r.right() <= clip.right() + 1.0 && r.left() >= clip.left() - 1.0,
                        "{code}: {text:?} cut off: {r:?} in {clip:?} (panel {panel:?})"
                    );
                }
                assert!(panel.width() <= size.x * 0.5 + 1.0, "{code}: {panel:?}");
            });
        }
    }
    /// Texts drawn by one frame of the preview at `size`, with where.
    fn frame_texts(
        app: &mut App,
        ctx: &egui::Context,
        size: egui::Vec2,
    ) -> Vec<(String, egui::Rect)> {
        fn walk(s: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
            match s {
                egui::Shape::Text(t) => {
                    out.push((t.galley.text().to_string(), t.visual_bounding_rect()))
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| show(app, ui),
        );
        out.textures_delta.clear();
        let mut texts = Vec::new();
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
        texts
    }

    /// The options at the left stand two to a row, as the report's sketch
    /// has them: All pages beside Current page, Pages with its list beside
    /// Selection, Fit to page beside Odd and even pages, then Bleed beside
    /// Crop marks, Registration marks beside Color bars, Densitometer
    /// scale beside Page information and Mirror beside Invert (#951).
    #[test]
    fn the_options_stand_two_to_a_row() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        app.print_preview(None);
        let mut texts = Vec::new();
        for _ in 0..4 {
            texts = frame_texts(&mut app, &ctx, egui::vec2(1400.0, 900.0));
        }
        let at = |key: &str| {
            let label = tr(key);
            texts
                .iter()
                .find(|(t, _)| *t == label)
                .map(|(_, r)| r.left_top())
                .unwrap_or_else(|| panic!("{key} shown"))
        };
        for (left, right) in [
            ("dialog.all_pages", "print.range_page"),
            ("print.range_pages", "print.range_selection"),
            ("dialog.fit_to_page", "print.odd_and_even"),
            ("dialog.print_bleed", "dialog.crop_marks"),
            ("dialog.registration_marks", "print.color_bars"),
            ("print.densitometer", "print.page_info_mark"),
            ("dialog.mirror", "dialog.invert"),
            ("print.grayscale", "print.reverse_order"),
        ] {
            let (l, r) = (at(left), at(right));
            assert!((l.y - r.y).abs() < 4.0, "{left} {l:?} beside {right} {r:?}");
            assert!(
                r.x > l.x + 40.0 && r.x < 480.0,
                "{right} {r:?} right of {left}"
            );
        }
        // The rows follow each other down the panel.
        assert!(at("print.range_pages").y > at("dialog.all_pages").y);
        assert!(at("dialog.print_bleed").y > at("dialog.fit_to_page").y);
        assert!(at("dialog.mirror").y > at("print.densitometer").y);
        assert!(at("print.grayscale").y > at("dialog.mirror").y);
    }

    /// The bleed limit is set beside Bleed, before Crop marks, and is the
    /// Print dialog's setting: a value typed there is what prints and
    /// what the Print dialog keeps (#1653).
    #[test]
    fn the_bleed_limit_is_set_beside_bleed() {
        let ctx = crate::theme::ui_context();
        let size = egui::vec2(1400.0, 900.0);
        let mut app = App::headless();
        app.show_welcome = false;
        app.print_preview(None);
        if let Some(pv) = app.print_preview.as_mut() {
            pv.st.looking_for_printers = false;
            pv.st.bleed = true;
            pv.st.bleed_limit = 4.0;
        }
        let mut texts = Vec::new();
        for _ in 0..4 {
            texts = frame_texts(&mut app, &ctx, size);
        }
        let find = |texts: &[(String, egui::Rect)], f: &dyn Fn(&str) -> bool| {
            texts.iter().find(|(t, _)| f(t)).map(|(_, r)| *r)
        };
        let bleed_label = tr("dialog.print_bleed");
        let bleed = find(&texts, &|t| t == bleed_label).expect("Bleed shown");
        let crop_label = tr("dialog.crop_marks");
        let crop = find(&texts, &|t| t == crop_label).expect("Crop marks shown");
        let field = texts
            .iter()
            .filter(|(t, r)| {
                t.ends_with(" mm")
                    && (r.center().y - bleed.center().y).abs() < 6.0
                    && r.left() > bleed.right()
            })
            .map(|(t, r)| (t.clone(), *r))
            .next()
            .expect("the bleed limit beside Bleed");
        assert!(field.0.starts_with('4'), "{:?}", field.0);
        assert!(field.1.right() < crop.left(), "{field:?} before {crop:?}");
        // Typed in: the preview's settings, which print and which the
        // Print dialog and Options > Printing keep, take it.
        let at = field.1.center();
        let send = |app: &mut App, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| show(app, ui),
            );
            out.textures_delta.clear();
        };
        send(&mut app, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            send(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
        send(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        send(&mut app, vec![egui::Event::Text("7".into())]);
        send(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        send(&mut app, Vec::new());
        let st = app
            .print_preview
            .as_ref()
            .map(|pv| pv.st.clone())
            .expect("open");
        assert_eq!(st.bleed_limit, 7.0);
        let doc = app.doc().clone();
        assert_eq!(crate::export::print_layout(&app, &st, &doc).bleed, 7.0);
        app.settings.printing.remember(&st);
        assert_eq!(app.settings.printing.state().bleed_limit, 7.0);
    }

    /// The pages printed are chosen in the preview: the current page, a
    /// list, odd or even pages; the preview's sheets follow (#951).
    #[test]
    fn the_preview_prints_the_pages_chosen() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        let size = app.doc().pages[0].size;
        for _ in 0..4 {
            app.run(tracedraw_core::Command::AddPage {
                name: None,
                size,
                layer: None,
            });
        }
        let n = app.doc().pages.len();
        assert_eq!(n, 5);
        app.print_preview(None);
        let count = |app: &mut App| {
            let _ = frame_texts(app, &ctx, egui::vec2(1400.0, 900.0));
            let pv = app.print_preview.as_mut().expect("preview");
            let st = pv.st.clone();
            print_sheets(app, &st).pages.len()
        };
        assert_eq!(count(&mut app), 5);
        let set = |app: &mut App, f: &dyn Fn(&mut PrintState)| {
            if let Some(pv) = app.print_preview.as_mut() {
                f(&mut pv.st);
            }
        };
        set(&mut app, &|st| st.parity = 1);
        assert_eq!(count(&mut app), 3, "pages 1, 3 and 5");
        set(&mut app, &|st| st.parity = 2);
        assert_eq!(count(&mut app), 2, "pages 2 and 4");
        set(&mut app, &|st| {
            st.range = 2;
            st.pages = "2-5".into();
            st.parity = 1;
        });
        assert_eq!(count(&mut app), 2, "pages 3 and 5");
        set(&mut app, &|st| st.range = 1);
        assert_eq!(count(&mut app), 1, "the current page, whatever the parity");
        let st = &app.print_preview.as_ref().expect("preview").st;
        assert!(!st.all_pages, "the current page alone");
    }

    /// The anchor buttons put the pages at a corner, a side or the
    /// middle of the paper inside its margins, and Print prints them so.
    #[test]
    fn an_anchor_puts_the_pages_at_that_place_of_the_paper() {
        use tracedraw_core::geometry::{Rect, Size};
        let sheet = Size::new(200.0, 100.0);
        let page = [Rect::new(20.0, 30.0, 60.0, 50.0)];
        let inset = [5.0, 10.0, 15.0, 20.0];
        // Top left, middle, bottom right.
        assert_eq!(
            anchor_offset(&page, sheet, inset, 0, (0.0, 0.0)),
            (-15.0, 35.0)
        );
        assert_eq!(
            anchor_offset(&page, sheet, inset, 4, (1.0, 1.0)),
            (58.5, 13.5)
        );
        assert_eq!(
            anchor_offset(&page, sheet, inset, 8, (0.0, 0.0)),
            (130.0, -10.0)
        );
        assert_eq!(anchor_offset(&[], sheet, inset, 8, (3.0, 4.0)), (3.0, 4.0));
        // On a printed sheet: half size on A4 with 10 mm margins.
        let app = App::headless();
        let mut st = PrintState {
            fit_to_page: false,
            scale: 50.0,
            sheet: crate::ui::dialogs::SHEETS
                .iter()
                .position(|(n, _)| *n == "A4")
                .expect("A4"),
            margins: [10.0; 4],
            ..PrintState::default()
        };
        let first = print_sheets(&app, &st);
        let p = &first.pages[0];
        let boxes: Vec<Rect> = sheet_cells(p).into_iter().map(|(_, r)| r).collect();
        st.art_offset = anchor_offset(&boxes, p.size, st.margins, 0, st.art_offset);
        let moved = print_sheets(&app, &st);
        let r = sheet_cells(&moved.pages[0])[0].1;
        assert!((r.x0 - 10.0).abs() < 1e-6, "{r:?}");
        assert!(
            (r.y1 - (moved.pages[0].size.height - 10.0)).abs() < 1e-6,
            "{r:?}"
        );
    }

    /// Arrange pages: a page dragged onto another place of the sheet
    /// trades places with the page there, and the artwork stays put.
    #[test]
    fn a_page_dragged_onto_another_place_trades_places() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        app.print_preview(None);
        if let Some(pv) = app.print_preview.as_mut() {
            pv.st.looking_for_printers = false;
            pv.st.layout_kind = 3;
            pv.st.cols = 2;
            pv.st.rows = 2;
            pv.arrange = true;
        }
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| show(app, ui),
            );
            out.textures_delta.clear();
        };
        let button = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for _ in 0..3 {
            run(&mut app, Vec::new());
        }
        let cells = app.print_preview.as_ref().unwrap().shown_cells.clone();
        assert_eq!(cells.len(), 4, "{cells:?}");
        let (a, b) = (cells[0].1.center(), cells[3].1.center());
        run(&mut app, vec![egui::Event::PointerMoved(a)]);
        run(&mut app, vec![button(a, true)]);
        for i in 1..=6 {
            let t = i as f32 / 6.0;
            run(&mut app, vec![egui::Event::PointerMoved(a + (b - a) * t)]);
        }
        assert_eq!(app.print_preview.as_ref().unwrap().arrange_from, Some(0));
        run(&mut app, vec![button(b, false)]);
        run(&mut app, Vec::new());
        let pv = app.print_preview.as_mut().unwrap();
        assert_eq!(pv.st.cell_order, vec![3, 1, 2, 0]);
        assert_eq!(pv.arrange_from, None);
        // The art did not move with the drag.
        assert_eq!(pv.st.art_offset, (0.0, 0.0));
    }

    /// With Transform on and several pages on the sheet, a click on a
    /// page chooses it and Rotation turns only that place, on every
    /// sheet; a second click turns them all again (#2587).
    #[test]
    fn a_click_chooses_the_page_rotation_turns() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        app.print_preview(None);
        if let Some(pv) = app.print_preview.as_mut() {
            pv.st.looking_for_printers = false;
            pv.st.layout_kind = 3;
            pv.st.cols = 2;
            pv.st.rows = 2;
            pv.transform = true;
        }
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| show(app, ui),
            );
            out.textures_delta.clear();
        };
        let click = |app: &mut App, at: egui::Pos2| {
            run(app, vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                run(
                    app,
                    vec![egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            run(app, Vec::new());
        };
        for _ in 0..3 {
            run(&mut app, Vec::new());
        }
        let cells = app.print_preview.as_ref().unwrap().shown_cells.clone();
        assert_eq!(cells.len(), 4, "{cells:?}");
        let (second, r) = cells[1];
        assert_eq!(second, 1);
        click(&mut app, r.center() + egui::vec2(0.0, r.height() / 4.0));
        let pv = app.print_preview.as_mut().unwrap();
        assert_eq!(pv.chosen_cell, Some(1));
        // What the 180 degree button does with the page chosen.
        *rotation_of(&mut pv.st, pv.chosen_cell) += 180.0;
        assert_eq!(pv.st.cell_rotation, vec![0.0, 180.0]);
        assert_eq!(pv.st.art_rotation, 0.0);
        click(&mut app, r.center() + egui::vec2(0.0, r.height() / 4.0));
        let pv = app.print_preview.as_mut().unwrap();
        assert_eq!(pv.chosen_cell, None);
        assert_eq!(
            cell_at(
                &[(3, tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0))],
                5.0,
                5.0
            ),
            Some(3)
        );
    }

    /// Runs Print Preview for a frame at 1400 x 900 with these events.
    fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| show(app, ui),
        );
        out.textures_delta.clear();
    }

    /// What the View button shows is kept for the next Print Preview
    /// (#3603): the strip and the rulers turned off stay off when it is
    /// opened again, and come back once turned on.
    #[test]
    fn the_view_choices_are_kept_for_the_next_preview() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let frame = |app: &mut App| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| show(app, ui),
            );
            out.textures_delta.clear();
        };
        app.print_preview(Some(PrintState::default()));
        let pv = app.print_preview.as_ref().expect("open");
        assert!(pv.rulers && pv.strip && pv.options, "all shown at first");
        if let Some(pv) = app.print_preview.as_mut() {
            pv.strip = false;
            pv.rulers = false;
        }
        frame(&mut app);
        app.print_preview = None;
        app.print_preview(Some(PrintState::default()));
        let pv = app.print_preview.as_ref().expect("open again");
        assert!(!pv.strip && !pv.rulers && pv.options);
        if let Some(pv) = app.print_preview.as_mut() {
            pv.strip = true;
        }
        frame(&mut app);
        app.print_preview = None;
        app.print_preview(Some(PrintState::default()));
        let pv = app.print_preview.as_ref().expect("and again");
        assert!(pv.strip && !pv.rulers);
    }

    /// A zoom keeps the sheet point under the pointer where it is, and a
    /// box zooms so that it fills the view, its middle in the middle.
    #[test]
    fn zooms_keep_the_point_under_the_pointer() {
        let mut pv = PrintPreview::new(PrintState::default());
        let center = egui::pos2(500.0, 400.0);
        let at = egui::pos2(700.0, 300.0);
        // The sheet point under `at`, in fitted units from the sheet's
        // middle.
        let under = |pv: &PrintPreview| (at - (center + pv.pan)) / pv.zoom;
        let before = under(&pv);
        zoom_at(&mut pv, at, center, 2.0);
        assert_eq!(pv.zoom, 2.0);
        assert!((under(&pv) - before).length() < 1e-4, "{pv:?}", pv = pv.pan);
        zoom_at(&mut pv, at, center, 1000.0);
        assert_eq!(pv.zoom, ZOOM_MAX, "the zoom stays in its range");
        let mut pv = PrintPreview::new(PrintState::default());
        let area = egui::Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(800.0, 800.0));
        let want = egui::Rect::from_min_size(egui::pos2(600.0, 500.0), egui::vec2(100.0, 50.0));
        // The sheet point at the box's middle before the zoom.
        let mid = (want.center() - (area.center() + pv.pan)) / pv.zoom;
        zoom_to_box(&mut pv, want, area);
        assert!((pv.zoom - 8.0).abs() < 1e-4, "{}", pv.zoom);
        let now = area.center() + pv.pan + mid * pv.zoom;
        assert!((now - area.center()).length() < 1e-3, "{now:?}");
    }

    /// The wheel over the sheet zooms about the pointer (Options' wheel
    /// zooms by default), and the Zoom tool's click zooms in twice, its
    /// right click out.
    #[test]
    fn the_wheel_and_the_zoom_tool_zoom_the_sheet() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        app.settings.wheel_zooms = true;
        app.print_preview(None);
        app.print_preview.as_mut().unwrap().st.looking_for_printers = false;
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new());
        }
        let area = app.print_preview.as_ref().unwrap().view_area;
        assert!(area.is_positive(), "{area:?}");
        let at = area.center() + egui::vec2(120.0, -80.0);
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(at)]);
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let pv = app.print_preview.as_ref().unwrap();
        let step = app.settings.wheel_zoom_factor();
        assert!((pv.zoom - step).abs() < 1e-4, "{} vs {step}", pv.zoom);
        // The point under the pointer stayed: the sheet's middle moved
        // away from it by the zoom.
        let mid = area.center() + pv.pan;
        let expect = at + (area.center() - at) * step;
        assert!((mid - expect).length() < 0.5, "{mid:?} {expect:?}");

        let pv = app.print_preview.as_mut().unwrap();
        pv.zoom = 1.0;
        pv.pan = egui::Vec2::ZERO;
        pv.tool = PreviewTool::Zoom;
        let c = area.center();
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(c)]);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: c,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert_eq!(app.print_preview.as_ref().unwrap().zoom, 2.0);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: c,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert_eq!(app.print_preview.as_ref().unwrap().zoom, 1.0);
    }

    /// The Pan tool's drag moves the view, not the artwork.
    #[test]
    fn the_pan_tool_moves_the_view_not_the_artwork() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        app.print_preview(None);
        let pv = app.print_preview.as_mut().unwrap();
        pv.st.looking_for_printers = false;
        pv.zoom = 4.0;
        pv.tool = PreviewTool::Pan;
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new());
        }
        let c = app.print_preview.as_ref().unwrap().view_area.center();
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(c)]);
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerButton {
                pos: c,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        for i in 1..=6 {
            let p = c + egui::vec2(-10.0 * i as f32, -5.0 * i as f32);
            frame(&ctx, &mut app, vec![egui::Event::PointerMoved(p)]);
        }
        let end = c + egui::vec2(-60.0, -30.0);
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let pv = app.print_preview.as_ref().unwrap();
        assert!(pv.pan.x < -20.0 && pv.pan.y < -10.0, "{:?}", pv.pan);
        assert_eq!(pv.st.art_offset, (0.0, 0.0));
    }

    /// The frames of the thumbnails lie inside them: the strip clips
    /// what lies outside its edge, which cut off the left side of the
    /// first sheet's accent frame (#3242).
    #[test]
    fn thumbnail_frames_stay_inside_the_thumbnail() {
        for shown in [true, false] {
            let (stroke, kind) = thumb_frame(shown);
            assert_eq!(kind, egui::StrokeKind::Inside);
            assert!(stroke.width > 0.0);
        }
        assert!(thumb_frame(true).0.width > thumb_frame(false).0.width);
    }

    /// The strip shows every sheet; a click on one shows it. The status
    /// line counts what goes to the printer, and Print this sheet keeps
    /// the sheet shown alone.
    #[test]
    fn the_strip_chooses_the_sheet_and_one_sheet_prints_alone() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        app.add_page();
        app.add_page();
        app.print_preview(None);
        app.print_preview.as_mut().unwrap().st.looking_for_printers = false;
        for _ in 0..6 {
            frame(&ctx, &mut app, Vec::new());
        }
        let pv = app.print_preview.as_ref().unwrap();
        assert_eq!(pv.thumbs.1.len(), 3);
        assert!(
            pv.thumbs.1.iter().all(|t| t.is_some()),
            "every sheet drawn small"
        );
        // The third thumbnail: the strip runs left to right from the
        // window's left edge, just above the status line.
        let strip =
            egui::containers::panel::PanelState::load(&ctx, egui::Id::new("print_preview_strip"))
                .map(|s| s.outer_rect)
                .expect("the strip is shown");
        let w = THUMB_H * (210.0 / 297.0);
        let spacing = ctx.global_style().spacing.item_spacing.x;
        let at = egui::pos2(
            strip.left() + 8.0 + 2.0 * (w + spacing) + w / 2.0,
            strip.top() + 4.0 + THUMB_H / 2.0,
        );
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert_eq!(app.print_preview.as_ref().unwrap().sheet, 2);

        let mut st = PrintState {
            copies: 3,
            ..PrintState::default()
        };
        assert_eq!(pages_to_print(3, &st, &[]), 9);
        st.separations = true;
        assert_eq!(pages_to_print(3, &st, &[true, false, true, true]), 27);

        let pv = app.print_preview.as_ref().unwrap();
        let mut doc = print_sheets(&app, &pv.st);
        assert_eq!(doc.pages.len(), 3);
        let third = doc.pages[2].id;
        crate::export::keep_one_sheet(&mut doc, 2);
        assert_eq!(doc.pages.len(), 1);
        assert_eq!(doc.pages[0].id, third);
        crate::export::keep_one_sheet(&mut doc, 7);
        assert_eq!(doc.pages.len(), 1, "past the end keeps the last");
    }

    /// Width fits the sheet's width to the view with its top in view;
    /// Fit puts the whole sheet back.
    #[test]
    fn width_fits_the_sheet_s_width() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        app.print_preview(None);
        app.print_preview.as_mut().unwrap().st.looking_for_printers = false;
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new());
        }
        let mut pv = app.print_preview.take().unwrap();
        fit_width(&mut pv, &app);
        let area = pv.view_area;
        let size = sheets(&app, &mut pv).pages[0].size;
        let k = pv.fit_k * f64::from(pv.zoom);
        let w = (size.width * k) as f32;
        assert!((w - (area.width() - 48.0)).abs() < 1.0, "{w} in {area:?}");
        let h = (size.height * k) as f32;
        let top = area.center().y + pv.pan.y - h / 2.0;
        assert!(
            (top - (area.top() + 24.0)).abs() < 1.0,
            "top {top} {area:?}"
        );
    }
}
