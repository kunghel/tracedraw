//! Picture bytes shared between copies of a document.
//!
//! Undo steps, background renders and copied objects all clone the
//! drawing; with plain `Vec<u8>` every clone copied every picture. A
//! [`SharedBytes`] clone is a reference count, and code that makes new
//! pixels replaces the whole value. In the JSON format it is the same
//! base64 text as before, so files do not change.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::Deref;
use std::sync::Arc;

/// Immutable bytes behind a shared pointer (a PNG, as a rule).
#[derive(Clone, Default)]
pub struct SharedBytes(Arc<[u8]>);

impl SharedBytes {
    /// Whether both share one allocation (a cheap "same picture" test).
    pub fn ptr_eq(a: &SharedBytes, b: &SharedBytes) -> bool {
        Arc::ptr_eq(&a.0, &b.0)
    }

    /// The shared buffer itself.
    pub fn arc(&self) -> &Arc<[u8]> {
        &self.0
    }

    /// The bytes as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// The address of the shared allocation, a key for caches. Once every
    /// clone is gone the address can be reused by other bytes, so a cache
    /// that outlives the value it was keyed from must keep a clone of it.
    pub fn addr(&self) -> usize {
        self.0.as_ptr() as usize
    }
}

impl Deref for SharedBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for SharedBytes {
    fn from(v: Vec<u8>) -> Self {
        SharedBytes(v.into())
    }
}

impl From<&[u8]> for SharedBytes {
    fn from(v: &[u8]) -> Self {
        SharedBytes(v.into())
    }
}

impl<const N: usize> From<[u8; N]> for SharedBytes {
    fn from(v: [u8; N]) -> Self {
        SharedBytes(v.as_slice().into())
    }
}

impl From<SharedBytes> for Vec<u8> {
    fn from(v: SharedBytes) -> Self {
        v.0.to_vec()
    }
}

impl PartialEq for SharedBytes {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0[..] == other.0[..]
    }
}

impl Eq for SharedBytes {}

impl PartialEq<[u8]> for SharedBytes {
    fn eq(&self, other: &[u8]) -> bool {
        self.0[..] == *other
    }
}

impl PartialEq<Vec<u8>> for SharedBytes {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.0[..] == other[..]
    }
}

impl PartialEq<SharedBytes> for Vec<u8> {
    fn eq(&self, other: &SharedBytes) -> bool {
        self[..] == other.0[..]
    }
}

impl std::hash::Hash for SharedBytes {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0[..].hash(state)
    }
}

/// Prints like the byte list it replaces.
impl std::fmt::Debug for SharedBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0[..].fmt(f)
    }
}

/// Base64 text, as `png_bytes` writes a byte vector; in a packed
/// `.tdraw` file ([`BlobTable`]), a reference to the file's table.
impl Serialize for SharedBytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match blob_reference(Blob::Shared(self.0.clone())) {
            Some(r) => s.serialize_str(&r),
            None => crate::document::png_bytes::serialize(&self.0, s),
        }
    }
}

impl<'de> Deserialize<'de> for SharedBytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        if let Some(found) = blob_read(&text) {
            return found.map_err(serde::de::Error::custom);
        }
        decode_base64(&text)
            .map(SharedBytes::from)
            .map_err(serde::de::Error::custom)
    }
}

pub(crate) fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|e| e.to_string())
}

/// The start of a reference to a packed file's table of pictures and
/// fonts: `"blob:12"` stands for the table's entry 12. Base64 has no
/// colon, so no picture's text is ever taken for one.
pub const BLOB_PREFIX: &str = "blob:";

/// Bytes the table keeps until it is written.
#[derive(Clone)]
pub(crate) enum Blob {
    Shared(Arc<[u8]>),
    Font(Arc<Vec<u8>>),
}

impl Blob {
    fn bytes(&self) -> &[u8] {
        match self {
            Blob::Shared(b) => b,
            Blob::Font(b) => b,
        }
    }
}

/// The pictures and fonts of a `.tdraw` file being written packed (#2849):
/// each distinct one once, however many objects use it (copies,
/// symbols, an effect's original and its result, a pattern on many
/// shapes); the objects hold references ([`BLOB_PREFIX`]).
#[derive(Default)]
pub(crate) struct BlobTable {
    list: Vec<Blob>,
    /// Entries by their bytes' hash and length (equal bytes are checked).
    by_hash: std::collections::HashMap<(u64, usize), Vec<usize>>,
}

impl BlobTable {
    fn add(&mut self, blob: Blob) -> usize {
        use std::hash::{Hash, Hasher};
        let bytes = blob.bytes();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        let key = (h.finish(), bytes.len());
        let slot = self.by_hash.entry(key).or_default();
        if let Some(&i) = slot.iter().find(|&&i| self.list[i].bytes() == bytes) {
            return i;
        }
        let i = self.list.len();
        slot.push(i);
        self.list.push(blob);
        i
    }
}

/// An entry of the table being read: its text and, once decoded, its
/// bytes (shared by every object that refers to it).
type ReadEntry = (String, Option<SharedBytes>);

