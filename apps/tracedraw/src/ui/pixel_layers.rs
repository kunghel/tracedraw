//! The Layers panel of the image editing mode (Bitmaps > Edit Inside):
//! the merge mode and opacity of the layer in use on top, then the
//! layers top first, each a row with its show and lock buttons, a
//! miniature of its pixels, a miniature of its mask when it has one (a
//! click paints the mask, a click on the pixels' miniature paints the
//! pixels again), its name (a double click renames it) and its mode and
//! opacity; under them rows of icon buttons for the layer commands and
//! the mask commands, each named in its tip. A right click on a row
//! lists the commands for that layer.

use crate::i18n::tr;
use crate::pixel_edit::{self, PixelImage};
use crate::pixel_edit_app::{MaskTarget, PixelEdit};
use crate::theme::Tokens;
use crate::ui::icon_paint::{Pen, LINE, THIN};
use crate::ui::tips::RichTip;
use egui::{Color32, Rect, Sense, Ui, Vec2};

/// The side of a layer's miniatures, in points.
const THUMB: f32 = 34.0;
/// A layer row's height.
const ROW: f32 = THUMB + 8.0;
/// Seconds between remakes of the miniatures while the picture changes.
const THUMB_EVERY: f64 = 0.5;

/// The pictures of the panel's buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerIcon {
    Shown,
    Hidden,
    Locked,
    Unlocked,
    New,
    Duplicate,
    Delete,
    Up,
    Down,
    MergeDown,
    Flatten,
    AddMask,
    EditMask,
    InvertMask,
    ApplyMask,
    DeleteMask,
    Clip,
    CopyTo,
    CutTo,
    Paste,
    Adjustment,
    FitSurface,
    Rename,
    Style,
    TextLayer,
}

#[cfg(test)]
impl LayerIcon {
    pub const ALL: [LayerIcon; 25] = [
        LayerIcon::Shown,
        LayerIcon::Hidden,
        LayerIcon::Locked,
        LayerIcon::Unlocked,
        LayerIcon::New,
        LayerIcon::Duplicate,
        LayerIcon::Delete,
        LayerIcon::Up,
        LayerIcon::Down,
        LayerIcon::MergeDown,
        LayerIcon::Flatten,
        LayerIcon::AddMask,
        LayerIcon::EditMask,
        LayerIcon::InvertMask,
        LayerIcon::ApplyMask,
        LayerIcon::DeleteMask,
        LayerIcon::Clip,
        LayerIcon::CopyTo,
        LayerIcon::CutTo,
        LayerIcon::Paste,
        LayerIcon::Adjustment,
        LayerIcon::FitSurface,
        LayerIcon::Rename,
        LayerIcon::Style,
        LayerIcon::TextLayer,
    ];
}

