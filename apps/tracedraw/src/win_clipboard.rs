//! Vector data on the Windows clipboard (#2514). A drawing program
//! copies its objects in a vector format beside a picture of them: SVG
//! under the registered name `image/svg+xml`, PDF under `Portable
//! Document Format` (also `PDF` or `application/pdf`), or an enhanced
//! metafile (`CF_ENHMETAFILE`). The clipboard library reads text, files
//! and the picture only, so the paste came in as a bitmap; this reads the
//! first of those formats the clipboard offers, in that order.
//!
//! Pictures (#2349): PNG under `PNG` or `image/png`, else the
//! device-independent bitmap (`CF_DIBV5`, `CF_DIB`), read as a BMP file,
//! which takes the bitmaps of every depth and layout a paint program may
//! copy (the clipboard library reads 32-bit ones of one layout only).
//!
//! CorelDRAW (#1539): its own copy data, when a format whose registered
//! name mentions Corel, CMX or CDR holds a CMX or CDR file, is read
//! before the formats above, so a paste keeps what those files keep
//! (text as text with its font, colours, outlines, groups). A copy here
//! puts the selection on the clipboard as an enhanced metafile, a PDF and
//! SVG beside the SVG text ([`write`]), the vector formats CorelDRAW and
//! office programs paste as objects, where only the SVG text went before.

use std::ffi::c_void;

// 32-bit Windows: the DLLs export the plain names, not the stdcall
// decorated ones raw-dylib imports by default (#1975).
#[cfg_attr(
    target_arch = "x86",
    link(name = "user32", kind = "raw-dylib", import_name_type = "undecorated")
)]
#[cfg_attr(not(target_arch = "x86"), link(name = "user32", kind = "raw-dylib"))]
unsafe extern "system" {
    fn OpenClipboard(owner: *mut c_void) -> i32;
    fn CloseClipboard() -> i32;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn GetClipboardData(format: u32) -> *mut c_void;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
    fn EnumClipboardFormats(format: u32) -> u32;
    fn GetClipboardFormatNameW(format: u32, name: *mut u16, max: i32) -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, mem: *mut c_void) -> *mut c_void;
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
    fn GlobalLock(mem: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(mem: *mut c_void) -> i32;
    fn GlobalSize(mem: *mut c_void) -> usize;
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
    fn GlobalFree(mem: *mut c_void) -> *mut c_void;
}

#[cfg_attr(
    target_arch = "x86",
    link(name = "gdi32", kind = "raw-dylib", import_name_type = "undecorated")
)]
#[cfg_attr(not(target_arch = "x86"), link(name = "gdi32", kind = "raw-dylib"))]
unsafe extern "system" {
    fn GetEnhMetaFileBits(emf: *mut c_void, size: u32, data: *mut u8) -> u32;
    fn SetEnhMetaFileBits(size: u32, data: *const u8) -> *mut c_void;
    fn DeleteEnhMetaFile(emf: *mut c_void) -> i32;
}

/// The bitmap format numbers.
const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;

/// The enhanced metafile format number.
const CF_ENHMETAFILE: u32 = 14;

/// The UTF-16 text format number.
const CF_UNICODETEXT: u32 = 13;

/// Registered format numbers start here.
const FIRST_REGISTERED: u32 = 0xC000;

/// A memory block that can move, as the clipboard takes them.
const GMEM_MOVEABLE: u32 = 0x0002;

/// The registered names of the SVG and PDF formats, in the order a paste
/// prefers them.
const NAMED_FORMATS: [&str; 5] = [
    "image/svg+xml",
    "Portable Document Format",
    "PDF",
    "application/pdf",
    "image/svg",
];

/// The largest copy read (bytes): more is not a drawing's copy.
const MAX_BYTES: usize = 256 * 1024 * 1024;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The clipboard, opened for this thread until dropped.
struct Open;

