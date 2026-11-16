//! Bitmaps > Picture Captions (#2180): each picture's name written beside
//! it, below, above, left or right of it, a set gap away, in one step.
//! The name is the one the picture came in with (its file's name, kept as
//! the object's name and shown in the Objects docker), or the name given
//! to it there; a prefix and a suffix go around every name, and one font
//! and size are used for all. A recipe or catalogue page of many
//! pictures gets its labels at once.
//!
//! The pictures are the selected ones (a picture, a picture in a frame,
//! or any object with a name), else every picture on the page. Each
//! caption remembers its picture, so running the command again changes
//! the captions already there (another side, gap, prefix or font)
//! instead of adding more. With Group on, each picture and its caption
//! become one group, which moves as one.

use crate::app::App;
use serde::{Deserialize, Serialize};
use tracedraw_core::{
    Affine, Color, Command, Fill, Point, Rect, Shape, ShapeId, ShapeKind, TextAlign, TextSpan,
};

/// Which side of its picture a caption goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Side {
    #[default]
    Below,
    Above,
    Left,
    Right,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Below, Side::Above, Side::Left, Side::Right];

    /// The language key of its name.
    pub fn key(self) -> &'static str {
        match self {
            Side::Below => "dialog.captions_below",
            Side::Above => "dialog.captions_above",
            Side::Left => "dialog.captions_left",
            Side::Right => "dialog.captions_right",
        }
    }

    /// How a caption's lines line up: towards its picture.
    fn align(self) -> TextAlign {
        match self {
            Side::Below | Side::Above => TextAlign::Center,
            Side::Left => TextAlign::Right,
            Side::Right => TextAlign::Left,
        }
    }
}

/// The settings of Picture Captions, kept for the next time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Captions {
    pub side: Side,
    /// Between the picture's edge and the caption's (mm).
    pub gap_mm: f64,
    pub prefix: String,
    pub suffix: String,
    /// Empty: the text tool's font.
    pub font: String,
    pub size_pt: f64,
    /// Each picture and its caption as one group.
    pub group: bool,
}

impl Default for Captions {
    fn default() -> Self {
        Captions {
            side: Side::Below,
            gap_mm: 2.0,
            prefix: String::new(),
            suffix: String::new(),
            font: String::new(),
            size_pt: 10.0,
            group: true,
        }
    }
}

/// The data key a caption keeps its picture's id under.
pub const CAPTION_OF: &str = "caption.of";

/// The text of a caption.
pub fn caption_text(c: &Captions, name: &str) -> String {
    format!("{}{}{}", c.prefix, name, c.suffix)
}

/// How far a caption whose box is `text` moves to sit `gap` beside the
/// picture box `picture` on `side` (Y up), centred along that side.
pub fn offset(side: Side, gap: f64, picture: Rect, text: Rect) -> tracedraw_core::Vec2 {
    let (pc, tc) = (picture.center(), text.center());
    let (dx, dy) = match side {
        Side::Below => (pc.x - tc.x, picture.y0 - gap - text.y1),
        Side::Above => (pc.x - tc.x, picture.y1 + gap - text.y0),
        Side::Left => (picture.x0 - gap - text.x1, pc.y - tc.y),
        Side::Right => (picture.x1 + gap - text.x0, pc.y - tc.y),
    };
    tracedraw_core::Vec2::new(dx, dy)
}

/// The file name (without its folder and extension) a linked picture
/// points at.
fn link_stem(s: &Shape) -> Option<String> {
    let link = s.data.iter().find(|(k, _)| k == "bitmap.link")?.1.as_str();
    std::path::Path::new(link)
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
}

/// The name of the picture `s` stands for: its own, else its linked
/// file's; a frame's picture's when the frame has none.
pub fn picture_name(s: &Shape) -> Option<String> {
    let own = s
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    own.or_else(|| link_stem(s)).or_else(|| match &s.kind {
        ShapeKind::ClipFrame { contents, .. } => contents.iter().find_map(picture_name),
        _ => None,
    })
}

