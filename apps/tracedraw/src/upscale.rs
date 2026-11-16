//! Enlarge before tracing: the Trace dialog's "Enlarge small pictures
//! first" traces a small picture (a logo saved at a few hundred pixels,
//! often as a JPEG) enlarged up to four times, so the curves follow
//! clean edges instead of pixel steps and compression noise, and puts
//! the curves back at the picture's own size.
//!
//! With the AI model downloaded (once, on request: the compact "general
//! x4v3" model of the Real-ESRGAN project, BSD-3-Clause, from the
//! project's own release page) the enlargement is redrawn by the network of
//! `upscale_net.rs`, on this computer, on a thread of its own; without
//! it (or in a browser, or on a picture too large for it) the picture is
//! enlarged and sharpened (`resample::Method::Detail`). Each enlargement
//! is kept for the picture it was made of, so changing a setting traces
//! again without enlarging again. See docs/behavior/trace-enlarge.md.

use crate::trace::{Cancel, Settings, Traced};
use image::RgbaImage;
use std::sync::{Arc, Condvar, Mutex};

/// Where the model comes from, its file name, size and SHA-256.
pub const MODEL_URL: &str =
    "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-x4v3.pth";
pub const MODEL_FILE: &str = "realesr-general-x4v3.pth";
pub const MODEL_BYTES: u64 = 4_885_111;
pub const MODEL_SHA256: &str = "8dc7edb9ac80ccdc30c3a5dca6616509367f05fbc184ad95b731f05bece96292";

/// The enlarged picture's longest side is about this many pixels.
const TARGET_SIDE: f64 = 1600.0;
/// At most four times larger (the model's own scale), and only when it
/// is at least half as large again: a picture over 1,066 pixels across
/// is traced as it is.
const MAX_FACTOR: f64 = 4.0;
const MIN_FACTOR: f64 = 1.5;
/// Pictures of more pixels are enlarged and sharpened, not by the
/// model (it takes about a second per 100,000 pixels on four cores).
const AI_MAX_PIXELS: u64 = 1_200_000;
/// A picture of up to this many pixels goes to the model enlarged twice
/// first (smoothly), so letters a few pixels tall reach it large enough
/// to be redrawn whole: on a 300 pixel logo with letters 7 pixels tall,
/// a B keeps its straight side and square counters and a G its opening,
/// where straight from the picture they came out rounded and closed.
/// It takes four times as long.
const PRE_ENLARGE_MAX_PIXELS: u64 = 100_000;
/// Enlargements kept, the latest last.
const KEEP: usize = 3;

/// How much larger `w x h` is traced, when at all.
pub fn factor_for(w: u32, h: u32) -> Option<f64> {
    let side = w.max(h) as f64;
    if side < 1.0 {
        return None;
    }
    #[cfg(test)]
    let target = std::env::var("TRACE_TARGET_SIDE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(TARGET_SIDE);
    #[cfg(not(test))]
    let target = TARGET_SIDE;
    let f = (target / side).min(MAX_FACTOR);
    (f >= MIN_FACTOR).then_some(f)
}

/// How a picture was enlarged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    Ai,
    Sharpened,
}

/// An enlarged picture.
#[derive(Debug)]
pub struct Enlarged {
    pub img: RgbaImage,
    pub how: How,
}

/// What [`enlarged`] gives.
#[derive(Debug, Clone)]
pub enum Prep {
    /// The picture is traced as it is.
    Original,
    Enlarged(Arc<Enlarged>),
    /// The caller gave up while the model was working.
    Cancelled,
}

enum State {
    Running,
    Done(Arc<Enlarged>),
}

struct Job {
    key: u64,
    state: State,
}

static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());
static JOB_DONE: Condvar = Condvar::new();
/// The model's share done (per mille, plus one) while it works; 0 idle.
static PROGRESS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The share (0..1) of the enlargement the model is making, if any.
pub fn enlarging() -> Option<f32> {
    match PROGRESS.load(std::sync::atomic::Ordering::Relaxed) {
        0 => None,
        n => Some((n - 1) as f32 / 1000.0),
    }
}

fn image_key(img: &RgbaImage) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    img.dimensions().hash(&mut h);
    img.as_raw().hash(&mut h);
    h.finish()
}

/// `img` enlarged and sharpened to `w x h`.
fn sharpened(img: &RgbaImage, w: u32, h: u32) -> Enlarged {
    Enlarged {
        img: crate::resample::resample(img, w, h, crate::resample::Method::Detail),
        how: How::Sharpened,
    }
}

