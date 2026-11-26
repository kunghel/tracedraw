//! Text layers of the image editing mode (Bitmaps > Edit Inside, #4370):
//! a layer whose pixels are drawn from its text, font, size, colour and
//! spacing, so the text can be changed at any time; it becomes plain
//! pixels when it is painted on, merged down, flattened or finished.
//!
//! The text is laid out and drawn by the drawing's own text engine (every
//! installed font, bold, italic, underline, spacing), as an artistic text
//! object drawn on a clear picture of the layer's size.

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use tracedraw_core::{
    geometry::{Point, Size},
    Color, Document, Fill, Shape, ShapeKind, TextAlign, TextSpan,
};

/// How the lines of a text layer line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

impl Align {
    pub const ALL: [Align; 3] = [Align::Left, Align::Center, Align::Right];

    pub fn key(self) -> &'static str {
        match self {
            Align::Left => "pixel.text.align_left",
            Align::Center => "pixel.text.align_center",
            Align::Right => "pixel.text.align_right",
        }
    }
}

/// A text layer's text and how it looks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PixelText {
    pub text: String,
    pub font: String,
    /// The letters' size (the font's em), picture pixels.
    pub size: f32,
    /// Straight RGBA.
    pub color: [u8; 4],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub align: Align,
    /// Line spacing, percent of the font's line height.
    pub leading: f32,
    /// Space between the letters, percent of the em (0 none).
    pub tracking: f32,
    /// Where the first line's baseline starts (its middle or right end
    /// with those alignments), in the pixels of the picture as it was
    /// first opened: a crop leaves the text where it was.
    pub at: (f32, f32),
}

impl Default for PixelText {
    fn default() -> Self {
        PixelText {
            text: String::new(),
            font: "DejaVu Sans".into(),
            size: 48.0,
            color: [0, 0, 0, 255],
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            align: Align::Left,
            leading: 100.0,
            tracking: 0.0,
            at: (0.0, 0.0),
        }
    }
}

/// Points per picture pixel: the text is drawn one pixel to a millimetre.
const PT_PER_PX: f64 = 72.0 / 25.4;

impl PixelText {
    /// The text drawn on a clear picture `w` by `h`, the picture's top
    /// left pixel lying at `origin` in the picture as first opened.
    pub fn render(&self, w: u32, h: u32, origin: (i64, i64)) -> RgbaImage {
        let blank = || RgbaImage::new(w, h);
        if w == 0 || h == 0 || self.text.trim().is_empty() {
            return blank();
        }
        let mut doc = Document::new("text", Size::new(w as f64, h as f64));
        let mut ids = doc.ids().clone();
        let mut span = TextSpan::new(self.text.clone(), self.font.clone(), self.size as f64 * PT_PER_PX);
        span.bold = self.bold;
        span.italic = self.italic;
        span.tracking_pct = self.tracking as f64;
        if self.underline {
            span.underline = tracedraw_core::LineStyle::SingleThin;
        }
        if self.strike {
            span.strikethrough = tracedraw_core::LineStyle::SingleThin;
        }
        let mut para = tracedraw_core::ParagraphStyle::default();
        para.leading_pct = self.leading.clamp(10.0, 1000.0) as f64;
        let x = self.at.0 as f64 - origin.0 as f64;
        let y = self.at.1 as f64 - origin.1 as f64;
        let mut shape = Shape::new(
            ids.shape(),
            ShapeKind::Text {
                spans: vec![span],
                // The page's y runs up from its bottom edge.
                origin: Point::new(x, h as f64 - y),
                frame: None,
                align: match self.align {
                    Align::Left => TextAlign::Left,
                    Align::Center => TextAlign::Center,
                    Align::Right => TextAlign::Right,
                },
                para,
                on_path: None,
            },
        );
        let [r, g, b, a] = self.color;
        shape.fill = Fill::Solid(Color::rgb8(r, g, b));
        shape.stroke = None;
        shape.opacity = a as f64 / 255.0;
        let page = doc.pages[0].id;
        if let Some(l) = doc.pages[0].layers.first_mut() {
            l.shapes.push(shape);
        }
        doc.set_ids(ids);
        let view = tracedraw_render::ViewTransform {
            zoom: 1.0,
            origin_x: 0.0,
            origin_y: h as f64,
            stretch_y: 1.0,
        };
        let Some(pm) = tracedraw_render::render_page(
            &doc,
            page,
            &tracedraw_render::RenderOptions {
                width: w,
                height: h,
                view,
                ..tracedraw_render::RenderOptions::default()
            },
        ) else {
            return blank();
        };
        crate::pixel_edit::pixmap_to_rgba(&pm, w, h).unwrap_or_else(blank)
    }
}

/// The box of the pixels that show in `img` (x0, y0, x1, y1; the ends
/// left out), `None` when none does.
pub fn shown_box(img: &RgbaImage) -> Option<[u32; 4]> {
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (x, y, q) in img.enumerate_pixels() {
        if q[3] > 8 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1, y1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_drawn_from_its_baseline_in_its_colour() {
        tracedraw_text::install();
        if tracedraw_text::fonts().families().is_empty() {
            return;
        }
        let t = PixelText {
            text: "Hello".into(),
            size: 40.0,
            color: [255, 0, 0, 255],
            at: (20.0, 60.0),
            ..PixelText::default()
        };
        let img = t.render(240, 100, (0, 0));
        let b = shown_box(&img).expect("the text shows");
        // Starts near its x, sits on its baseline, about its size tall.
        assert!(b[0] >= 18 && b[0] <= 26, "{b:?}");
        assert!(b[3] <= 62 && b[3] >= 55, "{b:?}");
        assert!(b[1] >= 20 && b[1] <= 35, "{b:?}");
        let red = img.pixels().filter(|q| q[3] == 255).all(|q| q[0] > 200 && q[1] < 40);
        assert!(red);
        // Moved with the picture's origin: a crop of 10 pixels from the
        // left leaves it 10 pixels further left in the picture.
        let moved = t.render(240, 100, (10, 0));
        let m = shown_box(&moved).expect("still shows");
        assert_eq!(m[0] + 10, b[0]);
    }

    #[test]
    fn bigger_letters_and_bold_take_more_room_and_empty_text_draws_nothing() {
        tracedraw_text::install();
        if tracedraw_text::fonts().families().is_empty() {
            return;
        }
        let t = PixelText {
            text: "Wide".into(),
            size: 30.0,
            at: (5.0, 50.0),
            ..PixelText::default()
        };
        let small = shown_box(&t.render(300, 100, (0, 0))).unwrap();
        let big = shown_box(
            &PixelText {
                size: 60.0,
                at: (5.0, 80.0),
                ..t.clone()
            }
            .render(300, 100, (0, 0)),
        )
        .unwrap();
        assert!(big[2] - big[0] > (small[2] - small[0]) * 3 / 2);
        let empty = PixelText {
            text: "  ".into(),
            ..t
        };
        assert!(shown_box(&empty.render(50, 50, (0, 0))).is_none());
    }
}
