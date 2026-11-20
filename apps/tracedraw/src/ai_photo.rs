//! AI photo commands: Turn Object in 3D, Depth Map, Sketch to Curves and
//! Remove Object, made by the one AI service the user sets up with their
//! own key (Tools > Options > AI > Service), when it can make pictures.
//! They are shown only while AI features are on (Tools > Options > AI >
//! General); with AI off nothing of them shows and nothing changes.
//! TraceDraw has no AI of its own and nothing is sent until the user
//! asks. The requests go through `ai_image.rs` (its jobs, retries and
//! errors); this module holds the requests' words, the dialogs and what
//! is done with each answer. See docs/behavior/ai-photo.md.

use image::{GrayImage, Luma, Rgba, RgbaImage};

use crate::ai_image::{
    build_edit, chroma_key, has_transparency, GenImage, ImageProvider, ImageStatus, Job,
};
use crate::app::App;
use crate::i18n::tr;
use crate::net::HttpRequest;
use tracedraw_core::document::{Shape, ShapeKind};
use tracedraw_core::{Command, Rect, ShapeId};

/// The undo steps' names.
const LABEL_TURN: &str = "Turn Object in 3D";
const LABEL_DEPTH: &str = "Depth Map";
const LABEL_REMOVE: &str = "Remove Object";

/// The most degrees an object turns either way, and tilts.
pub const MAX_TURN: f32 = 180.0;
pub const MAX_TILT: f32 = 80.0;

/// Turn Object in 3D: how far, and what comes back.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnState {
    /// Degrees around the object's upright axis; positive turns it to its
    /// right (its left side comes into view).
    pub turn: f32,
    /// Degrees up or down; positive shows more of the top.
    pub tilt: f32,
    /// The picture comes back with its background (else the object alone,
    /// on a transparent background).
    pub keep_background: bool,
    /// More words for the service (optional).
    pub words: String,
}

impl Default for TurnState {
    fn default() -> Self {
        TurnState {
            turn: 45.0,
            tilt: 0.0,
            keep_background: false,
            words: String::new(),
        }
    }
}

impl TurnState {
    /// It asks for a change of view at all.
    pub fn moves(&self) -> bool {
        self.turn.abs() >= 1.0 || self.tilt.abs() >= 1.0
    }
}

/// Sketch to Curves: the request in words.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SketchState {
    pub words: String,
}

/// Remove Object: what to take out, in words (the painted marks of the
/// photo editor go with [`App::ai_remove_object`] instead).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RemoveState {
    pub words: String,
}

/// The words of a turn. `chroma`: the service gives no transparency, so
/// the object comes on flat green, keyed out afterwards.
pub fn turn_prompt(st: &TurnState, chroma: bool) -> String {
    let mut moves = Vec::new();
    if st.turn.abs() >= 1.0 {
        moves.push(format!(
            "turned {:.0} degrees to its {} around its upright axis",
            st.turn.abs().min(MAX_TURN),
            if st.turn > 0.0 { "right" } else { "left" }
        ));
    }
    if st.tilt.abs() >= 1.0 {
        moves.push(format!(
            "tilted {:.0} degrees so that more of its {} shows",
            st.tilt.abs().min(MAX_TILT),
            if st.tilt > 0.0 { "top" } else { "underside" }
        ));
    }
    let mut p = format!(
        "Show exactly the same object from a new viewpoint: {}. Keep its identity, shape, \
         proportions, colours, materials, markings and lighting. Invent the sides that were \
         hidden in the original so they match the visible ones naturally. Keep the whole \
         object in the frame at about the same size and place.",
        moves.join(" and ")
    );
    p.push(' ');
    p.push_str(if st.keep_background {
        "Keep the scene around it as it is and fill in any part of it the object no longer covers."
    } else if chroma {
        "Show the object alone on a flat, even, pure green background (#00FF00) with no shadows or gradients."
    } else {
        "Show the object alone on a transparent background."
    });
    let w = st.words.trim();
    if !w.is_empty() {
        p.push_str("\n\nAlso: ");
        p.push_str(w);
    }
    p
}

/// The words of a depth map.
pub const DEPTH_PROMPT: &str = "Make a depth map of this picture: exactly the same framing and \
size, in grayscale only, where white is nearest to the viewer and black is farthest, with \
smooth, gradual shading that follows the 3D shape of every surface. No outlines, no texture, \
no text, no colour. Reply with the depth map picture only.";

/// The request used when Sketch to Curves is given no words.
pub const SKETCH_DEFAULT: &str = "Ink the final lines and fill the shapes in solid black.";

