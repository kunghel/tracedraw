//! The Light tool of Bitmaps > Edit Inside: light painted on a photo.
//! Each light has a colour, a strength, a way of mixing with the photo
//! (Lighten, Glow or Tint), a direction and a relief, and a mask that
//! the light brush paints: the light shows where you paint, as you
//! paint. Several lights can be lit at once. Relief reads the photo's
//! brightness as a surface, so the side of a shape turned to the light
//! catches more of it than the side turned away.
//!
//! While the Light tool is in use the lights stay live: their colours,
//! strengths and directions can still change, a stroke can be undone on
//! its own, and painting with Alt (or with Erase light on) takes light
//! away. Choosing another tool, running a command on the picture or
//! Finish keeps the lit pixels. See `docs/behavior/edit-inside-picture.md`.

use crate::pixel_edit::{dab_weight, union, Dirty, PixelImage};
use crate::pixel_edit_app::PixelEdit;
use image::{GrayImage, Luma, Rgba, RgbaImage};

/// How a light mixes with the photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightBlend {
    /// Brightens towards the light's colour, never past white.
    Lighten,
    /// Adds the light's colour: strong light burns out to white.
    Glow,
    /// Colours the photo with the light, keeping its lights and darks.
    Tint,
}

impl LightBlend {
    pub const ALL: [LightBlend; 3] = [LightBlend::Lighten, LightBlend::Glow, LightBlend::Tint];

    pub fn key(self) -> &'static str {
        match self {
            LightBlend::Lighten => "pixel.light.blend.lighten",
            LightBlend::Glow => "pixel.light.blend.glow",
            LightBlend::Tint => "pixel.light.blend.tint",
        }
    }
}

/// One light.
#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    pub color: [u8; 3],
    /// 0 to 3; 1 is full.
    pub strength: f32,
    pub blend: LightBlend,
    /// Where the light comes from, degrees: 0 the right, 90 the top.
    pub direction: f32,
    /// How much the photo's shapes turn towards or away from the light,
    /// 0 to 1.
    pub relief: f32,
    pub visible: bool,
    /// Where the light falls (255 full), the picture's size; only while
    /// the lights are live.
    mask: Option<GrayImage>,
    /// The part of the mask painted so far.
    bounds: Option<Dirty>,
}

/// The colours new lights take in turn.
const COLORS: [[u8; 3]; 5] = [
    [255, 150, 60],
    [70, 140, 255],
    [255, 80, 200],
    [90, 230, 130],
    [255, 244, 224],
];

impl Light {
    pub fn new(n: usize) -> Self {
        Light {
            color: COLORS[n % COLORS.len()],
            strength: 1.0,
            blend: LightBlend::Lighten,
            direction: 135.0,
            relief: 0.5,
            visible: true,
            mask: None,
            bounds: None,
        }
    }

    /// The settings alone, to see whether any changed.
    fn look(&self) -> ([u8; 3], u32, LightBlend, u32, u32, bool) {
        (
            self.color,
            self.strength.to_bits(),
            self.blend,
            self.direction.to_bits(),
            self.relief.to_bits(),
            self.visible,
        )
    }

    pub fn is_painted(&self) -> bool {
        self.bounds.is_some()
    }
}

/// A light's mask as it was before a stroke, for its undo.
struct Step {
    light: usize,
    mask: Option<GrayImage>,
    bounds: Option<Dirty>,
}

/// The lights of a picture being edited.
pub struct Lights {
    pub list: Vec<Light>,
    pub active: usize,
    /// Painting takes light away.
    pub erase: bool,
    /// The picture under the lights; `Some` while they are live.
    base: Option<RgbaImage>,
    /// The picture's layer the lights are on.
    layer: usize,
    /// The picture's brightness, softened, as a surface for the relief.
    heights: Vec<f32>,
    steps: Vec<Step>,
    stroke: Option<LightStroke>,
}

impl Default for Lights {
    fn default() -> Self {
        Lights {
            list: vec![Light::new(0)],
            active: 0,
            erase: false,
            base: None,
            layer: 0,
            heights: Vec::new(),
            steps: Vec::new(),
            stroke: None,
        }
    }
}