fn finish(key: u64, e: Arc<Enlarged>) {
    if let Ok(mut jobs) = JOBS.lock() {
        match jobs.iter_mut().find(|j| j.key == key) {
            Some(j) => j.state = State::Done(e),
            None => jobs.push(Job {
                key,
                state: State::Done(e),
            }),
        }
        // The oldest enlargements made go; one still being made stays,
        // or whoever waits for it would give up.
        while jobs
            .iter()
            .filter(|j| matches!(j.state, State::Done(_)))
            .count()
            > KEEP
        {
            match jobs.iter().position(|j| matches!(j.state, State::Done(_))) {
                Some(i) => {
                    jobs.remove(i);
                }
                None => break,
            }
        }
    }
    JOB_DONE.notify_all();
}

/// `img` enlarged for tracing ([`factor_for`]), by the model when it is
/// downloaded and the picture small enough, else sharpened; the same
/// picture again gets the enlargement made before. While the model
/// works (on a thread of its own, which goes on when the caller gives
/// up, so the next trace finds it done) this waits, giving up once
/// `cancel` is set.
pub fn enlarged(img: &RgbaImage, cancel: Option<&Cancel>) -> Prep {
    let (w, h) = img.dimensions();
    let Some(f) = factor_for(w, h) else {
        return Prep::Original;
    };
    let (bw, bh) = (
        ((w as f64 * f).round() as u32).max(1),
        ((h as f64 * f).round() as u32).max(1),
    );
    let net = net().filter(|_| (w as u64) * (h as u64) <= AI_MAX_PIXELS);
    let key = image_key(img) ^ (net.is_some() as u64).rotate_left(17);
    let start = match JOBS.lock() {
        Ok(mut jobs) => match jobs.iter().find(|j| j.key == key) {
            Some(Job {
                state: State::Done(e),
                ..
            }) => return Prep::Enlarged(e.clone()),
            Some(_) => false,
            None => {
                jobs.push(Job {
                    key,
                    state: State::Running,
                });
                true
            }
        },
        Err(_) => return Prep::Enlarged(Arc::new(sharpened(img, bw, bh))),
    };
    if start {
        match net {
            None => {
                let e = Arc::new(sharpened(img, bw, bh));
                finish(key, e.clone());
                return Prep::Enlarged(e);
            }
            Some(net) => spawn_ai(net, img.clone(), key, bw, bh),
        }
    }
    let Ok(mut jobs) = JOBS.lock() else {
        return Prep::Cancelled;
    };
    loop {
        match jobs.iter().find(|j| j.key == key) {
            Some(Job {
                state: State::Done(e),
                ..
            }) => return Prep::Enlarged(e.clone()),
            Some(_) => {}
            // Dropped meanwhile (other pictures enlarged since): the
            // next trace starts it again.
            None => return Prep::Cancelled,
        }
        if crate::trace::cancelled(cancel) {
            return Prep::Cancelled;
        }
        jobs = match JOB_DONE.wait_timeout(jobs, std::time::Duration::from_millis(50)) {
            Ok((g, _)) => g,
            Err(_) => return Prep::Cancelled,
        };
    }
}

/// Enlarge `img` by the model on a thread of its own, the enlargement
/// sized to `w x h`; a failure (or a panic) falls back to sharpening.
#[cfg(not(target_arch = "wasm32"))]
fn spawn_ai(net: Arc<crate::upscale_net::Net>, img: RgbaImage, key: u64, w: u32, h: u32) {
    use std::sync::atomic::Ordering;
    let run = move || {
        PROGRESS.store(1, Ordering::Relaxed);
        let (iw, ih) = img.dimensions();
        let input = if (iw as u64) * (ih as u64) <= PRE_ENLARGE_MAX_PIXELS {
            std::borrow::Cow::Owned(image::imageops::resize(
                &img,
                iw * 2,
                ih * 2,
                image::imageops::FilterType::Lanczos3,
            ))
        } else {
            std::borrow::Cow::Borrowed(&img)
        };
        let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            net.upscale(&input, &|| false, &|p| {
                PROGRESS.store(1 + (p.clamp(0.0, 1.0) * 1000.0) as u32, Ordering::Relaxed);
            })
        }))
        .ok()
        .flatten();
        let e = match made {
            Some(big) => {
                let big = if big.dimensions() == (w, h) {
                    big
                } else {
                    image::imageops::resize(&big, w, h, image::imageops::FilterType::Lanczos3)
                };
                Enlarged {
                    img: big,
                    how: How::Ai,
                }
            }
            None => {
                log::warn!("the AI enlargement failed; the picture is sharpened instead");
                sharpened(&img, w, h)
            }
        };
        PROGRESS.store(0, Ordering::Relaxed);
        finish(key, Arc::new(e));
    };
    if std::thread::Builder::new()
        .name("enlarge".into())
        .spawn(run)
        .is_err()
    {
        log::warn!("no thread for the AI enlargement");
    }
}

