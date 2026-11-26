//! The splash screen drawn by the system, without the graphics card
//! (#4321). The splash screen of `splash.rs` is drawn by the renderer
//! the window opens with, so while that renderer is being tried nothing
//! could show: a driver that hangs before its first frame left the
//! screen empty for up to `gpu::WINDOW_WAIT` (20 seconds) before the
//! start went on with the next renderer, and a renderer that drew only
//! black closed its window and started again with nothing on the screen
//! either. People took TraceDraw for not starting and clicked again.
//!
//! On Windows a plain system window with the same picture (the brand
//! gradient, the icon, the name, the tagline and the version) shows the
//! moment the program starts, before the renderer is set up, with what
//! is happening above a moving bar: "Setting up the graphics card…", or
//! "Trying another way to draw the window…" in a start that replaces a
//! renderer that failed. It goes once the first real frame reaches the
//! screen (`gpu::note_presented`), where the splash drawn by the
//! renderer takes its place, and a start that hands over to the next
//! renderer keeps it up until that start shows its own, so there is no
//! blank gap. Other systems keep the splash drawn by the renderer only.
//!
//! `--no-splash` and `TRACEDRAW_NO_SPLASH=1` leave it out with the other
//! splash; with the splash turned off in Options it still shows when the
//! start replaces a renderer that failed, as that start can take long.
// The picture is drawn by the Windows window only; its tests run everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use egui::Color32;

/// The environment variable a start that replaces a failed renderer is
/// given, so that its splash says it is trying another way.
pub const RETRY_ENV: &str = "TRACEDRAW_RENDERER_RETRY";

/// What the splash says above its bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The renderer is being set up.
    SettingUp,
    /// The renderer before did not work; another is being tried.
    TryingAnother,
}

impl Status {
    pub fn key(self) -> &'static str {
        match self {
            Status::SettingUp => "splash.setting_up_graphics",
            Status::TryingAnother => "splash.trying_another_renderer",
        }
    }

    fn code(self) -> u8 {
        match self {
            Status::SettingUp => 0,
            Status::TryingAnother => 1,
        }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn from_code(c: u8) -> Self {
        if c == 1 {
            Status::TryingAnother
        } else {
            Status::SettingUp
        }
    }
}

/// Whether this start replaces a renderer that failed (the start before
/// handed over to it, or noted a renderer that never showed a frame).
pub fn retrying(last_failed: bool) -> bool {
    last_failed || std::env::var_os(RETRY_ENV).is_some_and(|v| !v.is_empty() && v != "0")
}

/// Whether the system splash shows for this start: not with
/// `--no-splash`; with the splash on in Options, or in a start that
/// replaces a renderer that failed.
pub fn wanted(plain: bool, splash_on: bool, retrying: bool) -> bool {
    !plain && (splash_on || retrying)
}

static STATUS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Show the splash saying `status` (Windows only; elsewhere nothing).
pub fn show(status: Status) {
    STATUS.store(status.code(), std::sync::atomic::Ordering::SeqCst);
    #[cfg(windows)]
    imp::show();
}

/// Change what the splash says, showing it again if it was closed.
pub fn set_status(status: Status) {
    show(status);
}

/// Close the splash: the first frame reached the screen.
pub fn close() {
    #[cfg(windows)]
    imp::close();
}

/// A new start with the next renderer takes over: the splash says so and
/// stays up a moment longer, until that start shows its own.
pub fn hand_over() {
    set_status(Status::TryingAnother);
    #[cfg(windows)]
    {
        let _ = std::thread::Builder::new()
            .name("tracedraw-splash-handover".into())
            .spawn(|| {
                std::thread::sleep(HAND_OVER);
                close();
            });
    }
}

/// How long a start that hands over keeps its splash up after starting
/// the next one: long enough for that start to show its own.
pub const HAND_OVER: std::time::Duration = std::time::Duration::from_millis(1500);

/// The splash's size in points, as the splash drawn by the renderer.
pub fn size_points() -> (f32, f32) {
    (crate::splash::SIZE.x, crate::splash::SIZE.y)
}

