//! Inclusive page positions, parsed once and tested without allocations.

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::iter::Peekable;
use std::str::Chars;

/// A normalized set of 1-based page intervals. Positions refer to the
/// document's current order and can include pages added later.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct PageRanges(Vec<PageInterval>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
struct PageInterval {
    first: u64,
    last: u64,
}

impl PageRanges {
    /// Positive page numbers and inclusive ranges, separated by commas,
    /// semicolons or whitespace. Reversed ranges are put in page order.
    /// An empty or invalid part rejects the input as a whole.
    pub fn parse(text: &str) -> Option<Self> {
        let mut chars = text.chars().peekable();
        skip_whitespace(&mut chars);
        let mut intervals = Vec::new();
        loop {
            let first = number(&mut chars)?;
            let mut spaced = skip_whitespace(&mut chars);
            let last = if chars.peek() == Some(&'-') {
                chars.next();
                skip_whitespace(&mut chars);
                let last = number(&mut chars)?;
                spaced = skip_whitespace(&mut chars);
                last
            } else {
                first
            };
            intervals.push(PageInterval { first, last });
            match chars.peek().copied() {
                None => break,
                Some(',' | ';' | '，' | '；') => {
                    chars.next();
                    skip_whitespace(&mut chars);
                }
                Some(c) if spaced && c.is_ascii_digit() => {}
                _ => return None,
            }
        }
        Some(Self::normalized(intervals))
    }

    /// Whether the 0-based document position is in these intervals.
    pub fn contains(&self, index: usize) -> bool {
        let Some(page) = u64::try_from(index).ok().and_then(|n| n.checked_add(1)) else {
            return false;
        };
        let next = self.0.partition_point(|span| span.last < page);
        self.0.get(next).is_some_and(|span| span.first <= page)
    }

    /// Count matching positions within the existing pages, in interval order.
    pub fn count_within(&self, page_count: usize) -> usize {
        let last_page = page_count as u64;
        // Normalized intervals are positive and disjoint, so each clipped
        // count and their sum fit `page_count` on 32- and 64-bit targets.
        self.0
            .iter()
            .take_while(|span| span.first <= last_page)
            .map(|span| (span.last.min(last_page) - span.first + 1) as usize)
            .sum()
    }

    /// An empty set selects no pages.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn normalized(intervals: Vec<PageInterval>) -> Self {
        let mut intervals: Vec<_> = intervals
            .into_iter()
            .filter(|span| span.first > 0 && span.last > 0)
            .map(|span| PageInterval {
                first: span.first.min(span.last),
                last: span.first.max(span.last),
            })
            .collect();
        intervals.sort_unstable_by_key(|span| (span.first, span.last));
        let mut merged: Vec<PageInterval> = Vec::with_capacity(intervals.len());
        for span in intervals {
            if let Some(previous) = merged.last_mut() {
                if span.first <= previous.last.saturating_add(1) {
                    previous.last = previous.last.max(span.last);
                    continue;
                }
            }
            merged.push(span);
        }
        Self(merged)
    }
}

impl<'de> Deserialize<'de> for PageRanges {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let intervals = Vec::<PageInterval>::deserialize(deserializer)?;
        // Invalid empty/zero intervals in an optional file field select no
        // pages. They must never turn the layer into an unrestricted master.
        Ok(Self::normalized(intervals))
    }
}

impl fmt::Display for PageRanges {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, span) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            if span.first == span.last {
                write!(f, "{}", span.first)?;
            } else {
                write!(f, "{}-{}", span.first, span.last)?;
            }
        }
        Ok(())
    }
}

fn skip_whitespace(chars: &mut Peekable<Chars<'_>>) -> bool {
    let mut skipped = false;
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
        skipped = true;
    }
    skipped
}

