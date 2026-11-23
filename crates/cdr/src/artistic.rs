//! Linked brush strokes (#2878, #3680): the objects an artistic
//! brush repeats along a curve, which the file keeps as a linked group
//! (`LIST lnkg` whose own `lgob` holds a `loda` of object type 28) beside
//! the curve, its guide. The document's link table (`lnkt`) pairs the
//! group with its guide by their numbers (`spnd`).
//!
//! The reader keeps the objects as the file draws them and marks the
//! guide with them, in their order along it ([`DATA_KEY`]); each object
//! and the guide take the file's own identity (`spid`) so the mark names
//! them. The application turns the mark into a live fit of the objects to
//! the guide when the objects lie on it at even steps, so moving or
//! reshaping the guide places them again (see `fit_live.rs`).
//!
//! Read from the version 27 file of #3680 (see CLEANROOM.md): one curve,
//! then a linked group of 120 objects; `lnkt` is 52 bytes: its length,
//! the entry count (1), the offset of the entry offsets (12), the entry
//! offsets (20, and 52 where the entries end), then the entry: a u32 of
//! 0x100, the group's number (1), the guide's number (2) and zeros.

use tracedraw_core::document::Uid;
use tracedraw_core::Shape;

/// Object type of an artistic brush's linked group.
pub(crate) const OBJ_ARTISTIC_GROUP: u32 = 28;

/// The object data key the reader puts on the guide: a JSON object whose
/// `objects` lists the identities (32 hexadecimal digits) of the objects
/// the stroke repeats, in their order along the guide.
pub const DATA_KEY: &str = "cdr.brush_stroke";

/// The most entries read from one link table.
const MAX_LINKS: usize = 10_000;

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    let b = d.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The pairs of a link table (`lnkt`): (linked group's number, the
/// number of the object it is linked to). Entries that do not read are
/// left out; a table that does not read gives none.
pub(crate) fn links(d: &[u8]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let Some(count) = u32_at(d, 4).map(|n| n as usize) else {
        return out;
    };
    let Some(table) = u32_at(d, 8).map(|n| n as usize) else {
        return out;
    };
    for k in 0..count.min(MAX_LINKS) {
        let Some(entry) = k
            .checked_mul(4)
            .and_then(|o| table.checked_add(o))
            .and_then(|at| u32_at(d, at))
            .map(|n| n as usize)
        else {
            break;
        };
        // The entry's kind, then the group and the object linked.
        let (Some(group), Some(object)) = (
            entry.checked_add(4).and_then(|at| u32_at(d, at)),
            entry.checked_add(8).and_then(|at| u32_at(d, at)),
        ) else {
            continue;
        };
        if group != object {
            out.push((group, object));
        }
    }
    out
}

/// An identity from a `spid` chunk's 16 bytes; `None` for any other
/// length or all zeros.
pub(crate) fn uid_of_spid(d: &[u8]) -> Option<Uid> {
    let b: [u8; 16] = d.try_into().ok()?;
    let n = u128::from_le_bytes(b);
    (n != 0).then_some(Uid(n))
}

/// Mark `guide` with `objects` (the stroke's objects in their order), as
/// [`DATA_KEY`] says. Every one needs an identity.
pub(crate) fn mark(guide: &mut Shape, objects: &[Shape]) -> bool {
    let ids: Option<Vec<String>> = objects
        .iter()
        .map(|s| s.uid.map(|u| format!("{:032x}", u.0)))
        .collect();
    let Some(ids) = ids.filter(|v| v.len() >= 2) else {
        return false;
    };
    if guide.uid.is_none() || objects.iter().any(|o| o.uid == guide.uid) {
        return false;
    }
    let text = serde_json::json!({ "objects": ids }).to_string();
    guide.data.retain(|(k, _)| k != DATA_KEY);
    guide.data.push((DATA_KEY.to_string(), text));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(entries: &[(u32, u32, u32)]) -> Vec<u8> {
        let n = entries.len() as u32;
        let first = 12 + 4 * (n + 1);
        let mut d = Vec::new();
        let total = first + 32 * n;
        for v in [total, n, 12] {
            d.extend(v.to_le_bytes());
        }
        for k in 0..=n {
            d.extend((first + 32 * k).to_le_bytes());
        }
        for &(kind, group, object) in entries {
            for v in [kind, group, object, 0, 0, 0, 0, 0] {
                d.extend(v.to_le_bytes());
            }
        }
        d
    }

    #[test]
    fn the_link_table_pairs_a_group_with_its_guide() {
        let d = table(&[(0x100, 1, 2)]);
        assert_eq!(d.len(), 52);
        assert_eq!(links(&d), vec![(1, 2)]);
        assert_eq!(links(&table(&[(0x100, 1, 2), (0x100, 5, 9)])), vec![(1, 2), (5, 9)]);
        // Cut short or empty: nothing, and no panic.
        assert!(links(&d[..22]).is_empty());
        assert!(links(&[]).is_empty());
    }

    #[test]
    fn spids_give_identities() {
        let mut b = [0u8; 16];
        assert_eq!(uid_of_spid(&b), None);
        b[0] = 7;
        assert_eq!(uid_of_spid(&b), Some(Uid(7)));
        assert_eq!(uid_of_spid(&b[..8]), None);
    }
}