/// A pixel `0x00RRGGBB`, as a 32-bit bottom-up or top-down DIB holds it.
fn rgb(c: Color32) -> u32 {
    (u32::from(c.r()) << 16) | (u32::from(c.g()) << 8) | u32::from(c.b())
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (f32::from(a) + (f32::from(b) - f32::from(a)) * t.clamp(0.0, 1.0)).round() as u8
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgb(lerp(a.r(), b.r(), t), lerp(a.g(), b.g(), t), lerp(a.b(), b.b(), t))
}

/// Blend `top` (with alpha `alpha`, 0 to 255) over the pixel `under`.
fn over(under: u32, top: Color32, alpha: u8) -> u32 {
    let u = Color32::from_rgb((under >> 16) as u8, (under >> 8) as u8, under as u8);
    rgb(mix(u, top, f32::from(alpha) / 255.0))
}

/// Where the parts of the splash go, in pixels, for a splash `w` by `h`
/// pixels drawn at `s` pixels per point (the layout of `splash.rs`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub s: f32,
    pub pad: f32,
    /// The icon's box: left, top and side.
    pub icon: (f32, f32, f32),
    /// The top of the progress bar.
    pub bar_y: f32,
    /// The top of the darker strip along the bottom.
    pub strip_y: f32,
    /// The baseline area of the footer (version).
    pub foot_y: f32,
}

pub fn layout(w: usize, h: usize) -> Layout {
    let (pw, ph) = size_points();
    let s = (w as f32 / pw).min(h as f32 / ph).max(0.5);
    let pad = 44.0 * s;
    let bar_y = h as f32 - 54.0 * s;
    Layout {
        s,
        pad,
        icon: (pad, 64.0 * s, 96.0 * s),
        bar_y,
        strip_y: bar_y - 40.0 * s,
        foot_y: h as f32 - 34.0 * s,
    }
}

/// The splash's picture without its texts, `w` by `h` pixels, top row
/// first, each pixel `0x00RRGGBB`: the brand gradient, the darker strip,
/// the icon (`icon`, straight RGBA) and the progress bar, whose moving
/// part sits at `phase` (0 to 1, going round).
pub fn compose(w: usize, h: usize, phase: f32, icon: Option<&image::RgbaImage>) -> Vec<u32> {
    use crate::ui::brand;
    let l = layout(w, h);
    let mut px = vec![0u32; w * h];
    let (wf, hf) = ((w.max(2) - 1) as f32, (h.max(2) - 1) as f32);
    for y in 0..h {
        let ty = y as f32 / hf;
        for x in 0..w {
            let top = mix(brand::TOP_LEFT, brand::TOP_RIGHT, x as f32 / wf);
            px[y * w + x] = rgb(mix(top, brand::BOTTOM, ty));
        }
    }
    // The darker strip with a faint line along its top.
    let strip = l.strip_y.max(0.0) as usize;
    for y in strip.min(h)..h {
        for x in 0..w {
            let p = &mut px[y * w + x];
            *p = over(*p, Color32::from_rgb(0x0A, 0x12, 0x22), 225);
            if y == strip {
                *p = over(*p, Color32::WHITE, 22);
            }
        }
    }
    // The icon, scaled to its box with the nearest pixel.
    if let Some(img) = icon {
        let (ix, iy, side) = l.icon;
        let n = side.round().max(1.0) as usize;
        let (iw, ih) = (img.width() as usize, img.height() as usize);
        for dy in 0..n {
            for dx in 0..n {
                let (x, y) = (ix as usize + dx, iy as usize + dy);
                if x >= w || y >= h || iw == 0 || ih == 0 {
                    continue;
                }
                let sx = (dx * iw / n).min(iw - 1);
                let sy = (dy * ih / n).min(ih - 1);
                let c = img.get_pixel(sx as u32, sy as u32).0;
                if c[3] > 0 {
                    let p = &mut px[y * w + x];
                    *p = over(*p, Color32::from_rgb(c[0], c[1], c[2]), c[3]);
                }
            }
        }
    }
    // The bar: a faint track and a moving part that says it is busy.
    let (x0, x1) = (l.pad, w as f32 - l.pad);
    let bar_h = (3.0 * l.s).round().max(1.0) as usize;
    let part = (x1 - x0) * 0.28;
    let start = x0 - part + (x1 - x0 + part) * phase.rem_euclid(1.0);
    for y in (l.bar_y.max(0.0) as usize)..(l.bar_y.max(0.0) as usize + bar_h).min(h) {
        for x in (x0.max(0.0) as usize)..(x1.max(0.0) as usize).min(w) {
            let xf = x as f32;
            let p = &mut px[y * w + x];
            *p = if xf >= start && xf < start + part {
                rgb(brand::CURVE)
            } else {
                over(*p, Color32::WHITE, 30)
            };
        }
    }
    // A thin light border round the window.
    for x in 0..w {
        for y in [0, h.saturating_sub(1)] {
            let p = &mut px[y * w + x];
            *p = over(*p, Color32::WHITE, 40);
        }
    }
    for y in 1..h.saturating_sub(1) {
        for x in [0, w.saturating_sub(1)] {
            let p = &mut px[y * w + x];
            *p = over(*p, Color32::WHITE, 40);
        }
    }
    px
}