/// Whether `s` is a picture: a bitmap, or a frame holding one alone.
pub fn is_picture(s: &Shape) -> bool {
    match &s.kind {
        ShapeKind::Bitmap { .. } => true,
        ShapeKind::ClipFrame { contents, .. } => {
            matches!(contents.as_slice(), [c] if matches!(c.kind, ShapeKind::Bitmap { .. }))
        }
        _ => false,
    }
}

/// The picture whose caption `s` is.
fn caption_of(s: &Shape) -> Option<u64> {
    s.data
        .iter()
        .find(|(k, _)| k == CAPTION_OF)
        .and_then(|(_, v)| v.parse().ok())
}

/// One picture to caption: the picture, and the group it already shares
/// with its caption (with that caption) when there is one.
struct Target {
    picture: Shape,
    grouped: Option<(ShapeId, ShapeId)>,
}

/// A group of a picture and its caption: the picture (in page space) and
/// the caption's id.
fn captioned_group(g: &Shape) -> Option<(Shape, ShapeId)> {
    let ShapeKind::Group { children } = &g.kind else {
        return None;
    };
    let [a, b] = children.as_slice() else {
        return None;
    };
    let (pic, cap) = if caption_of(b) == Some(a.id.raw()) {
        (a, b)
    } else if caption_of(a) == Some(b.id.raw()) {
        (b, a)
    } else {
        return None;
    };
    let mut pic = pic.clone();
    pic.transform = g.transform * pic.transform;
    Some((pic, cap.id))
}

impl App {
    /// The pictures Picture Captions would label: the selected pictures
    /// and named objects, else every picture on the page's unlocked,
    /// shown layers.
    fn caption_targets(&self) -> Vec<Target> {
        let from = |s: &Shape, any_named: bool| -> Option<Target> {
            if let Some((picture, cap)) = captioned_group(s) {
                return Some(Target {
                    picture,
                    grouped: Some((s.id, cap)),
                });
            }
            if caption_of(s).is_some() {
                return None;
            }
            (is_picture(s) || (any_named && picture_name(s).is_some())).then(|| Target {
                picture: s.clone(),
                grouped: None,
            })
        };
        let selected = self.selected_shapes();
        if !selected.is_empty() {
            return selected.iter().filter_map(|s| from(s, true)).collect();
        }
        let Some(page) = self.doc().pages.get(self.page_index()) else {
            return Vec::new();
        };
        page.layers
            .iter()
            .filter(|l| l.visible && !l.locked)
            .flat_map(|l| l.shapes.iter())
            .filter_map(|s| from(s, false))
            .collect()
    }

    /// Whether Picture Captions has pictures to label.
    pub fn can_caption_pictures(&self) -> bool {
        !self.caption_targets().is_empty()
    }

    /// The caption of `picture` (page space), placed.
    fn caption_shape(&mut self, c: &Captions, picture: &Shape, name: &str) -> Shape {
        let font = if c.font.is_empty() {
            self.text_font.clone()
        } else {
            c.font.clone()
        };
        let id = self.engine.new_shape_id();
        let mut s = Shape::new(
            id,
            ShapeKind::Text {
                spans: vec![TextSpan::new(
                    caption_text(c, name),
                    font,
                    c.size_pt.max(1.0),
                )],
                origin: Point::ZERO,
                frame: None,
                align: c.side.align(),
                para: Default::default(),
                on_path: None,
            },
        );
        s.fill = Fill::Solid(Color::BLACK);
        s.stroke = None;
        s.name = Some(format!("{name} caption"));
        s.data
            .push((CAPTION_OF.into(), picture.id.raw().to_string()));
        let moved = offset(
            c.side,
            c.gap_mm.max(0.0),
            picture.visual_bounds(),
            s.bounds(),
        );
        s.transform = Affine::translate(moved);
        s
    }