/// Draw `icon` in `r` on the 24 unit grid of the toolbox icons, with
/// `ink`.
pub fn draw_icon(p: &egui::Painter, r: Rect, icon: LayerIcon, ink: Color32) {
    let plain = ink == Tokens::icon_off() || ink.a() < 255;
    let acc = if plain {
        ink
    } else {
        crate::ui::icons::accent()
    };
    let pen = Pen::new(p, r, ink, acc, Tokens::panel());
    let clear = Color32::TRANSPARENT;
    // A layer as a flat sheet seen at a slant.
    let sheet = |pen: &Pen, y: f32, body: bool, c: Color32| {
        let d = format!("M12 {}l9 4.5-9 4.5-9-4.5z", y);
        if body {
            pen.body(&d);
        } else {
            pen.stroke(&d, LINE, c);
        }
    };
    let eye = |pen: &Pen| {
        pen.line("M2 12c3-5 6.5-7 10-7s7 2 10 7c-3 5-6.5 7-10 7s-7-2-10-7z", ink);
        pen.circle(12.0, 12.0, 3.2, ink, 0.0, ink);
    };
    let lock_body = |pen: &Pen| {
        pen.rect(5.0, 11.0, 14.0, 10.0, 2.0, pen.wash(), LINE, ink);
        pen.circle(12.0, 16.0, 1.4, ink, 0.0, ink);
    };
    let mask = |pen: &Pen, x: f32, y: f32, s: f32| {
        pen.rect(x, y, s, s, 1.5, pen.wash(), LINE, ink);
        pen.circle(x + s / 2.0, y + s / 2.0, s * 0.27, ink, 0.0, ink);
    };
    let plus = |pen: &Pen, x: f32, y: f32| {
        pen.stroke(&format!("M{} {}h6M{} {}v6", x - 3.0, y, x, y - 3.0), LINE, acc);
    };
    match icon {
        LayerIcon::Shown => eye(&pen),
        LayerIcon::Hidden => {
            pen.ghost("M2 12c3-5 6.5-7 10-7s7 2 10 7c-3 5-6.5 7-10 7s-7-2-10-7z", ink);
            pen.line("M4 20L20 4", ink);
        }
        LayerIcon::Locked => {
            pen.line("M8 11V8a4 4 0 0 1 8 0v3", ink);
            lock_body(&pen);
        }
        LayerIcon::Unlocked => {
            pen.ghost("M8 11V8a4 4 0 0 1 8 0", ink);
            pen.ghost("M5 11h14v10H5z", ink);
        }
        LayerIcon::New => {
            sheet(&pen, 8.0, true, ink);
            plus(&pen, 19.0, 5.0);
        }
        LayerIcon::Duplicate => {
            sheet(&pen, 4.0, false, Pen::alpha(ink, 0.5));
            sheet(&pen, 10.0, true, ink);
        }
        LayerIcon::Delete => {
            pen.line("M4 6h16M9 6V3.5h6V6", ink);
            pen.body("M6 6l1.2 15h9.6L18 6z");
            pen.thin("M10 10v7M14 10v7", ink);
        }
        LayerIcon::Up | LayerIcon::Down => {
            let up = icon == LayerIcon::Up;
            sheet(&pen, if up { 11.0 } else { 3.0 }, true, ink);
            let (y0, y1) = if up { (9.5, 2.5) } else { (14.5, 21.5) };
            pen.line(&format!("M12 {y0}V{y1}"), acc);
            pen.arrow_head(12.0, y1, 0.0, if up { -1.0 } else { 1.0 }, 3.0, acc);
        }
        LayerIcon::MergeDown => {
            sheet(&pen, 3.0, false, Pen::alpha(ink, 0.5));
            sheet(&pen, 12.0, true, ink);
            pen.line("M12 8.5v5", acc);
            pen.arrow_head(12.0, 13.5, 0.0, 1.0, 3.0, acc);
        }
        LayerIcon::Flatten => {
            for y in [2.0, 6.0] {
                sheet(&pen, y, false, Pen::alpha(ink, 0.45));
            }
            sheet(&pen, 12.0, true, ink);
        }
        LayerIcon::AddMask => {
            mask(&pen, 3.0, 6.0, 15.0);
            plus(&pen, 19.0, 5.0);
        }
        LayerIcon::EditMask => {
            mask(&pen, 3.0, 3.0, 14.0);
            pen.stroke("M21 12l-8 8", 2.6, acc);
            pen.circle(12.0, 21.0, 1.6, acc, 0.0, acc);
        }
        LayerIcon::InvertMask => {
            pen.rect(4.0, 4.0, 16.0, 16.0, 1.5, clear, LINE, ink);
            pen.fill("M12 4h6.5a1.5 1.5 0 0 1 1.5 1.5v13a1.5 1.5 0 0 1-1.5 1.5H12z", ink, false);
            pen.circle(12.0, 12.0, 4.3, clear, LINE, acc);
        }
        LayerIcon::ApplyMask => {
            mask(&pen, 3.0, 3.0, 13.0);
            pen.stroke("M12 17l3.5 3.5L22 13", LINE, acc);
        }
        LayerIcon::DeleteMask => {
            mask(&pen, 3.0, 3.0, 13.0);
            pen.stroke("M15 15l6 6M21 15l-6 6", LINE, acc);
        }
        LayerIcon::Clip => {
            sheet(&pen, 12.0, true, ink);
            pen.line("M6 3v5.5h8", acc);
            pen.arrow_head(14.0, 8.5, 1.0, 0.0, 3.0, acc);
        }
        LayerIcon::CopyTo | LayerIcon::CutTo => {
            sheet(&pen, 11.0, true, ink);
            pen.dashed("M4 2.5h9v7H4z", THIN, ink, 2.0, 1.8);
            if icon == LayerIcon::CutTo {
                pen.circle(17.5, 4.0, 2.0, clear, THIN, acc);
                pen.circle(17.5, 9.0, 2.0, clear, THIN, acc);
                pen.thin("M19 5l3-1M19 8l3 1", acc);
            } else {
                pen.line("M17 3v6", acc);
                pen.arrow_head(17.0, 9.0, 0.0, 1.0, 2.6, acc);
            }
        }
        LayerIcon::Paste => {
            pen.rect(5.0, 4.0, 14.0, 17.0, 2.0, pen.wash(), LINE, ink);
            pen.rect(9.0, 2.5, 6.0, 3.5, 1.0, Tokens::panel(), LINE, ink);
            sheet(&pen, 10.0, false, acc);
        }
        LayerIcon::Adjustment => {
            pen.circle(12.0, 12.0, 8.0, clear, LINE, ink);
            pen.fill("M12 4a8 8 0 0 1 0 16z", ink, false);
        }
        LayerIcon::FitSurface => {
            pen.line("M2 15c3-3 5-3 7 0s5 3 7 0 4-3 6-1", ink);
            sheet(&pen, 2.0, false, acc);
        }
        LayerIcon::Rename => {
            // A name line with a pencil writing it.
            pen.line("M3 20h9", ink);
            pen.body("M16 3.5l4.5 4.5L10 18.5 5 19.5l1-5z");
            pen.thin("M14 5.5l4.5 4.5", acc);
        }
        LayerIcon::Style => {
            // A sheet with its shadow and a spark of light.
            sheet(&pen, 11.0, false, Pen::alpha(ink, 0.45));
            sheet(&pen, 7.0, true, ink);
            pen.thin("M19 2v5M16.5 4.5h5", acc);
        }
        LayerIcon::TextLayer => {
            pen.line("M5 5h14M12 5v14", ink);
            pen.thin("M8 21h8", acc);
        }
    }
}