#[cfg(target_arch = "wasm32")]
fn spawn_ai(_net: Arc<()>, img: RgbaImage, key: u64, w: u32, h: u32) {
    finish(key, Arc::new(sharpened(&img, w, h)));
}

/// [`crate::trace::trace_unless`] of `img`, enlarged first when
/// `enlarge` asks for it ([`enlarged`]), the curves (and the centreline
/// stroke width, the second value, in pixels) brought back to `img`'s
/// own pixels. `None` once `cancel` is set.
pub fn trace_unless(
    img: &RgbaImage,
    s: &Settings,
    enlarge: bool,
    cancel: Option<&Cancel>,
) -> Option<(Vec<Traced>, f64)> {
    if enlarge {
        match enlarged(img, cancel) {
            Prep::Cancelled => return None,
            Prep::Enlarged(e) => {
                log::debug!(
                    "tracing {:?} enlarged to {:?} ({:?})",
                    img.dimensions(),
                    e.img.dimensions(),
                    e.how
                );
                let (w, h) = img.dimensions();
                let (bw, bh) = e.img.dimensions();
                let sx = w as f64 / bw.max(1) as f64;
                let sy = h as f64 / bh.max(1) as f64;
                // Every measure the trace takes in pixels is in pixels
                // of the picture, not of the enlargement.
                let mut s = s.clone();
                s.pixel_scale *= 2.0 / (sx + sy);
                let traced = crate::trace::trace_unless(&e.img, &s, cancel)?;
                let stroke = crate::trace::last_stroke_width_px();
                return Some((scaled(traced, sx, sy), stroke * 0.5 * (sx + sy)));
            }
            Prep::Original => {}
        }
    }
    let traced = crate::trace::trace_unless(img, s, cancel)?;
    Some((traced, crate::trace::last_stroke_width_px()))
}

/// The curves of `traced` scaled by `sx, sy` (pixel space).
fn scaled(mut traced: Vec<Traced>, sx: f64, sy: f64) -> Vec<Traced> {
    let map = kurbo::Affine::scale_non_uniform(sx, sy);
    for t in &mut traced {
        t.path.apply_affine(map);
        if let Some(b) = t.blend.as_mut() {
            b.from = map * b.from;
            b.to = map * b.to;
        }
    }
    traced
}

// ----- the model file ---------------------------------------------------------------

/// Where the model is kept: TraceDraw's own `models` folder (none in a
/// browser).
pub fn model_path() -> Option<std::path::PathBuf> {
    if crate::files::WEB {
        return None;
    }
    crate::settings::config_dir().map(|d| d.join("models").join(MODEL_FILE))
}

/// Whether the AI enlargement can be offered at all (not in a browser).
pub fn ai_offered() -> bool {
    !crate::files::WEB
}

/// The model is downloaded (whole, by its size).
pub fn model_ready() -> bool {
    static SEEN: Mutex<Option<(std::path::PathBuf, bool)>> = Mutex::new(None);
    let Some(path) = model_path() else {
        return false;
    };
    let Ok(mut seen) = SEEN.lock() else {
        return false;
    };
    if let Some((p, ready)) = seen.as_ref() {
        // Once there, it stays (the folder is TraceDraw's own); until
        // then it is looked for again after each download.
        if *p == path && (*ready || downloading().is_some()) {
            return *ready;
        }
    }
    let ready = std::fs::metadata(&path).is_ok_and(|m| m.len() == MODEL_BYTES);
    *seen = Some((path, ready));
    ready
}

#[cfg(not(target_arch = "wasm32"))]
type NetRef = Arc<crate::upscale_net::Net>;
#[cfg(target_arch = "wasm32")]
type NetRef = Arc<()>;

/// The model's network, read once from the downloaded file; none when
/// it is not downloaded or cannot be read (logged once).
fn net() -> Option<NetRef> {
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static NET: Mutex<Option<Result<NetRef, String>>> = Mutex::new(None);
        if !model_ready() {
            return None;
        }
        let mut held = NET.lock().ok()?;
        if held.is_none() {
            let read = model_path()
                .ok_or_else(|| "no folder".to_string())
                .and_then(|p| std::fs::read(p).map_err(|e| e.to_string()))
                .and_then(|b| crate::upscale_net::Net::from_pth(&b))
                .map(Arc::new);
            if let Err(e) = &read {
                log::warn!("the AI model cannot be read: {e}");
            }
            *held = Some(read);
        }
        held.as_ref().and_then(|r| r.as_ref().ok().cloned())
    }
}

