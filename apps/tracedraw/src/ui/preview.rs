//! Full-screen preview (F9) and the Page Sorter view.

use crate::ui::btn_icons::IconUi;
use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use egui::Ui;
use tracedraw_core::Command;

/// Capture the editor's center and scale before the window changes size.
/// The larger screen reveals more canvas without moving the editor's view.
pub struct PreviewSession {
    pub(crate) bounds: tracedraw_core::geometry::Rect,
    pub(crate) zoom: f64,
    was_fullscreen: bool,
    /// The page fills the screen (Options > Display > Full-screen
    /// preview, #4239) until the preview is zoomed.
    fit_page: bool,
    /// When the preview opened, to wait for the window to fill the screen.
    started: f64,
    hint_until: f64,
    texture: Option<(PreviewKey, egui::TextureHandle)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewKey {
    width: u32,
    height: u32,
    pixels_per_point: u32,
    document: (u64, u64),
    page: tracedraw_core::PageId,
    font_generation: u64,
}

pub fn begin(app: &mut App, ctx: &egui::Context, selected_only: bool) {
    if app.preview_session.is_some() {
        return;
    }
    app.zoom_glide.stop();
    app.pan_inertia.stop();
    app.preview_selected_only = selected_only;
    app.preview_session = Some(PreviewSession {
        bounds: preview_bounds(app),
        zoom: f64::from(app.view.zoom),
        was_fullscreen: ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)),
        fit_page: app.settings.preview_fit_page && !selected_only,
        started: ctx.input(|i| i.time),
        hint_until: ctx.input(|i| i.time) + 1.5,
        texture: None,
    });
    app.fullscreen_preview = true;
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
}

pub fn end(app: &mut App, ctx: &egui::Context) {
    if let Some(session) = app.preview_session.take() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(session.was_fullscreen));
    }
    app.fullscreen_preview = false;
    app.preview_selected_only = false;
}

/// The editor's visible canvas when preview starts. Distant objects do
/// not change its center or scale.
pub fn preview_bounds(app: &App) -> tracedraw_core::geometry::Rect {
    if let Some(session) = &app.preview_session {
        return session.bounds;
    }
    let page = app.page_rect();
    if app.preview_selected_only {
        return app.selection_bounds().unwrap_or(page);
    }
    let bounds = app.view.visible_page_rect(app.canvas_rect);
    if bounds.is_finite() && bounds.width() > 0.0 && bounds.height() > 0.0 {
        bounds
    } else {
        page
    }
}

/// A page's miniature as it is cached for the Pages docker and the page
/// tabs (#2932).
#[derive(Clone)]
struct PageThumb {
    /// The drawing's revision it was drawn at.
    revision: u64,
    texture: Option<egui::TextureHandle>,
    /// Its size, interface points.
    size: egui::Vec2,
}