/// The most strokes the lights can undo one by one.
const MAX_STEPS: usize = 20;

/// One drag of the light brush.
struct LightStroke {
    erase: bool,
    last: Option<(f32, f32)>,
    /// How much the stroke covers each pixel so far, 0 to 1.
    cover: Vec<f32>,
}

impl Lights {
    /// True while the lights can still be changed.
    pub fn is_live(&self) -> bool {
        self.base.is_some()
    }

    /// Light `img`, the pixels of layer `layer`: from now on it is the
    /// picture under the lights.
    pub fn begin(&mut self, img: &RgbaImage, layer: usize) {
        let (w, h) = img.dimensions();
        self.layer = layer;
        self.heights = soft_heights(img);
        self.base = Some(img.clone());
        for l in &mut self.list {
            l.mask = Some(GrayImage::new(w, h));
            l.bounds = None;
        }
        self.steps.clear();
        self.stroke = None;
    }

    /// Keep the lit pixels: the lights keep their settings, with nothing
    /// painted.
    pub fn bake(&mut self) {
        if self.base.is_none() {
            return;
        }
        self.base = None;
        self.heights = Vec::new();
        for l in &mut self.list {
            l.mask = None;
            l.bounds = None;
        }
        self.steps.clear();
        self.stroke = None;
    }

    /// Every painted part of every light.
    pub fn painted(&self) -> Option<Dirty> {
        self.list
            .iter()
            .filter_map(|l| l.bounds)
            .reduce(|a, b| union(Some(a), b))
    }

    pub fn add(&mut self) {
        let mut l = Light::new(self.list.len());
        if let Some(b) = &self.base {
            l.mask = Some(GrayImage::new(b.width(), b.height()));
        }
        self.list.push(l);
        self.active = self.list.len() - 1;
    }

    /// Take the active light away (one is always left). Returns what it
    /// had painted, to light again.
    pub fn remove(&mut self) -> Option<Dirty> {
        if self.list.len() <= 1 {
            return None;
        }
        let gone = self.list.remove(self.active.min(self.list.len() - 1));
        self.active = self.active.min(self.list.len() - 1);
        // The undo steps name lights by place.
        self.steps.clear();
        gone.bounds
    }

    fn remember(&mut self, light: usize) {
        let Some(l) = self.list.get(light) else {
            return;
        };
        self.steps.push(Step {
            light,
            mask: l.mask.clone(),
            bounds: l.bounds,
        });
        if self.steps.len() > MAX_STEPS {
            self.steps.remove(0);
        }
    }

    /// Undo the last stroke. Returns the pixels to light again, or None
    /// when there is no stroke to undo.
    pub fn undo(&mut self) -> Option<Option<Dirty>> {
        let s = self.steps.pop()?;
        let l = self.list.get_mut(s.light)?;
        let redo = match (l.bounds, s.bounds) {
            (Some(a), b) => Some(union(b, a)),
            (None, b) => b,
        };
        l.mask = s.mask;
        l.bounds = s.bounds;
        Some(redo)
    }

    pub fn can_undo(&self) -> bool {
        !self.steps.is_empty()
    }

    /// Fill the active light inside the selection (all of the picture
    /// with none).
    pub fn fill(&mut self, pi: &PixelImage) -> Option<Dirty> {
        let b = pi.sel_bounds()?;
        self.remember(self.active);
        let l = self.list.get_mut(self.active)?;
        let m = l.mask.as_mut()?;
        for y in b[1]..b[3] {
            for x in b[0]..b[2] {
                let v = (pi.sel_at(x, y) * 255.0).round() as u8;
                let p = m.get_pixel_mut(x, y);
                p[0] = p[0].max(v);
            }
        }
        l.bounds = Some(union(l.bounds, b));
        Some(b)
    }

    /// Take all the active light away.
    pub fn clear(&mut self) -> Option<Dirty> {
        let b = self.list.get(self.active)?.bounds?;
        self.remember(self.active);
        let l = self.list.get_mut(self.active)?;
        if let Some(m) = l.mask.as_mut() {
            m.pixels_mut().for_each(|p| p[0] = 0);
        }
        l.bounds = None;
        Some(b)
    }

