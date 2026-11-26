//! Object data that opens again (#2879).
//!
//! An object's data (the fields of Object Properties > Data, and what
//! TraceDraw keeps there itself, such as the link of a clone to its
//! master) has no record of its own in the file, nor has the object
//! style an object follows. A file of version 14 or later (a ZIP
//! package) carries them in a member of its own, [`MEMBER`],
//! by the static id (`usdn`) each object was written with, together with
//! the id the object had in the drawing saved. The reader gives each
//! object found by its static id its data back, and a clone's link,
//! which names its master by that saved id, names the master's id in the
//! drawing read. Other readers leave the member alone; an object another
//! program removed opens without it.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracedraw_core::{
    document::{Shape, Uid},
    Document, ShapeId, ShapeKind,
};

/// The package member holding the data.
pub const MEMBER: &str = "tracedraw/data.json";

/// The most the member may inflate to when read.
pub(crate) const MAX_MEMBER: u64 = 16 << 20;

/// The layout of [`MEMBER`]; a reader skips a member of a later one.
const FORMAT: u32 = 1;

/// The data key of a clone naming its master's id.
const CLONE_MASTER: &str = "clone.master";

/// One object as the member holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct KeptData {
    /// The static id it was written with.
    pub usdn: u32,
    /// Its id in the drawing saved.
    pub id: u64,
    /// Its data, in order.
    pub data: Vec<(String, String)>,
    /// The object style it follows, by the style's id; older members
    /// have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<u32>,
    /// Its stable identity, kept for the objects another object's data
    /// names by it (#4275: the path, objects and copies of a live Fit
    /// Objects to Path); older members have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<Uid>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Member {
    format: u32,
    objects: Vec<KeptData>,
}

/// Every object of `doc`, pages and master layers, inside groups and
/// clip frames too.
fn all_shapes(doc: &Document) -> Vec<&Shape> {
    fn walk<'a>(s: &'a Shape, out: &mut Vec<&'a Shape>) {
        out.push(s);
        match &s.kind {
            ShapeKind::Group { children } => {
                for c in children {
                    walk(c, out);
                }
            }
            ShapeKind::ClipFrame { frame, contents } => {
                walk(frame, out);
                for c in contents {
                    walk(c, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for l in doc
        .pages
        .iter()
        .flat_map(|p| p.layers.iter())
        .chain(&doc.master)
    {
        for s in &l.shapes {
            walk(s, &mut out);
        }
    }
    out
}

/// The 32-hex-digit words of `text`: the identities it may name.
fn named_uids(text: &str, out: &mut std::collections::HashSet<u128>) {
    for word in text.split(|c: char| !c.is_ascii_hexdigit()) {
        if word.len() == 32 {
            if let Ok(v) = u128::from_str_radix(word, 16) {
                out.insert(v);
            }
        }
    }
}

/// The member's bytes for the objects of `doc` written with the static
/// ids `static_ids`: those with data or an object style, the masters
/// their clones name (with no data of their own, so a clone finds its
/// master again), and the objects whose identity some object's data
/// names (#4275). None when no object has data or a style.
pub(crate) fn encode(doc: &Document, static_ids: &HashMap<ShapeId, u32>) -> Option<Vec<u8>> {
    let shapes = all_shapes(doc);
    let masters: Vec<u64> = shapes
        .iter()
        .filter_map(|s| s.data.iter().find(|(k, _)| k == CLONE_MASTER))
        .filter_map(|(_, v)| v.parse().ok())
        .collect();
    let mut named = std::collections::HashSet::new();
    for s in &shapes {
        for (_, v) in &s.data {
            named_uids(v, &mut named);
        }
    }
    let is_named = |s: &Shape| s.uid.is_some_and(|u| named.contains(&u.0));
    let mut objects: Vec<KeptData> = shapes
        .iter()
        .filter(|s| {
            !s.data.is_empty()
                || s.style.is_some()
                || masters.contains(&s.id.raw())
                || is_named(s)
        })
        .filter_map(|s| {
            static_ids.get(&s.id).map(|&usdn| KeptData {
                usdn,
                id: s.id.raw(),
                data: s.data.clone(),
                style: s.style,
                uid: s.uid.filter(|_| is_named(s)),
            })
        })
        .collect();
    if objects
        .iter()
        .all(|o| o.data.is_empty() && o.style.is_none())
    {
        return None;
    }
    objects.sort_by_key(|o| o.usdn);
    serde_json::to_vec(&Member {
        format: FORMAT,
        objects,
    })
    .ok()
}

/// The objects of a member's bytes; none when it is damaged or of a
/// later layout (with a warning in the log).
pub(crate) fn decode(bytes: &[u8]) -> Vec<KeptData> {
    match serde_json::from_slice::<Member>(bytes) {
        Ok(m) if m.format == FORMAT => m.objects,
        Ok(m) => {
            log::warn!("{MEMBER} of layout {}; object data left out", m.format);
            Vec::new()
        }
        Err(e) => {
            log::warn!("{MEMBER} unreadable ({e}); object data left out");
            Vec::new()
        }
    }
}

/// Give the objects read by the static ids `ids` the data `kept` holds
/// for them (keys an object already has keep its value), and point each
/// clone at its master's id in `doc`. Returns how many objects took data.
pub(crate) fn restore(doc: &mut Document, kept: &[KeptData], ids: &HashMap<u32, ShapeId>) -> usize {
    // The saved drawing's ids to the read drawing's.
    let renamed: HashMap<u64, ShapeId> = kept
        .iter()
        .filter_map(|k| ids.get(&k.usdn).map(|&id| (k.id, id)))
        .collect();
    // Identities first (#4275), each given once and never one an object
    // of the drawing read already has.
    let mut taken: std::collections::HashSet<Uid> =
        all_shapes(doc).iter().filter_map(|s| s.uid).collect();
    let mut seen = std::collections::HashSet::new();
    let doubled: std::collections::HashSet<Uid> = kept
        .iter()
        .filter_map(|k| k.uid)
        .filter(|u| !seen.insert(*u))
        .collect();
    for k in kept {
        let Some(uid) = k.uid.filter(|u| u.0 != 0 && !doubled.contains(u)) else {
            continue;
        };
        let Some(&id) = ids.get(&k.usdn) else {
            continue;
        };
        if taken.contains(&uid) {
            continue;
        }
        let Some(s) = doc.find_shape_mut(id) else {
            continue;
        };
        if let Some(old) = s.uid.replace(uid) {
            taken.remove(&old);
        }
        taken.insert(uid);
    }
    let mut n = 0;
    for k in kept
        .iter()
        .filter(|k| !k.data.is_empty() || k.style.is_some())
    {
        let Some(&id) = ids.get(&k.usdn) else {
            continue;
        };
        let Some(s) = doc.find_shape_mut(id) else {
            continue;
        };
        if s.style.is_none() {
            s.style = k.style;
        }
        for (key, value) in &k.data {
            if s.data.iter().any(|(have, _)| have == key) {
                continue;
            }
            let value = if key == CLONE_MASTER {
                match value.parse::<u64>().ok().and_then(|m| renamed.get(&m)) {
                    Some(master) => master.raw().to_string(),
                    // The master is gone: the clone stays a plain copy.
                    None => continue,
                }
            } else {
                value.clone()
            };
            s.data.push((key.clone(), value));
        }
        // No master, no link: its state goes with it.
        if !s.data.iter().any(|(key, _)| key == CLONE_MASTER) {
            s.data.retain(|(key, _)| key != "clone.state");
        }
        n += 1;
    }
    n
}
