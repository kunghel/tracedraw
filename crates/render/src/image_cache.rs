//! Decoded pictures, kept between draws: decoding a large PNG takes far
//! longer than drawing it, and a page draws its pictures again on every
//! change of the view. Pictures are found by a fingerprint of their PNG
//! bytes; past a memory budget the least recently drawn go first, and
//! the budget floats with the machine: what its free memory and page
//! file afford, capped at a share of its memory, read again at every
//! decode.
//!
//! One cache serves every thread: the canvas draws in bands on threads
//! of their own, and a cache per thread made every band decode every
//! picture it crosses again, on every render. A picture being decoded
//! is decoded once; other threads that need it wait for that result.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};
use tiny_skia::Pixmap;

/// Bytes of decoded pixels kept for the whole program where the free
/// memory of the machine cannot be read (the browser, Android, other
/// systems).
#[cfg(target_arch = "wasm32")]
const BUDGET: usize = 256 << 20;
#[cfg(not(target_arch = "wasm32"))]
const BUDGET: usize = 512 << 20;

/// The floating budget never falls below this, so a nearly full
/// machine still keeps a working set of pictures.
const MIN_BUDGET: usize = 64 << 20;

/// The ceiling for one budget: a quarter of the memory of the machine,
/// but never past a quarter of the address space (a 32-bit process
/// cannot use the rest), and never below the least a working cache
/// needs. `MIN_BUDGET` is far below a quarter of even a 32-bit address
/// space, so the clamp cannot see its ends cross.
fn max_budget(total_physical: usize) -> usize {
    (total_physical / 4).clamp(MIN_BUDGET, usize::MAX / 4)
}

/// The cache budget from the free memory of the machine: half of the
/// scarcer of the two free amounts, counting back `held`, clamped to
/// sane limits. Half, because the other half belongs to the document
/// and everything else that runs; `held` counts back what the cache
/// already holds, whose bytes were free before they were decoded, so
/// a full cache does not shrink its own budget and the budget does
/// not chase its own tail. The cap comes from [`max_budget`], which
/// never goes below `MIN_BUDGET`.
fn budget_from(free_physical: usize, free_commit: usize, held: usize, cap: usize) -> usize {
    let afford = (free_physical.saturating_add(held) / 2).min(free_commit.saturating_add(held) / 2);
    afford.clamp(MIN_BUDGET, cap)
}

/// The budget right now: what the machine affords, or the fixed
/// default where free memory cannot be read. Read per decode, never
/// while the cache lock is held: reading the memory of the machine
/// can block, and the lock is there to be held briefly.
fn budget() -> usize {
    match system_budget_recent() {
        Some((phys, commit, total)) => budget_from(phys, commit, cache().bytes, max_budget(total)),
        None => BUDGET,
    }
}

/// [`system_budget`], read at most once a second: every draw of every
/// picture asks for the budget, cache hits included, and reading the
/// memory of the machine (a file read on Linux) for each of them cost
/// more than the draw of a picture already decoded.
#[cfg(target_arch = "wasm32")]
fn system_budget_recent() -> Option<(usize, usize, usize)> {
    // No clock in the browser, and nothing to read there anyway.
    system_budget()
}

/// See the browser's twin above.
#[cfg(not(target_arch = "wasm32"))]
fn system_budget_recent() -> Option<(usize, usize, usize)> {
    type Read = Option<(std::time::Instant, Option<(usize, usize, usize)>)>;
    static LAST: Mutex<Read> = Mutex::new(None);
    let now = std::time::Instant::now();
    if let Ok(last) = LAST.lock() {
        if let Some((at, read)) = *last {
            if now.duration_since(at) < std::time::Duration::from_secs(1) {
                return read;
            }
        }
    }
    // Read outside the lock: it can block.
    let read = system_budget();
    if let Ok(mut last) = LAST.lock() {
        *last = Some((now, read));
    }
    read
}