/// The miniature of `page`, `long` points on its longer side, with its
/// size in points (#2932). It is cached by page and size and drawn again
/// once the drawing changed; at most one page is drawn in a frame, the
/// others keep their old picture (or none yet) until the next frames, so
/// a drawing of many pages neither lags nor stops while the docker or a
/// tab's tip shows them.
pub fn page_thumb(
    ui: &Ui,
    doc: &tracedraw_core::Document,
    revision: u64,
    page: tracedraw_core::PageId,
    long: f32,
) -> Option<(egui::TextureId, egui::Vec2)> {
    let ctx = ui.ctx();
    let ppp = ctx.pixels_per_point();
    let px = (long * ppp).round().clamp(8.0, 1024.0) as u32;
    let id = egui::Id::new(("page_thumb", page.raw(), px));
    let cached: Option<PageThumb> = ctx.data(|d| d.get_temp(id));
    let current = cached.as_ref().is_some_and(|c| c.revision == revision);
    if !current {
        let frame = ctx.cumulative_frame_nr();
        let turn = egui::Id::new("page_thumb_frame");
        if ctx.data(|d| d.get_temp::<u64>(turn)) == Some(frame) {
            // Another page was drawn this frame: this one comes next.
            ctx.request_repaint();
        } else {
            ctx.data_mut(|d| d.insert_temp(turn, frame));
            let drawn = doc.page(page).ok().and_then(|p| {
                let longer = p.size.width.max(p.size.height);
                if longer <= 0.0 || !longer.is_finite() {
                    return None;
                }
                let dpi = (px as f64 / longer * 25.4).max(1.0);
                tracedraw_render::render_page_image(doc, page, dpi)
            });
            let thumb = match drawn {
                Some(pm) => {
                    let image = egui::ColorImage::from_rgba_premultiplied(
                        [pm.width() as usize, pm.height() as usize],
                        pm.data(),
                    );
                    let texture = ctx.load_texture(
                        format!("page_thumb_{}_{px}", page.raw()),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    PageThumb {
                        revision,
                        texture: Some(texture),
                        size: egui::vec2(pm.width() as f32, pm.height() as f32) / ppp,
                    }
                }
                None => PageThumb {
                    revision,
                    texture: None,
                    size: egui::Vec2::ZERO,
                },
            };
            ctx.data_mut(|d| d.insert_temp(id, thumb.clone()));
            return thumb.texture.map(|t| (t.id(), thumb.size));
        }
    }
    cached.and_then(|c| c.texture.map(|t| (t.id(), c.size)))
}

/// Paint a page's miniature fitted in `rect`, on a white sheet with a thin
/// edge; the sheet alone while the picture is still to come.
pub fn paint_page_thumb(
    ui: &Ui,
    rect: egui::Rect,
    thumb: Option<(egui::TextureId, egui::Vec2)>,
    page_size: tracedraw_core::Size,
) {
    let aspect = (page_size.width / page_size.height.max(1e-6)) as f32;
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    let fit = if aspect >= rect.width() / rect.height().max(1.0) {
        egui::vec2(rect.width(), rect.width() / aspect)
    } else {
        egui::vec2(rect.height() * aspect, rect.height())
    };
    let sheet = egui::Rect::from_center_size(rect.center(), fit);
    let p = ui.painter();
    p.rect_filled(sheet, 0.0, egui::Color32::WHITE);
    if let Some((tex, _)) = thumb {
        p.image(
            tex,
            sheet,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    p.rect_stroke(
        sheet,
        0.0,
        egui::Stroke::new(1.0, Tokens::text_dim()),
        egui::StrokeKind::Outside,
    );
}

/// Expand the canvas to the available screen while keeping its center
/// and zoom. Selection preview keeps fitting its own bounds instead.
fn frame_bounds(app: &App, size: egui::Vec2) -> tracedraw_core::geometry::Rect {
    let bounds = preview_bounds(app);
    if app.preview_selected_only {
        return bounds;
    }
    let zoom = app
        .preview_session
        .as_ref()
        .map_or(f64::from(app.view.zoom), |session| session.zoom)
        .max(1e-6);
    let center = bounds.center();
    let half_width = f64::from(size.x) / (2.0 * zoom);
    let half_height = f64::from(size.y) / (2.0 * zoom);
    tracedraw_core::geometry::Rect::new(
        center.x - half_width,
        center.y - half_height,
        center.x + half_width,
        center.y + half_height,
    )
}

/// How long, in seconds, the preview waits at most for the window to say
/// it fills the screen before it draws the drawing (#3830).
const FULL_SCREEN_WAIT: f64 = 0.5;

/// What a key or the wheel asks of the full-screen preview (#3930).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PreviewInput {
    /// Zoom by this factor about the middle of the screen.
    Zoom(f64),
    /// The next page (+1) or the one before (-1), fitted to the screen.
    Page(i32),
}

/// The keys and wheel turns of this frame, in the full-screen preview:
/// the plus and minus keys (and the wheel) zoom, Page Down / Page Up and
/// the arrows go to the next or previous page (#3930).
pub(crate) fn preview_keys(i: &egui::InputState) -> Vec<PreviewInput> {
    use egui::Key;
    let mut out = Vec::new();
    if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
        out.push(PreviewInput::Zoom(1.25));
    }
    if i.key_pressed(Key::Minus) {
        out.push(PreviewInput::Zoom(0.8));
    }
    let wheel = i.smooth_scroll_delta.y;
    if wheel != 0.0 {
        out.push(PreviewInput::Zoom(1.0015_f64.powf(f64::from(wheel))));
    }
    for k in [Key::PageDown, Key::ArrowRight, Key::ArrowDown] {
        if i.key_pressed(k) {
            out.push(PreviewInput::Page(1));
        }
    }
    for k in [Key::PageUp, Key::ArrowLeft, Key::ArrowUp] {
        if i.key_pressed(k) {
            out.push(PreviewInput::Page(-1));
        }
    }
    out
}

/// Do what [`preview_keys`] found: zoom the preview, or show another
/// page fitted to a screen of `size` points. Selection preview, which
/// fits the selection, only changes page.
pub(crate) fn apply_preview_keys(app: &mut App, keys: &[PreviewInput], size: egui::Vec2) {
    for k in keys {
        match *k {
            PreviewInput::Zoom(f) => {
                if let Some(session) = &mut app.preview_session {
                    session.zoom = (session.zoom * f).clamp(0.01, 500.0);
                    session.fit_page = false;
                }
            }
            PreviewInput::Page(step) => {
                let n = app.doc().pages.len();
                let at = app.page_index() as i64 + i64::from(step);
                if n < 2 || at < 0 || at >= n as i64 {
                    continue;
                }
                app.goto_page(at as usize);
                let page = app.page_rect();
                if let Some(session) = &mut app.preview_session {
                    let fit = (f64::from(size.x) / page.width().max(1e-6))
                        .min(f64::from(size.y) / page.height().max(1e-6));
                    session.bounds = page;
                    session.zoom = (fit * 0.95).max(1e-6);
                }
            }
        }
    }
}

pub fn fullscreen(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if app.preview_session.is_none() {
        begin(app, &ctx, app.preview_selected_only);
    }
    let keys = ctx.input(preview_keys);
    if !keys.is_empty() {
        let size = ctx.content_rect().size();
        apply_preview_keys(app, &keys, size);
    }
    // The page filling the screen, when the option asks for it: the
    // page's middle in the screen's, as large as the screen lets it.
    let screen = ctx.content_rect().size();
    let page = app.page_rect();
    if let Some(session) = app.preview_session.as_mut().filter(|s| s.fit_page) {
        let fit = (f64::from(screen.x) / page.width().max(1e-6))
            .min(f64::from(screen.y) / page.height().max(1e-6));
        if fit.is_finite() && fit > 0.0 {
            session.bounds = page;
            session.zoom = fit;
        }
    }
    let background = app.preview_background_color();
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(if app.preview_selected_only {
                    egui::Color32::from_gray(40)
                } else {
                    background
                })
                .inner_margin(0.0),
        )
        .show(ui, |ui| {
            let rect = ui.available_rect_before_wrap();
            let page = app.page_rect();
            let bounds = frame_bounds(app, rect.size());
            let painter = ui.painter_at(rect);
            // Only selection preview fits a crop. Ordinary preview uses
            // every screen pixel at the editor's original scale.
            let margin = if app.preview_selected_only { 20.0 } else { 0.0 };
            let sx = (rect.width() - 2.0 * margin) as f64 / bounds.width().max(1e-6);
            let sy = (rect.height() - 2.0 * margin) as f64 / bounds.height().max(1e-6);
            let zoom = sx.min(sy).max(1e-6);
            let size = egui::vec2(
                (bounds.width() * zoom) as f32,
                (bounds.height() * zoom) as f32,
            );
            let pixels_per_point = ctx.pixels_per_point() as f64;
            let pixel_zoom = zoom * pixels_per_point;
            let w = (bounds.width() * pixel_zoom).ceil().max(1.0) as u32;
            let h = (bounds.height() * pixel_zoom).ceil().max(1.0) as u32;
            let key = PreviewKey {
                width: w,
                height: h,
                pixels_per_point: ctx.pixels_per_point().to_bits(),
                document: (app.engine.instance(), app.engine.revision()),
                page: app.page,
                font_generation: app.font_generation,
            };
            let view = tracedraw_render::ViewTransform {
                zoom: pixel_zoom,
                origin_x: -bounds.x0 * pixel_zoom,
                origin_y: bounds.y1 * pixel_zoom,
                stretch_y: 1.0,
            };
            let opts = tracedraw_render::RenderOptions {
                width: w.max(1),
                height: h.max(1),
                view,
                preview: None,
                wireframe: false,
                ..tracedraw_render::RenderOptions::default()
            };
            let cached = app
                .preview_session
                .as_ref()
                .is_some_and(|s| s.texture.as_ref().is_some_and(|(k, _)| *k == key));
            // The drawing waits for the window to fill the screen (#3830):
            // drawn first in the window as it was and again a moment later
            // in the whole screen, it jumped and flickered as F9 opened.
            // A system that never says the window is full screen gets the
            // drawing after a moment anyway.
            let now = ctx.input(|i| i.time);
            let waited = app
                .preview_session
                .as_ref()
                .map_or(f64::INFINITY, |s| now - s.started);
            let ready = app
                .preview_session
                .as_ref()
                .is_none_or(|s| s.was_fullscreen)
                || ctx.input(|i| i.viewport().fullscreen) == Some(true)
                || waited >= FULL_SCREEN_WAIT;
            if !ready {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                    (FULL_SCREEN_WAIT - waited).max(0.0),
                ));
            }
            // While a screenshot tool or another window has the focus, or
            // the window is minimised, the picture already made stays: the
            // window's size and scale change as the system takes the
            // screen, and drawing the whole page again for each change
            // hung the program (#4198). It is drawn again once the window
            // is back.
            let away = ctx.input(|i| {
                i.viewport().minimized == Some(true) || i.viewport().focused == Some(false)
            });
            let has_picture = app
                .preview_session
                .as_ref()
                .is_some_and(|s| s.texture.is_some());
            let tiny = rect.width() < 16.0 || rect.height() < 16.0;
            if !cached && ready && !tiny && !(away && has_picture) {
                if let Some(pm) = tracedraw_render::render_page(app.doc(), app.page, &opts) {
                    let image = egui::ColorImage::from_rgba_premultiplied(
                        [pm.width() as usize, pm.height() as usize],
                        pm.data(),
                    );
                    let tex =
                        ctx.load_texture("fullscreen_preview", image, egui::TextureOptions::LINEAR);
                    if let Some(session) = &mut app.preview_session {
                        session.texture = Some((key, tex));
                    }
                }
            }
            if let Some((_, tex)) = app
                .preview_session
                .as_ref()
                .filter(|_| ready)
                .and_then(|s| s.texture.as_ref())
            {
                let pos = rect.center() - size / 2.0;
                let page_rect = egui::Rect::from_min_size(pos, size);
                let painter = painter.with_clip_rect(page_rect);
                let at = |x: f64, y: f64| {
                    page_rect.min
                        + egui::vec2(
                            ((x - bounds.x0) * zoom) as f32,
                            ((bounds.y1 - y) * zoom) as f32,
                        )
                };
                let sheet = egui::Rect::from_two_pos(at(page.x0, page.y0), at(page.x1, page.y1));
                if !app.preview_selected_only {
                    // The colour chosen round the page, the page itself
                    // white as paper (#3930).
                    painter.rect_filled(page_rect, 0.0, background);
                    painter.rect_filled(sheet, 0.0, egui::Color32::WHITE);
                }
                painter.image(
                    tex.id(),
                    page_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                // Options > Display > Full-screen preview > Show page border:
                // round the page, inside what is shown.
                if !app.preview_selected_only && app.settings.preview_page_border {
                    painter.rect_stroke(
                        sheet,
                        0.0,
                        egui::Stroke::new(1.0, Tokens::page_border()),
                        egui::StrokeKind::Outside,
                    );
                }
            }
            let hint_left = app
                .preview_session
                .as_ref()
                .map_or(0.0, |session| session.hint_until - now);
            if hint_left > 0.0 {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(hint_left));
                // How to leave, and the keys that zoom and turn the pages
                // (#3930).
                let text = if app.preview_selected_only {
                    tr("preview.exit_hint")
                } else {
                    format!("{}\n{}", tr("preview.exit_hint"), tr("preview.keys_hint"))
                };
                let hint = painter.layout_no_wrap(
                    text,
                    egui::FontId::proportional(12.0),
                    egui::Color32::from_gray(200),
                );
                let hint_pos = egui::pos2(
                    rect.center().x - hint.size().x / 2.0,
                    rect.max.y - hint.size().y - 10.0,
                );
                painter.rect_filled(
                    egui::Rect::from_min_size(hint_pos, hint.size()).expand2(egui::vec2(8.0, 4.0)),
                    4.0,
                    egui::Color32::from_gray(40),
                );
                painter.galley(hint_pos, hint, egui::Color32::from_gray(200));
            }
        });
    let exit = ctx.input(|i| {
        i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::F9) || i.pointer.any_click()
    });
    if exit {
        end(app, &ctx);
    }
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod viewport_tests;