/// The words of Sketch to Curves.
pub fn sketch_prompt(words: &str) -> String {
    let w = match words.trim() {
        "" => SKETCH_DEFAULT,
        w => w,
    };
    format!(
        "Redraw this hand-drawn sketch as clean, finished artwork that will be traced into \
         vector curves. Follow the sketch's shapes, letters, proportions and composition \
         exactly; do not add or remove elements. Use smooth, confident outlines and flat solid \
         colours only: no pencil texture, no construction or hatching lines, no shading, no \
         gradients, on a plain white background.\n\nRequest: {w}"
    )
}

/// The words of Remove Object: by name, or under the marks (`marked`).
pub fn remove_prompt(words: &str, marked: bool) -> String {
    let what = match (words.trim(), marked) {
        ("", true) => "the object painted over in red".to_string(),
        (w, true) => format!("{w} (painted over in red)"),
        (w, false) => w.to_string(),
    };
    format!(
        "Remove {what} from this photo completely, together with every trace it leaves: its \
         shadow, reflection, smoke, dust, footprints and any glow or colour it casts. Fill the \
         area with what would naturally be behind it, matching the surrounding background, \
         perspective, lighting, grain and focus, so no sign of it remains. Change nothing \
         else; keep the same framing and size."
    )
}

/// The picture with the marked area tinted red (for a service that takes
/// no mask), `mask` white where marked.
pub fn mark_red(img: &RgbaImage, mask: &GrayImage) -> RgbaImage {
    let mut out = img.clone();
    for (x, y, p) in out.enumerate_pixels_mut() {
        let m = mask
            .get_pixel_checked(x, y)
            .map(|Luma([v])| *v as f32 / 255.0)
            .unwrap_or(0.0);
        if m > 0.0 {
            let t = 0.7 * m;
            for (c, red) in p.0.iter_mut().take(3).zip([255.0, 0.0, 0.0]) {
                *c = (*c as f32 * (1.0 - t) + red * t).round() as u8;
            }
            p.0[3] = 255;
        }
    }
    out
}

/// The OpenAI-style mask: transparent where the picture is to change.
pub fn openai_mask(mask: &GrayImage) -> RgbaImage {
    RgbaImage::from_fn(mask.width(), mask.height(), |x, y| {
        let v = mask.get_pixel(x, y).0[0];
        Rgba([0, 0, 0, 255 - v])
    })
}

/// An edit with a mask, for the OpenAI Images API (`image` and `mask`).
pub fn build_masked_edit(
    prefs: &crate::ai_image::ImagePrefs,
    key: &str,
    prompt: &str,
    png: &[u8],
    mask_png: &[u8],
) -> HttpRequest {
    let mut http = build_edit(prefs, key, prompt, png, false);
    if prefs.provider != ImageProvider::OpenAi {
        return http;
    }
    let boundary = "tracedraw-ai-image-boundary-7f3a9c";
    let end = format!("\r\n--{boundary}--\r\n");
    if http.bytes.ends_with(end.as_bytes()) {
        let keep = http.bytes.len() - end.len();
        http.bytes.truncate(keep);
        http.bytes.extend_from_slice(
            format!(
                "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"mask\"; filename=\"mask.png\"\r\nContent-Type: image/png\r\n\r\n"
            )
            .as_bytes(),
        );
        http.bytes.extend_from_slice(mask_png);
        http.bytes.extend_from_slice(end.as_bytes());
    }
    http
}

/// A depth map of `w` x `h` from the service's picture: its brightness,
/// at the bitmap's size.
pub fn depth_from_picture(img: &RgbaImage, w: u32, h: u32) -> GrayImage {
    let gray = image::DynamicImage::ImageRgba8(img.clone()).to_luma8();
    if gray.dimensions() == (w, h) {
        gray
    } else {
        image::imageops::resize(
            &gray,
            w.max(1),
            h.max(1),
            image::imageops::FilterType::Triangle,
        )
    }
}

impl App {
    /// The AI photo commands show (AI features are on).
    pub fn ai_photo_shown(&self) -> bool {
        self.ai_features_on()
    }

    /// The AI photo commands can run now: the service makes pictures, one
    /// bitmap is selected and no request is waiting.
    pub fn ai_photo_enabled(&self) -> bool {
        self.ai_image_ready() && self.ai_source_bitmap().is_some() && !self.ai_image.busy()
    }

    /// Image > Turn Object in 3D with AI...
    pub fn open_ai_turn(&mut self) {
        self.dialog = crate::ui::dialogs::Dialog::AiTurn(self.ai_image.turn.clone());
    }

    /// Image > Sketch to Curves with AI...
    pub fn open_ai_sketch(&mut self) {
        self.dialog = crate::ui::dialogs::Dialog::AiSketch(self.ai_image.sketch.clone());
    }

    /// Image > Remove Object with AI...
    pub fn open_ai_remove(&mut self) {
        self.dialog = crate::ui::dialogs::Dialog::AiRemove(RemoveState::default());
    }