/// The download's state.
#[derive(Debug, Clone, PartialEq)]
pub enum Download {
    Idle,
    /// The share received, 0..1.
    Running(f32),
    Failed(String),
}

static DOWNLOAD: Mutex<Download> = Mutex::new(Download::Idle);

/// The share of the model received while it downloads.
pub fn downloading() -> Option<f32> {
    match DOWNLOAD.lock().ok()?.clone() {
        Download::Running(p) => Some(p),
        _ => None,
    }
}

/// Why the last download failed, if it did.
pub fn download_failed() -> Option<String> {
    match DOWNLOAD.lock().ok()?.clone() {
        Download::Failed(e) => Some(e),
        _ => None,
    }
}

/// Start downloading the model (nothing when it is downloading or
/// downloaded, or in a browser).
pub fn start_download() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if model_ready() {
            return;
        }
        let Some(path) = model_path() else {
            return;
        };
        {
            let Ok(mut d) = DOWNLOAD.lock() else {
                return;
            };
            if matches!(*d, Download::Running(_)) {
                return;
            }
            *d = Download::Running(0.0);
        }
        let _ = std::thread::Builder::new()
            .name("model download".into())
            .spawn(move || {
                let got = fetch_model(&path, |p| {
                    if let Ok(mut d) = DOWNLOAD.lock() {
                        *d = Download::Running(p);
                    }
                });
                if let Ok(mut d) = DOWNLOAD.lock() {
                    *d = match got {
                        Ok(()) => Download::Idle,
                        Err(e) => Download::Failed(e),
                    };
                }
            });
    }
}

