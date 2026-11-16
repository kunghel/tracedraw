//! TraceDraw: a picture dragged over a window from another program
//! (#2349).
//!
//! winit 0.30 takes a drag of files only: on Windows the files of a
//! `CF_HDROP`, on X11 and Wayland a `text/uri-list`. A picture dragged
//! out of a paint program's canvas (or a browser's page) comes as the
//! picture itself, as PNG or a device-independent bitmap, so the system
//! showed the drag as refused. The backends now take such a drag too:
//! the picture is written to a file of its own in the temporary folder
//! as the drag enters, and that file goes out as `HoveredFile` and
//! `DroppedFile` like a file from a file manager. TraceDraw imports it
//! as it imports any picture file, and leaves out the link to the file
//! (`is_dropped_picture`).
//!
//! This file depends on nothing in the crate, and TraceDraw includes it
//! (its Windows clipboard reads bitmaps with `bmp_from_dib`) and runs its
//! tests (`winit_picture_drop::tests`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// The folder in the temporary folder that holds dropped pictures.
pub const FOLDER: &str = "tracedraw-dropped-pictures";

/// The picture types a drag may offer, the best first, with the
/// extension their file gets.
pub const PICTURE_TYPES: [(&str, &str); 8] = [
    ("image/png", "png"),
    ("PNG", "png"),
    ("image/bmp", "bmp"),
    ("image/x-bmp", "bmp"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/gif", "gif"),
    ("image/tiff", "tif"),
];

/// The best picture type among `offered` (MIME types, or registered
/// clipboard format names on Windows), with its extension.
pub fn best_type<'a>(
    offered: impl IntoIterator<Item = &'a str> + Clone,
) -> Option<(&'static str, &'static str)> {
    PICTURE_TYPES
        .iter()
        .find(|(t, _)| {
            offered
                .clone()
                .into_iter()
                .any(|o| o.eq_ignore_ascii_case(t))
        })
        .copied()
}

/// A BMP file of a device-independent bitmap (Windows' `CF_DIB` or
/// `CF_DIBV5`: the bitmap header, its colour masks or palette, then the
/// pixels): the 14-byte file header put in front, which says where the
/// pixels start. `None` when the header cannot be read.
pub fn bmp_from_dib(dib: &[u8]) -> Option<Vec<u8>> {
    let u32_at = |at: usize| -> Option<u32> {
        dib.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let u16_at = |at: usize| -> Option<u16> {
        dib.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    let header = u32_at(0)? as usize;
    // BITMAPCOREHEADER (12), BITMAPINFOHEADER (40), V2/V3 (52/56), V4
    // (108), V5 (124).
    if !matches!(header, 12 | 40 | 52 | 56 | 64 | 108 | 124) || dib.len() < header {
        return None;
    }
    let (bits, compression, used) = if header == 12 {
        (u16_at(10)?, 0, 0)
    } else {
        (u16_at(14)?, u32_at(16)?, u32_at(32)?)
    };
    // The three colour masks follow a plain info header when the pixels
    // are bit fields (BI_BITFIELDS 3; BI_ALPHABITFIELDS 6 has four).
    let masks = match (header, compression) {
        (40, 3) => 12,
        (40, 6) => 16,
        _ => 0,
    };
    let entry = if header == 12 { 3 } else { 4 };
    let palette = if used > 0 {
        used as usize
    } else if bits <= 8 {
        1usize << bits
    } else {
        0
    };
    let offset = 14 + header + masks + palette * entry;
    if offset > 14 + dib.len() {
        return None;
    }
    let size = u32::try_from(14 + dib.len()).ok()?;
    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

/// Whether `path` is a picture dropped from another program: a file this
/// module wrote.
pub fn is_dropped_picture(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|f| f == FOLDER)
}

/// Write `bytes`, a picture file of extension `ext`, to a file of its own
/// in the temporary folder; its path. `None` when it cannot be written.
pub fn save(bytes: &[u8], ext: &str) -> Option<PathBuf> {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    if bytes.is_empty() {
        return None;
    }
    let folder = std::env::temp_dir().join(FOLDER);
    std::fs::create_dir_all(&folder).ok()?;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = folder.join(format!("picture-{}-{n}.{ext}", std::process::id()));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// A `text/uri-list` naming the file `path`, for a backend whose file
/// drag reads such a list (Wayland, `crate::file_drop`).
pub fn uri_list(path: &Path) -> Vec<u8> {
    let mut out = b"file://".to_vec();
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            out.push(b);
        } else {
            out.extend_from_slice(format!("%{b:02X}").as_bytes());
        }
    }
    out.extend_from_slice(b"\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_best_offered_picture_type_wins() {
        assert_eq!(
            best_type(["text/plain", "image/jpeg", "image/png"]),
            Some(("image/png", "png"))
        );
        assert_eq!(best_type(["image/BMP"]), Some(("image/bmp", "bmp")));
        assert_eq!(best_type(["text/uri-list"]), None);
    }

    /// A 2x1 bitmap of 32 bits with an info header and its three masks,
    /// as Windows hands one over: the file header says the pixels start
    /// after the masks.
    #[test]
    fn a_dib_becomes_a_bmp_file() {
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&2i32.to_le_bytes());
        dib.extend_from_slice(&1i32.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&3u32.to_le_bytes());
        dib.extend_from_slice(&8u32.to_le_bytes());
        dib.extend_from_slice(&[0; 16]);
        for m in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff] {
            dib.extend_from_slice(&m.to_le_bytes());
        }
        dib.extend_from_slice(&[0, 0, 255, 0, 0, 255, 0, 0]);
        let bmp = bmp_from_dib(&dib).unwrap();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(
            u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize,
            bmp.len()
        );
        assert_eq!(
            u32::from_le_bytes(bmp[10..14].try_into().unwrap()),
            14 + 40 + 12
        );
        assert!(bmp_from_dib(&[1, 2, 3]).is_none());
    }

    #[test]
    fn a_saved_picture_is_known_as_dropped() {
        let path = save(b"\x89PNG", "png").unwrap();
        assert!(is_dropped_picture(&path));
        assert_eq!(path.extension().unwrap(), "png");
        assert!(!is_dropped_picture(Path::new("/home/me/picture.png")));
        assert_eq!(
            uri_list(Path::new("/tmp/a b.png")),
            b"file:///tmp/a%20b.png\r\n".to_vec()
        );
        let _ = std::fs::remove_file(path);
    }
}