    /// Start a request on the selected bitmap; the problem in the status
    /// bar when it cannot go.
    fn ai_photo_start(&mut self) -> Option<(ShapeId, Vec<u8>, crate::ai_image::ImagePrefs)> {
        if self.ai_image.busy() {
            return None;
        }
        if let Some(p) = self.ai_image_problem() {
            self.status = p.clone();
            self.ai_image.status = ImageStatus::Failed(p);
            return None;
        }
        let Some((shape, png)) = self.ai_source_bitmap() else {
            self.status = tr("aiimg.need_bitmap");
            return None;
        };
        let prefs = self.ai_image_prefs()?;
        Some((shape, png.to_vec(), prefs))
    }

    /// Turn the selected bitmap's object in 3D; the new view goes over it.
    pub fn ai_turn_object(&mut self, st: &TurnState) {
        self.ai_image.turn = st.clone();
        if !st.moves() {
            self.status = tr("aiphoto.turn_none");
            return;
        }
        let Some((shape, png, prefs)) = self.ai_photo_start() else {
            return;
        };
        let chroma = !st.keep_background && prefs.provider == ImageProvider::Gemini;
        let cutout = !st.keep_background && !chroma;
        let http = build_edit(&prefs, &self.ai.key, &turn_prompt(st, chroma), &png, cutout);
        self.ai_image_begin(1);
        self.status = tr("aiphoto.turning");
        self.ai_image_send(
            http,
            Job::Turn {
                source: shape,
                transparent: !st.keep_background,
            },
        );
        self.poll_ai_image();
    }

    /// Ask for a depth map of `shape` (a bitmap); when it comes it is kept
    /// for the photo editor's brush and stamp ([`App::ai_take_depth`]) and,
    /// with `place`, also put over the bitmap as a grey picture.
    pub fn ai_depth_map(&mut self, shape: ShapeId, place: bool) {
        if self.ai_image.busy() {
            return;
        }
        if let Some(p) = self.ai_image_problem() {
            self.status = p.clone();
            self.ai_image.status = ImageStatus::Failed(p);
            return;
        }
        let png = match self.doc().find_shape(shape).map(|s| &s.kind) {
            Some(ShapeKind::Bitmap { png, .. }) => png.to_vec(),
            _ => {
                self.status = tr("aiimg.need_bitmap");
                return;
            }
        };
        let Some(prefs) = self.ai_image_prefs() else {
            return;
        };
        let http = build_edit(&prefs, &self.ai.key, DEPTH_PROMPT, &png, false);
        self.ai_image_begin(1);
        self.status = tr("aiphoto.depthing");
        self.ai_image_send(http, Job::Depth { shape, place });
        self.poll_ai_image();
    }

    /// Image > Depth Map with AI: of the selected bitmap, placed over it.
    pub fn ai_depth_map_selected(&mut self) {
        match self.ai_source_bitmap() {
            Some((shape, _)) => self.ai_depth_map(shape, true),
            None => self.status = tr("aiimg.need_bitmap"),
        }
    }

    /// The depth map that came for `shape` (255 nearest, the bitmap's size
    /// in pixels), taken: it is given once.
    pub fn ai_take_depth(&mut self, shape: ShapeId) -> Option<GrayImage> {
        match &self.ai_image.depth {
            Some((s, _)) if *s == shape => self.ai_image.depth.take().map(|(_, d)| d),
            _ => None,
        }
    }

    /// Sketch to Curves: the selected sketch redrawn as the words ask and
    /// traced into curves over it.
    pub fn ai_sketch_to_curves(&mut self, st: &SketchState) {
        self.ai_image.sketch = st.clone();
        let Some((shape, png, prefs)) = self.ai_photo_start() else {
            return;
        };
        let http = build_edit(&prefs, &self.ai.key, &sketch_prompt(&st.words), &png, false);
        self.ai_image_begin(1);
        self.status = tr("aiphoto.sketching");
        self.ai_image_send(http, Job::Sketch { source: shape });
        self.poll_ai_image();
    }

