//! Recovering what can be read from a desktop publishing `.pmd` file
//! (#2822): the text it holds and the pictures stored inside it. The
//! layout of these files is not published and is not read here: where
//! each frame sits, the type, the shapes and the pages stay behind.
//!
//! What is read comes from the containers alone, which are public:
//! newer files are a compound document (the Microsoft Compound File
//! Binary format, [MS-CFB]) whose streams are read whole; older files
//! are read as they are. The text is the runs of readable Windows-1252
//! characters that look like writing: the program's own lists (the
//! names of the fonts, styles and colours a file keeps, stored one after
//! another at a fixed spacing or more than once) are left out. The
//! pictures are the JPEG, PNG, GIF, BMP and TIFF files found inside,
//! by their signatures; the caller decodes them and drops those that
//! do not decode. See docs/behavior/pmd-import.md.

/// The signature of a compound file.
const CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// The shortest text run kept, in characters.
const MIN_RUN: usize = 4;
/// The most text kept, in characters.
const MAX_TEXT: usize = 2_000_000;
/// The most pictures looked at.
const MAX_PICTURES: usize = 200;
/// Spacings between entries of a list the file keeps, bytes.
const LIST_STRIDE: std::ops::RangeInclusive<usize> = 8..=1024;

/// Words the files keep for themselves that are not in any list.
const OWN_WORDS: [&str; 6] = [
    "DISPLAY", "None", "Contents", "Index", "Default", "Untitled",
];

/// A picture found inside the file.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundPicture {
    /// "jpeg", "png", "gif", "bmp" or "tiff".
    pub format: &'static str,
    /// The picture file's bytes (a TIFF or GIF runs to the end of the
    /// stream it was found in: its decoder stops where it ends).
    pub bytes: Vec<u8>,
}

/// What was recovered.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recovered {
    /// The paragraphs of text, in the order they are stored.
    pub paragraphs: Vec<String>,
    pub pictures: Vec<FoundPicture>,
    /// Whether the file is a compound document (else read as it is).
    pub compound: bool,
}

impl Recovered {
    pub fn is_empty(&self) -> bool {
        self.paragraphs.is_empty() && self.pictures.is_empty()
    }
}

/// Recover the text and the pictures of a `.pmd` file.
pub fn read(bytes: &[u8]) -> Result<Recovered, String> {
    if bytes.is_empty() {
        return Err("empty file".into());
    }
    let (streams, compound) = if bytes.starts_with(&CFB_MAGIC) {
        (compound_streams(bytes)?, true)
    } else {
        (vec![bytes.to_vec()], false)
    };
    let mut out = Recovered {
        compound,
        ..Default::default()
    };
    let mut chars = 0usize;
    for s in &streams {
        let (found, text) = pictures(s);
        for p in paragraphs(&text) {
            if chars >= MAX_TEXT {
                break;
            }
            chars += p.chars().count();
            if !out.paragraphs.contains(&p) || p.chars().count() > 40 {
                out.paragraphs.push(p);
            }
        }
        for pic in found {
            if out.pictures.len() >= MAX_PICTURES {
                break;
            }
            if !out.pictures.iter().any(|q| q.bytes == pic.bytes) {
                out.pictures.push(pic);
            }
        }
    }
    Ok(out)
}