impl Open {
    /// Another program may hold the clipboard for a moment: a few tries.
    fn new() -> Option<Open> {
        for attempt in 0..5 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // SAFETY: a null owner opens the clipboard for this thread.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return Some(Open);
            }
        }
        None
    }

    /// The bytes of a memory block format.
    fn global_bytes(&self, format: u32) -> Option<Vec<u8>> {
        // SAFETY: the clipboard is open; the handle stays valid until it
        // closes and is only read between GlobalLock and GlobalUnlock,
        // within the size GlobalSize gives.
        unsafe {
            if IsClipboardFormatAvailable(format) == 0 {
                return None;
            }
            let mem = GetClipboardData(format);
            if mem.is_null() {
                return None;
            }
            let size = GlobalSize(mem);
            if size == 0 || size > MAX_BYTES {
                return None;
            }
            let ptr = GlobalLock(mem) as *const u8;
            if ptr.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
            GlobalUnlock(mem);
            Some(bytes)
        }
    }

    /// The enhanced metafile's bytes.
    fn metafile_bytes(&self) -> Option<Vec<u8>> {
        // SAFETY: the clipboard is open and owns the metafile handle; it
        // is only read, first for its size, then into a buffer that size.
        unsafe {
            if IsClipboardFormatAvailable(CF_ENHMETAFILE) == 0 {
                return None;
            }
            let emf = GetClipboardData(CF_ENHMETAFILE);
            if emf.is_null() {
                return None;
            }
            let size = GetEnhMetaFileBits(emf, 0, std::ptr::null_mut()) as usize;
            if size == 0 || size > MAX_BYTES {
                return None;
            }
            let mut bytes = vec![0u8; size];
            let got = GetEnhMetaFileBits(emf, size as u32, bytes.as_mut_ptr()) as usize;
            (got == size).then_some(bytes)
        }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: this thread opened the clipboard.
        unsafe {
            CloseClipboard();
        }
    }
}

/// A memory block may be larger than what was put in it: a PDF ends at
/// its last `%%EOF` line, an SVG text at its first NUL.
fn trimmed(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.starts_with(b"%PDF") {
        if let Some(i) = bytes.windows(5).rposition(|w| w == b"%%EOF") {
            let mut end = i + 5;
            while end < bytes.len() && matches!(bytes[end], b'\r' | b'\n') {
                end += 1;
            }
            bytes.truncate(end);
        }
    } else if let Some(i) = bytes.iter().position(|b| *b == 0) {
        bytes.truncate(i);
    }
    bytes
}

/// The registered name of clipboard format `format`.
fn format_name(format: u32) -> Option<String> {
    let mut buf = [0u16; 256];
    // SAFETY: the buffer holds `max` characters; the call writes at most
    // that many and gives how many it wrote.
    let n = unsafe { GetClipboardFormatNameW(format, buf.as_mut_ptr(), buf.len() as i32) };
    (n > 0).then(|| String::from_utf16_lossy(&buf[..n as usize]))
}

/// CorelDRAW's own copy data (#1539): the CMX or CDR file a format named
/// for Corel, CMX or CDR holds; `None` when no such format holds one.
fn read_corel(open: &Open) -> Option<Vec<u8>> {
    let mut format = 0;
    let mut found = Vec::new();
    loop {
        // SAFETY: the clipboard is open; 0 starts the list and 0 ends it.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if format >= FIRST_REGISTERED
            && format_name(format).is_some_and(|n| crate::clipboard::is_corel_format_name(&n))
        {
            found.push(format);
        }
    }
    found
        .into_iter()
        .find_map(|f| open.global_bytes(f).and_then(crate::clipboard::corel_file))
}

/// CorelDRAW's CMX or CDR copy data, else SVG, PDF or enhanced metafile
/// bytes on the clipboard, the first offered in that order; `None` when
/// it offers none of them.
pub fn read_vector() -> Option<Vec<u8>> {
    let open = Open::new()?;
    if let Some(bytes) = read_corel(&open) {
        return Some(bytes);
    }
    for name in NAMED_FORMATS {
        let w = wide(name);
        // SAFETY: a NUL-terminated name; registering an existing name
        // gives its number.
        let format = unsafe { RegisterClipboardFormatW(w.as_ptr()) };
        if format == 0 {
            continue;
        }
        if let Some(bytes) = open.global_bytes(format).map(trimmed) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }
    open.metafile_bytes()
}

/// The picture on the clipboard as a picture file's bytes (PNG or BMP),
/// the best offered first (#2349); `None` when it offers none.
pub fn read_picture() -> Option<Vec<u8>> {
    read_picture_result().ok().flatten()
}