    /// Start a stroke of the light brush on the active light.
    pub fn start_stroke(&mut self, erase: bool) {
        self.remember(self.active);
        let n = self
            .base
            .as_ref()
            .map(|b| b.width() as usize * b.height() as usize)
            .unwrap_or(0);
        self.stroke = Some(LightStroke {
            erase,
            last: None,
            cover: vec![0.0; n],
        });
    }

    pub fn end_stroke(&mut self) {
        self.stroke = None;
    }

    /// The light brush taken to (x, y): dabs along the way, a quarter of
    /// the brush apart. Returns the pixels whose light changed.
    pub fn stroke_to(
        &mut self,
        pi: &PixelImage,
        x: f32,
        y: f32,
        size: f32,
        hardness: f32,
        flow: f32,
    ) -> Option<Dirty> {
        let step = (size * 0.25).max(1.0);
        let last = self.stroke.as_ref()?.last;
        let mut changed = None;
        let mut dab = |s: &mut Self, cx: f32, cy: f32| {
            if let Some(b) = s.dab(pi, cx, cy, size, hardness, flow) {
                changed = Some(union(changed, b));
            }
        };
        match last {
            None => {
                dab(self, x, y);
                if let Some(s) = self.stroke.as_mut() {
                    s.last = Some((x, y));
                }
            }
            Some((lx, ly)) => {
                let d = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                if d < step {
                    return None;
                }
                let n = (d / step).floor() as usize;
                for i in 1..=n {
                    let t = i as f32 * step / d;
                    dab(self, lx + (x - lx) * t, ly + (y - ly) * t);
                }
                let t = n as f32 * step / d;
                if let Some(s) = self.stroke.as_mut() {
                    s.last = Some((lx + (x - lx) * t, ly + (y - ly) * t));
                }
            }
        }
        changed
    }