/// Download the model to `path`: into a `.part` file, checked against
/// [`MODEL_SHA256`], then renamed.
#[cfg(not(target_arch = "wasm32"))]
fn fetch_model(path: &std::path::Path, progress: impl Fn(f32)) -> Result<(), String> {
    use sha2::Digest;
    use std::io::{Read, Write};
    let dir = path.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut src = crate::updates::download(MODEL_URL)?;
    let part = path.with_extension("part");
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hash = sha2::Sha256::new();
    let mut got: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024];
    let read = loop {
        let n = match src.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(e) => break Err(e.to_string()),
        };
        got += n as u64;
        if got > MODEL_BYTES {
            break Err("the file is larger than the model".to_string());
        }
        hash.update(&buf[..n]);
        if let Err(e) = out.write_all(&buf[..n]) {
            break Err(e.to_string());
        }
        progress(got as f32 / MODEL_BYTES as f32);
    };
    drop(out);
    let checked = read.and_then(|()| {
        let sum: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if got == MODEL_BYTES && sum == MODEL_SHA256 {
            Ok(())
        } else {
            Err("the file downloaded is not the model (checksum)".to_string())
        }
    });
    match checked {
        Ok(()) => std::fs::rename(&part, path).map_err(|e| e.to_string()),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small pictures are enlarged to about 1,600 pixels across, at most
    /// four times; large ones are traced as they are.
    #[test]
    fn small_pictures_are_enlarged_up_to_four_times() {
        assert_eq!(factor_for(300, 200), Some(4.0));
        assert_eq!(factor_for(400, 100), Some(4.0));
        let f = factor_for(800, 600).unwrap();
        assert!((f - 2.0).abs() < 1e-9, "{f}");
        assert_eq!(factor_for(1100, 900), None);
        assert_eq!(factor_for(4000, 3000), None);
        assert_eq!(factor_for(0, 0), None);
    }

    fn disc(n: u32) -> RgbaImage {
        RgbaImage::from_fn(n, n, |x, y| {
            let c = n as f32 / 2.0;
            let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
            if d < c * 0.6 {
                image::Rgba([200, 30, 30, 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            }
        })
    }

    /// Traced enlarged (sharpened, no model in the tests), the curves
    /// come back at the picture's own size and place: the disc's curve
    /// spans the same pixels as when traced as it is, and the second
    /// trace of the picture reuses the enlargement.
    #[test]
    fn an_enlarged_trace_lands_at_the_pictures_own_size() {
        let img = disc(120);
        let s = crate::trace::Preset::Logo.settings();
        let (plain, _) = trace_unless(&img, &s, false, None).unwrap();
        let (big, _) = trace_unless(&img, &s, true, None).unwrap();
        let red = |t: &[Traced]| {
            t.iter()
                .filter(|t| t.color.to_rgb8()[1] < 120)
                .map(|t| kurbo::Shape::bounding_box(&t.path))
                .reduce(|a, b| a.union(b))
                .unwrap()
        };
        let (a, b) = (red(&plain), red(&big));
        for (p, q) in [(a.x0, b.x0), (a.y0, b.y0), (a.x1, b.x1), (a.y1, b.y1)] {
            assert!((p - q).abs() < 1.0, "{a:?} {b:?}");
        }
        // 72 pixels across, centred.
        assert!((b.width() - 72.0).abs() < 1.5, "{b:?}");
        let Prep::Enlarged(e1) = enlarged(&img, None) else {
            panic!("not enlarged");
        };
        let Prep::Enlarged(e2) = enlarged(&img, None) else {
            panic!("not enlarged");
        };
        assert!(Arc::ptr_eq(&e1, &e2));
        assert_eq!(e1.img.dimensions(), (480, 480));
        assert_eq!(e1.how, How::Sharpened);
        // A large picture is traced as it is.
        assert!(matches!(enlarged(&disc(1200), None), Prep::Original));
    }

    /// Only enlargements already made are dropped for newer ones: one
    /// still being made stays, so the trace waiting for it gets it.
    #[test]
    fn an_enlargement_being_made_is_never_dropped() {
        let running = 0x5eed_0000_0000_0001u64;
        JOBS.lock().unwrap().push(Job {
            key: running,
            state: State::Running,
        });
        let img = Arc::new(Enlarged {
            img: RgbaImage::new(1, 1),
            how: How::Sharpened,
        });
        for k in 0..(KEEP as u64 + 3) {
            finish(0x5eed_0000_0000_1000 + k, img.clone());
        }
        let jobs = JOBS.lock().unwrap();
        assert!(jobs.iter().any(|j| j.key == running));
        assert!(
            jobs.iter()
                .filter(|j| matches!(j.state, State::Done(_)))
                .count()
                <= KEEP
        );
        drop(jobs);
        finish(running, img);
    }

    /// With a temporary folder and no download, the model is not ready
    /// and no network is read.
    #[test]
    fn without_the_model_no_network_is_used() {
        let path = model_path();
        if let Some(p) = path {
            if !p.exists() {
                assert!(!model_ready());
                assert!(net().is_none());
            }
        }
    }

    /// A look at a picture traced as it is and enlarged first, side by
    /// side at four times its size (Logo and Detailed logo presets):
    /// `TRACE_PICTURE=logo.png TRACE_OUT=dir cargo test -p tracedraw
    /// --release --lib upscale::tests::compare -- --ignored`. With the
    /// model in `$TRACEDRAW_CONFIG_DIR/models` the enlargement is the
    /// model's.
    #[test]
    #[ignore]
    fn compare() {
        let (Ok(pic), Ok(out)) = (std::env::var("TRACE_PICTURE"), std::env::var("TRACE_OUT"))
        else {
            return;
        };
        let img = image::open(&pic).expect("picture").to_rgba8();
        let (w, h) = img.dimensions();
        let started = web_time::Instant::now();
        if let Prep::Enlarged(e) = enlarged(&img, None) {
            e.img.save(format!("{out}/enlarged.png")).expect("save");
            eprintln!("enlarged {:?} in {:?}", e.how, started.elapsed());
        }
        for preset in [
            crate::trace::Preset::Logo,
            crate::trace::Preset::DetailedLogo,
        ] {
            let s = preset.settings();
            let mut row = Vec::new();
            for enlarge in [false, true] {
                let (t, px) = trace_unless(&img, &s, enlarge, None).expect("trace");
                eprintln!(
                    "{preset:?} enlarge={enlarge}: {:?}",
                    crate::trace::stats(&t)
                );
                row.push(crate::trace::render_traced_scaled(
                    &t,
                    w * 4,
                    h * 4,
                    4.0,
                    px,
                ));
            }
            let mut sheet =
                RgbaImage::from_pixel(w * 8 + 8, h * 4, image::Rgba([255, 0, 255, 255]));
            image::imageops::replace(&mut sheet, &row[0], 0, 0);
            image::imageops::replace(&mut sheet, &row[1], (w * 4 + 8) as i64, 0);
            sheet
                .save(format!(
                    "{out}/{preset:?}{}.png",
                    std::env::var("TRACE_TAG").unwrap_or_default()
                ))
                .expect("save");
        }
    }
}