    /// Bitmaps > Picture Captions: each picture's name beside it, as one
    /// undo step; captions the pictures already have are replaced. The
    /// captions (or the groups made) end up selected.
    pub fn caption_pictures(&mut self, c: &Captions) {
        self.finish_text();
        let targets = self.caption_targets();
        // Captions already there for these pictures, outside groups.
        let ids: std::collections::HashSet<u64> =
            targets.iter().map(|t| t.picture.id.raw()).collect();
        let mut old: Vec<ShapeId> = Vec::new();
        if let Some(page) = self.doc().pages.get(self.page_index()) {
            for l in &page.layers {
                for s in &l.shapes {
                    if caption_of(s).is_some_and(|p| ids.contains(&p)) {
                        old.push(s.id);
                    }
                }
            }
        }
        let mut cmds = Vec::new();
        if !old.is_empty() {
            cmds.push(Command::DeleteShapes { shapes: old });
        }
        let mut made = Vec::new();
        let mut unnamed = 0usize;
        for t in &targets {
            let Some(name) = picture_name(&t.picture) else {
                unnamed += 1;
                continue;
            };
            let at = match t.grouped {
                Some((g, _)) => g,
                None => t.picture.id,
            };
            let Some(layer) = self.doc().locate(at).ok().map(|(l, _)| l) else {
                continue;
            };
            let caption = self.caption_shape(c, &t.picture, &name);
            let cap_id = caption.id;
            if let Some((g, old_cap)) = t.grouped {
                cmds.push(Command::Ungroup { group: g });
                cmds.push(Command::DeleteShapes {
                    shapes: vec![old_cap],
                });
            }
            cmds.push(Command::AddShape {
                layer,
                shape: caption,
            });
            if c.group {
                cmds.push(Command::Group {
                    shapes: vec![t.picture.id, cap_id],
                });
            } else {
                cmds.push(Command::PlaceBeside {
                    shapes: vec![cap_id],
                    reference: t.picture.id,
                    in_front: true,
                });
            }
            made.push((t.picture.id, cap_id));
        }
        if made.is_empty() {
            self.status = crate::i18n::tr(if unnamed > 0 {
                "status.captions_unnamed"
            } else {
                "status.captions_none"
            });
            return;
        }
        if let Err(e) = self.engine.run_batch("Picture Captions", &cmds) {
            self.status = e.to_string();
            return;
        }
        let doc = self.doc();
        let mut sel: Vec<ShapeId> = made
            .iter()
            .filter_map(|(pic, cap)| {
                if c.group {
                    doc.top_level_of(*pic)
                } else {
                    Some(*cap)
                }
            })
            .collect();
        sel.dedup();
        self.select(sel);
        self.status = if unnamed > 0 {
            crate::i18n::trf(
                "status.captions_some_unnamed",
                &[("n", &unnamed.to_string())],
            )
        } else {
            String::new()
        };
    }
}

