//! Text > Chinese (#2182): the selected text turned from Simplified to
//! Traditional Chinese characters or back in one click, and its pinyin
//! written above the characters. The tables come from openly licensed
//! data (see `assets/cjk/` and ATTRIBUTION.md), made by
//! `scripts/cjk/make_tables.py`. See docs/behavior/chinese-text.md.

use std::collections::HashMap;
use std::sync::OnceLock;

use tracedraw_core::{Affine, Command, Point, Shape, ShapeId, ShapeKind, TextAlign, TextSpan};

/// Which characters the text is turned into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Traditional,
    Simplified,
}

/// One direction of conversion: the characters, and the phrases that
/// settle the characters with more than one answer (发 is 發 in 发现 and
/// 髮 in 头发).
struct Table {
    chars: HashMap<char, char>,
    phrases: HashMap<String, String>,
    /// The most characters in a phrase.
    longest: usize,
}

fn parse_table(src: &str) -> Table {
    let mut t = Table {
        chars: HashMap::new(),
        phrases: HashMap::new(),
        longest: 1,
    };
    for line in src.lines() {
        if line.starts_with('#') {
            continue;
        }
        let Some((from, to)) = line.split_once(' ') else {
            continue;
        };
        let n = from.chars().count();
        // Only same-length answers keep the styles of the characters.
        if n != to.chars().count() || n == 0 {
            continue;
        }
        if n == 1 {
            if let (Some(a), Some(b)) = (from.chars().next(), to.chars().next()) {
                t.chars.insert(a, b);
            }
        } else {
            t.longest = t.longest.max(n);
            t.phrases.insert(from.to_string(), to.to_string());
        }
    }
    t
}

fn table(to: Script) -> &'static Table {
    static S2T: OnceLock<Table> = OnceLock::new();
    static T2S: OnceLock<Table> = OnceLock::new();
    match to {
        Script::Traditional => {
            S2T.get_or_init(|| parse_table(include_str!("../assets/cjk/s2t.txt")))
        }
        Script::Simplified => {
            T2S.get_or_init(|| parse_table(include_str!("../assets/cjk/t2s.txt")))
        }
    }
}

/// `text` in the other characters: the longest known phrase at each
/// place first, then character by character. The result has as many
/// characters as `text`, so the styles of the characters stay put.
pub fn convert(text: &str, to: Script) -> String {
    let t = table(to);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'outer: while i < chars.len() {
        for n in (2..=t.longest.min(chars.len() - i)).rev() {
            let key: String = chars[i..i + n].iter().collect();
            if let Some(v) = t.phrases.get(&key) {
                out.push_str(v);
                i += n;
                continue 'outer;
            }
        }
        out.push(t.chars.get(&chars[i]).copied().unwrap_or(chars[i]));
        i += 1;
    }
    out
}

/// The most common Mandarin reading of a character, with its tone mark.
pub fn pinyin(c: char) -> Option<&'static str> {
    static READINGS: OnceLock<HashMap<char, &'static str>> = OnceLock::new();
    READINGS
        .get_or_init(|| {
            let mut m = HashMap::new();
            for line in include_str!("../assets/cjk/pinyin.txt").lines() {
                if line.starts_with('#') {
                    continue;
                }
                if let Some((reading, chars)) = line.split_once('\t') {
                    for c in chars.chars() {
                        m.insert(c, reading);
                    }
                }
            }
            m
        })
        .get(&c)
        .copied()
}

/// `spans` with their text converted as one piece (phrases may cross
/// from one style to the next), each span keeping its characters.
pub fn convert_spans(spans: &[TextSpan], to: Script) -> Vec<TextSpan> {
    let all: String = spans.iter().map(|s| s.text.as_str()).collect();
    let mut done = convert(&all, to).chars().collect::<Vec<_>>().into_iter();
    spans
        .iter()
        .map(|s| TextSpan {
            text: done.by_ref().take(s.text.chars().count()).collect(),
            ..s.clone()
        })
        .collect()
}

/// The pinyin size against the size of its character.
pub const PINYIN_SCALE: f64 = 0.4;