/// An icon button of the panel; `on` draws it pushed in.
pub fn icon_button(ui: &mut Ui, icon: LayerIcon, tip: &str, enabled: bool, on: bool) -> egui::Response {
    let side = Tokens::prop_button();
    let (r, resp) = ui.allocate_exact_size(
        Vec2::splat(side),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    crate::theme::paint_icon_button(ui.painter(), r, enabled && resp.hovered(), on);
    let ink = crate::ui::propbar::icon_color(enabled);
    draw_icon(
        ui.painter(),
        Rect::from_center_size(r.center(), Vec2::splat(Tokens::prop_icon())),
        icon,
        ink,
    );
    let resp = resp.rich_tip_always(tip);
    if enabled {
        resp
    } else {
        // Never clicked while greyed.
        resp.interact(Sense::hover())
    }
}

/// A command's tip: its name, and what it does when there is more.
fn tip(name: &str, more: Option<&str>) -> String {
    match more {
        Some(k) => format!("{}\n{}", tr(name), tr(k)),
        None => tr(name),
    }
}

/// The miniatures of the layers (and their masks), made again at most
/// every half second while the picture changes, and not mid-stroke.
#[derive(Default)]
pub struct LayerThumbs {
    pub pics: Vec<(egui::TextureHandle, Option<egui::TextureHandle>)>,
    /// The picture changed since they were made.
    pub stale: bool,
    pub made_at: f64,
}

/// `img` scaled down to fit a `side` pixel square, its proportions kept.
pub fn shrink(img: &image::RgbaImage, side: u32) -> image::RgbaImage {
    if img.width() == 0 || img.height() == 0 {
        return image::RgbaImage::new(1, 1);
    }
    let (tw, th) = shrink_size(img.dimensions(), side);
    image::imageops::thumbnail(img, tw, th)
}

fn mask_rgba(m: &image::GrayImage) -> image::RgbaImage {
    image::RgbaImage::from_fn(m.width(), m.height(), |x, y| {
        let v = m.get_pixel(x, y)[0];
        image::Rgba([v, v, v, 255])
    })
}

fn texture(ctx: &egui::Context, name: &str, img: &image::RgbaImage) -> egui::TextureHandle {
    let c = egui::ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_raw(),
    );
    ctx.load_texture(name, c, egui::TextureOptions::LINEAR)
}

/// The layers' miniatures, made again when the picture changed.
fn thumbs(ui: &Ui, pe: &PixelEdit) {
    let now = ui.input(|i| i.time);
    let n = pe.pi.layers.len();
    let mut v = pe.view.borrow_mut();
    let t = &mut v.thumbs;
    let due = t.pics.len() != n
        || (t.stale && pe.stroke.is_none() && now - t.made_at >= THUMB_EVERY);
    if !due {
        if t.stale && pe.stroke.is_none() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(THUMB_EVERY));
        }
        return;
    }
    let side = (THUMB * ui.ctx().pixels_per_point()).round() as u32;
    t.pics = (0..n)
        .map(|i| {
            let px = layer_picture(&pe.pi, i, side);
            let mask = pe.pi.layers.get(i).and_then(|l| l.mask.as_ref()).map(|m| {
                let (tw, th) = shrink_size(m.dimensions(), side);
                let small = image::imageops::thumbnail(m, tw, th);
                texture(ui.ctx(), "pixel_layer_mask", &mask_rgba(&small))
            });
            (texture(ui.ctx(), "pixel_layer", &px), mask)
        })
        .collect();
    t.stale = false;
    t.made_at = now;
}

/// The size of a `w` by `h` picture fitted in a `side` pixel square,
/// never enlarged.
fn shrink_size((w, h): (u32, u32), side: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (1, 1);
    }
    let k = (side as f32 / w.max(h) as f32).min(1.0);
    (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1))
}

/// A layer's miniature: its pixels, or nothing for an adjustment layer
/// (drawn with its icon instead).
fn layer_picture(pi: &PixelImage, i: usize, side: u32) -> image::RgbaImage {
    match pi.layer_pixels(i) {
        Some(img) if img.width() > 0 => shrink(img, side),
        _ => image::RgbaImage::new(1, 1),
    }
}

/// A miniature in `r`: a checkerboard under the picture, fitted.
fn paint_thumb(ui: &Ui, r: Rect, tex: Option<&egui::TextureHandle>, picked: bool) {
    let p = ui.painter();
    p.rect_filled(r, 0.0, Color32::from_gray(250));
    let cell = r.width() / 6.0;
    for row in 0..6 {
        for col in 0..6 {
            if (row + col) % 2 == 1 {
                let c = Rect::from_min_size(
                    r.min + Vec2::new(col as f32 * cell, row as f32 * cell),
                    Vec2::splat(cell),
                );
                p.rect_filled(c, 0.0, Color32::from_gray(214));
            }
        }
    }
    if let Some(t) = tex {
        let [w, h] = t.size();
        let k = (r.width() / w.max(h) as f32).min(r.width());
        let fit = Rect::from_center_size(r.center(), Vec2::new(w as f32 * k, h as f32 * k));
        p.image(
            t.id(),
            fit,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    let line = if picked {
        egui::Stroke::new(2.0, Tokens::ACCENT)
    } else {
        egui::Stroke::new(1.0, Tokens::border())
    };
    p.rect_stroke(r, 0.0, line, egui::StrokeKind::Inside);
}

/// What a click on a layer row asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowAsk {
    Use,
    UsePixels,
    UseMask,
    Shown,
    Locked,
    Rename,
    /// A command from the row's right-click menu, for that layer.
    Cmd(RowCmd),
}

/// The commands of a layer row's right-click menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowCmd {
    Rename,
    Duplicate,
    Delete,
    Up,
    Down,
    MergeDown,
    Flatten,
    AddMask,
    PaintMask,
    InvertMask,
    DeleteMask,
    Clip,
    Shown,
    Locked,
    ClearStyle,
    TextToPixels,
}