/// Every stream of a compound file, the largest first (the document's
/// own stream is far larger than the small ones beside it).
fn compound_streams(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    use std::io::Read;
    let mut file =
        cfb::CompoundFile::open(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let paths: Vec<std::path::PathBuf> = file
        .walk()
        .filter(|e| e.is_stream())
        .map(|e| e.path().to_path_buf())
        .collect();
    let mut out = Vec::new();
    for p in paths {
        let Ok(mut s) = file.open_stream(&p) else {
            continue;
        };
        let mut data = Vec::new();
        if s.read_to_end(&mut data).is_ok() && !data.is_empty() {
            out.push(data);
        }
    }
    out.sort_by_key(|d| std::cmp::Reverse(d.len()));
    Ok(out)
}

/// A byte that can be part of written text in Windows-1252: printable,
/// a tab or a paragraph end.
fn readable(b: u8) -> bool {
    (0x20..0x7F).contains(&b) || b >= 0xA0 || matches!(b, 0x09 | 0x0D | 0x0A | 0x92 | 0x93 | 0x94)
}

/// A run of readable bytes and where it starts.
struct Run {
    at: usize,
    text: String,
}

/// The runs of `data` that look like writing.
fn runs(data: &[u8]) -> Vec<Run> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if !readable(data[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < data.len() && readable(data[i]) {
            i += 1;
        }
        let text: String = data[start..i]
            .iter()
            .map(|&b| crate::text_import::cp1252(b))
            .collect();
        if looks_written(&text) {
            out.push(Run { at: start, text });
        }
    }
    out
}

/// Whether `s` reads as words rather than as numbers stored as bytes.
fn looks_written(s: &str) -> bool {
    let t = s.trim();
    let n = t.chars().count();
    if n < MIN_RUN {
        return false;
    }
    // Paths and file names the program records.
    if t.contains(":\\") || t.contains('\\') || t.starts_with('@') {
        return false;
    }
    // The same character over and over (filler bytes).
    let chars: Vec<char> = t.chars().collect();
    if chars.windows(4).any(|w| w.iter().all(|c| *c == w[0])) && !t.contains(' ') {
        return false;
    }
    let ascii_letters = chars.iter().filter(|c| c.is_ascii_alphabetic()).count();
    let letters = chars.iter().filter(|c| c.is_alphabetic()).count();
    let spaces = chars.iter().filter(|c| c.is_whitespace()).count();
    let other = n - letters - spaces;
    // Mostly letters, the plain ones most of all: random bytes give as
    // many accented letters and signs as plain ones.
    if letters * 10 < (n - spaces) * 6 || ascii_letters * 10 < letters * 6 {
        return false;
    }
    if other * 3 > n {
        return false;
    }
    // A word needs a vowel (any script beyond Latin passes).
    let has_vowel = chars
        .iter()
        .any(|c| "aeiouyAEIOUY".contains(*c) || (c.is_alphabetic() && !c.is_ascii()));
    has_vowel
}

/// The paragraphs of `data`: the runs that look like writing, less the
/// program's own lists and words.
fn paragraphs(data: &[u8]) -> Vec<String> {
    let runs = runs(data);
    // Lists hold names: only short runs count as their entries.
    let short = |r: &Run| r.text.trim().chars().count() <= 40;
    let offsets: std::collections::HashSet<usize> =
        runs.iter().filter(|r| short(r)).map(|r| r.at).collect();
    let mut count: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for r in &runs {
        *count.entry(r.text.trim()).or_default() += 1;
    }
    // An entry of a list: two more runs at the same spacing around it.
    let listed = |i: usize| -> bool {
        if !short(&runs[i]) {
            return false;
        }
        let at = runs[i].at;
        let near = runs
            .iter()
            .skip(i.saturating_sub(4))
            .take(9)
            .filter(|r| r.at != at)
            .map(|r| r.at.abs_diff(at))
            .filter(|d| LIST_STRIDE.contains(d));
        for d in near {
            let has = |o: Option<usize>| o.is_some_and(|o| offsets.contains(&o));
            let (back, fwd) = (at.checked_sub(d), at.checked_add(d));
            let (back2, fwd2) = (at.checked_sub(2 * d), at.checked_add(2 * d));
            if (has(back) && has(fwd)) || (has(fwd) && has(fwd2)) || (has(back) && has(back2)) {
                return true;
            }
        }
        false
    };
    let mut out = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        let t = r.text.trim();
        // Short names kept more than once are the program's (a style or
        // colour name, a colour system); text is long or stored once.
        if count[t] > 1 && t.chars().count() <= 40 {
            continue;
        }
        if OWN_WORDS.contains(&t) || listed(i) {
            continue;
        }
        for p in t.split(['\r', '\n']) {
            let p = p.trim_end();
            if !p.trim().is_empty() {
                out.push(p.to_string());
            }
        }
    }
    out
}