fn number(chars: &mut Peekable<Chars<'_>>) -> Option<u64> {
    let mut value = 0u64;
    let mut found = false;
    while let Some(digit) = chars.peek().and_then(|c| c.to_digit(10)) {
        if !chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u64::from(digit))?;
        found = true;
        chars.next();
    }
    (found && value > 0).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Engine, Error, Layer, LayerId, MasterScope, Size};

    fn ranges(text: &str) -> PageRanges {
        PageRanges::parse(text).expect("valid page range")
    }

    fn document() -> (Document, LayerId) {
        let mut doc = Document::default();
        for _ in 1..6 {
            Command::AddPage {
                name: None,
                size: Size::new(210.0, 297.0),
                layer: None,
            }
            .apply(&mut doc)
            .expect("page");
        }
        let layer = doc.ids_mut().layer();
        doc.master
            .push(Layer::master(layer, "Chapter", MasterScope::All));
        (doc, layer)
    }

    fn shown(doc: &Document, layer: LayerId) -> Vec<usize> {
        doc.pages
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                doc.layers_for_page(p.id)
                    .expect("page layers")
                    .iter()
                    .any(|l| l.id == layer)
            })
            .map(|(index, _)| index + 1)
            .collect()
    }

    #[test]
    fn page_ranges_parse_normalize_and_use_document_positions() {
        let parsed = ranges(" 9-7, 2-4 ; 1， 4-6 ； 12  15 - 17 ");
        assert_eq!(parsed.to_string(), "1-9, 12, 15-17");
        assert!(parsed.contains(0));
        assert!(parsed.contains(8));
        assert!(!parsed.contains(9));
        assert!(parsed.contains(11));
        assert!(!parsed.contains(12));
        assert!(parsed.contains(16));
        assert!(!parsed.contains(17));
        assert!(!parsed.is_empty());
    }

    #[test]
    fn page_ranges_reject_empty_parts_zero_bad_numbers_and_overflow() {
        for bad in [
            "",
            " ",
            ",",
            "1,",
            ",1",
            "1,,2",
            "1; ;2",
            "1，；2",
            "0",
            "0-1",
            "1-0",
            "-1",
            "1-",
            "1--2",
            "1-2-3",
            "one",
            "1.2",
            "1e2",
            "1x2",
            "18446744073709551616",
            "1-18446744073709551616",
        ] {
            assert!(PageRanges::parse(bad).is_none(), "{bad:?}");
        }
        assert!(PageRanges::default().is_empty());
        assert!(!PageRanges::default().contains(0));
    }

    #[test]
    fn page_ranges_keep_future_positions_and_handle_the_largest_bound() {
        let parsed = ranges("4-99999999999");
        assert!(parsed.contains(3));
        assert!(parsed.contains(100));
        assert!(!parsed.contains(2));
        let largest = ranges("18446744073709551614, 18446744073709551615");
        assert_eq!(
            largest.to_string(),
            "18446744073709551614-18446744073709551615"
        );
        assert!(!largest.contains(usize::MAX));
    }

    #[test]
    fn page_range_counts_clip_intervals_and_fit_the_largest_page_count() {
        let parsed = ranges("2-4, 8, 11-15, 1000-18446744073709551615");
        for count in 0..20 {
            assert_eq!(
                parsed.count_within(count),
                (0..count).filter(|index| parsed.contains(*index)).count(),
                "{count}"
            );
        }
        assert_eq!(PageRanges::default().count_within(usize::MAX), 0);
        assert_eq!(ranges("1-18446744073709551615").count_within(0), 0);
        assert_eq!(
            ranges("1-18446744073709551615").count_within(usize::MAX),
            usize::MAX
        );
        assert_eq!(
            ranges("1-2, 4-18446744073709551615").count_within(usize::MAX),
            usize::MAX - 1
        );
        assert_eq!(
            ranges("18446744073709551615").count_within(usize::MAX),
            if usize::BITS == 64 { 1 } else { 0 }
        );
    }

    #[test]
    fn page_ranges_normalize_file_data_and_preserve_empty_constraints() {
        let parsed: PageRanges = serde_json::from_str(
            r#"[{"first":9,"last":7},{"first":1,"last":2},{"first":2,"last":6}]"#,
        )
        .expect("file intervals");
        assert_eq!(parsed.to_string(), "1-9");
        assert_eq!(
            serde_json::from_str::<PageRanges>(&serde_json::to_string(&parsed).expect("write"))
                .expect("read"),
            parsed
        );
        for json in ["[]", r#"[{"first":0,"last":3}]"#, r#"[{"first":2}]"#] {
            let parsed: PageRanges = serde_json::from_str(json).expect("empty file constraint");
            assert!(parsed.is_empty());
            assert!(!parsed.contains(0));
        }
        for bad in ["null", r#""bad""#, r#"[{"first":-1,"last":3}]"#] {
            assert!(serde_json::from_str::<PageRanges>(bad).is_err());
        }
    }

    #[test]
    fn page_range_commands_are_undoable_and_scope_changes_clear_ranges() {
        let (doc, layer) = document();
        let mut engine = Engine::new(doc);
        engine
            .run(&Command::SetLayerPageRange {
                layer,
                page_range: Some(ranges("2-3, 5")),
            })
            .expect("set range");
        assert_eq!(shown(engine.document(), layer), vec![2, 3, 5]);
        engine.undo().expect("undo");
        assert_eq!(shown(engine.document(), layer), vec![1, 2, 3, 4, 5, 6]);
        engine.redo().expect("redo");
        assert_eq!(shown(engine.document(), layer), vec![2, 3, 5]);
        engine
            .run(&Command::SetLayerScope {
                layer,
                scope: MasterScope::Odd,
            })
            .expect("odd pages");
        assert_eq!(shown(engine.document(), layer), vec![1, 3, 5]);
        assert!(engine
            .document()
            .layer(layer)
            .expect("layer")
            .page_range
            .is_none());
        engine
            .run(&Command::SetLayerPageRange {
                layer,
                page_range: Some(ranges("2-4")),
            })
            .expect("custom pages");
        assert_eq!(
            engine.document().layer(layer).expect("layer").scope,
            MasterScope::All
        );
        engine
            .run(&Command::SetLayerPageRange {
                layer,
                page_range: None,
            })
            .expect("clear range");
        assert_eq!(shown(engine.document(), layer), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn range_layer_creation_is_one_undo_step_and_rejects_empty_ranges() {
        let mut engine = Engine::default();
        let before = engine.document().clone();
        assert!(matches!(
            engine.run(&Command::AddRangeLayer {
                name: "Invalid".into(),
                page_range: PageRanges::default(),
            }),
            Err(Error::InvalidPageRange)
        ));
        assert_eq!(engine.document(), &before);
        assert!(engine.undo_label().is_none());
        engine
            .run(&Command::AddRangeLayer {
                name: "Chapter".into(),
                page_range: ranges("1-3, 7"),
            })
            .expect("range layer");
        let layer = engine.document().master.last().expect("created layer");
        assert_eq!(layer.name, "Chapter");
        assert_eq!(layer.scope, MasterScope::All);
        assert_eq!(
            layer.page_range.as_ref().expect("range").to_string(),
            "1-3, 7"
        );
        assert!(!layer.desktop);
        assert!(!layer.in_front);
        let created = engine.document().clone();
        let allocated_ids = created.ids().clone();
        assert!(allocated_ids.peek_next() > before.ids().peek_next());
        engine.undo().expect("undo creation");
        let mut expected = before.clone();
        expected.set_ids(allocated_ids);
        assert_eq!(engine.document(), &expected);
        assert!(engine.undo_label().is_none());
        engine.redo().expect("redo creation");
        assert_eq!(engine.document(), &created);
    }

    #[test]
    fn page_ranges_survive_native_files_and_old_layers_keep_their_scope() {
        let (mut doc, layer) = document();
        Command::SetLayerPageRange {
            layer,
            page_range: Some(ranges("2-3, 5")),
        }
        .apply(&mut doc)
        .expect("range");
        let back =
            Document::from_json(&doc.to_json().expect("write drawing")).expect("read drawing");
        assert_eq!(shown(&back, layer), vec![2, 3, 5]);
        let mut old: serde_json::Value =
            serde_json::from_str(&doc.to_json().expect("write")).expect("json");
        old["master"][0]
            .as_object_mut()
            .expect("master object")
            .remove("page_range");
        let old = Document::from_json(&old.to_string()).expect("old drawing");
        assert_eq!(shown(&old, layer), vec![1, 2, 3, 4, 5, 6]);
        for scope in [MasterScope::All, MasterScope::Odd, MasterScope::Even] {
            let mut l = Layer::master(LayerId(99), "Existing master", scope);
            assert!((0..6).all(|i| l.applies_to(i) == scope.applies(i)));
            l.page_range = Some(PageRanges::default());
            assert!((0..6).all(|i| !l.applies_to(i)));
        }
    }

    #[test]
    fn page_ranges_keep_objects_and_future_endpoints_in_packed_native_files() {
        let (mut doc, layer) = document();
        Command::SetLayerPageRange {
            layer,
            page_range: Some(ranges("2-3, 5, 12-15")),
        }
        .apply(&mut doc)
        .expect("range");
        let marker = crate::Shape::new(
            doc.ids_mut().shape(),
            crate::ShapeKind::Rect {
                rect: crate::Rect::new(10.0, 20.0, 30.0, 40.0),
                radius: 0.0,
                corners: None,
            },
        );
        doc.layer_mut(layer)
            .expect("range layer")
            .shapes
            .push(marker);
        let mut bytes = Vec::new();
        doc.write_tdraw_version(&mut bytes, crate::document::TDRAW_FORMAT_VERSION)
            .expect("packed drawing");
        assert!(crate::document::is_gzip(&bytes));
        let back = Document::from_tdraw_bytes(&bytes).expect("packed readback");
        assert_eq!(back.master, doc.master);
        assert_eq!(shown(&back, layer), vec![2, 3, 5]);
        assert_eq!(back.pages, doc.pages);
    }

    #[test]
    fn range_layers_keep_their_front_and_back_drawing_order() {
        let (mut doc, behind) = document();
        Command::SetLayerPageRange {
            layer: behind,
            page_range: Some(ranges("2-3")),
        }
        .apply(&mut doc)
        .expect("range");
        let front = doc.ids_mut().layer();
        let mut layer = Layer::master(front, "Front", MasterScope::All);
        layer.in_front = true;
        layer.page_range = Some(ranges("2-3"));
        doc.master.push(layer);
        for (index, page) in doc.pages.iter().enumerate() {
            let layers = doc.layers_for_page(page.id).expect("layers");
            let ids: Vec<_> = layers.iter().map(|layer| layer.id).collect();
            let expected = if (1..=2).contains(&index) {
                vec![behind, page.layers[0].id, front]
            } else {
                vec![page.layers[0].id]
            };
            assert_eq!(ids, expected);
        }
    }

    #[test]
    fn page_ranges_follow_positions_after_insert_delete_and_reorder() {
        let (mut doc, layer) = document();
        Command::SetLayerPageRange {
            layer,
            page_range: Some(ranges("2-3, 7")),
        }
        .apply(&mut doc)
        .expect("range");
        let was_second = doc.pages[1].id;
        assert_eq!(shown(&doc, layer), vec![2, 3]);
        Command::InsertPage {
            at: 0,
            name: None,
            size: Size::new(210.0, 297.0),
            layer: None,
            shapes: Vec::new(),
        }
        .apply(&mut doc)
        .expect("insert");
        assert_eq!(doc.pages[2].id, was_second);
        assert_eq!(shown(&doc, layer), vec![2, 3, 7]);
        Command::MovePage {
            page: was_second,
            to: 5,
        }
        .apply(&mut doc)
        .expect("reorder");
        assert_eq!(shown(&doc, layer), vec![2, 3, 7]);
        Command::DeletePage { page: was_second }
            .apply(&mut doc)
            .expect("delete");
        assert_eq!(shown(&doc, layer), vec![2, 3]);
    }

    #[test]
    fn page_range_commands_reject_page_desktop_and_empty_constraints_atomically() {
        let (mut doc, layer) = document();
        let own = doc.pages[0].layers[0].id;
        let desktop = doc.ids_mut().layer();
        let mut desk = Layer::master(desktop, "Desktop", MasterScope::All);
        desk.desktop = true;
        doc.master.push(desk);
        let mut engine = Engine::new(doc);
        let before = engine.document().clone();
        for layer in [own, desktop, LayerId(u64::MAX)] {
            assert!(matches!(
                engine.run(&Command::SetLayerPageRange {
                    layer,
                    page_range: Some(ranges("1-3"))
                }),
                Err(Error::LayerNotFound(_))
            ));
            assert_eq!(engine.document(), &before);
        }
        assert!(matches!(
            engine.run(&Command::SetLayerPageRange {
                layer,
                page_range: Some(PageRanges::default())
            }),
            Err(Error::InvalidPageRange)
        ));
        assert_eq!(engine.document(), &before);
    }
}