/// Run `c` on the layer in use.
fn run_cmd(pe: &mut PixelEdit, c: RowCmd) {
    let full = |pi: &PixelImage| Some([0, 0, pi.img.width(), pi.img.height()]);
    let name = crate::ui::pixel_bar::new_layer_name(pe);
    let i = pe.pi.active;
    match c {
        RowCmd::Rename => {}
        RowCmd::Duplicate => {
            pe.apply_all(true, |pi| {
                pi.duplicate_layer(name.clone());
                full(pi)
            });
        }
        RowCmd::Delete => {
            pe.apply_all(true, |pi| pi.delete_layer().then(|| full(pi)).flatten());
        }
        RowCmd::Up | RowCmd::Down => {
            let up = c == RowCmd::Up;
            pe.apply_all(true, |pi| pi.move_layer(up).then(|| full(pi)).flatten());
        }
        RowCmd::MergeDown => {
            pe.apply_all(true, |pi| pi.merge_down().then(|| full(pi)).flatten());
        }
        RowCmd::Flatten => {
            pe.apply_all(true, |pi| pi.flatten().then(|| full(pi)).flatten());
        }
        RowCmd::AddMask => {
            if pe.apply_all(true, |pi| pi.add_layer_mask().then(|| full(pi)).flatten()) {
                pe.set_edit_layer_mask(true);
            }
        }
        RowCmd::PaintMask => pe.set_edit_layer_mask(true),
        RowCmd::InvertMask => {
            pe.apply_all(true, |pi| pi.invert_layer_mask().then(|| full(pi)).flatten());
        }
        RowCmd::DeleteMask => {
            pe.apply_all(true, |pi| pi.delete_layer_mask().then(|| full(pi)).flatten());
            pe.set_edit_layer_mask(false);
        }
        RowCmd::ClearStyle => {
            pe.apply_all(true, |pi| {
                let l = pi.layers.get_mut(i)?;
                l.style.take()?;
                full(pi)
            });
        }
        RowCmd::TextToPixels => {
            pe.end_text();
            pe.apply_all(true, |pi| {
                let l = pi.layers.get_mut(i)?;
                l.text.take()?;
                Some([0, 0, 0, 0])
            });
        }
        RowCmd::Clip | RowCmd::Shown | RowCmd::Locked => {
            pe.apply_all(true, |pi| {
                let l = pi.layers.get_mut(i)?;
                match c {
                    RowCmd::Clip => l.clip = !l.clip,
                    RowCmd::Shown => l.visible = !l.visible,
                    _ => l.locked = !l.locked,
                }
                full(pi)
            });
        }
    }
}

/// The right-click menu of layer `i`'s row: the command picked.
fn row_menu(ui: &mut Ui, pe: &PixelEdit, i: usize) -> Option<RowCmd> {
    let l = pe.pi.layers.get(i)?;
    let n = pe.pi.layers.len();
    let has_mask = l.mask.is_some();
    let mut picked = None;
    let mut entry = |ui: &mut Ui, key: &str, icon: LayerIcon, enabled: bool, c: RowCmd| {
        let pic = move |p: &egui::Painter, r: Rect, ink: Color32| draw_icon(p, r.expand(2.0), icon, ink);
        if crate::ui::menus::item_drawn(ui, &tr(key), "", enabled, key, &pic) {
            picked = Some(c);
        }
    };
    entry(ui, "pixel.layer.rename_cmd", LayerIcon::Rename, true, RowCmd::Rename);
    entry(ui, "pixel.layer.duplicate", LayerIcon::Duplicate, true, RowCmd::Duplicate);
    entry(ui, "pixel.layer.delete", LayerIcon::Delete, n > 1, RowCmd::Delete);
    crate::ui::menus::sep(ui);
    let (key, icon) = if l.visible {
        ("pixel.layer.hide_cmd", LayerIcon::Hidden)
    } else {
        ("pixel.layer.show_cmd", LayerIcon::Shown)
    };
    entry(ui, key, icon, true, RowCmd::Shown);
    let (key, icon) = if l.locked {
        ("pixel.layer.unlock_cmd", LayerIcon::Unlocked)
    } else {
        ("pixel.layer.lock_cmd", LayerIcon::Locked)
    };
    entry(ui, key, icon, true, RowCmd::Locked);
    entry(ui, "pixel.layer.clip", LayerIcon::Clip, i > 0, RowCmd::Clip);
    crate::ui::menus::sep(ui);
    entry(ui, "pixel.layer.up", LayerIcon::Up, i + 1 < n, RowCmd::Up);
    entry(ui, "pixel.layer.down", LayerIcon::Down, i > 0, RowCmd::Down);
    entry(ui, "pixel.layer.merge_down", LayerIcon::MergeDown, i > 0, RowCmd::MergeDown);
    entry(ui, "pixel.layer.flatten", LayerIcon::Flatten, pe.pi.is_layered(), RowCmd::Flatten);
    crate::ui::menus::sep(ui);
    if has_mask {
        entry(ui, "pixel.mask.edit", LayerIcon::EditMask, true, RowCmd::PaintMask);
        entry(ui, "pixel.mask.invert", LayerIcon::InvertMask, true, RowCmd::InvertMask);
        entry(ui, "pixel.mask.delete", LayerIcon::DeleteMask, true, RowCmd::DeleteMask);
    } else {
        entry(ui, "pixel.mask.add", LayerIcon::AddMask, true, RowCmd::AddMask);
    }
    if l.style.is_some() || l.text.is_some() {
        crate::ui::menus::sep(ui);
    }
    if l.style.is_some() {
        entry(ui, "pixel.style.clear", LayerIcon::Style, true, RowCmd::ClearStyle);
    }
    if l.text.is_some() {
        entry(ui, "pixel.text.to_pixels", LayerIcon::TextLayer, true, RowCmd::TextToPixels);
    }
    picked
}