/// The pictures inside `data`, found by their signatures, and `data`
/// with the bytes of each picture of known length blanked, so they are
/// not read as text.
fn pictures(data: &[u8]) -> (Vec<FoundPicture>, Vec<u8>) {
    let mut out = Vec::new();
    let mut text = data.to_vec();
    let mut i = 0;
    while i + 8 <= data.len() && out.len() < MAX_PICTURES {
        let rest = &data[i..];
        let found = if rest.starts_with(&[0xFF, 0xD8, 0xFF]) {
            jpeg_len(rest).map(|n| ("jpeg", n))
        } else if rest.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            png_len(rest).map(|n| ("png", n))
        } else if rest.starts_with(b"GIF87a") || rest.starts_with(b"GIF89a") {
            Some(("gif", rest.len()))
        } else if rest.starts_with(b"II*\0") || rest.starts_with(b"MM\0*") {
            tiff_ok(rest).then_some(("tiff", rest.len()))
        } else if rest.starts_with(b"BM") {
            bmp_len(rest).map(|n| ("bmp", n))
        } else {
            None
        };
        match found {
            Some((format, n)) => {
                out.push(FoundPicture {
                    format,
                    bytes: rest[..n].to_vec(),
                });
                // A TIFF or GIF runs to the end: look on past its header.
                if n == rest.len() {
                    i += 8;
                } else {
                    text[i..i + n].fill(0);
                    i += n;
                }
            }
            None => i += 1,
        }
    }
    (out, text)
}

fn u16_be(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]) as usize)
}