pub fn page_sorter(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.heading(tr("menu.view.page_sorter"));
        if ui.ibutton("preview.back_to_page").clicked() {
            app.page_sorter = false;
        }
        if ui.ibutton("menu.layout.insert_page").clicked() {
            app.add_page();
        }
    });
    ui.separator();
    let pages: Vec<(tracedraw_core::PageId, String, tracedraw_core::Size)> = app
        .doc()
        .pages
        .iter()
        .map(|p| (p.id, p.name.clone(), p.size))
        .collect();
    let n = pages.len();
    let doc = app.engine.document().clone();
    let mut goto: Option<usize> = None;
    let mut mv: Option<(tracedraw_core::PageId, usize)> = None;
    let mut del: Option<tracedraw_core::PageId> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (i, (id, name, size)) in pages.iter().enumerate() {
                let thumb_h = 150.0f32;
                let thumb_w = (thumb_h as f64 * size.width / size.height.max(1e-6)) as f32;
                ui.vertical(|ui| {
                    let (rect, resp) = ui.allocate_exact_size(
                        egui::vec2(thumb_w.max(40.0), thumb_h),
                        egui::Sense::click(),
                    );
                    let active = *id == app.page;
                    ui.painter().rect_filled(
                        rect.expand(3.0),
                        3.0,
                        if active {
                            Tokens::tool_active()
                        } else {
                            Tokens::panel_dark()
                        },
                    );
                    let dpi = thumb_h as f64 / (size.height / 25.4);
                    if let Some(pm) = tracedraw_render::render_page_image(&doc, *id, dpi.max(4.0)) {
                        let image = egui::ColorImage::from_rgba_premultiplied(
                            [pm.width() as usize, pm.height() as usize],
                            pm.data(),
                        );
                        let tex = ui.ctx().load_texture(
                            format!("sorter_{}", id.raw()),
                            image,
                            egui::TextureOptions::LINEAR,
                        );
                        ui.painter().image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                    if resp.clicked() {
                        goto = Some(i);
                    }
                    if resp.double_clicked() {
                        goto = Some(i);
                        app.page_sorter = false;
                    }
                    ui.horizontal(|ui| {
                        ui.label(format!("{}. {name}", i + 1));
                        if i > 0 && ui.small_button("\u{25C0}").clicked() {
                            mv = Some((*id, i - 1));
                        }
                        if i + 1 < n && ui.small_button("\u{25B6}").clicked() {
                            mv = Some((*id, i + 1));
                        }
                        if n > 1 && ui.small_button("x").clicked() {
                            del = Some(*id);
                        }
                    });
                });
                ui.add_space(12.0);
            }
        });
    });
    if let Some(i) = goto {
        app.goto_page(i);
    }
    if let Some((id, to)) = mv {
        app.run(Command::MovePage { page: id, to });
    }
    if let Some(id) = del {
        app.run(Command::DeletePage { page: id });
        if app.page == id {
            app.page = app.doc().pages[0].id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    /// Page miniatures (#2932): one page drawn a frame, the next one the
    /// frame after, a drawn page kept while the drawing stays the same and
    /// drawn again once it changed.
    #[test]
    fn page_thumbs_are_drawn_one_a_frame_and_follow_changes() {
        let mut app = App::headless();
        app.show_welcome = false;
        app.add_page();
        let ctx = crate::theme::ui_context();
        let ids: Vec<_> = app.doc().pages.iter().map(|p| p.id).collect();
        let mut frame = |doc: &tracedraw_core::Document, rev: u64| {
            let mut got = Vec::new();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                got = ids.iter().map(|id| page_thumb(ui, doc, rev, *id, 40.0)).collect();
            });
            out.textures_delta.clear();
            got
        };
        let doc = app.doc().clone();
        let rev = app.engine.revision();
        let first = frame(&doc, rev);
        let (tex0, size) = first[0].expect("the first page is drawn");
        assert!(first[1].is_none(), "the second waits for the next frame");
        let longer = size.x.max(size.y);
        assert!((longer - 40.0).abs() <= 1.5, "{size:?}");
        let second = frame(&doc, rev);
        assert_eq!(second[0].map(|t| t.0), Some(tex0), "kept, not drawn again");
        let tex1 = second[1].expect("the second page now").0;
        let third = frame(&doc, rev);
        assert_eq!(third[1].map(|t| t.0), Some(tex1));
        // The drawing changed: the first page is drawn again.
        let changed = frame(&doc, rev + 1);
        let again = changed[0].expect("drawn again").0;
        assert_ne!(again, tex0);
        assert_eq!(changed[1].map(|t| t.0), Some(tex1), "the old picture meanwhile");
    }

    /// Objects beside the page still render when the canvas shows them;
    /// Preview Selected Only keeps its selection crop (#2317).
    #[test]
    fn the_preview_shows_objects_off_the_page() {
        let mut app = App::headless();
        app.show_welcome = false;
        let page = app.page_rect();
        assert_eq!(preview_bounds(&app), page, "an empty drawing: its page");
        // A wide bar above the page, as a title drawn off the sheet.
        let bar = Rect::new(
            page.x0 - 200.0,
            page.y1 + 50.0,
            page.x1 + 200.0,
            page.y1 + 90.0,
        );
        let id = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: bar,
                radius: 0.0,
                corners: None,
            })
            .expect("a rectangle");
        app.canvas_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        app.view.fit(page.union(bar), app.canvas_rect);
        let b = preview_bounds(&app);
        assert_eq!(b, app.view.visible_page_rect(app.canvas_rect));
        assert!(
            b.x0 <= bar.x0 + 1e-6 && b.x1 >= bar.x1 - 1e-6 && b.y1 >= bar.y1 - 1e-6,
            "{b:?} {bar:?}"
        );
        assert!(b.y0 <= page.y0 + 1e-6, "the page stays in: {b:?}");
        // Rendered as the preview renders it, the bar shows.
        let zoom = 0.5;
        let opts = tracedraw_render::RenderOptions {
            width: (b.width() * zoom).ceil() as u32,
            height: (b.height() * zoom).ceil() as u32,
            view: tracedraw_render::ViewTransform {
                zoom,
                origin_x: -b.x0 * zoom,
                origin_y: b.y1 * zoom,
                stretch_y: 1.0,
            },
            preview: None,
            wireframe: false,
            ..tracedraw_render::RenderOptions::default()
        };
        let doc = app.engine.document().clone();
        let pm = tracedraw_render::render_page(&doc, app.page, &opts).expect("render");
        // Some of the bar's outline is drawn in the strip above the page.
        let top = ((b.y1 - bar.y1) * zoom).floor() as u32;
        let bottom = ((b.y1 - bar.y0) * zoom).ceil() as u32;
        let drawn = (top..bottom.min(pm.height()))
            .flat_map(|y| (0..pm.width()).map(move |x| (x, y)))
            .filter(|(x, y)| pm.pixel(*x, *y).is_some_and(|p| p.alpha() > 0))
            .count();
        assert!(drawn > 0, "the bar is drawn");
        // Preview Selected Only: the selection's box.
        app.select(vec![id]);
        app.preview_selected_only = true;
        let sel = app.selection_bounds().expect("selected");
        assert_eq!(preview_bounds(&app), sel);
    }

    /// One frame of the whole window with `events`.
    fn window(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
        out.textures_delta.clear();
    }

    fn f9() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::F9,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn f9_opens_the_full_screen_preview_and_closes_it() {
        // #2195: F9 worked only from the View menu.
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        window(&ctx, &mut app, Vec::new());
        assert!(!app.fullscreen_preview);
        window(&ctx, &mut app, vec![f9()]);
        assert!(app.fullscreen_preview, "F9 opens the preview");
        window(&ctx, &mut app, Vec::new());
        assert!(app.fullscreen_preview, "it stays open");
        window(&ctx, &mut app, vec![f9()]);
        assert!(!app.fullscreen_preview, "F9 again closes it");
        window(&ctx, &mut app, Vec::new());
        assert!(!app.fullscreen_preview);
    }

    /// #4198: while a screenshot tool has the focus (the window losing
    /// it, its size changing as the system takes the screen), the
    /// preview keeps the picture it made instead of drawing the page again
    /// at every change, and stays open; back in focus, it draws again.
    #[test]
    fn a_screenshot_tool_taking_the_focus_keeps_the_preview_picture() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        let run = |app: &mut App, t: f64, size: (f32, f32), focused: bool, events| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size.0, size.1),
                )),
                time: Some(t),
                events,
                ..Default::default()
            };
            input.viewports.insert(
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    focused: Some(focused),
                    fullscreen: Some(true),
                    ..Default::default()
                },
            );
            let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
            out.textures_delta.clear();
        };
        let key = |app: &App| {
            app.preview_session
                .as_ref()
                .and_then(|s| s.texture.as_ref().map(|(k, _)| k.clone()))
        };
        run(&mut app, 0.0, (1400.0, 900.0), true, Vec::new());
        run(&mut app, 0.1, (1400.0, 900.0), true, vec![f9()]);
        for k in 0..3 {
            run(&mut app, 1.0 + f64::from(k), (1400.0, 900.0), true, Vec::new());
        }
        assert!(app.fullscreen_preview);
        let made = key(&app).expect("the preview picture");
        for (k, size) in [(1200.0, 800.0), (1000.0, 700.0), (8.0, 8.0)].into_iter().enumerate() {
            run(&mut app, 5.0 + k as f64, size, false, Vec::new());
            assert_eq!(key(&app).as_ref(), Some(&made), "kept while away");
            assert!(app.fullscreen_preview, "still open");
        }
        run(&mut app, 10.0, (1200.0, 800.0), true, Vec::new());
        let again = key(&app).expect("drawn again");
        assert_ne!(again, made, "drawn again at the new size");
    }
}