pub(crate) fn layers_section(ui: &mut Ui, pe: &mut PixelEdit) {
    thumbs(ui, pe);
    mode_and_opacity(ui, pe);
    ui.add_space(2.0);
    let n = pe.pi.layers.len();
    let rename_id = ui.make_persistent_id("pixel_layer_rename");
    let mut renaming: Option<(usize, String)> = ui.data(|d| d.get_temp(rename_id));
    let mut asks: Vec<(usize, RowAsk)> = Vec::new();
    let frame = egui::Frame::new()
        .fill(Tokens::field())
        .stroke(egui::Stroke::new(1.0, Tokens::border()))
        .inner_margin(egui::Margin::same(1));
    frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        // Top layer first, as they stack.
        for i in (0..n).rev() {
            if let Some(a) = layer_row(ui, pe, i, &mut renaming) {
                asks.push((i, a));
            }
        }
    });
    let mut commit: Option<(usize, String)> = None;
    for (i, a) in asks {
        match a {
            RowAsk::Use => use_layer(pe, i),
            RowAsk::UsePixels => {
                use_layer(pe, i);
                pe.set_edit_layer_mask(false);
            }
            RowAsk::UseMask => {
                use_layer(pe, i);
                pe.set_edit_layer_mask(true);
            }
            RowAsk::Shown | RowAsk::Locked => {
                pe.apply_all(true, |pi| {
                    let l = pi.layers.get_mut(i)?;
                    if a == RowAsk::Shown {
                        l.visible = !l.visible;
                    } else {
                        l.locked = !l.locked;
                    }
                    Some([0, 0, pi.img.width(), pi.img.height()])
                });
            }
            RowAsk::Rename => {
                if let Some(r) = renaming.take() {
                    commit = Some(r);
                }
            }
            RowAsk::Cmd(RowCmd::Rename) => {
                if let Some(l) = pe.pi.layers.get(i) {
                    renaming = Some((i, l.name.clone()));
                }
            }
            RowAsk::Cmd(c) => {
                use_layer(pe, i);
                run_cmd(pe, c);
            }
        }
    }
    if let Some((i, name)) = commit {
        if pe.pi.layers.get(i).is_some_and(|l| l.name != name) {
            pe.remember();
            if let Some(l) = pe.pi.layers.get_mut(i) {
                l.name = name;
            }
        }
    }
    ui.data_mut(|d| match &renaming {
        Some(r) => {
            d.insert_temp(rename_id, r.clone());
        }
        None => {
            d.remove::<(usize, String)>(rename_id);
        }
    });
    ui.add_space(4.0);
    layer_buttons(ui, pe);
    mask_buttons(ui, pe);
    ui.add_space(4.0);
    crate::ui::pixel_styles::style_section(ui, pe);
}

fn use_layer(pe: &mut PixelEdit, i: usize) {
    if pe.pi.active != i {
        pe.apply_all(true, |pi| {
            pi.set_active(i);
            Some([0, 0, 0, 0])
        });
    }
}

/// The merge mode and opacity of the layer in use; a drag of the
/// opacity is one undo step.
fn mode_and_opacity(ui: &mut Ui, pe: &mut PixelEdit) {
    let i = pe.pi.active;
    let Some(l) = pe.pi.layers.get(i).cloned() else {
        return;
    };
    let mut op = (l.opacity * 100.0).round();
    let mut merge = l.merge;
    egui::ComboBox::from_id_salt("pixel_layer_merge")
        .width((ui.available_width() - 8.0).clamp(80.0, 220.0))
        .selected_text(merge.name())
        .show_ui(ui, |ui| {
            for m in pixel_edit::MergeMode::ALL {
                ui.selectable_value(&mut merge, m, m.name());
            }
        });
    let slider = ui.add(
        crate::ui::slider::Slider::new(&mut op, 0.0..=100.0)
            .label(tr("pixel.opt.opacity"))
            .suffix(" %")
            .integer(),
    );
    if (op / 100.0 - l.opacity).abs() > 1e-4 || merge != l.merge {
        if !pe.layer_drag {
            pe.remember();
        }
        if let Some(m) = pe.pi.layers.get_mut(i) {
            m.opacity = op / 100.0;
            m.merge = merge;
        }
        pe.view.borrow_mut().full = true;
    }
    pe.layer_drag = slider.dragged();
}