/// The memory of the machine the cache may take, in bytes: the free
/// physical memory, what the commit limit still has room for (physical
/// plus what the page file may still take), and the memory of the
/// whole machine. `None` where it cannot be read.
#[cfg(target_os = "windows")]
fn system_budget() -> Option<(usize, usize, usize)> {
    use std::mem::size_of;

    /// The `MEMORYSTATUSEX` record of the Windows API.
    #[repr(C)]
    struct Status {
        length: u32,
        load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page: u64,
        avail_page: u64,
        total_virt: u64,
        avail_virt: u64,
        avail_ext: u64,
    }

    #[cfg_attr(
        target_arch = "x86",
        link(
            name = "kernel32",
            kind = "raw-dylib",
            import_name_type = "undecorated"
        )
    )]
    #[cfg_attr(not(target_arch = "x86"), link(name = "kernel32", kind = "raw-dylib"))]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(status: *mut Status) -> i32;
    }

    let mut st = Status {
        length: size_of::<Status>() as u32,
        load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page: 0,
        avail_page: 0,
        total_virt: 0,
        avail_virt: 0,
        avail_ext: 0,
    };
    // SAFETY: `st` is a `Status` of the length its first field
    // declares, and nothing else touches it during the call.
    if unsafe { GlobalMemoryStatusEx(&mut st) } == 0 {
        return None;
    }
    let bytes = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
    Some((
        bytes(st.avail_phys),
        bytes(st.avail_page),
        bytes(st.total_phys),
    ))
}

/// The free memory of Linux from `/proc/meminfo`: `MemAvailable` (what
/// can be taken without swapping) stands for the free physical
/// memory, that plus the free swap for what the commit limit still
/// has room for, and `MemTotal` for the memory of the whole machine.
#[cfg(target_os = "linux")]
fn system_budget() -> Option<(usize, usize, usize)> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kilobytes = |name: &str| -> Option<usize> {
        info.lines().find_map(|line| {
            let rest = line.strip_prefix(name)?.strip_prefix(':')?;
            rest.trim().strip_suffix(" kB")?.parse::<usize>().ok()
        })
    };
    let total = kilobytes("MemTotal")?;
    let phys = kilobytes("MemAvailable")?;
    let swap = kilobytes("SwapFree").unwrap_or(0);
    let phys = phys.saturating_mul(1024);
    Some((
        phys,
        phys.saturating_add(swap.saturating_mul(1024)),
        total.saturating_mul(1024),
    ))
}

/// Free memory is not read on macOS, Android and other systems: they
/// get the fixed default.
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn system_budget() -> Option<(usize, usize, usize)> {
    None
}

/// A picture, or why there is none, once decoded.
type Slot = Arc<OnceLock<Option<Arc<Pixmap>>>>;

struct Entry {
    slot: Slot,
    /// Decoded bytes counted in the budget; 0 while being decoded.
    size: usize,
    last_use: u64,
}

#[derive(Default)]
struct Cache {
    map: HashMap<u64, Entry>,
    bytes: usize,
    tick: u64,
}

impl Cache {
    /// The slot of `key`, a new empty one when it has none.
    fn slot(&mut self, key: u64) -> Slot {
        self.tick += 1;
        let tick = self.tick;
        let e = self.map.entry(key).or_insert_with(|| Entry {
            slot: Slot::default(),
            size: 0,
            last_use: tick,
        });
        e.last_use = tick;
        e.slot.clone()
    }

    /// Count the decoded picture of `slot` under `key`, dropping the least
    /// recently used pictures until the cache fits `budget`; a picture
    /// larger than the whole budget is not kept.
    fn decoded(&mut self, key: u64, slot: &Slot, budget: usize) {
        let Some(e) = self.map.get_mut(&key) else {
            return;
        };
        if !Arc::ptr_eq(&e.slot, slot) || e.size != 0 {
            return;
        }
        let size = match slot.get() {
            Some(Some(img)) => img.data().len(),
            _ => return,
        };
        if size > budget {
            self.map.remove(&key);
            return;
        }
        e.size = size;
        self.bytes += size;
        while self.bytes > budget {
            let oldest = self
                .map
                .iter()
                .filter(|(_, e)| e.size > 0)
                .min_by_key(|(_, e)| e.last_use)
                .map(|(&k, _)| k);
            let Some(gone) = oldest.and_then(|k| self.map.remove(&k)) else {
                break;
            };
            self.bytes -= gone.size;
        }
    }
}