thread_local! {
    /// The table of the packed file being written on this thread.
    static WRITING: std::cell::RefCell<Option<BlobTable>> = const { std::cell::RefCell::new(None) };
    /// The table of the packed file being read on this thread.
    static READING: std::cell::RefCell<Option<Vec<ReadEntry>>> = const { std::cell::RefCell::new(None) };
}

/// While a packed file is written: `blob`'s reference, the bytes kept
/// in the table. `None` otherwise (the bytes are written in place).
pub(crate) fn blob_reference(blob: Blob) -> Option<String> {
    WRITING.with(|w| {
        w.borrow_mut()
            .as_mut()
            .map(|t| format!("{BLOB_PREFIX}{}", t.add(blob)))
    })
}

/// While a packed file is read: the bytes `text` refers to, or `None`
/// when it is no reference (or no table is being read).
pub(crate) fn blob_read(text: &str) -> Option<Result<SharedBytes, String>> {
    let n: usize = text.strip_prefix(BLOB_PREFIX)?.parse().ok()?;
    READING.with(|r| {
        let mut r = r.borrow_mut();
        let table = r.as_mut()?;
        let Some((text, done)) = table.get_mut(n) else {
            return Some(Err(format!("picture {n} is not in the file's table")));
        };
        if let Some(b) = done {
            return Some(Ok(b.clone()));
        }
        // A picture saved as the JPEG it was imported from (#2849).
        let b = match text.strip_prefix(crate::jpeg_source::JPEG_PREFIX) {
            Some(jpeg) => decode_base64(jpeg).and_then(|j| crate::jpeg_source::png_of_jpeg(&j)),
            None => decode_base64(text).map(SharedBytes::from),
        };
        if let Ok(b) = &b {
            *done = Some(b.clone());
            // The text is not needed once decoded.
            *text = String::new();
        }
        Some(b)
    })
}

/// Writing a packed file on this thread until dropped: the table that
/// [`BlobsOut`] writes.
pub(crate) struct WritingTable;

impl WritingTable {
    pub(crate) fn start() -> Self {
        WRITING.with(|w| *w.borrow_mut() = Some(BlobTable::default()));
        WritingTable
    }
}

impl Drop for WritingTable {
    fn drop(&mut self) {
        WRITING.with(|w| *w.borrow_mut() = None);
    }
}

/// Reading a packed file's `table` (each entry's base64 text) on this
/// thread until dropped.
pub(crate) struct ReadingTable;

impl ReadingTable {
    pub(crate) fn start(table: Vec<String>) -> Self {
        READING.with(|r| *r.borrow_mut() = Some(table.into_iter().map(|t| (t, None)).collect()));
        ReadingTable
    }
}

impl Drop for ReadingTable {
    fn drop(&mut self) {
        READING.with(|r| *r.borrow_mut() = None);
    }
}

/// The table of the file being written, as a list of base64 texts;
/// serialized after the drawing, whose pictures and fonts filled it.
pub(crate) struct BlobsOut;

impl Serialize for BlobsOut {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let list = WRITING.with(|w| w.borrow_mut().as_mut().map(|t| std::mem::take(&mut t.list)));
        let list = list.unwrap_or_default();
        let mut seq = s.serialize_seq(Some(list.len()))?;
        for b in &list {
            // A picture imported from a JPEG goes out as that JPEG.
            let jpeg = match b {
                Blob::Shared(png) => crate::jpeg_source::jpeg_of(png),
                Blob::Font(_) => None,
            };
            match jpeg {
                Some(j) => seq.serialize_element(&format!(
                    "{}{}",
                    crate::jpeg_source::JPEG_PREFIX,
                    base64_text(&j)
                ))?,
                None => seq.serialize_element(&Base64(b.bytes()))?,
            }
        }
        seq.end()
    }
}

struct Base64<'a>(&'a [u8]);

fn base64_text(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

impl Serialize for Base64<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        crate::document::png_bytes::serialize(self.0, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_and_serde_matches_the_byte_vector_form() {
        let a = SharedBytes::from(vec![1u8, 2, 3, 250]);
        let b = a.clone();
        assert!(SharedBytes::ptr_eq(&a, &b));
        assert_eq!(a, vec![1u8, 2, 3, 250]);
        #[derive(Serialize)]
        struct Old {
            #[serde(with = "crate::document::png_bytes")]
            png: Vec<u8>,
        }
        #[derive(Serialize, Deserialize)]
        struct New {
            png: SharedBytes,
        }
        let old = serde_json::to_string(&Old {
            png: vec![1, 2, 3, 250],
        })
        .unwrap();
        let new = serde_json::to_string(&New { png: a.clone() }).unwrap();
        assert_eq!(old, new);
        let back: New = serde_json::from_str(&old).unwrap();
        assert_eq!(back.png, a);
        assert_eq!(format!("{a:?}"), format!("{:?}", vec![1u8, 2, 3, 250]));
    }
}