/// One layer's row; what a click on it asks for.
fn layer_row(
    ui: &mut Ui,
    pe: &PixelEdit,
    i: usize,
    renaming: &mut Option<(usize, String)>,
) -> Option<RowAsk> {
    let l = pe.pi.layers.get(i)?;
    let picked = pe.pi.active == i;
    let (row, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::click());
    let p = ui.painter();
    if picked {
        p.rect_filled(row, 0.0, Tokens::selected_row());
    } else if resp.hovered() {
        p.rect_filled(row, 0.0, Tokens::hovered_row());
    }
    // A thin line between the rows.
    if i > 0 {
        p.hline(row.x_range(), row.max.y - 0.5, egui::Stroke::new(1.0, Tokens::border()));
    }
    let mut ask = resp.clicked().then_some(RowAsk::Use);
    egui::Popup::context_menu(&resp)
        .id(resp.id.with("pixel_layer_menu"))
        .style(crate::ui::menus::menu_popup_style)
        .show(|ui| {
            crate::ui::menus::body(ui, |ui| {
                if let Some(c) = row_menu(ui, pe, i) {
                    ask = Some(RowAsk::Cmd(c));
                }
            })
        });
    let mut x = row.min.x + 2.0;
    let small = Vec2::splat(20.0);
    // Show and lock.
    for (on, yes, no, key, a) in [
        (l.visible, LayerIcon::Shown, LayerIcon::Hidden, "pixel.layer.visible_tip", RowAsk::Shown),
        (l.locked, LayerIcon::Locked, LayerIcon::Unlocked, "pixel.layer.locked_tip", RowAsk::Locked),
    ] {
        let r = Rect::from_min_size(egui::pos2(x, row.center().y - 10.0), small);
        let b = ui
            .interact(r, ui.id().with(("pixel_layer_btn", i, key)), Sense::click())
            .rich_tip(tr(key));
        crate::theme::paint_icon_button(ui.painter(), r, b.hovered(), false);
        let ink = if on { Tokens::icon() } else { Tokens::icon_off() };
        draw_icon(ui.painter(), r.shrink(2.0), if on { yes } else { no }, ink);
        if b.clicked() {
            ask = Some(a);
        }
        x += 22.0;
    }
    // A text layer and a styled one say so with a small picture.
    if l.text.is_some() || l.shown_style().is_some() {
        let icon = if l.text.is_some() { LayerIcon::TextLayer } else { LayerIcon::Style };
        let r = Rect::from_min_size(egui::pos2(row.max.x - 20.0, row.min.y + 3.0), Vec2::splat(16.0));
        draw_icon(ui.painter(), r, icon, Tokens::text_dim());
        if l.text.is_some() && l.shown_style().is_some() {
            let r2 = r.translate(Vec2::new(0.0, 18.0));
            draw_icon(ui.painter(), r2, LayerIcon::Style, Tokens::text_dim());
        }
    }
    // A clipped layer is set in, under a bent arrow.
    if l.clip {
        let r = Rect::from_min_size(egui::pos2(x, row.center().y - 8.0), Vec2::splat(16.0));
        draw_icon(ui.painter(), r, LayerIcon::Clip, Tokens::text_dim());
        x += 16.0;
    }
    let editing_mask = picked && pe.target == MaskTarget::LayerMask;
    let v = pe.view.borrow();
    let pics = v.thumbs.pics.get(i);
    // The pixels' miniature (an adjustment layer's icon).
    let tr_px = Rect::from_min_size(egui::pos2(x + 2.0, row.min.y + 4.0), Vec2::splat(THUMB));
    if l.adjust.is_some() {
        ui.painter().rect_filled(tr_px, 2.0, Tokens::panel());
        draw_icon(ui.painter(), tr_px.shrink(6.0), LayerIcon::Adjustment, Tokens::icon());
    } else {
        paint_thumb(ui, tr_px, pics.map(|p| &p.0), picked && l.mask.is_some() && !editing_mask);
    }
    let b = ui.interact(tr_px, ui.id().with(("pixel_layer_px", i)), Sense::click());
    if b.clicked() {
        ask = Some(RowAsk::UsePixels);
    }
    x = tr_px.max.x + 4.0;
    // The mask's miniature.
    if l.mask.is_some() {
        let r = Rect::from_min_size(egui::pos2(x, row.min.y + 4.0), Vec2::splat(THUMB));
        paint_thumb(ui, r, pics.and_then(|p| p.1.as_ref()), editing_mask);
        let b = ui
            .interact(r, ui.id().with(("pixel_layer_mask", i)), Sense::click())
            .rich_tip(tip("pixel.mask.edit", Some("pixel.mask.thumb_tip")));
        if b.clicked() {
            ask = Some(RowAsk::UseMask);
        }
        x = r.max.x + 4.0;
    }
    drop(v);
    // The name, and the mode and opacity under it; a double click
    // renames it.
    let name_rect = Rect::from_min_max(egui::pos2(x + 2.0, row.min.y + 3.0), egui::pos2(row.max.x - 2.0, row.max.y - 3.0));
    let label = crate::ui::pixel_bar::layer_label(i, &l.name);
    match renaming {
        Some((k, text)) if *k == i => {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(name_rect));
            let edit = child.add(egui::TextEdit::singleline(text).desired_width(name_rect.width()));
            if !edit.has_focus() && !edit.lost_focus() {
                edit.request_focus();
            }
            if edit.lost_focus() {
                ask = Some(RowAsk::Rename);
            }
        }
        _ => {
            let font = egui::FontId::proportional(13.0);
            let ink = Tokens::text();
            let galley = ui.painter().layout(
                label.clone(),
                font,
                ink,
                name_rect.width(),
            );
            let top = name_rect.min + Vec2::new(0.0, 2.0);
            ui.painter().with_clip_rect(name_rect).galley(top, galley, ink);
            let mut more = format!("{} \u{00B7} {:.0} %", l.merge.name(), l.opacity * 100.0);
            if !l.visible {
                more = format!("{more} \u{00B7} {}", tr("pixel.layer.hidden"));
            }
            ui.painter().with_clip_rect(name_rect).text(
                egui::pos2(name_rect.min.x, name_rect.max.y - 2.0),
                egui::Align2::LEFT_BOTTOM,
                more,
                egui::FontId::proportional(11.0),
                Tokens::text_dim(),
            );
            if resp.double_clicked() {
                *renaming = Some((i, l.name.clone()));
            }
        }
    }
    resp.rich_tip(tip("pixel.layer.rename", Some("pixel.layer.row_tip")));
    ask
}

