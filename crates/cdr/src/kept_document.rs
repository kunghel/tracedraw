//! The drawing's own colours and styles that open again.
//!
//! The writer has no record yet for the document palette and the names
//! its colours go by, the spot inks of the drawing, the colour styles or
//! the object styles. A file of version 14 or later (a ZIP package)
//! carries them in a member of its own, [`MEMBER`]. The reader gives
//! them back to the drawing it read, where the file does not give them
//! itself: a document palette the file holds wins, and an ink the file
//! names keeps its own entry. Other readers leave the member alone.

use serde::{Deserialize, Serialize};
use tracedraw_core::{
    document::{ColorStyle, Ink, ObjectStyle, PaletteName},
    Color, Document,
};

/// The package member holding them.
pub const MEMBER: &str = "tracedraw/document.json";

/// The most the member may inflate to when read: an object style can
/// hold a picture fill.
pub(crate) const MAX_MEMBER: u64 = 16 << 20;

/// The layout of [`MEMBER`]; a reader skips a member of a later one.
const FORMAT: u32 = 1;

/// What the member holds.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct KeptDocument {
    #[serde(default)]
    format: u32,
    #[serde(default)]
    pub palette: Vec<Color>,
    #[serde(default)]
    pub palette_names: Vec<PaletteName>,
    #[serde(default)]
    pub inks: Vec<Ink>,
    #[serde(default)]
    pub color_styles: Vec<ColorStyle>,
    #[serde(default)]
    pub object_styles: Vec<ObjectStyle>,
}

/// The member's bytes for `doc`; none when it has none of these.
pub(crate) fn encode(doc: &Document) -> Option<Vec<u8>> {
    if doc.palette.is_empty()
        && doc.inks.is_empty()
        && doc.color_styles.is_empty()
        && doc.object_styles.is_empty()
    {
        return None;
    }
    serde_json::to_vec(&KeptDocument {
        format: FORMAT,
        palette: doc.palette.clone(),
        palette_names: doc.palette_names.clone(),
        inks: doc.inks.clone(),
        color_styles: doc.color_styles.clone(),
        object_styles: doc.object_styles.clone(),
    })
    .ok()
}

/// What a member's bytes hold; nothing when it is damaged or of a later
/// layout (with a warning in the log).
pub(crate) fn decode(bytes: &[u8]) -> Option<KeptDocument> {
    match serde_json::from_slice::<KeptDocument>(bytes) {
        Ok(m) if m.format == FORMAT => Some(m),
        Ok(m) => {
            log::warn!("{MEMBER} of layout {}; left out", m.format);
            None
        }
        Err(e) => {
            log::warn!("{MEMBER} unreadable ({e}); left out");
            None
        }
    }
}

/// Give `doc` what `kept` holds where the file read gave none: the
/// palette with its names, the colour and object styles, and each ink
/// whose id the drawing does not have yet.
pub(crate) fn restore(doc: &mut Document, kept: KeptDocument) {
    if doc.palette.is_empty() {
        doc.palette = kept.palette;
        doc.palette_names = kept.palette_names;
    }
    for ink in kept.inks {
        if !doc.inks.iter().any(|i| i.id == ink.id) {
            doc.inks.push(ink);
        }
    }
    if doc.color_styles.is_empty() {
        doc.color_styles = kept.color_styles;
    }
    if doc.object_styles.is_empty() {
        doc.object_styles = kept.object_styles;
    }
}