    /// Remove Object: what the words name (or what `mask` marks, white
    /// where painted, at the bitmap's size) taken out of `shape` with its
    /// shadow, reflection and other traces, the background filled in; the
    /// bitmap changes in place, as one undo step.
    pub fn ai_remove_object(&mut self, shape: ShapeId, words: &str, mask: Option<&GrayImage>) {
        if self.ai_image.busy() {
            return;
        }
        if let Some(p) = self.ai_image_problem() {
            self.status = p.clone();
            self.ai_image.status = ImageStatus::Failed(p);
            return;
        }
        if words.trim().is_empty() && mask.is_none() {
            self.status = tr("aiphoto.remove_need_words");
            return;
        }
        let png = match self.doc().find_shape(shape).map(|s| &s.kind) {
            Some(ShapeKind::Bitmap { png, .. }) => png.to_vec(),
            _ => {
                self.status = tr("aiimg.need_bitmap");
                return;
            }
        };
        let Some(prefs) = self.ai_image_prefs() else {
            return;
        };
        let prompt = remove_prompt(words, mask.is_some());
        let http = match mask.zip(crate::bitmap_fx::decode(&png)) {
            Some((m, img)) if m.dimensions() == img.dimensions() => match prefs.provider {
                ImageProvider::OpenAi => {
                    let mask_png = crate::bitmap_fx::encode(&openai_mask(m)).unwrap_or_default();
                    build_masked_edit(&prefs, &self.ai.key, &prompt, &png, &mask_png)
                }
                ImageProvider::Gemini => {
                    let marked = crate::bitmap_fx::encode(&mark_red(&img, m)).unwrap_or(png);
                    build_edit(&prefs, &self.ai.key, &prompt, &marked, false)
                }
            },
            _ => build_edit(
                &prefs,
                &self.ai.key,
                &remove_prompt(words, false),
                &png,
                false,
            ),
        };
        self.ai_image_begin(1);
        self.status = tr("aiphoto.removing");
        self.ai_image_send(http, Job::RemoveObject { shape });
        self.poll_ai_image();
    }

    /// Image > Remove Object with AI...: on the selected bitmap.
    pub fn ai_remove_object_selected(&mut self, st: &RemoveState) {
        match self.ai_source_bitmap() {
            Some((shape, _)) => self.ai_remove_object(shape, &st.words, None),
            None => self.status = tr("aiimg.need_bitmap"),
        }
    }

    /// A turned view came: over its source, the same height.
    pub(crate) fn ai_receive_turn(&mut self, source: ShapeId, img: GenImage, transparent: bool) {
        let Some(mut out) = crate::bitmap_fx::decode(&img.png) else {
            self.ai_image.errors.push(tr("aiimg.unreadable_image"));
            return;
        };
        if transparent && !has_transparency(&out) {
            chroma_key(&mut out);
        }
        if self.ai_photo_place(source, &out, tr("aiphoto.turned_name"), LABEL_TURN) {
            self.status = tr("aiphoto.turned");
        }
    }

    /// A depth map came: kept, and over the bitmap when asked.
    pub(crate) fn ai_receive_depth(&mut self, shape: ShapeId, img: GenImage, place: bool) {
        let Some(size) = self.doc().find_shape(shape).and_then(|s| match &s.kind {
            ShapeKind::Bitmap {
                width_px,
                height_px,
                ..
            } => Some((*width_px, *height_px)),
            _ => None,
        }) else {
            self.ai_image.errors.push(tr("aiimg.source_gone"));
            return;
        };
        let Some(pic) = crate::bitmap_fx::decode(&img.png) else {
            self.ai_image.errors.push(tr("aiimg.unreadable_image"));
            return;
        };
        let depth = depth_from_picture(&pic, size.0, size.1);
        if place {
            let rgba = image::DynamicImage::ImageLuma8(depth.clone()).to_rgba8();
            self.ai_photo_place(shape, &rgba, tr("aiphoto.depth_name"), LABEL_DEPTH);
        } else {
            self.ai_image.made += 1;
        }
        self.ai_image.depth = Some((shape, depth));
        self.status = tr("aiphoto.depth_ready");
    }

    /// The object taken out: the bitmap's picture replaced in place.
    pub(crate) fn ai_receive_removed(&mut self, shape: ShapeId, img: GenImage) {
        let Some(src) = self.doc().find_shape(shape).cloned() else {
            self.ai_image.errors.push(tr("aiimg.source_gone"));
            return;
        };
        let ShapeKind::Bitmap {
            rect,
            width_px,
            height_px,
            ..
        } = &src.kind
        else {
            self.ai_image.errors.push(tr("aiimg.source_gone"));
            return;
        };
        let Some(pic) = crate::bitmap_fx::decode(&img.png) else {
            self.ai_image.errors.push(tr("aiimg.unreadable_image"));
            return;
        };
        // Back at the bitmap's own size, so it stays in place.
        let pic = if pic.dimensions() == (*width_px, *height_px) {
            pic
        } else {
            image::imageops::resize(
                &pic,
                (*width_px).max(1),
                (*height_px).max(1),
                image::imageops::FilterType::Lanczos3,
            )
        };
        let Some(png) = crate::bitmap_fx::encode(&pic) else {
            return;
        };
        let cmd = Command::SetShapeKind {
            shape,
            kind: ShapeKind::Bitmap {
                rect: *rect,
                width_px: pic.width(),
                height_px: pic.height(),
                png: png.into(),
                fx: None,
            },
        };
        match self.engine.run_batch(LABEL_REMOVE, &[cmd]) {
            Ok(_) => {
                self.ai_image.made += 1;
                self.status = tr("aiphoto.removed");
            }
            Err(e) => self.ai_image.errors.push(e.to_string()),
        }
    }