    fn dab(
        &mut self,
        pi: &PixelImage,
        cx: f32,
        cy: f32,
        size: f32,
        hardness: f32,
        flow: f32,
    ) -> Option<Dirty> {
        let (w, h) = pi.img.dimensions();
        let r = (size / 2.0).max(0.5);
        let x0 = (cx - r).floor().max(0.0) as u32;
        let y0 = (cy - r).floor().max(0.0) as u32;
        let x1 = ((cx + r).ceil().max(0.0) as u32).min(w);
        let y1 = ((cy + r).ceil().max(0.0) as u32).min(h);
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let b = [x0, y0, x1, y1];
        let before = self.steps.last().filter(|s| s.light == self.active);
        let flow = flow.clamp(0.01, 1.0);
        let stroke = self.stroke.as_mut()?;
        let l = self.list.get_mut(self.active)?;
        let m = l.mask.as_mut()?;
        let at_press = before.and_then(|s| s.mask.as_ref());
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let wgt = dab_weight(d, size, hardness) * pi.sel_at(x, y);
                if wgt <= 0.0 {
                    continue;
                }
                let i = (y * w + x) as usize;
                let Some(c) = stroke.cover.get_mut(i) else {
                    continue;
                };
                *c = (*c + (1.0 - *c) * wgt * flow).min(1.0);
                let old = at_press.map(|a| a.get_pixel(x, y)[0]).unwrap_or(0) as f32;
                let v = if stroke.erase {
                    old * (1.0 - *c)
                } else {
                    old.max(*c * 255.0)
                };
                m.put_pixel(x, y, Luma([v.round() as u8]));
            }
        }
        if !stroke.erase {
            l.bounds = Some(union(l.bounds, b));
        }
        Some(b)
    }

    /// Light the pixels of `b` in `img` from the picture under the lights.
    pub fn render(&self, img: &mut RgbaImage, b: Dirty) {
        let Some(base) = &self.base else {
            return;
        };
        let (w, h) = base.dimensions();
        if img.dimensions() != (w, h) {
            return;
        }
        let (x1, y1) = (b[2].min(w), b[3].min(h));
        let lit: Vec<(&Light, &GrayImage, f32, f32)> = self
            .list
            .iter()
            .filter(|l| l.visible && l.strength > 0.0)
            .filter_map(|l| {
                let m = l.mask.as_ref()?;
                let a = l.direction.to_radians();
                Some((l, m, a.cos(), -a.sin()))
            })
            .collect();
        let hgt = |x: i64, y: i64| -> f32 {
            let x = x.clamp(0, w as i64 - 1) as u32;
            let y = y.clamp(0, h as i64 - 1) as u32;
            self.heights
                .get((y * w + x) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        for y in b[1]..y1 {
            for x in b[0]..x1 {
                let q = base.get_pixel(x, y);
                let mut c = [
                    q[0] as f32 / 255.0,
                    q[1] as f32 / 255.0,
                    q[2] as f32 / 255.0,
                ];
                for (l, m, lx, ly) in &lit {
                    let v = m.get_pixel(x, y)[0];
                    if v == 0 {
                        continue;
                    }
                    let mut amt = v as f32 / 255.0 * l.strength;
                    if l.relief > 0.0 {
                        // The surface rises towards higher brightness, so
                        // it faces the light where it climbs away from it.
                        let (xi, yi) = (x as i64, y as i64);
                        let gx = hgt(xi + 1, yi) - hgt(xi - 1, yi);
                        let gy = hgt(xi, yi + 1) - hgt(xi, yi - 1);
                        let facing = -(gx * lx + gy * ly);
                        amt *= (1.0 + facing * RELIEF_DEPTH * l.relief).clamp(0.0, 2.5);
                    }
                    if amt <= 0.0 {
                        continue;
                    }
                    for (k, ck) in c.iter_mut().enumerate() {
                        let lc = l.color[k] as f32 / 255.0;
                        *ck = match l.blend {
                            LightBlend::Lighten => 1.0 - (1.0 - *ck) * (1.0 - (lc * amt).min(1.0)),
                            LightBlend::Glow => *ck + lc * amt,
                            LightBlend::Tint => {
                                let t = if *ck < 0.5 {
                                    2.0 * *ck * lc
                                } else {
                                    1.0 - 2.0 * (1.0 - *ck) * (1.0 - lc)
                                };
                                *ck + (t - *ck) * amt.min(1.0)
                            }
                        }
                        .clamp(0.0, 1.0);
                    }
                }
                img.put_pixel(
                    x,
                    y,
                    Rgba([
                        (c[0] * 255.0).round() as u8,
                        (c[1] * 255.0).round() as u8,
                        (c[2] * 255.0).round() as u8,
                        q[3],
                    ]),
                );
            }
        }
    }
}

/// How strongly the surface's slope turns the light at relief 100 %.
const RELIEF_DEPTH: f32 = 14.0;

/// The picture's brightness, 0 to 1, softened so the relief follows its
/// shapes rather than its grain.
fn soft_heights(img: &RgbaImage) -> Vec<f32> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let src: Vec<f32> = img
        .pixels()
        .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0)
        .collect();
    let r = (w.min(h) / 250).clamp(1, 8) as i64;
    let a = box_blur(&src, w, h, r, true);
    box_blur(&a, w, h, r, false)
}

fn box_blur(src: &[f32], w: usize, h: usize, r: i64, horizontal: bool) -> Vec<f32> {
    let mut out = vec![0.0; src.len()];
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let at = |line: usize, i: usize| {
        if horizontal {
            line * w + i
        } else {
            i * w + line
        }
    };
    for line in 0..lines {
        let mut sum = 0.0;
        let get = |i: i64| src[at(line, i.clamp(0, len as i64 - 1) as usize)];
        for i in -r..=r {
            sum += get(i);
        }
        for i in 0..len {
            out[at(line, i)] = sum / (2 * r + 1) as f32;
            sum += get(i as i64 + r + 1) - get(i as i64 - r);
        }
    }
    out
}

// ---------------------------------------------------------------------
// The lights in the picture being edited
// ---------------------------------------------------------------------