/// The Picture Captions dialog: its settings, kept in the options; OK
/// writes the captions.
pub fn dialog(app: &mut App, ctx: &egui::Context, st: &mut Captions, close: &mut bool) {
    use crate::i18n::tr;
    use crate::ui::tips::RichTip as _;
    let mut ok = false;
    crate::ui::dialogs::window(ctx, tr("dialog.captions")).show(ctx, |ui| {
        ui.set_min_width(360.0);
        ui.label(
            egui::RichText::new(tr("dialog.captions_hint")).color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(6.0);
        egui::Grid::new("captions_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr("dialog.captions_side"));
                ui.horizontal(|ui| {
                    for side in Side::ALL {
                        ui.selectable_value(&mut st.side, side, tr(side.key()))
                            .rich_tip(crate::ui::tips::keyed(side.key()));
                    }
                });
                ui.end_row();
                ui.label(tr("dialog.captions_gap"));
                ui.add(
                    crate::ui::field::NumField::new(&mut st.gap_mm)
                        .range(0.0..=1000.0)
                        .suffix(" mm"),
                )
                .rich_tip(crate::ui::tips::keyed("dialog.captions_gap"));
                ui.end_row();
                ui.label(tr("dialog.captions_prefix"));
                ui.add(egui::TextEdit::singleline(&mut st.prefix).desired_width(200.0))
                    .rich_tip(crate::ui::tips::keyed("dialog.captions_prefix"));
                ui.end_row();
                ui.label(tr("dialog.captions_suffix"));
                ui.add(egui::TextEdit::singleline(&mut st.suffix).desired_width(200.0))
                    .rich_tip(crate::ui::tips::keyed("dialog.captions_suffix"));
                ui.end_row();
                ui.label(tr("dialog.captions_font"));
                let current = if st.font.is_empty() {
                    app.text_font.clone()
                } else {
                    st.font.clone()
                };
                let (pick, _) = crate::ui::tips::tip_over(
                    ui,
                    crate::ui::tips::keyed("dialog.captions_font"),
                    |ui| {
                        crate::ui::font_list::font_list_live(
                            app,
                            ui,
                            "captions_font",
                            &current,
                            200.0,
                        )
                    },
                );
                if let Some(pick) = pick {
                    st.font = pick.family;
                }
                ui.end_row();
                ui.label(tr("dialog.captions_size"));
                ui.add(
                    crate::ui::field::NumField::new(&mut st.size_pt)
                        .range(1.0..=1000.0)
                        .suffix(" pt"),
                )
                .rich_tip(crate::ui::tips::keyed("dialog.captions_size"));
                ui.end_row();
            });
        ui.checkbox(&mut st.group, tr("dialog.captions_group"))
            .rich_tip(crate::ui::tips::keyed("dialog.captions_group"));
        ui.add_space(4.0);
        // What OK labels: the selection, else the page's pictures.
        let what = if app.selection.is_empty() {
            "dialog.captions_on_page"
        } else {
            "dialog.captions_on_selection"
        };
        ui.label(egui::RichText::new(tr(what)).color(crate::theme::Tokens::text_dim()));
        ui.add_space(6.0);
        ok = crate::ui::dialogs::ok_cancel(ui, close);
    });
    if ok {
        app.settings.picture_captions = st.clone();
        app.caption_pictures(st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(app: &mut App, x: f64, name: Option<&str>) -> ShapeId {
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 200, 30, 255]));
        let id = app
            .new_shape(ShapeKind::Bitmap {
                rect: Rect::new(x, 100.0, x + 40.0, 130.0),
                width_px: 4,
                height_px: 4,
                png: crate::bitmap_fx::encode(&img).expect("png").into(),
                fx: None,
            })
            .expect("bitmap");
        if let Some(n) = name {
            app.engine
                .run(&Command::SetShapeName {
                    shape: id,
                    name: Some(n.into()),
                })
                .expect("name");
        }
        id
    }

    fn captions(app: &App) -> Vec<Shape> {
        let mut out = Vec::new();
        fn walk(s: &Shape, t: Affine, out: &mut Vec<Shape>) {
            if caption_of(s).is_some() {
                let mut c = s.clone();
                c.transform = t * c.transform;
                out.push(c);
            }
            if let ShapeKind::Group { children } = &s.kind {
                for ch in children {
                    walk(ch, t * s.transform, out);
                }
            }
        }
        for l in &app.doc().pages[0].layers {
            for s in &l.shapes {
                walk(s, Affine::IDENTITY, &mut out);
            }
        }
        out
    }

    fn text_of(s: &Shape) -> String {
        match &s.kind {
            ShapeKind::Text { spans, .. } => tracedraw_core::spans_text(spans),
            _ => String::new(),
        }
    }

    #[test]
    fn offset_puts_the_box_on_each_side() {
        let pic = Rect::new(0.0, 0.0, 40.0, 30.0);
        let text = Rect::new(-5.0, -1.0, 5.0, 3.0);
        let at = |side| text + offset(side, 2.0, pic, text);
        let below = at(Side::Below);
        assert!((below.y1 - -2.0).abs() < 1e-9 && (below.center().x - 20.0).abs() < 1e-9);
        let above = at(Side::Above);
        assert!((above.y0 - 32.0).abs() < 1e-9);
        let left = at(Side::Left);
        assert!((left.x1 - -2.0).abs() < 1e-9 && (left.center().y - 15.0).abs() < 1e-9);
        let right = at(Side::Right);
        assert!((right.x0 - 42.0).abs() < 1e-9);
    }

    #[test]
    fn names_go_under_every_picture_and_change_in_place() {
        tracedraw_text::install();
        let mut app = App::headless();
        let a = picture(&mut app, 10.0, Some("Pasta"));
        let b = picture(&mut app, 80.0, Some("Salad"));
        let _unnamed = picture(&mut app, 150.0, None);
        app.select(Vec::new());
        let mut c = Captions {
            prefix: "No. ".into(),
            suffix: "!".into(),
            group: false,
            ..Default::default()
        };
        app.caption_pictures(&c);
        let caps = captions(&app);
        assert_eq!(caps.len(), 2);
        let mut texts: Vec<String> = caps.iter().map(text_of).collect();
        texts.sort();
        assert_eq!(texts, ["No. Pasta!", "No. Salad!"]);
        for cap in &caps {
            let pic = app
                .doc()
                .find_shape(ShapeId(caption_of(cap).unwrap()))
                .unwrap()
                .bounds();
            let r = cap.bounds();
            assert!((r.y1 - (pic.y0 - 2.0)).abs() < 0.01, "{r:?} under {pic:?}");
            assert!((r.center().x - pic.center().x).abs() < 0.01);
        }
        assert!(app.status.contains('1'), "{}", app.status);
        // Again, to the right: the same two captions, moved.
        c.side = Side::Right;
        c.gap_mm = 5.0;
        app.select(Vec::new());
        app.caption_pictures(&c);
        let caps = captions(&app);
        assert_eq!(caps.len(), 2);
        let cap_a = caps
            .iter()
            .find(|s| caption_of(s) == Some(a.raw()))
            .unwrap();
        let pic_a = app.doc().find_shape(a).unwrap().bounds();
        assert!((cap_a.bounds().x0 - (pic_a.x1 + 5.0)).abs() < 0.01);
        // One undo step back to the captions under the pictures.
        app.undo();
        let caps = captions(&app);
        assert_eq!(caps.len(), 2);
        let pic_b = app.doc().find_shape(b).unwrap().bounds();
        let cap_b = caps
            .iter()
            .find(|s| caption_of(s) == Some(b.raw()))
            .unwrap();
        assert!(cap_b.bounds().y1 < pic_b.y0);
    }

    #[test]
    fn grouped_captions_are_replaced_inside_their_group() {
        tracedraw_text::install();
        let mut app = App::headless();
        let a = picture(&mut app, 10.0, Some("Soup"));
        app.select(vec![a]);
        let mut c = Captions::default();
        app.caption_pictures(&c);
        let sel = app.selected_shapes();
        assert_eq!(sel.len(), 1);
        assert!(matches!(&sel[0].kind, ShapeKind::Group { children } if children.len() == 2));
        // The group selected, captioned again above with a new font size.
        c.side = Side::Above;
        c.size_pt = 20.0;
        c.font = "Arial".into();
        app.caption_pictures(&c);
        let caps = captions(&app);
        assert_eq!(caps.len(), 1);
        assert_eq!(text_of(&caps[0]), "Soup");
        let pic = app.doc().pages[0]
            .layers
            .iter()
            .flat_map(|l| l.shapes.iter())
            .find_map(|s| captioned_group(s).map(|(p, _)| p.bounds()))
            .expect("one group of the picture and its caption");
        assert!(caps[0].bounds().y0 >= pic.y1 + 1.9);
        match &caps[0].kind {
            ShapeKind::Text { spans, .. } => assert_eq!(spans[0].size_pt, 20.0),
            _ => unreachable!(),
        }
    }
}