impl crate::app::App {
    /// Text > Chinese > To Traditional / To Simplified: every selected
    /// text in the other characters, as one undo step.
    pub fn convert_chinese(&mut self, to: Script) {
        let mut cmds = Vec::new();
        for s in self.text_shapes() {
            let ShapeKind::Text {
                spans,
                origin,
                frame,
                align,
                para,
                on_path,
            } = s.kind.clone()
            else {
                continue;
            };
            let done = convert_spans(&spans, to);
            if done == spans {
                continue;
            }
            cmds.push(Command::SetShapeKind {
                shape: s.id,
                kind: ShapeKind::Text {
                    spans: done,
                    origin,
                    frame,
                    align,
                    para,
                    on_path,
                },
            });
        }
        if cmds.is_empty() {
            self.status = crate::i18n::tr("status.chinese_nothing");
            return;
        }
        let name = match to {
            Script::Traditional => "To Traditional Chinese",
            Script::Simplified => "To Simplified Chinese",
        };
        if let Err(e) = self.engine.run_batch(name, &cmds) {
            self.status = e.to_string();
        }
    }

    /// Text > Chinese > Add Pinyin: over each Chinese character of the
    /// selected texts, its reading as a small text of its own, centred on
    /// the character, in the text's font and colour. A text lying on a
    /// layer is grouped with its pinyin, so they move together; a text in
    /// a group gets its pinyin beside it there. Text on a path and
    /// vertical text are left alone.
    pub fn add_pinyin(&mut self) {
        self.finish_text();
        let mut cmds = Vec::new();
        let mut made_any = false;
        for s in self.text_shapes() {
            let made = self.pinyin_shapes(&s);
            if made.is_empty() {
                continue;
            }
            made_any = true;
            let ids: Vec<ShapeId> = made.iter().map(|m| m.id).collect();
            let on_layer = self.doc().locate(s.id).ok().map(|(l, _)| l);
            let Some(layer) = on_layer.or_else(|| self.active_layer()) else {
                continue;
            };
            for shape in made {
                cmds.push(Command::AddShape { layer, shape });
            }
            if on_layer.is_some() {
                let mut members = vec![s.id];
                members.extend(ids);
                cmds.push(Command::Group { shapes: members });
            } else {
                cmds.push(Command::PlaceBeside {
                    shapes: ids,
                    reference: s.id,
                    in_front: true,
                });
            }
        }
        if !made_any {
            self.status = crate::i18n::tr("status.chinese_no_pinyin");
            return;
        }
        let before: Vec<ShapeId> = self.selected_shapes().iter().map(|s| s.id).collect();
        if let Err(e) = self.engine.run_batch("Add Pinyin", &cmds) {
            self.status = e.to_string();
            return;
        }
        // The groups made (or the texts themselves) stay selected.
        let doc = self.doc();
        let mut sel: Vec<ShapeId> = before
            .iter()
            .filter_map(|id| {
                if doc.locate(*id).is_ok() {
                    Some(*id)
                } else {
                    doc.top_level_of(*id)
                }
            })
            .collect();
        sel.dedup();
        self.select(sel);
    }