/// The layer commands as icon buttons.
fn layer_buttons(ui: &mut Ui, pe: &mut PixelEdit) {
    let n = pe.pi.layers.len();
    let i = pe.pi.active;
    let has_mask = pe.pi.layers.get(i).is_some_and(|l| l.mask.is_some());
    let full = |pi: &PixelImage| Some([0, 0, pi.img.width(), pi.img.height()]);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let name = crate::ui::pixel_bar::new_layer_name(pe);
        if icon_button(ui, LayerIcon::New, &tip("pixel.layer.new", Some("pixel.layer.new_tip")), true, false).clicked() {
            pe.apply_all(true, |pi| {
                pi.add_layer(name.clone());
                full(pi)
            });
        }
        if icon_button(ui, LayerIcon::Duplicate, &tip("pixel.layer.duplicate", Some("pixel.layer.duplicate_tip")), true, false).clicked() {
            pe.apply_all(true, |pi| {
                pi.duplicate_layer(name.clone());
                full(pi)
            });
        }
        if !has_mask
            && icon_button(
                ui,
                LayerIcon::AddMask,
                &tip("pixel.mask.add", Some("pixel.mask.add_tip")),
                true,
                false,
            )
            .clicked()
            && pe.apply_all(true, |pi| pi.add_layer_mask().then(|| full(pi)).flatten())
        {
            pe.set_edit_layer_mask(true);
        }
        // The effect chosen in the Effects panel as a layer that keeps
        // it changeable.
        let effect = pe.effect.clone();
        if icon_button(
            ui,
            LayerIcon::Adjustment,
            &tip("pixel.layer.adjustment", Some("pixel.layer.adjustment_tip")),
            effect.is_some(),
            false,
        )
        .clicked()
        {
            if let Some(e) = effect {
                let label = tr(&format!("fx.{}", e.id));
                pe.apply_all(true, |pi| {
                    pi.add_adjustment_layer(label, e);
                    full(pi)
                });
                pe.effect = None;
                pe.preview = None;
            }
        }
        ui.separator();
        if icon_button(ui, LayerIcon::Up, &tip("pixel.layer.up", Some("pixel.layer.up_tip")), i + 1 < n, false).clicked() {
            pe.apply_all(true, |pi| pi.move_layer(true).then(|| full(pi)).flatten());
        }
        if icon_button(ui, LayerIcon::Down, &tip("pixel.layer.down", Some("pixel.layer.down_tip")), i > 0, false).clicked() {
            pe.apply_all(true, |pi| pi.move_layer(false).then(|| full(pi)).flatten());
        }
        if icon_button(ui, LayerIcon::MergeDown, &tip("pixel.layer.merge_down", Some("pixel.layer.merge_down_tip")), i > 0, false).clicked() {
            pe.apply_all(true, |pi| pi.merge_down().then(|| full(pi)).flatten());
        }
        if icon_button(ui, LayerIcon::Flatten, &tip("pixel.layer.flatten", Some("pixel.layer.flatten_tip")), pe.pi.is_layered(), false)
            .clicked()
        {
            pe.apply_all(true, |pi| pi.flatten().then(|| full(pi)).flatten());
        }
        if icon_button(ui, LayerIcon::Delete, &tip("pixel.layer.delete", Some("pixel.layer.delete_tip")), n > 1, false).clicked() {
            pe.apply_all(true, |pi| pi.delete_layer().then(|| full(pi)).flatten());
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let name = crate::ui::pixel_bar::new_layer_name(pe);
        let has_sel = pe.pi.sel.is_some();
        if icon_button(
            ui,
            LayerIcon::CopyTo,
            &tip("pixel.layer.copy_to", Some("pixel.layer.copy_to_tip")),
            has_sel,
            false,
        )
        .clicked()
        {
            pe.apply_all(true, |pi| pi.selection_to_layer(name.clone(), false).then(|| full(pi)).flatten());
        }
        if icon_button(
            ui,
            LayerIcon::CutTo,
            &tip("pixel.layer.cut_to", Some("pixel.layer.cut_to_tip")),
            has_sel,
            false,
        )
        .clicked()
        {
            pe.apply_all(true, |pi| pi.selection_to_layer(name.clone(), true).then(|| full(pi)).flatten());
        }
        if icon_button(
            ui,
            LayerIcon::Paste,
            &tip("pixel.layer.paste", Some("pixel.layer.paste_tip")),
            true,
            false,
        )
        .clicked()
        {
            // What Ctrl+C took goes back where it was (#4263).
            pe.paste_pixels(name.clone());
        }
        // The layer laid onto the photo beneath it, with the depth and
        // light of the brush's surface options.
        if icon_button(
            ui,
            LayerIcon::FitSurface,
            &tip("pixel.layer.fit_surface", Some("pixel.layer.fit_surface_tip")),
            pe.pi.active > 0 && !pe.pi.active_locked(),
            false,
        )
        .clicked()
        {
            let map = crate::pixel_surface::SurfaceMap::new(
                &crate::pixel_surface::seen_beneath(&pe.pi),
                pe.depth.as_ref(),
            );
            let (d, l) = (pe.opts.surface_depth, pe.opts.surface_light);
            pe.apply(true, |pi| crate::pixel_surface::fit_to_surface(pi, &map, d, l));
        }
        // Clip to the layer below.
        let clip = pe.pi.layers.get(i).is_some_and(|l| l.clip);
        if icon_button(
            ui,
            LayerIcon::Clip,
            &tip("pixel.layer.clip", Some("pixel.layer.clip_tip")),
            i > 0,
            clip,
        )
        .clicked()
        {
            pe.apply_all(true, |pi| {
                let l = pi.layers.get_mut(i)?;
                l.clip = !l.clip;
                full(pi)
            });
        }
    });
}