impl PixelEdit {
    fn light_mark(&self, b: Option<Dirty>) {
        if let Some(b) = b {
            let mut v = self.view.borrow_mut();
            v.dirty = Some(union(v.dirty, b));
        }
    }

    /// Make the lights live on the picture as it is now, as one undo step.
    fn lights_live(&mut self) {
        if self.lights.is_live() && self.lights.layer != self.pi.active {
            self.lights.bake();
        }
        if self.lights.is_live() {
            return;
        }
        self.history.push(&self.pi);
        self.changed = true;
        self.preview = None;
        self.lights.begin(&self.pi.img, self.pi.active);
    }

    /// Light the pixels of `b` again.
    pub fn relight(&mut self, b: Option<Dirty>) {
        // Another layer was chosen: the lit pixels stay on their layer.
        if self.lights.is_live() && self.lights.layer != self.pi.active {
            self.lights.bake();
            return;
        }
        if let Some(b) = b {
            self.lights.render(&mut self.pi.img, b);
            self.light_mark(Some(b));
        }
    }

    /// The pointer on the picture with the Light tool, in picture pixels.
    pub fn light_input(&mut self, x: f32, y: f32, ev: LightEvent, alt: bool) {
        let (size, hardness, flow) = (self.opts.size, self.opts.hardness, self.opts.flow);
        match ev {
            LightEvent::Press | LightEvent::Click => {
                self.lights_live();
                let erase = self.lights.erase || alt;
                self.lights.start_stroke(erase);
                let b = self.lights.stroke_to(&self.pi, x, y, size, hardness, flow);
                self.relight(b);
                if ev == LightEvent::Click {
                    self.lights.end_stroke();
                }
            }
            LightEvent::Drag => {
                let b = self.lights.stroke_to(&self.pi, x, y, size, hardness, flow);
                self.relight(b);
            }
            LightEvent::Release => self.lights.end_stroke(),
        }
    }

    /// Fill the active light inside the selection.
    pub fn fill_light(&mut self) {
        self.lights_live();
        let b = self.lights.fill(&self.pi);
        self.relight(b);
    }

    /// Take all of the active light away.
    pub fn clear_light(&mut self) {
        let b = self.lights.clear();
        self.relight(b);
    }

    pub fn add_light(&mut self) {
        self.lights.add();
    }

    pub fn remove_light(&mut self) {
        let b = self.lights.remove();
        self.relight(b);
    }

    /// After the lights' settings were changed in `f`: light the picture
    /// again where they show.
    pub fn change_lights(&mut self, f: impl FnOnce(&mut Lights)) {
        let before: Vec<_> = self.lights.list.iter().map(Light::look).collect();
        f(&mut self.lights);
        let after: Vec<_> = self.lights.list.iter().map(Light::look).collect();
        if before != after {
            let b = self.lights.painted();
            self.relight(b);
        }
    }

    /// Undo the last stroke of light. False when there is none and the
    /// lights are no longer live (the picture's own undo goes on).
    pub fn undo_light(&mut self) -> bool {
        match self.lights.undo() {
            Some(b) => {
                self.relight(b);
                true
            }
            None => {
                self.lights.bake();
                false
            }
        }
    }
}