fn u32_le(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// The length of the JPEG file at the start of `b`: its segments up to
/// the start of scan, then the scan data to the end of image.
fn jpeg_len(b: &[u8]) -> Option<usize> {
    let mut i = 2;
    loop {
        if *b.get(i)? != 0xFF {
            return None;
        }
        let marker = *b.get(i + 1)?;
        match marker {
            0xFF => i += 1,
            0xD8 | 0x01 | 0xD0..=0xD7 => i += 2,
            0xD9 => return Some(i + 2),
            0xDA => {
                i += 2 + u16_be(b, i + 2)?;
                // Entropy-coded data: 0xFF is followed by 0x00 or a restart
                // marker inside it.
                while i + 1 < b.len() {
                    if b[i] == 0xFF && !matches!(b[i + 1], 0x00 | 0xD0..=0xD7 | 0xFF) {
                        break;
                    }
                    i += 1;
                }
                if i + 1 >= b.len() {
                    return None;
                }
            }
            _ => {
                let n = u16_be(b, i + 2)?;
                if n < 2 {
                    return None;
                }
                i += 2 + n;
            }
        }
    }
}

/// The length of the PNG file at the start of `b`, through its `IEND`
/// chunk.
fn png_len(b: &[u8]) -> Option<usize> {
    let mut i = 8;
    loop {
        let n = u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize;
        let kind = b.get(i + 4..i + 8)?;
        let end = i.checked_add(12)?.checked_add(n)?;
        if end > b.len() {
            return None;
        }
        if kind == b"IEND" {
            return Some(end);
        }
        i = end;
    }
}

/// Whether the TIFF header at the start of `b` points inside it.
fn tiff_ok(b: &[u8]) -> bool {
    let off = if b[0] == b'I' {
        u32_le(b, 4)
    } else {
        b.get(4..8)
            .and_then(|s| s.try_into().ok())
            .map(|s: [u8; 4]| u32::from_be_bytes(s) as usize)
    };
    off.is_some_and(|o| o >= 8 && o + 2 <= b.len())
}

/// The length of the BMP file at the start of `b`, as its header gives
/// it, when the header is a picture's.
fn bmp_len(b: &[u8]) -> Option<usize> {
    let n = u32_le(b, 2)?;
    let pixels_at = u32_le(b, 10)?;
    let header = u32_le(b, 14)?;
    let ok = (54..=b.len()).contains(&n)
        && matches!(header, 12 | 40 | 52 | 56 | 108 | 124)
        && pixels_at >= 14 + header
        && pixels_at < n;
    ok.then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream like the files keep: lists of names at a fixed spacing,
    /// a colour system's name twice, filler bytes, a path, and the text.
    fn stream() -> Vec<u8> {
        let mut d = vec![0u8; 4096];
        let put = |d: &mut Vec<u8>, at: usize, s: &[u8]| d[at..at + s.len()].copy_from_slice(s);
        for (i, name) in ["Arial", "Courier New", "Tahoma", "Verdana"]
            .iter()
            .enumerate()
        {
            put(&mut d, 100 + i * 94, name.as_bytes());
        }
        for (i, name) in ["Registration", "Paper", "Black", "Magenta"]
            .iter()
            .enumerate()
        {
            put(&mut d, 600 + i * 210, name.as_bytes());
        }
        put(&mut d, 1500, b"Standard Colour System");
        put(&mut d, 1600, b"Standard Colour System");
        put(&mut d, 1700, &[0xFF; 12]);
        put(&mut d, 1750, b"C:\\Programs\\Layout\\RSRC");
        put(&mut d, 1810, b"Contents");
        put(&mut d, 1850, b"MP2\x06\x344t\x22");
        put(
            &mut d,
            2000,
            "Hello world.\rA second paragraph, caf\u{e9}.".as_bytes(),
        );
        // The é as Windows-1252.
        let at = 2000 + "Hello world.\rA second paragraph, caf".len();
        d[at] = 0xE9;
        d[at + 1] = b'.';
        d[at + 2] = 0;
        d[at + 3] = 0;
        d
    }

    #[test]
    fn text_is_kept_and_the_programs_lists_are_not() {
        let p = paragraphs(&stream());
        assert_eq!(
            p,
            vec![
                "Hello world.".to_string(),
                "A second paragraph, caf\u{e9}.".to_string()
            ]
        );
    }

    #[test]
    fn pictures_inside_are_found_by_signature() {
        // A 2 x 2 PNG.
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        // A tiny JPEG.
        let mut jpg = Vec::new();
        image::RgbImage::from_pixel(8, 8, image::Rgb([0, 0, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut jpg),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        let mut d = vec![7u8; 100];
        d.extend_from_slice(&png);
        d.extend_from_slice(&[1, 2, 3, b'B', b'M', 0, 0]);
        d.extend_from_slice(&jpg);
        d.extend_from_slice(&[9u8; 50]);
        let found = pictures(&d).0;
        assert_eq!(
            found.len(),
            2,
            "{:?}",
            found.iter().map(|f| f.format).collect::<Vec<_>>()
        );
        assert_eq!(found[0].format, "png");
        assert_eq!(found[0].bytes, png);
        assert_eq!(found[1].format, "jpeg");
        assert_eq!(found[1].bytes, jpg);
        assert!(image::load_from_memory(&found[1].bytes).is_ok());
    }

    #[test]
    fn a_compound_file_is_read_stream_by_stream() {
        let mut cf = cfb::CompoundFile::create(std::io::Cursor::new(Vec::new())).unwrap();
        {
            use std::io::Write;
            let mut s = cf.create_stream("/Main").unwrap();
            s.write_all(&stream()).unwrap();
        }
        let bytes = cf.into_inner().into_inner();
        let r = read(&bytes).unwrap();
        assert!(r.compound);
        assert_eq!(r.paragraphs.len(), 2);
        assert!(r.pictures.is_empty());
        // Read as it is when it is not a compound file.
        let plain = read(&stream()).unwrap();
        assert!(!plain.compound);
        assert_eq!(plain.paragraphs, r.paragraphs);
    }

    #[test]
    fn broken_and_empty_files_do_not_panic() {
        assert!(read(&[]).is_err());
        let mut bad = CFB_MAGIC.to_vec();
        bad.extend_from_slice(&[0u8; 100]);
        assert!(read(&bad).is_err());
        for n in 0..64 {
            let junk: Vec<u8> = (0..n * 37).map(|i| (i * 131 % 251) as u8).collect();
            let _ = read(&junk);
        }
        // Truncated pictures are not taken.
        assert!(pictures(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, 1, 2, 3, 4])
            .0
            .is_empty());
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&[0, 0, 0, 13, b'I', b'H', b'D', b'R']);
        assert!(pictures(&png).0.is_empty());
    }
}
