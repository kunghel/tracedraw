//! A synthetic, bounded-size regression for the live memory of band
//! assembly. Run this binary alone: its allocator measures all threads.
#![cfg(not(target_arch = "wasm32"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tracedraw_core::{geometry, Color, Document, Fill, Shape, ShapeId, ShapeKind};
use tracedraw_render::{render_page_progressive, RenderOptions, ViewTransform};

struct Measured;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn added(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

// Delegate allocations unchanged; only successful allocations count.
unsafe impl GlobalAlloc for Measured {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            added(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            added(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, size) };
        if !new.is_null() {
            if size >= layout.size() {
                added(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        new
    }
}

#[global_allocator]
static ALLOCATOR: Measured = Measured;

fn start_measurement() -> usize {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    baseline
}

#[test]
fn progressive_assembly_does_not_retain_a_second_picture() {
    if std::thread::available_parallelism().map_or(1, |n| n.get()) < 2 {
        eprintln!("two CPUs are needed to exercise progressive bands");
        return;
    }
    let mut doc = Document::new("synthetic", geometry::Size::new(100.0, 100.0));
    let page = doc.pages[0].id;
    let mut shape = Shape::new(
        ShapeId(1),
        ShapeKind::Rect {
            rect: geometry::Rect::new(0.0, 0.0, 100.0, 100.0),
            radius: 0.0,
            corners: None,
        },
    );
    shape.fill = Fill::Solid(Color::rgb8(32, 96, 160));
    shape.stroke = None;
    doc.pages[0].layers[0].shapes.push(shape);
    let opts = RenderOptions {
        width: 4096,
        height: 4096,
        view: ViewTransform {
            zoom: 40.96,
            origin_x: 0.0,
            origin_y: 4096.0,
            stretch_y: 1.0,
        },
        anti_alias: false,
        ..RenderOptions::default()
    };
    let bytes = 4096 * 4096 * 4;
    let cancel = AtomicBool::new(false);
    let rows_seen = AtomicUsize::new(0);
    let baseline = start_measurement();
    let started = std::time::Instant::now();
    let image = render_page_progressive(&doc, page, &opts, &cancel, 2048, &|_, rows| {
        assert!(rows.data().chunks_exact(4).all(|p| p == [32, 96, 160, 255]));
        rows_seen.fetch_add(rows.height() as usize, Ordering::Relaxed);
        // A slow consumer must not cause finished pixmaps to accumulate.
        std::thread::sleep(std::time::Duration::from_millis(2));
    })
    .expect("the synthetic page renders");
    let elapsed = started.elapsed();
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    assert_eq!(rows_seen.load(Ordering::Relaxed), 4096);
    assert!(image
        .data()
        .chunks_exact(4)
        .all(|p| p == [32, 96, 160, 255]));
    eprintln!("progressive assembly: {peak} live bytes above baseline, {elapsed:?}");
    // One output plus the current worker bands, not two whole pictures.
    // Leave room for worker bookkeeping and allocator-independent overhead.
    assert!(peak < bytes * 7 / 4 + (4 << 20), "peak {peak}");
    drop(image);

    cancel.store(true, Ordering::Relaxed);
    let baseline = start_measurement();
    assert!(
        render_page_progressive(&doc, page, &opts, &cancel, 2048, &|_, _| {
            panic!("a cancelled render must not publish bands")
        })
        .is_none()
    );
    assert!(PEAK.load(Ordering::Relaxed).saturating_sub(baseline) < (1 << 20));

    // Cancellation from a callback must also return without a final image.
    cancel.store(false, Ordering::Relaxed);
    assert!(
        render_page_progressive(&doc, page, &opts, &cancel, 2048, &|_, _| {
            cancel.store(true, Ordering::Relaxed);
        })
        .is_none()
    );
    assert!(LIVE.load(Ordering::Relaxed).saturating_sub(baseline) < (1 << 20));
}