/// Unlike the best-effort reader, distinguish contention and unreadable data
/// from a clipboard that offers no picture, so background paste can retry.
pub fn read_picture_result() -> Result<Option<Vec<u8>>, String> {
    let open = Open::new().ok_or_else(|| "the system clipboard is busy".to_string())?;
    let mut offered = false;
    for name in ["PNG", "image/png"] {
        let w = wide(name);
        // SAFETY: a NUL-terminated name; registering an existing name
        // gives its number.
        let format = unsafe { RegisterClipboardFormatW(w.as_ptr()) };
        if format == 0 {
            continue;
        }
        // SAFETY: the clipboard is open and this only inspects its formats.
        offered |= unsafe { IsClipboardFormatAvailable(format) != 0 };
        if let Some(bytes) = open.global_bytes(format) {
            if bytes.starts_with(b"\x89PNG") {
                return Ok(Some(bytes));
            }
        }
    }
    for format in [CF_DIBV5, CF_DIB] {
        // SAFETY: the clipboard is open and this only inspects its formats.
        offered |= unsafe { IsClipboardFormatAvailable(format) != 0 };
        if let Some(bytes) = open
            .global_bytes(format)
            .and_then(|dib| crate::winit_picture_drop::bmp_from_dib(&dib))
        {
            return Ok(Some(bytes));
        }
    }
    if offered {
        Err("the clipboard picture is unreadable or exceeds the supported size".into())
    } else {
        Ok(None)
    }
}

/// What a copy puts on the clipboard (#1539).
pub enum Item {
    /// Under a registered name.
    Named(&'static str, Vec<u8>),
    /// UTF-16 text (`CF_UNICODETEXT`).
    Text(String),
    /// An enhanced metafile file's bytes (`CF_ENHMETAFILE`).
    Metafile(Vec<u8>),
}

/// A movable memory block holding `bytes`, as `SetClipboardData` takes.
fn global_with(bytes: &[u8]) -> Option<*mut c_void> {
    // SAFETY: the block is allocated `bytes.len()` long (one at least),
    // locked, filled within that length and unlocked; it is freed by the
    // caller when the clipboard does not take it.
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
        if mem.is_null() {
            return None;
        }
        let ptr = GlobalLock(mem) as *mut u8;
        if ptr.is_null() {
            GlobalFree(mem);
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        GlobalUnlock(mem);
        Some(mem)
    }
}

/// Replace what the clipboard holds with `items`, each in its format.
/// True when the clipboard took the text, or every item when there is
/// no text.
pub fn write(items: Vec<Item>) -> bool {
    let Some(_open) = Open::new() else {
        return false;
    };
    // SAFETY: the clipboard is open for this thread.
    if unsafe { EmptyClipboard() } == 0 {
        return false;
    }
    let mut all = true;
    let mut text = None;
    for item in items {
        let ok = match item {
            Item::Named(name, bytes) => {
                let w = wide(name);
                // SAFETY: a NUL-terminated name.
                let format = unsafe { RegisterClipboardFormatW(w.as_ptr()) };
                format != 0 && set_global(format, &bytes)
            }
            Item::Text(t) => {
                let units: Vec<u8> = t
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .flat_map(u16::to_le_bytes)
                    .collect();
                let ok = set_global(CF_UNICODETEXT, &units);
                text = Some(ok);
                ok
            }
            Item::Metafile(bytes) => set_metafile(&bytes),
        };
        all &= ok;
    }
    text.unwrap_or(all)
}

/// Put `bytes` on the open clipboard under `format`.
fn set_global(format: u32, bytes: &[u8]) -> bool {
    let Some(mem) = global_with(bytes) else {
        return false;
    };
    // SAFETY: the clipboard is open; it owns the block once it takes it,
    // else the block is freed here.
    unsafe {
        if SetClipboardData(format, mem).is_null() {
            GlobalFree(mem);
            return false;
        }
    }
    true
}

/// Put the enhanced metafile `bytes` on the open clipboard.
fn set_metafile(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > u32::MAX as usize {
        return false;
    }
    // SAFETY: the metafile is made from `bytes`, read within their
    // length; the clipboard owns it once it takes it, else it is deleted
    // here.
    unsafe {
        let emf = SetEnhMetaFileBits(bytes.len() as u32, bytes.as_ptr());
        if emf.is_null() {
            return false;
        }
        if SetClipboardData(CF_ENHMETAFILE, emf).is_null() {
            DeleteEnhMetaFile(emf);
            return false;
        }
    }
    true
}