fn cache() -> std::sync::MutexGuard<'static, Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE
        .get_or_init(|| Mutex::new(Cache::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// A fingerprint of `png`: its length and bytes taken from its start, its
/// end (the last chunks and their checksums) and evenly in between.
pub(crate) fn fingerprint(png: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let n = png.len();
    n.hash(&mut h);
    let mut take = |at: usize, len: usize| {
        png.get(at.min(n)..at.saturating_add(len).min(n))
            .hash(&mut h);
    };
    take(0, 64);
    take(n.saturating_sub(64), 64);
    for i in 1..32 {
        take(n / 32 * i, 32);
    }
    h.finish()
}

/// The picture of `png`, decoded once and then kept.
pub(crate) fn decode(png: &[u8]) -> Option<Arc<Pixmap>> {
    decode_in(png, budget())
}

fn decode_in(png: &[u8], budget: usize) -> Option<Arc<Pixmap>> {
    let key = fingerprint(png);
    made_once(key, budget, || {
        #[cfg(test)]
        count_decode(key);
        Pixmap::decode_png(png).ok().map(Arc::new)
    })
}

/// The picture under `key`, made by `make` once and then kept (other
/// threads asking meanwhile wait for that result).
fn made_once(
    key: u64,
    budget: usize,
    make: impl FnOnce() -> Option<Arc<Pixmap>>,
) -> Option<Arc<Pixmap>> {
    let slot = cache().slot(key);
    if let Some(done) = slot.get() {
        return done.clone();
    }
    // Made outside the lock; a thread that asks meanwhile waits here.
    let img = slot.get_or_init(make).clone();
    cache().decoded(key, &slot, budget);
    img
}

/// Deepest reduced level kept: 1/4096 of the picture's sides.
pub(crate) const MAX_LEVEL: u32 = 12;

/// The picture of `png` reduced `level` times to half its width and
/// height (each pixel the average of the four beneath it; level 0 is the
/// picture itself), made once and kept in the same budget as the
/// decoded pictures. A picture drawn much smaller than its pixels is
/// drawn from the level nearest its size: sampling the full picture
/// there picks a few scattered pixels, so it looked noisy and broken
/// up, and it reads far more memory per drawn pixel (#853). Levels a
/// view no longer uses go first when memory runs short, the full
/// picture too.
pub(crate) fn decode_level(png: &[u8], level: u32) -> Option<Arc<Pixmap>> {
    decode_level_in(png, level, budget())
}

/// `decode_level` with the budget read once for the whole chain of
/// levels, so one picture reads the machine one time, not one time
/// per level.
fn decode_level_in(png: &[u8], level: u32, budget: usize) -> Option<Arc<Pixmap>> {
    let level = level.min(MAX_LEVEL);
    if level == 0 {
        return decode_in(png, budget);
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (fingerprint(png), level).hash(&mut h);
    made_once(h.finish(), budget, || {
        let above = decode_level_in(png, level - 1, budget)?;
        if above.width() <= 1 && above.height() <= 1 {
            return Some(above);
        }
        halve(&above).map(Arc::new)
    })
}

/// `src` at half its width and height (rounded up), each pixel the
/// average of the (up to) four beneath it. Premultiplied colours average
/// to premultiplied colours.
pub(crate) fn halve(src: &Pixmap) -> Option<Pixmap> {
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let (w, h) = (sw.div_ceil(2).max(1), sh.div_ceil(2).max(1));
    let mut out = Pixmap::new(w as u32, h as u32)?;
    let from = src.data();
    let to = out.data_mut();
    for y in 0..h {
        let ys = [2 * y, (2 * y + 1).min(sh - 1)];
        for x in 0..w {
            let xs = [2 * x, (2 * x + 1).min(sw - 1)];
            let mut sum = [0u32; 4];
            for yy in ys {
                for xx in xs {
                    let i = (yy * sw + xx) * 4;
                    if let Some(px) = from.get(i..i + 4) {
                        for (s, v) in sum.iter_mut().zip(px) {
                            *s += *v as u32;
                        }
                    }
                }
            }
            let o = (y * w + x) * 4;
            if let Some(px) = to.get_mut(o..o + 4) {
                for (d, s) in px.iter_mut().zip(sum) {
                    *d = ((s + 2) / 4) as u8;
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
static DECODES: Mutex<Option<HashMap<u64, usize>>> = Mutex::new(None);

#[cfg(test)]
fn count_decode(key: u64) {
    if let Ok(mut d) = DECODES.lock() {
        *d.get_or_insert_with(HashMap::new).entry(key).or_default() += 1;
    }
}

/// How often the picture of `png` was decoded, for tests.
#[cfg(test)]
pub(crate) fn decodes_of(png: &[u8]) -> usize {
    DECODES
        .lock()
        .ok()
        .and_then(|d| d.as_ref()?.get(&fingerprint(png)).copied())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, shade: u8) -> Vec<u8> {
        let mut p = Pixmap::new(w, h).expect("pixmap");
        p.fill(tiny_skia::Color::from_rgba8(shade, 0, 0, 255));
        p.encode_png().expect("png")
    }

    #[test]
    fn a_picture_is_decoded_once() {
        let a = png(4, 4, 10);
        let b = png(4, 4, 200);
        let first = decode(&a).expect("decoded");
        let again = decode(&a).expect("decoded");
        assert!(Arc::ptr_eq(&first, &again));
        let other = decode(&b).expect("decoded");
        assert!(!Arc::ptr_eq(&first, &other));
        assert_eq!(other.pixel(0, 0).map(|p| p.red()), Some(200));
        assert!(decode(b"not a png").is_none());
        assert!(decode(b"not a png").is_none());
        assert_ne!(fingerprint(&a), fingerprint(&b));
        assert_ne!(fingerprint(&[]), fingerprint(&[0]));
    }

    /// Threads asking for one picture at once share a single decode.
    #[test]
    fn threads_share_one_decode() {
        let a = png(64, 64, 77);
        let all: Vec<Arc<Pixmap>> = std::thread::scope(|sc| {
            let hs: Vec<_> = (0..8).map(|_| sc.spawn(|| decode(&a))).collect();
            hs.into_iter()
                .filter_map(|h| h.join().ok().flatten())
                .collect()
        });
        assert_eq!(all.len(), 8);
        assert_eq!(decodes_of(&a), 1);
        assert!(all.iter().all(|p| Arc::ptr_eq(p, &all[0])));
    }

    /// Each level halves the sides (rounded up) and averages the pixels:
    /// a one-pixel checkerboard turns an even grey.
    #[test]
    fn reduced_levels_average_the_pixels() {
        let mut p = Pixmap::new(9, 4).expect("pixmap");
        for (i, px) in p.data_mut().chunks_mut(4).enumerate() {
            let (x, y) = (i % 9, i / 9);
            let v = if (x + y) % 2 == 0 { 255 } else { 0 };
            px.copy_from_slice(&[v, v, v, 255]);
        }
        let png = p.encode_png().expect("png");
        let one = decode_level(&png, 1).expect("level 1");
        assert_eq!((one.width(), one.height()), (5, 2));
        let c = one.pixel(1, 1).expect("pixel");
        assert!((126..=129).contains(&c.red()), "{c:?}");
        assert_eq!(c.alpha(), 255);
        let deep = decode_level(&png, 40).expect("deepest");
        assert_eq!((deep.width(), deep.height()), (1, 1));
        assert!(Arc::ptr_eq(&one, &decode_level(&png, 1).expect("kept")));
        assert!(Arc::ptr_eq(
            &decode(&png).expect("decoded"),
            &decode_level(&png, 0).expect("level 0")
        ));
    }

    #[test]
    fn the_oldest_pictures_go_past_the_budget() {
        let mut c = Cache::default();
        let put = |c: &mut Cache, key, shade| {
            let slot = c.slot(key);
            let img = Pixmap::decode_png(&png(8, 8, shade)).expect("png");
            let _ = slot.set(Some(Arc::new(img)));
            c.decoded(key, &slot, 600);
        };
        let has = |c: &mut Cache, key| c.map.get(&key).is_some_and(|e| e.size > 0);
        // 256 bytes each, a budget of 600: two fit.
        put(&mut c, 1, 1);
        put(&mut c, 2, 2);
        c.slot(1); // 1 is now newer than 2
        put(&mut c, 3, 3);
        assert!(!has(&mut c, 2));
        assert!(has(&mut c, 1) && has(&mut c, 3));
        assert_eq!(c.bytes, 512);
        // Counted once however often it is reported.
        let slot = c.slot(3);
        c.decoded(3, &slot, 600);
        assert_eq!(c.bytes, 512);
        // Too large for the budget: not kept.
        let slot = c.slot(4);
        let _ = slot.set(Some(Arc::new(
            Pixmap::decode_png(&png(16, 16, 4)).expect("png"),
        )));
        c.decoded(4, &slot, 600);
        assert!(!c.map.contains_key(&4));
        assert_eq!(c.bytes, 512);
    }

    /// Half of the scarcer of the two free amounts, with the bytes the
    /// cache holds counted back in, clamped at both ends.
    #[test]
    fn the_budget_holds_half_the_free_memory() {
        let cap = max_budget(1 << 30); // 256 MiB on a 1 GiB machine
        assert_eq!(budget_from(200 << 20, 1000 << 20, 0, cap), 100 << 20);
        assert_eq!(budget_from(1000 << 20, 200 << 20, 0, cap), 100 << 20);
        // Bytes the cache holds already were free before they were
        // decoded: they are not punished.
        assert_eq!(budget_from(200 << 20, 200 << 20, 200 << 20, cap), 200 << 20);
        // Clamped at both ends.
        assert_eq!(budget_from(0, 0, 0, cap), MIN_BUDGET);
        // A nearly empty machine keeps the minimum: half of almost
        // nothing falls below it.
        assert_eq!(budget_from(2 << 20, 10 << 20, 0, cap), MIN_BUDGET);
        // The ceiling binds: free memory past a quarter of the machine
        // is left alone.
        assert_eq!(budget_from(usize::MAX, usize::MAX, 0, cap), cap);
    }

    /// The ceiling: a quarter of the memory of the machine, capped by
    /// the address space on a 32-bit process, never below the least a
    /// working cache needs.
    #[test]
    fn the_ceiling_is_a_quarter_of_the_machine() {
        assert_eq!(max_budget(1 << 30), 256 << 20);
        assert_eq!(max_budget(512 << 20), 128 << 20);
        // A small machine keeps the minimum a working cache needs.
        assert_eq!(max_budget(64 << 20), MIN_BUDGET);
        assert_eq!(max_budget(0), MIN_BUDGET);
        // The address space caps a 32-bit process as much as its
        // memory does.
        assert_eq!(max_budget(usize::MAX), usize::MAX / 4);
    }

    /// On this machine the budget stays sane: never below the minimum
    /// (the fixed default where free memory is not read is above it
    /// too).
    #[test]
    fn the_floating_budget_stays_sane() {
        let b = budget();
        assert!(b >= MIN_BUDGET, "{b}");
    }

    /// A picture that fits the whole budget is kept, even when it takes
    /// most of it: it used to be dropped at half the budget and decoded
    /// again on every draw, so a large picture on a small machine never
    /// stayed decoded. The oldest still go to make room.
    #[test]
    fn a_picture_up_to_the_budget_is_kept() {
        let mut c = Cache::default();
        let put = |c: &mut Cache, key, size: (u32, u32), shade| {
            let slot = c.slot(key);
            let img = Pixmap::decode_png(&png(size.0, size.1, shade)).expect("png");
            let _ = slot.set(Some(Arc::new(img)));
            c.decoded(key, &slot, 600);
        };
        let has = |c: &mut Cache, key| c.map.get(&key).is_some_and(|e| e.size > 0);
        // 256 bytes each, a budget of 600: both fit.
        put(&mut c, 1, (8, 8), 1);
        put(&mut c, 2, (8, 8), 2);
        assert_eq!(c.bytes, 512);
        // A 512-byte picture takes most of the budget: it is kept, and
        // the two older ones go to make room.
        put(&mut c, 3, (16, 8), 3);
        assert!(has(&mut c, 3));
        assert!(!has(&mut c, 1) && !has(&mut c, 2));
        assert_eq!(c.bytes, 512);
    }
}