/// The mask commands of the layer in use, when it has a mask.
fn mask_buttons(ui: &mut Ui, pe: &mut PixelEdit) {
    let i = pe.pi.active;
    if !pe.pi.layers.get(i).is_some_and(|l| l.mask.is_some()) {
        return;
    }
    let full = |pi: &PixelImage| Some([0, 0, pi.img.width(), pi.img.height()]);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.label(egui::RichText::new(tr("pixel.mask.title")).color(Tokens::text_dim()));
        let editing = pe.target == MaskTarget::LayerMask;
        if icon_button(
            ui,
            LayerIcon::EditMask,
            &tip("pixel.mask.edit", Some("pixel.mask.edit_tip")),
            true,
            editing,
        )
        .clicked()
        {
            pe.set_edit_layer_mask(!editing);
        }
        if icon_button(ui, LayerIcon::InvertMask, &tip("pixel.mask.invert", Some("pixel.mask.invert_tip")), true, false).clicked() {
            pe.apply_all(true, |pi| pi.invert_layer_mask().then(|| full(pi)).flatten());
        }
        if icon_button(
            ui,
            LayerIcon::ApplyMask,
            &tip("pixel.mask.apply", Some("pixel.mask.apply_tip")),
            true,
            false,
        )
        .clicked()
        {
            pe.apply(true, |pi| pi.apply_layer_mask().then(|| full(pi)).flatten());
            pe.set_edit_layer_mask(false);
        }
        if icon_button(ui, LayerIcon::DeleteMask, &tip("pixel.mask.delete", Some("pixel.mask.delete_tip")), true, false).clicked() {
            pe.apply_all(true, |pi| pi.delete_layer_mask().then(|| full(pi)).flatten());
            pe.set_edit_layer_mask(false);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miniatures_keep_the_pictures_proportions() {
        let img = image::RgbaImage::new(400, 100);
        assert_eq!(shrink(&img, 40).dimensions(), (40, 10));
        let small = image::RgbaImage::new(8, 16);
        assert_eq!(shrink(&small, 40).dimensions(), (8, 16), "never enlarged");
        assert_eq!(shrink_size((0, 5), 40), (1, 1));
    }

    #[test]
    fn the_row_menu_commands_work_on_the_layer_in_use() {
        use tracedraw_core::{geometry::Rect as R, ShapeKind};
        let mut app = crate::app::App::headless();
        let img = image::RgbaImage::from_pixel(20, 10, image::Rgba([255, 255, 255, 255]));
        let id = app
            .new_shape(ShapeKind::Bitmap {
                rect: R::new(0.0, 0.0, 20.0, 10.0),
                width_px: 20,
                height_px: 10,
                png: crate::bitmap_fx::encode(&img).unwrap().into(),
                fx: None,
            })
            .expect("a picture");
        assert!(app.edit_bitmap_inside(id));
        let pe = app.pixel_edit.as_mut().unwrap();
        run_cmd(pe, RowCmd::Duplicate);
        assert_eq!(pe.pi.layers.len(), 2);
        run_cmd(pe, RowCmd::Shown);
        assert!(!pe.pi.layers[1].visible);
        run_cmd(pe, RowCmd::Locked);
        assert!(pe.pi.layers[1].locked);
        run_cmd(pe, RowCmd::Clip);
        assert!(pe.pi.layers[1].clip);
        run_cmd(pe, RowCmd::AddMask);
        assert!(pe.pi.layers[1].mask.is_some());
        assert_eq!(pe.target, MaskTarget::LayerMask);
        run_cmd(pe, RowCmd::DeleteMask);
        assert!(pe.pi.layers[1].mask.is_none());
        run_cmd(pe, RowCmd::Down);
        assert_eq!(pe.pi.active, 0);
        run_cmd(pe, RowCmd::Delete);
        assert_eq!(pe.pi.layers.len(), 1);
    }

    #[test]
    fn every_layer_icon_draws_in_its_box() {
        for icon in LayerIcon::ALL {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(40.0))),
                ..Default::default()
            };
            let box_ = Rect::from_min_size(egui::pos2(4.0, 4.0), Vec2::splat(22.0));
            let mut out = ctx.run_ui(input, |ui| draw_icon(ui.painter(), box_, icon, Color32::BLACK));
            out.textures_delta.clear();
            assert!(!out.shapes.is_empty(), "{icon:?} drew nothing");
            for s in out.shapes {
                let r = s.shape.visual_bounding_rect();
                assert!(box_.expand(2.0).contains_rect(r), "{icon:?} leaves its box: {r:?}");
            }
        }
    }
}