/// The 128 px application icon, decoded for the system splash.
#[cfg_attr(not(windows), allow(dead_code))]
fn icon_image() -> Option<image::RgbaImage> {
    let bytes = include_bytes!("../../../assets/icon/tracedraw-128.png");
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()
        .map(|i| i.to_rgba8())
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, SIZE, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC,
        DeleteObject, EndPaint, GetDC, GetDeviceCaps, GetTextExtentPoint32W, InvalidateRect,
        ReleaseDC, SelectObject, SetBkMode, SetTextColor, TextOutW, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
        DEFAULT_PITCH, DIB_RGB_COLORS, FW_BOLD, FW_NORMAL, LOGPIXELSX, OUT_DEFAULT_PRECIS,
        PAINTSTRUCT, SRCCOPY, TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        GetSystemMetrics, LoadCursorW, LoadIconW, PostMessageW, PostQuitMessage, RegisterClassW,
        SetTimer, TranslateMessage, IDC_APPSTARTING, MSG, SM_CXSCREEN, SM_CYSCREEN, WM_CLOSE,
        WM_DESTROY, WM_ERASEBKGND, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_APPWINDOW, WS_POPUP,
        WS_VISIBLE,
    };

    /// The splash window, 0 when none is up.
    static WINDOW: AtomicIsize = AtomicIsize::new(0);
    /// Whether a splash thread is running (the window may still be being
    /// made).
    static RUNNING: Mutex<bool> = Mutex::new(false);
    /// Closed before its window was made: the thread makes none.
    static CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    struct Paint {
        icon: Option<image::RgbaImage>,
        started: web_time::Instant,
        name: String,
        tagline: String,
        version: String,
    }

    static PAINT: Mutex<Option<Paint>> = Mutex::new(None);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn colorref(c: Color32) -> u32 {
        u32::from(c.r()) | (u32::from(c.g()) << 8) | (u32::from(c.b()) << 16)
    }

    pub fn show() {
        CLOSED.store(false, Ordering::SeqCst);
        let hwnd = WINDOW.load(Ordering::SeqCst);
        if hwnd != 0 {
            unsafe {
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            return;
        }
        let mut running = RUNNING.lock().unwrap_or_else(|p| p.into_inner());
        if *running {
            return;
        }
        *running = true;
        drop(running);
        if let Ok(mut p) = PAINT.lock() {
            if p.is_none() {
                *p = Some(Paint {
                    icon: icon_image(),
                    started: web_time::Instant::now(),
                    name: "TraceDraw".into(),
                    tagline: crate::i18n::tr("dialog.about_tagline"),
                    version: format!("v{}", env!("CARGO_PKG_VERSION")),
                });
            }
        }
        let spawned = std::thread::Builder::new()
            .name("tracedraw-system-splash".into())
            .spawn(|| {
                unsafe { run_window() };
                WINDOW.store(0, Ordering::SeqCst);
                *RUNNING.lock().unwrap_or_else(|p| p.into_inner()) = false;
            });
        if spawned.is_err() {
            *RUNNING.lock().unwrap_or_else(|p| p.into_inner()) = false;
        }
    }

    pub fn close() {
        CLOSED.store(true, Ordering::SeqCst);
        let hwnd = WINDOW.load(Ordering::SeqCst);
        if hwnd != 0 {
            unsafe {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    }

    unsafe fn run_window() {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = wide("TraceDrawSystemSplash");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            // The program file's icon (resource 1, see build.rs), for the
            // taskbar.
            hIcon: LoadIconW(instance, 1 as _),
            hCursor: LoadCursorW(0, IDC_APPSTARTING),
            hbrBackground: 0,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        // Registered once per process; a second show finds it there.
        RegisterClassW(&wc);
        let screen = GetDC(0);
        let dpi = if screen != 0 {
            let d = GetDeviceCaps(screen, LOGPIXELSX as i32);
            ReleaseDC(0, screen);
            d
        } else {
            96
        };
        let scale = (dpi.max(96) as f32) / 96.0;
        let (pw, ph) = size_points();
        let (w, h) = ((pw * scale).round() as i32, (ph * scale).round() as i32);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        if CLOSED.load(Ordering::SeqCst) {
            return;
        }
        let title = wide("TraceDraw");
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            (sw - w) / 2,
            (sh - h) / 2,
            w,
            h,
            0,
            0,
            instance,
            std::ptr::null(),
        );
        if hwnd == 0 {
            return;
        }
        WINDOW.store(hwnd, Ordering::SeqCst);
        if CLOSED.load(Ordering::SeqCst) {
            DestroyWindow(hwnd);
        } else {
            // The bar moves: a frame every 40 ms.
            SetTimer(hwnd, 1, 40, None);
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_ERASEBKGND => 1,
            WM_TIMER => {
                InvalidateRect(hwnd, std::ptr::null(), 0);
                0
            }
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                if dc != 0 {
                    paint(hwnd, dc);
                }
                EndPaint(hwnd, &ps);
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                WINDOW.store(0, Ordering::SeqCst);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe fn paint(hwnd: HWND, dc: isize) {
        let mut r = std::mem::zeroed();
        windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut r);
        let (w, h) = ((r.right - r.left).max(1), (r.bottom - r.top).max(1));
        let guard = PAINT.lock().unwrap_or_else(|p| p.into_inner());
        let Some(p) = guard.as_ref() else {
            return;
        };
        let phase = p.started.elapsed().as_secs_f32() / 1.6;
        let pixels = compose(w as usize, h as usize, phase, p.icon.as_ref());
        let mem = CreateCompatibleDC(dc);
        if mem == 0 {
            return;
        }
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            // Negative: the top row first.
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, 0, 0);
        if bmp == 0 || bits.is_null() {
            DeleteDC(mem);
            return;
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), pixels.len());
        let old_bmp = SelectObject(mem, bmp);
        SetBkMode(mem, TRANSPARENT as i32);
        let l = layout(w as usize, h as usize);
        let s = l.s;
        let face = wide("Segoe UI");
        let font = |size: f32, bold: bool| {
            CreateFontW(
                -(size * s).round() as i32,
                0,
                0,
                0,
                (if bold { FW_BOLD } else { FW_NORMAL }) as i32,
                0,
                0,
                0,
                u32::from(DEFAULT_CHARSET),
                u32::from(OUT_DEFAULT_PRECIS),
                u32::from(CLIP_DEFAULT_PRECIS),
                u32::from(CLEARTYPE_QUALITY),
                u32::from(DEFAULT_PITCH),
                face.as_ptr(),
            )
        };
        let text = |f: isize, x: f32, y: f32, t: &str, c: Color32| -> i32 {
            let old = SelectObject(mem, f);
            SetTextColor(mem, colorref(c));
            let wt: Vec<u16> = t.encode_utf16().collect();
            TextOutW(mem, x.round() as i32, y.round() as i32, wt.as_ptr(), wt.len() as i32);
            let mut size = SIZE { cx: 0, cy: 0 };
            GetTextExtentPoint32W(mem, wt.as_ptr(), wt.len() as i32, &mut size);
            SelectObject(mem, old);
            size.cx
        };
        use crate::ui::brand;
        let (ix, iy, side) = l.icon;
        let left = ix + side + 24.0 * s;
        let name_font = font(46.0, true);
        let tag_font = font(17.0, false);
        let status_font = font(13.0, false);
        let small_font = font(11.5, false);
        // The name in two colours over the tagline, centred on the icon.
        let name_h = 46.0 * s * 1.25;
        let tag_h = 17.0 * s * 1.3;
        let mut y = iy + side / 2.0 - (name_h + tag_h) / 2.0;
        let (first, second) = p.name.split_at(5.min(p.name.len()));
        let wx = text(name_font, left, y, first, Color32::WHITE);
        text(name_font, left + wx as f32, y, second, brand::NAME_ACCENT);
        y += name_h;
        text(tag_font, left + 2.0 * s, y, &p.tagline, brand::TAGLINE);
        let status = crate::i18n::tr(Status::from_code(STATUS.load(Ordering::SeqCst)).key());
        text(
            status_font,
            l.pad,
            l.bar_y - 8.0 * s - 13.0 * s * 1.35,
            &status,
            brand::TAGLINE,
        );
        // The version at the right end of the footer.
        let vw = {
            let old = SelectObject(mem, small_font);
            let wt: Vec<u16> = p.version.encode_utf16().collect();
            let mut size = SIZE { cx: 0, cy: 0 };
            GetTextExtentPoint32W(mem, wt.as_ptr(), wt.len() as i32, &mut size);
            SelectObject(mem, old);
            size.cx as f32
        };
        text(
            small_font,
            w as f32 - l.pad - vw,
            l.foot_y,
            &p.version,
            brand::FOOTNOTE,
        );
        BitBlt(dc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
        for f in [name_font, tag_font, status_font, small_font] {
            DeleteObject(f);
        }
        SelectObject(mem, old_bmp);
        DeleteObject(bmp);
        DeleteDC(mem);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picture fills the window with the brand gradient, the icon in
    /// its box and the moving part of the bar where `phase` puts it.
    #[test]
    fn the_picture_has_the_gradient_icon_and_bar() {
        let (w, h) = (640, 380);
        let icon = icon_image().expect("the icon decodes");
        let px = compose(w, h, 0.5, Some(&icon));
        assert_eq!(px.len(), w * h);
        let l = layout(w, h);
        // Top left is the gradient's light blue, the bottom darker.
        let top = px[w * 2 + 2];
        let bottom = px[(h - 3) * w + w / 2];
        let lum = |p: u32| ((p >> 16) & 0xff) + ((p >> 8) & 0xff) + (p & 0xff);
        assert!(lum(top) > lum(bottom), "{top:06x} {bottom:06x}");
        // The icon changes the pixels in its box.
        let (ix, iy, side) = l.icon;
        let c = ((iy + side / 2.0) as usize) * w + (ix + side / 2.0) as usize;
        let plain = compose(w, h, 0.5, None);
        assert_ne!(px[c], plain[c]);
        // The moving part of the bar is somewhere along the bar's row.
        let row = l.bar_y as usize * w;
        let bar = rgb(crate::ui::brand::CURVE);
        assert!(px[row..row + w].contains(&bar));
        // And moves with the phase.
        let later = compose(w, h, 0.9, None);
        assert_ne!(plain[row..row + w], later[row..row + w]);
    }

    /// The system splash shows unless `--no-splash`; with the splash off
    /// in Options only in a start that replaces a renderer that failed.
    #[test]
    fn when_the_system_splash_shows() {
        assert!(wanted(false, true, false));
        assert!(wanted(false, false, true));
        assert!(!wanted(false, false, false));
        assert!(!wanted(true, true, true));
        assert!(retrying(true));
    }
}