    /// The pinyin texts of one text object, placed on the page (the
    /// space of the layer).
    fn pinyin_shapes(&mut self, s: &Shape) -> Vec<Shape> {
        let ShapeKind::Text { spans, on_path, .. } = &s.kind else {
            return Vec::new();
        };
        if on_path.is_some() {
            return Vec::new();
        }
        let Some((layout, to_local)) =
            crate::text_editing::layout_of_text_shape(s, Affine::IDENTITY)
        else {
            return Vec::new();
        };
        if layout.vertical {
            return Vec::new();
        }
        let parent = self
            .doc()
            .parent_transform(s.id)
            .unwrap_or(Affine::IDENTITY);
        // Each character with the span it is written in.
        let chars: Vec<(char, &TextSpan)> = spans
            .iter()
            .flat_map(|sp| sp.text.chars().map(move |c| (c, sp)))
            .collect();
        let mut out = Vec::new();
        for line in &layout.lines {
            for i in line.start_char..line.end_char {
                let Some(&(c, sp)) = chars.get(i) else {
                    continue;
                };
                let Some(reading) = pinyin(c) else {
                    continue;
                };
                let k = i - line.start_char;
                let (Some(&x0), Some(&x1)) = (line.starts.get(k), line.ends.get(k)) else {
                    continue;
                };
                let size_pt = sp.size_pt * PINYIN_SCALE;
                let size_mm = size_pt * 25.4 / 72.0;
                let at = Point::new((x0 + x1) / 2.0, line.baseline + line.ascent + size_mm * 0.3);
                let id = self.engine.new_shape_id();
                let mut p = Shape::new(
                    id,
                    ShapeKind::Text {
                        spans: vec![TextSpan::new(reading, sp.font_family.clone(), size_pt)],
                        origin: Point::ZERO,
                        frame: None,
                        align: TextAlign::Center,
                        para: Default::default(),
                        on_path: None,
                    },
                );
                p.transform = parent * to_local * Affine::translate(at.to_vec2());
                p.fill = s.fill.clone();
                p.stroke = None;
                out.push(p);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_and_phrases_convert_both_ways() {
        assert_eq!(convert("汉字", Script::Traditional), "漢字");
        assert_eq!(convert("漢字", Script::Simplified), "汉字");
        // 发 has two answers; the phrase settles it.
        assert_eq!(convert("头发", Script::Traditional), "頭髮");
        assert_eq!(convert("发现", Script::Traditional), "發現");
        assert_eq!(convert("皇后", Script::Traditional), "皇后");
        // Letters, digits and kana stay as they are.
        assert_eq!(convert("A1 かな", Script::Traditional), "A1 かな");
        assert_eq!(convert("國語", Script::Simplified), "国语");
    }

    #[test]
    fn spans_keep_their_characters() {
        let a = TextSpan::new("头", "Sans", 20.0);
        let mut b = TextSpan::new("发很长", "Sans", 10.0);
        b.bold = true;
        let done = convert_spans(&[a, b], Script::Traditional);
        assert_eq!(done[0].text, "頭");
        assert_eq!(done[1].text, "髮很長");
        assert!(done[1].bold);
        assert_eq!(done[0].size_pt, 20.0);
    }

    #[test]
    fn readings_carry_tone_marks() {
        assert_eq!(pinyin('中'), Some("zhōng"));
        assert_eq!(pinyin('文'), Some("wén"));
        assert_eq!(pinyin('A'), None);
    }

    #[test]
    fn to_traditional_is_one_undo_step() {
        tracedraw_text::install();
        let mut app = crate::app::App::headless();
        app.start_text(Point::new(20.0, 100.0), None);
        app.text_insert("中国汉字");
        let id = app.text_edit.as_ref().expect("editing").shape;
        app.finish_text();
        app.select(vec![id]);
        app.convert_chinese(Script::Traditional);
        let text = |app: &crate::app::App| match &app.doc().find_shape(id).unwrap().kind {
            ShapeKind::Text { spans, .. } => tracedraw_core::spans_text(spans),
            _ => String::new(),
        };
        assert_eq!(text(&app), "中國漢字");
        app.convert_chinese(Script::Simplified);
        assert_eq!(text(&app), "中国汉字");
        app.undo();
        assert_eq!(text(&app), "中國漢字");
    }

    #[test]
    fn pinyin_sits_over_each_character_and_groups_with_the_text() {
        tracedraw_text::install();
        let mut app = crate::app::App::headless();
        app.start_text(Point::new(20.0, 100.0), None);
        app.text_insert("中文 ok");
        let id = app.text_edit.as_ref().expect("editing").shape;
        app.finish_text();
        app.select(vec![id]);
        let text_box = app.doc().find_shape(id).unwrap().bounds();
        app.add_pinyin();
        let sel = app.selected_shapes();
        assert_eq!(sel.len(), 1);
        let ShapeKind::Group { children } = &sel[0].kind else {
            panic!("a group: {:?}", sel[0].kind);
        };
        // The text and one reading for each of its two Chinese characters.
        assert_eq!(children.len(), 3);
        let readings: Vec<(String, tracedraw_core::Rect)> = children
            .iter()
            .filter(|c| c.id != id)
            .map(|c| match &c.kind {
                ShapeKind::Text { spans, .. } => (tracedraw_core::spans_text(spans), c.bounds()),
                _ => panic!("text"),
            })
            .collect();
        assert_eq!(readings[0].0, "zhōng");
        assert_eq!(readings[1].0, "wén");
        // Over the text (Y up), left to right, within its width.
        for (_, r) in &readings {
            assert!(r.y0 >= text_box.y1 - 0.5, "{r:?} over {text_box:?}");
            assert!(r.center().x > text_box.x0 && r.center().x < text_box.x1);
        }
        assert!(readings[0].1.center().x < readings[1].1.center().x);
        // One undo step takes it all back.
        app.undo();
        assert!(app.doc().locate(id).is_ok());
    }
}