/// What the pointer did, for [`PixelEdit::light_input`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightEvent {
    Press,
    Drag,
    Release,
    Click,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(w: u32, h: u32, v: u8) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([v, v, v, 255]))
    }

    fn paint(l: &mut Lights, pi: &mut PixelImage, from: (f32, f32), to: (f32, f32), erase: bool) {
        if !l.is_live() {
            l.begin(&pi.img, 0);
        }
        l.start_stroke(erase);
        let mut all = None;
        for p in [from, to] {
            if let Some(b) = l.stroke_to(pi, p.0, p.1, 10.0, 1.0, 1.0) {
                all = Some(union(all, b));
            }
        }
        l.end_stroke();
        if let Some(b) = all {
            l.render(&mut pi.img, b);
        }
    }

    #[test]
    fn the_light_shows_where_it_is_painted_in_its_colour() {
        let mut pi = PixelImage::new(gray(40, 20, 100));
        let mut l = Lights::default();
        l.list[0].color = [60, 120, 255];
        l.list[0].relief = 0.0;
        paint(&mut l, &mut pi, (5.0, 10.0), (15.0, 10.0), false);
        let lit = *pi.img.get_pixel(10, 10);
        assert!(lit[2] > 200 && lit[2] > lit[0], "{lit:?}");
        assert!(lit[0] > 100, "Lighten never darkens: {lit:?}");
        assert_eq!(*pi.img.get_pixel(35, 10), Rgba([100, 100, 100, 255]));
    }

    #[test]
    fn two_lights_add_up_and_a_hidden_one_goes_out() {
        let mut pi = PixelImage::new(gray(40, 20, 60));
        let mut l = Lights::default();
        l.list[0].relief = 0.0;
        l.list[0].color = [255, 0, 0];
        paint(&mut l, &mut pi, (10.0, 10.0), (30.0, 10.0), false);
        l.add();
        l.list[1].relief = 0.0;
        l.list[1].color = [0, 0, 255];
        paint(&mut l, &mut pi, (10.0, 10.0), (30.0, 10.0), false);
        let both = *pi.img.get_pixel(20, 10);
        assert!(both[0] > 200 && both[2] > 200 && both[1] < 80, "{both:?}");
        l.list[0].visible = false;
        let b = l.painted().unwrap();
        l.render(&mut pi.img, b);
        let blue = *pi.img.get_pixel(20, 10);
        assert!(blue[0] < 80 && blue[2] > 200, "{blue:?}");
    }

    #[test]
    fn erasing_and_undo_take_the_light_back() {
        let mut pi = PixelImage::new(gray(40, 20, 100));
        let mut l = Lights::default();
        l.list[0].relief = 0.0;
        paint(&mut l, &mut pi, (5.0, 10.0), (35.0, 10.0), false);
        assert_ne!(pi.img.get_pixel(20, 10)[0], 100);
        paint(&mut l, &mut pi, (15.0, 10.0), (25.0, 10.0), true);
        assert_eq!(*pi.img.get_pixel(20, 10), Rgba([100, 100, 100, 255]));
        assert_ne!(pi.img.get_pixel(8, 10)[0], 100);
        // Undo the erase, then the painting.
        let b = l.undo().unwrap().unwrap();
        l.render(&mut pi.img, b);
        assert_ne!(pi.img.get_pixel(20, 10)[0], 100);
        let b = l.undo().unwrap().unwrap();
        l.render(&mut pi.img, b);
        assert_eq!(*pi.img.get_pixel(8, 10), Rgba([100, 100, 100, 255]));
        assert!(l.undo().is_none());
    }

    #[test]
    fn relief_lights_the_side_turned_to_the_light() {
        // A bright ridge down the middle: its left slope faces left.
        let mut img = gray(60, 20, 40);
        for y in 0..20 {
            for x in 0..60u32 {
                let d = (x as f32 - 30.0).abs();
                let v = (220.0 - d * 12.0).max(40.0) as u8;
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }
        let mut pi = PixelImage::new(img.clone());
        let mut l = Lights::default();
        l.list[0].color = [255, 255, 255];
        l.list[0].strength = 0.5;
        l.list[0].direction = 180.0; // from the left
        l.list[0].relief = 1.0;
        l.begin(&pi.img, 0);
        let full = pi.sel_bounds().unwrap();
        l.fill(&pi);
        l.render(&mut pi.img, full);
        let gain = |x: u32| pi.img.get_pixel(x, 10)[0] as i32 - img.get_pixel(x, 10)[0] as i32;
        // Same brightness either side of the ridge, more light on the left.
        assert!(gain(24) > gain(36) + 5, "{} {}", gain(24), gain(36));
    }

    #[test]
    fn tint_and_glow_mix_differently() {
        let mut pi = PixelImage::new(gray(20, 20, 40));
        let mut l = Lights::default();
        l.list[0].relief = 0.0;
        l.list[0].color = [255, 120, 0];
        l.list[0].blend = LightBlend::Tint;
        l.begin(&pi.img, 0);
        l.fill(&pi);
        l.render(&mut pi.img, [0, 0, 20, 20]);
        let tint = *pi.img.get_pixel(5, 5);
        // Tint keeps a dark photo dark.
        assert!(tint[0] < 90 && tint[2] < 10, "{tint:?}");
        l.list[0].blend = LightBlend::Glow;
        l.list[0].strength = 3.0;
        l.render(&mut pi.img, [0, 0, 20, 20]);
        let glow = *pi.img.get_pixel(5, 5);
        assert_eq!(glow[0], 255);
    }

    #[test]
    fn baking_keeps_the_pixels_and_ends_the_lights() {
        let mut pi = PixelImage::new(gray(20, 20, 100));
        let mut l = Lights::default();
        paint(&mut l, &mut pi, (5.0, 5.0), (15.0, 5.0), false);
        let lit = pi.img.clone();
        l.bake();
        assert!(!l.is_live());
        assert!(l.painted().is_none());
        assert_eq!(pi.img, lit);
        assert_eq!(l.list.len(), 1);
    }

    #[test]
    fn the_light_tool_in_the_editor_paints_relights_undoes_and_keeps() {
        use crate::app::App;
        use crate::pixel_edit::PixelTool;
        use tracedraw_core::geometry::Rect;
        use tracedraw_core::ShapeKind;
        let mut app = App::headless();
        let img = gray(40, 20, 100);
        let png = crate::bitmap_fx::encode(&img).unwrap();
        let id = app
            .new_shape(ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 40.0, 20.0),
                width_px: 40,
                height_px: 20,
                png: png.into(),
                fx: None,
            })
            .expect("a layer");
        assert!(app.edit_bitmap_inside(id));
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.tool = PixelTool::Light;
        pe.opts.size = 10.0;
        pe.change_lights(|l| l.list[0].relief = 0.0);
        pe.light_input(5.0, 10.0, LightEvent::Press, false);
        pe.light_input(20.0, 10.0, LightEvent::Drag, false);
        pe.light_input(20.0, 10.0, LightEvent::Release, false);
        let warm = *pe.pi.img.get_pixel(12, 10);
        assert!(warm[0] > warm[2] + 30, "{warm:?}");
        // A new colour lights the painted part again.
        pe.change_lights(|l| l.list[0].color = [0, 80, 255]);
        let blue = *pe.pi.img.get_pixel(12, 10);
        assert!(blue[2] > blue[0] + 30, "{blue:?}");
        // A second stroke, then undo takes only it back.
        pe.light_input(30.0, 10.0, LightEvent::Click, false);
        assert_ne!(pe.pi.img.get_pixel(30, 10)[2], 100);
        pe.undo();
        assert_eq!(*pe.pi.img.get_pixel(30, 10), Rgba([100, 100, 100, 255]));
        assert_eq!(*pe.pi.img.get_pixel(12, 10), blue);
        // Another tool keeps the light.
        pe.tool = PixelTool::Brush;
        pe.lights.bake();
        assert!(!pe.lights.is_live());
        app.finish_pixel_edit();
        let out = match &app.doc().find_shape(id).unwrap().kind {
            ShapeKind::Bitmap { png, .. } => crate::bitmap_fx::decode(png).unwrap(),
            _ => panic!("not a picture"),
        };
        assert_eq!(*out.get_pixel(12, 10), blue);
        assert_eq!(*out.get_pixel(35, 3), Rgba([100, 100, 100, 255]));
    }

    #[test]
    fn painting_stays_inside_the_selection() {
        let mut pi = PixelImage::new(gray(40, 20, 100));
        pi.sel = crate::pixel_edit::rect_mask(40, 20, (0.0, 0.0), (20.0, 20.0));
        let mut l = Lights::default();
        l.list[0].relief = 0.0;
        paint(&mut l, &mut pi, (5.0, 10.0), (35.0, 10.0), false);
        assert_ne!(pi.img.get_pixel(10, 10)[0], 100);
        assert_eq!(*pi.img.get_pixel(30, 10), Rgba([100, 100, 100, 255]));
    }
}