    /// Put `img` over the bitmap `source` (same place and height, its own
    /// proportions), named, selected, as one undo step.
    fn ai_photo_place(
        &mut self,
        source: ShapeId,
        img: &RgbaImage,
        name: String,
        label: &'static str,
    ) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        let Some(src) = self.doc().find_shape(source).cloned() else {
            self.ai_image.errors.push(tr("aiimg.source_gone"));
            return false;
        };
        let ShapeKind::Bitmap { rect, .. } = &src.kind else {
            self.ai_image.errors.push(tr("aiimg.source_gone"));
            return false;
        };
        let Some(png) = crate::bitmap_fx::encode(img) else {
            self.ai_image.errors.push(tr("aiimg.unreadable_image"));
            return false;
        };
        let w = rect.height() * img.width() as f64 / img.height().max(1) as f64;
        let rect = Rect::new(rect.x0, rect.y0, rect.x0 + w, rect.y1);
        let id = self.engine.new_shape_id();
        let mut shape = Shape::new(
            id,
            ShapeKind::Bitmap {
                rect,
                width_px: img.width(),
                height_px: img.height(),
                png: png.into(),
                fx: None,
            },
        );
        shape.transform = src.transform;
        shape.fill = tracedraw_core::Fill::None;
        shape.stroke = None;
        shape.name = Some(name);
        if let Err(e) = self
            .engine
            .run_batch(label, &[Command::AddShape { layer, shape }])
        {
            self.ai_image.errors.push(e.to_string());
            return false;
        }
        self.ai_image.made += 1;
        self.select(vec![id]);
        true
    }
}

/// The note under each dialog: what is sent, and where.
fn sent_note(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(tr("aiphoto.sent_note"))
            .small()
            .color(crate::theme::Tokens::text_dim()),
    );
}

/// Image > Turn Object in 3D with AI...
pub fn turn_dialog(app: &mut App, ctx: &egui::Context, st: &mut TurnState, close: &mut bool) {
    use crate::ui::tips::RichTip as _;
    let mut ok = false;
    crate::ui::dialogs::window(ctx, tr("aiphoto.turn_title")).show(ctx, |ui| {
        ui.set_min_width(360.0);
        ui.label(
            egui::RichText::new(tr("aiphoto.turn_hint")).color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(6.0);
        egui::Grid::new("ai_turn_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr("aiphoto.turn_turn"));
                ui.add(egui::Slider::new(&mut st.turn, -MAX_TURN..=MAX_TURN).suffix("°"))
                    .rich_tip(crate::ui::tips::keyed("aiphoto.turn_turn"));
                ui.end_row();
                ui.label(tr("aiphoto.turn_tilt"));
                ui.add(egui::Slider::new(&mut st.tilt, -MAX_TILT..=MAX_TILT).suffix("°"))
                    .rich_tip(crate::ui::tips::keyed("aiphoto.turn_tilt"));
                ui.end_row();
                ui.label(tr("aiphoto.words"));
                ui.add(
                    egui::TextEdit::singleline(&mut st.words)
                        .hint_text(tr("aiphoto.turn_words_hint"))
                        .desired_width(220.0),
                )
                .rich_tip(crate::ui::tips::keyed("aiphoto.words"));
                ui.end_row();
            });
        ui.checkbox(&mut st.keep_background, tr("aiphoto.turn_keep_background"))
            .rich_tip(crate::ui::tips::keyed("aiphoto.turn_keep_background"));
        ui.add_space(4.0);
        sent_note(ui);
        ui.add_space(6.0);
        ok = crate::ui::dialogs::ok_cancel(ui, close);
    });
    if ok {
        app.ai_turn_object(st);
    }
}

/// Image > Sketch to Curves with AI...
pub fn sketch_dialog(app: &mut App, ctx: &egui::Context, st: &mut SketchState, close: &mut bool) {
    use crate::ui::tips::RichTip as _;
    let mut ok = false;
    crate::ui::dialogs::window(ctx, tr("aiphoto.sketch_title")).show(ctx, |ui| {
        ui.set_min_width(360.0);
        ui.label(
            egui::RichText::new(tr("aiphoto.sketch_hint")).color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::multiline(&mut st.words)
                .hint_text(tr("aiphoto.sketch_words_hint"))
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        )
        .rich_tip(crate::ui::tips::pair(
            "aiphoto.sketch_title",
            "aiphoto.sketch_words_desc",
        ));
        ui.add_space(4.0);
        sent_note(ui);
        ui.add_space(6.0);
        ok = crate::ui::dialogs::ok_cancel(ui, close);
    });
    if ok {
        app.ai_sketch_to_curves(st);
    }
}

/// Image > Remove Object with AI...
pub fn remove_dialog(app: &mut App, ctx: &egui::Context, st: &mut RemoveState, close: &mut bool) {
    use crate::ui::tips::RichTip as _;
    let mut ok = false;
    crate::ui::dialogs::window(ctx, tr("aiphoto.remove_title")).show(ctx, |ui| {
        ui.set_min_width(360.0);
        ui.label(
            egui::RichText::new(tr("aiphoto.remove_hint")).color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::singleline(&mut st.words)
                .hint_text(tr("aiphoto.remove_words_hint"))
                .desired_width(f32::INFINITY),
        )
        .rich_tip(crate::ui::tips::pair(
            "aiphoto.remove_title",
            "aiphoto.remove_words_desc",
        ));
        ui.add_space(4.0);
        sent_note(ui);
        ui.add_space(6.0);
        ok = crate::ui::dialogs::ok_cancel(ui, close);
    });
    if ok {
        if st.words.trim().is_empty() {
            *close = false;
            app.status = tr("aiphoto.remove_need_words");
        } else {
            app.ai_remove_object_selected(st);
        }
    }
}

/// The AI photo items of the Image menu and a bitmap's context menu,
/// drawn by `item(ui, key, enabled, tip)`; nothing while AI is off.
pub fn menu_items(
    app: &mut App,
    ui: &mut egui::Ui,
    mut item: impl FnMut(&mut egui::Ui, &str, bool, Option<&str>) -> bool,
) {
    if !app.ai_photo_shown() {
        return;
    }
    let on = app.ai_photo_enabled();
    let tip = app.ai_image_tip();
    if item(ui, "menu.bitmaps.ai_turn", on, tip.as_deref()) {
        app.open_ai_turn();
    }
    if item(ui, "menu.bitmaps.ai_remove_object", on, tip.as_deref()) {
        app.open_ai_remove();
    }
    if item(ui, "menu.bitmaps.ai_depth_map", on, tip.as_deref()) {
        app.ai_depth_map_selected();
    }
    if item(ui, "menu.bitmaps.ai_sketch", on, tip.as_deref()) {
        app.open_ai_sketch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{HttpResponse, HttpResult};
    use base64::Engine as _;
    use serde_json::json;

    fn png(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, y| Rgba(f(x, y)));
        crate::bitmap_fx::encode(&img).expect("png")
    }

    fn gemini_reply(png: &[u8]) -> HttpResult {
        Ok(HttpResponse {
            retry_after: None,
            status: 200,
            body: json!({"candidates": [{"content": {"parts": [
                {"inlineData": {"mimeType": "image/png",
                    "data": base64::engine::general_purpose::STANDARD.encode(png)}}
            ]}}]})
            .to_string(),
        })
    }

    fn ready_app(preset: &str) -> App {
        let mut app = App::headless();
        app.settings.ai_enabled = true;
        let (_, kind, endpoint) = crate::ai::PRESETS
            .iter()
            .find(|(k, _, _)| *k == preset)
            .copied()
            .expect("preset");
        app.settings.ai.provider = kind;
        app.settings.ai.endpoint = endpoint.to_string();
        app.ai.key = "k".into();
        app
    }

    /// A selected 8 x 8 white bitmap at 10,10 - 50,50.
    fn with_bitmap(app: &mut App) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Bitmap {
                rect: Rect::new(10.0, 10.0, 50.0, 50.0),
                width_px: 8,
                height_px: 8,
                png: png(8, 8, |_, _| [255, 255, 255, 255]).into(),
                fx: None,
            })
            .expect("bitmap");
        app.select(vec![id]);
        id
    }

    fn bitmaps(app: &App) -> Vec<Shape> {
        app.doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .filter(|s| matches!(s.kind, ShapeKind::Bitmap { .. }))
            .cloned()
            .collect()
    }

    fn pixels(s: &Shape) -> RgbaImage {
        let ShapeKind::Bitmap { png, .. } = &s.kind else {
            panic!("a bitmap")
        };
        crate::bitmap_fx::decode(png).expect("png")
    }

    fn sent_text(app: &App) -> String {
        let r = app.ai_image.sent.last().expect("sent");
        format!("{}{}", r.body, String::from_utf8_lossy(&r.bytes))
    }

    #[test]
    fn the_commands_show_only_while_ai_is_on() {
        let mut app = ready_app("aipreset.gemini");
        with_bitmap(&mut app);
        assert!(app.ai_photo_shown() && app.ai_photo_enabled());
        // A service that makes drawings only: shown, greyed, with a tip.
        app.settings.ai = crate::ai::AiPrefs::default();
        assert!(app.ai_photo_shown() && !app.ai_photo_enabled());
        assert!(app.ai_image_tip().is_some());
        // AI off: not shown at all, and nothing can be sent.
        app.settings.ai_enabled = false;
        assert!(!app.ai_photo_shown());
        app.ai_image.fake = Some(|_| panic!("nothing may be sent"));
        app.ai_turn_object(&TurnState::default());
        app.ai_depth_map_selected();
        app.ai_sketch_to_curves(&SketchState::default());
        app.ai_remove_object_selected(&RemoveState {
            words: "the car".into(),
        });
        assert!(app.ai_image.sent.is_empty());
        // No bitmap selected: greyed.
        let mut app = ready_app("aipreset.gemini");
        assert!(!app.ai_photo_enabled());
        app.select(vec![]);
        assert!(!app.ai_photo_enabled());
    }

    #[test]
    fn a_turn_asks_for_the_new_view() {
        let st = TurnState {
            turn: -30.0,
            tilt: 20.0,
            keep_background: false,
            words: "keep the label".into(),
        };
        let p = turn_prompt(&st, true);
        assert!(p.contains("30 degrees to its left"), "{p}");
        assert!(p.contains("20 degrees so that more of its top"), "{p}");
        assert!(
            p.contains("#00FF00") && p.contains("Also: keep the label"),
            "{p}"
        );
        let p = turn_prompt(
            &TurnState {
                keep_background: true,
                ..st.clone()
            },
            false,
        );
        assert!(
            p.contains("Keep the scene") && !p.contains("#00FF00"),
            "{p}"
        );
        assert!(turn_prompt(
            &TurnState {
                keep_background: false,
                ..st
            },
            false
        )
        .contains("transparent background"));
        // A turn of nothing sends nothing.
        let mut app = ready_app("aipreset.gemini");
        with_bitmap(&mut app);
        app.ai_image.fake = Some(|_| panic!("nothing may be sent"));
        app.ai_turn_object(&TurnState {
            turn: 0.0,
            tilt: 0.4,
            ..Default::default()
        });
        assert!(app.ai_image.sent.is_empty());
        assert_eq!(app.status, tr("aiphoto.turn_none"));
    }

    #[test]
    fn the_turned_view_goes_over_the_picture() {
        let mut app = ready_app("aipreset.gemini");
        let id = with_bitmap(&mut app);
        app.ai_image.fake = Some(|_| {
            gemini_reply(&png(16, 8, |x, _| {
                if x < 8 {
                    [0, 255, 0, 255]
                } else {
                    [200, 30, 30, 255]
                }
            }))
        });
        app.ai_turn_object(&TurnState::default());
        assert_eq!(app.ai_image.status, ImageStatus::Done(1));
        assert!(sent_text(&app).contains("45 degrees to its right"));
        let all = bitmaps(&app);
        assert_eq!(all.len(), 2, "the picture is kept");
        let turned = app.selected_shapes().pop().expect("the new view");
        assert_ne!(turned.id, id);
        assert_eq!(
            turned.name.as_deref(),
            Some(tr("aiphoto.turned_name").as_str())
        );
        let ShapeKind::Bitmap { rect, .. } = turned.kind else {
            panic!()
        };
        assert!((rect.height() - 40.0).abs() < 1e-9 && (rect.width() - 80.0).abs() < 1e-9);
        let img = pixels(&turned);
        assert_eq!(img.get_pixel(0, 0).0[3], 0, "the green keyed out");
        assert_eq!(img.get_pixel(15, 0).0[3], 255);
        // One undo step takes it away.
        app.undo();
        assert_eq!(bitmaps(&app).len(), 1);
        assert_eq!(app.ai_image.turn, TurnState::default(), "settings kept");
    }

    #[test]
    fn the_depth_map_is_kept_for_the_photo_editor() {
        let mut app = ready_app("aipreset.gemini");
        let id = with_bitmap(&mut app);
        // The service answers at another size.
        app.ai_image.fake = Some(|_| {
            gemini_reply(&png(32, 32, |x, _| {
                [x as u8 * 8, x as u8 * 8, x as u8 * 8, 255]
            }))
        });
        app.ai_depth_map_selected();
        assert_eq!(app.ai_image.status, ImageStatus::Done(1));
        assert!(sent_text(&app).contains("depth map"));
        let placed = app.selected_shapes().pop().expect("the depth map");
        assert_eq!(
            placed.name.as_deref(),
            Some(tr("aiphoto.depth_name").as_str())
        );
        assert_eq!(pixels(&placed).dimensions(), (8, 8));
        let other = app.engine.new_shape_id();
        assert_eq!(app.ai_take_depth(other), None, "only for its bitmap");
        let d = app.ai_take_depth(id).expect("kept");
        assert_eq!(d.dimensions(), (8, 8));
        assert!(d.get_pixel(7, 0).0[0] > d.get_pixel(0, 0).0[0]);
        assert!(app.ai_take_depth(id).is_none(), "given once");
        // For the photo editor alone: kept, not placed.
        app.ai_depth_map(id, false);
        assert_eq!(bitmaps(&app).len(), 2);
        assert!(app.ai_take_depth(id).is_some());
    }

    #[test]
    fn remove_object_changes_the_picture_in_place() {
        let mut app = ready_app("aipreset.gemini");
        let id = with_bitmap(&mut app);
        // No words: nothing sent.
        app.ai_image.fake = Some(|_| panic!("nothing may be sent"));
        app.ai_remove_object_selected(&RemoveState::default());
        assert!(app.ai_image.sent.is_empty());
        app.ai_image.fake = Some(|_| gemini_reply(&png(16, 16, |_, _| [10, 120, 10, 255])));
        app.ai_remove_object_selected(&RemoveState {
            words: "the red car".into(),
        });
        assert_eq!(app.ai_image.status, ImageStatus::Done(1));
        let t = sent_text(&app);
        assert!(
            t.contains("the red car") && t.contains("shadow, reflection"),
            "{t}"
        );
        let all = bitmaps(&app);
        assert_eq!(all.len(), 1, "in place");
        let ShapeKind::Bitmap { rect, width_px, .. } = &all[0].kind else {
            panic!()
        };
        assert_eq!((*rect, *width_px), (Rect::new(10.0, 10.0, 50.0, 50.0), 8));
        assert_eq!(pixels(&all[0]).get_pixel(3, 3).0, [10, 120, 10, 255]);
        app.undo();
        assert_eq!(
            pixels(&bitmaps(&app)[0]).get_pixel(3, 3).0,
            [255, 255, 255, 255]
        );
        // Painted marks: tinted red for Gemini.
        let mask = GrayImage::from_fn(8, 8, |x, _| Luma([if x < 4 { 255 } else { 0 }]));
        app.ai_remove_object(id, "", Some(&mask));
        let t = sent_text(&app);
        assert!(t.contains("painted over in red"), "{t}");
        let marked = mark_red(
            &RgbaImage::from_pixel(8, 8, Rgba([255, 255, 255, 255])),
            &mask,
        );
        assert!(
            marked.get_pixel(0, 0).0[1] < 100 && marked.get_pixel(7, 0).0 == [255, 255, 255, 255]
        );
        // And as the mask of the OpenAI Images API.
        let mut app = ready_app("aipreset.openai");
        let id = with_bitmap(&mut app);
        app.ai_image.fake = Some(|_| {
            Ok(HttpResponse {
                retry_after: None,
                status: 200,
                body: json!({"data": [{"b64_json": base64::engine::general_purpose::STANDARD
                    .encode(png(8, 8, |_, _| [1, 2, 3, 255]))}]})
                .to_string(),
            })
        });
        app.ai_remove_object(id, "the bottle", Some(&mask));
        assert_eq!(app.ai_image.status, ImageStatus::Done(1));
        let r = app.ai_image.sent.last().expect("sent");
        assert!(r.url.ends_with("/images/edits"));
        let body = String::from_utf8_lossy(&r.bytes);
        assert!(
            body.contains("name=\"mask\"") && body.trim_end().ends_with("--"),
            "{body}"
        );
        assert_eq!(
            openai_mask(&mask).get_pixel(0, 0).0[3],
            0,
            "transparent where to change"
        );
    }

    #[test]
    fn a_sketch_becomes_curves() {
        let mut app = ready_app("aipreset.gemini");
        with_bitmap(&mut app);
        app.ai_image.fake = Some(|_| {
            gemini_reply(&png(64, 64, |x, y| {
                if (x as i32 - 32).pow(2) + (y as i32 - 32).pow(2) < 400 {
                    [0, 0, 0, 255]
                } else {
                    [255, 255, 255, 255]
                }
            }))
        });
        app.ai_sketch_to_curves(&SketchState {
            words: "fill it with black".into(),
        });
        assert_eq!(app.ai_image.status, ImageStatus::Done(1));
        let t = sent_text(&app);
        assert!(t.contains("hand-drawn sketch") && t.contains("Request: fill it with black"));
        assert_eq!(
            bitmaps(&app).len(),
            1,
            "the sketch stays, the redraw is traced"
        );
        let groups = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .filter(|s| matches!(s.kind, ShapeKind::Group { .. }))
            .count();
        assert_eq!(groups, 1);
        assert!(sketch_prompt("").contains(SKETCH_DEFAULT));
    }
}
