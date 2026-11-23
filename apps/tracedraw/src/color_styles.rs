//! The Color Styles docker's work (#778): styles and harmonies (styles
//! grouped to change together) made, swapped, merged, sorted, recoloured
//! and shaded. Each change is worked out here as the new list of styles
//! and the colour pairs the drawing follows (objects using a style's old
//! colour take its new one), then run as one undo step
//! (`App::change_color_styles`). See `docs/behavior/color-styles.md`.

use crate::app::App;
use tracedraw_core::{Color, Fill};
use tracedraw_core::{ColorStyle, Command};

/// A change: the styles after it and the colours the objects follow.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Change {
    pub styles: Vec<ColorStyle>,
    pub pairs: Vec<(Color, Color)>,
}

/// The harmonies, in the order their first style comes.
pub fn harmonies(styles: &[ColorStyle]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in styles {
        if let Some(h) = &s.harmony {
            if !out.contains(h) {
                out.push(h.clone());
            }
        }
    }
    out
}

/// The places of a harmony's styles.
pub fn members(styles: &[ColorStyle], harmony: &str) -> Vec<usize> {
    styles
        .iter()
        .enumerate()
        .filter(|(_, s)| s.harmony.as_deref() == Some(harmony))
        .map(|(i, _)| i)
        .collect()
}

/// A name `prefix N` none of `taken` has, N from 1.
pub fn fresh_name(prefix: &str, taken: &[String]) -> String {
    (1..)
        .map(|n| format!("{prefix} {n}"))
        .find(|n| !taken.contains(n))
        .unwrap_or_default()
}

/// The style names in use.
fn names(styles: &[ColorStyle]) -> Vec<String> {
    styles.iter().map(|s| s.name.clone()).collect()
}

/// Swap two styles' colours: the objects of each take the other's.
pub fn swap(styles: &[ColorStyle], a: usize, b: usize) -> Option<Change> {
    if a == b || a >= styles.len() || b >= styles.len() {
        return None;
    }
    let mut st = styles.to_vec();
    let (ca, cb) = (st[a].color, st[b].color);
    st[a].color = cb;
    st[b].color = ca;
    Some(Change {
        styles: st,
        pairs: vec![(ca, cb), (cb, ca)],
    })
}

/// Merge styles into the first of `picked`: the others go, and their
/// objects take its colour.
pub fn merge(styles: &[ColorStyle], picked: &[usize]) -> Option<Change> {
    let mut picked: Vec<usize> = picked
        .iter()
        .copied()
        .filter(|&i| i < styles.len())
        .collect();
    picked.dedup();
    let (&keep, rest) = picked.split_first()?;
    if rest.is_empty() {
        return None;
    }
    let to = styles[keep].color;
    let pairs = rest.iter().map(|&i| (styles[i].color, to)).collect();
    let st = styles
        .iter()
        .enumerate()
        .filter(|(i, _)| !rest.contains(i))
        .map(|(_, s)| s.clone())
        .collect();
    Some(Change { styles: st, pairs })
}

/// Delete styles; the objects keep their colour.
pub fn delete(styles: &[ColorStyle], picked: &[usize]) -> Change {
    Change {
        styles: styles
            .iter()
            .enumerate()
            .filter(|(i, _)| !picked.contains(i))
            .map(|(_, s)| s.clone())
            .collect(),
        pairs: Vec::new(),
    }
}

/// Put styles into a new harmony named `name`.
pub fn group(styles: &[ColorStyle], picked: &[usize], name: &str) -> Change {
    let mut st = styles.to_vec();
    for &i in picked {
        if let Some(s) = st.get_mut(i) {
            s.harmony = Some(name.to_string());
        }
    }
    // The harmony's styles side by side, where its first one was.
    if let Some(&first) = picked.iter().min() {
        let (mut inside, mut outside): (Vec<_>, Vec<_>) = (Vec::new(), Vec::new());
        for (i, s) in st.into_iter().enumerate() {
            if picked.contains(&i) {
                inside.push(s);
            } else {
                outside.push((i, s));
            }
        }
        let at = outside.iter().take_while(|(i, _)| *i < first).count();
        let mut out: Vec<ColorStyle> = outside.into_iter().map(|(_, s)| s).collect();
        for (k, s) in inside.into_iter().enumerate() {
            out.insert(at + k, s);
        }
        st = out;
    }
    Change {
        styles: st,
        pairs: Vec::new(),
    }
}

/// Take styles out of their harmony.
pub fn ungroup(styles: &[ColorStyle], picked: &[usize]) -> Change {
    let mut st = styles.to_vec();
    for &i in picked {
        if let Some(s) = st.get_mut(i) {
            s.harmony = None;
        }
    }
    Change {
        styles: st,
        pairs: Vec::new(),
    }
}

/// Change the colours of `picked` together: hue turned by `hue` degrees,
/// saturation and brightness moved by `sat` and `bright` (-1 to 1, each
/// colour keeping its place next to the others). The objects follow.
pub fn adjust(styles: &[ColorStyle], picked: &[usize], hue: f64, sat: f64, bright: f64) -> Change {
    let mut st = styles.to_vec();
    let mut pairs = Vec::new();
    for &i in picked {
        let Some(s) = st.get_mut(i) else { continue };
        let old = s.color;
        let (h, sa, b) = old.to_hsb();
        let new = Color::from_hsb((h + hue).rem_euclid(360.0), sa + sat, b + bright);
        let new = in_model_of(old, new);
        if new != old {
            s.color = new;
            pairs.push((old, new));
        }
    }
    Change { styles: st, pairs }
}

/// `c` in the colour model of `like` (CMYK styles stay CMYK).
fn in_model_of(like: Color, c: Color) -> Color {
    match like {
        Color::Cmyk { .. } => c.convert_to("CMYK"),
        Color::Gray { .. } => c.convert_to("Grayscale"),
        _ => c,
    }
}

/// Give a harmony another rule: its colours worked out again from its
/// first one (the base), a style for each colour of the rule, the
/// styles it had first keeping their names. The objects of a style kept
/// follow its new colour.
pub fn set_rule(styles: &[ColorStyle], harmony: &str, rule: crate::palette::Harmony) -> Change {
    let at = members(styles, harmony);
    let Some(&first) = at.first() else {
        return Change {
            styles: styles.to_vec(),
            pairs: Vec::new(),
        };
    };
    let base = styles[first].color;
    let colors: Vec<Color> = rule
        .colors(base)
        .into_iter()
        .map(|c| in_model_of(base, c))
        .collect();
    // The base first, then the rule's others in order.
    let mut ordered = vec![base];
    ordered.extend(colors.into_iter().filter(|c| *c != base));
    let mut pairs = Vec::new();
    let mut taken = names(styles);
    let mut made = Vec::new();
    for (k, c) in ordered.into_iter().enumerate() {
        match at.get(k) {
            Some(&i) => {
                let mut s = styles[i].clone();
                if s.color != c {
                    pairs.push((s.color, c));
                }
                s.color = c;
                made.push(s);
            }
            None => {
                let name = fresh_name(harmony, &taken);
                taken.push(name.clone());
                made.push(ColorStyle {
                    name,
                    color: c,
                    harmony: Some(harmony.to_string()),
                });
            }
        }
    }
    // Styles past the rule's count leave the harmony (kept, as styles of
    // their own, so no object loses its style).
    let n_rule = made.len();
    let mut st: Vec<ColorStyle> = Vec::new();
    for (i, s) in styles.iter().enumerate() {
        if i == first {
            st.append(&mut made);
        }
        if !at.contains(&i) {
            st.push(s.clone());
        }
    }
    for &i in at.iter().skip(n_rule) {
        let mut s = styles[i].clone();
        s.harmony = None;
        st.push(s);
    }
    Change { styles: st, pairs }
}

/// A gradient harmony from `base`: `steps` colours (2 to 20) from a
/// lighter to a darker shade of it, `spread` (0 to 1) how far apart the
/// ends are. Named `name 1`, `name 2`, ...
pub fn gradient(base: Color, steps: usize, spread: f64) -> Vec<Color> {
    let steps = steps.clamp(2, 20);
    let spread = spread.clamp(0.0, 1.0);
    let (h, s, b) = base.to_hsb();
    (0..steps)
        .map(|k| {
            // -1 (lightest) to 1 (darkest), the base in the middle.
            let t = 2.0 * k as f64 / (steps - 1) as f64 - 1.0;
            let c = if t < 0.0 {
                // Lighter: less saturation, more brightness.
                let u = -t * spread;
                Color::from_hsb(h, s * (1.0 - u), b + (1.0 - b) * u)
            } else {
                let u = t * spread;
                Color::from_hsb(h, s + (1.0 - s) * u * 0.3, b * (1.0 - u * 0.8))
            };
            in_model_of(base, c)
        })
        .collect()
}

/// Sort the styles: by name, or by hue (greys first, by brightness).
/// Harmonies stay together, in the place of their first style.
pub fn sort(styles: &[ColorStyle], by_hue: bool) -> Vec<ColorStyle> {
    let key = |s: &ColorStyle| -> (u8, i64, String) {
        let (h, sa, b) = s.color.to_hsb();
        if by_hue {
            if sa < 0.05 {
                (0, (b * 1000.0) as i64, s.name.to_lowercase())
            } else {
                (1, (h * 10.0) as i64, s.name.to_lowercase())
            }
        } else {
            (0, 0, s.name.to_lowercase())
        }
    };
    let mut loose: Vec<ColorStyle> = styles
        .iter()
        .filter(|s| s.harmony.is_none())
        .cloned()
        .collect();
    loose.sort_by_key(key);
    let mut out = loose;
    for h in harmonies(styles) {
        let mut inside: Vec<ColorStyle> = styles
            .iter()
            .filter(|s| s.harmony.as_deref() == Some(h.as_str()))
            .cloned()
            .collect();
        inside.sort_by_key(key);
        out.extend(inside);
    }
    out
}

/// Styles in another colour model (`"CMYK"` or `"RGB"`); the objects
/// follow. Spot colours stay.
pub fn convert(styles: &[ColorStyle], picked: &[usize], model: &str) -> Change {
    let mut st = styles.to_vec();
    let mut pairs = Vec::new();
    for &i in picked {
        let Some(s) = st.get_mut(i) else { continue };
        if matches!(s.color, Color::Spot { .. } | Color::Registration) {
            continue;
        }
        let new = s.color.convert_to(model);
        if new != s.color {
            pairs.push((s.color, new));
            s.color = new;
        }
    }
    Change { styles: st, pairs }
}

/// Every colour the drawing's objects use (fills of one colour, the
/// stops of gradients, outlines), each once, in the order met.
pub fn document_colors(doc: &tracedraw_core::Document) -> Vec<Color> {
    let mut out: Vec<Color> = Vec::new();
    let mut add = |c: Color| {
        if !matches!(c, Color::Registration) && !out.contains(&c) {
            out.push(c);
        }
    };
    fn walk(s: &tracedraw_core::Shape, add: &mut dyn FnMut(Color)) {
        let mut s = s.clone();
        let seen = std::cell::RefCell::new(Vec::new());
        tracedraw_core::color_convert::map_shape_colors(&mut s, &|c| {
            seen.borrow_mut().push(c);
            c
        });
        for c in seen.into_inner() {
            add(c);
        }
    }
    for l in doc.all_layers() {
        for s in &l.shapes {
            walk(s, &mut add);
        }
    }
    out
}

/// The styles no object of the drawing uses.
pub fn unused(styles: &[ColorStyle], doc: &tracedraw_core::Document) -> Vec<usize> {
    let used = document_colors(doc);
    styles
        .iter()
        .enumerate()
        .filter(|(_, s)| !used.contains(&s.color))
        .map(|(i, _)| i)
        .collect()
}

/// Whether `s` (or what it holds) uses colour `c` as its fill or outline.
fn uses(s: &tracedraw_core::Shape, c: Color) -> bool {
    let mut s = s.clone();
    let hit = std::cell::Cell::new(false);
    tracedraw_core::color_convert::map_shape_colors(&mut s, &|x| {
        if x == c {
            hit.set(true);
        }
        x
    });
    hit.get()
}

impl App {
    /// Run a change of the styles as one undo step, the drawing's
    /// objects following its colour pairs.
    pub fn change_color_styles(&mut self, change: Change) {
        if change.styles == self.doc().color_styles && change.pairs.is_empty() {
            return;
        }
        let mut cmds = vec![Command::SetColorStyles {
            styles: change.styles,
        }];
        if !change.pairs.is_empty() {
            cmds.push(Command::ReplaceColors {
                pairs: change.pairs,
                shapes: Vec::new(),
            });
        }
        let _ = self.engine.run_batch("Color Styles", &cmds);
        let n = self.doc().color_styles.len();
        self.color_styles_sel.retain(|&i| i < n);
    }

    /// The objects of the active page using any of the picked styles'
    /// colours, selected. Returns how many.
    pub fn select_color_style_objects(&mut self, picked: &[usize]) -> usize {
        let colors: Vec<Color> = picked
            .iter()
            .filter_map(|&i| self.doc().color_styles.get(i).map(|s| s.color))
            .collect();
        let page = self.page_index();
        let ids: Vec<tracedraw_core::ShapeId> = self
            .doc()
            .pages
            .get(page)
            .into_iter()
            .flat_map(|p| p.layers.iter())
            .filter(|l| l.visible && !l.locked)
            .flat_map(|l| l.shapes.iter())
            .filter(|s| colors.iter().any(|&c| uses(s, c)))
            .map(|s| s.id)
            .collect();
        let n = ids.len();
        self.select(ids);
        n
    }

    /// New styles of every colour the drawing uses that has none yet.
    /// Returns how many.
    pub fn color_styles_from_document(&mut self) -> usize {
        let colors = document_colors(self.doc());
        self.add_color_styles(&colors, false)
    }

    /// A gradient harmony from a style's colour; returns its name.
    pub fn add_color_gradient(&mut self, base: Color, steps: usize, spread: f64) -> String {
        let mut st = self.doc().color_styles.clone();
        let harmony = fresh_name(&crate::i18n::tr("docker.cs_gradient"), &harmonies(&st));
        let mut taken = names(&st);
        for c in gradient(base, steps, spread) {
            let name = fresh_name(&harmony, &taken);
            taken.push(name.clone());
            st.push(ColorStyle {
                name,
                color: c,
                harmony: Some(harmony.clone()),
            });
        }
        self.change_color_styles(Change {
            styles: st,
            pairs: Vec::new(),
        });
        harmony
    }

    /// Fill or outline the selection with a style.
    pub fn apply_color_style(&mut self, i: usize, outline: bool) {
        let Some(c) = self.doc().color_styles.get(i).map(|s| s.color) else {
            return;
        };
        if outline {
            self.apply_outline_color(Some(c));
        } else {
            self.apply_fill(Fill::Solid(c));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Harmony;

    fn style(name: &str, c: Color, h: Option<&str>) -> ColorStyle {
        ColorStyle {
            name: name.into(),
            color: c,
            harmony: h.map(|s| s.to_string()),
        }
    }

    fn red() -> Color {
        Color::rgb8(255, 0, 0)
    }
    fn blue() -> Color {
        Color::rgb8(0, 0, 255)
    }
    fn green() -> Color {
        Color::rgb8(0, 160, 0)
    }

    #[test]
    fn swap_and_merge_say_what_the_objects_follow() {
        let st = vec![
            style("A", red(), None),
            style("B", blue(), None),
            style("C", green(), None),
        ];
        let c = swap(&st, 0, 1).expect("swap");
        assert_eq!(c.styles[0].color, blue());
        assert_eq!(c.styles[1].color, red());
        assert_eq!(c.pairs, vec![(red(), blue()), (blue(), red())]);
        assert!(swap(&st, 1, 1).is_none());

        let m = merge(&st, &[1, 0, 2]).expect("merge");
        assert_eq!(m.styles.len(), 1);
        assert_eq!(m.styles[0].name, "B");
        assert_eq!(m.pairs, vec![(red(), blue()), (green(), blue())]);
        assert!(merge(&st, &[2]).is_none());
    }

    #[test]
    fn grouping_keeps_a_harmony_together_and_adjusting_turns_it() {
        let st = vec![
            style("A", red(), None),
            style("B", blue(), None),
            style("C", green(), None),
        ];
        let g = group(&st, &[0, 2], "Harmony 1");
        let order: Vec<&str> = g.styles.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(order, ["A", "C", "B"]);
        assert_eq!(members(&g.styles, "Harmony 1"), vec![0, 1]);
        assert_eq!(harmonies(&g.styles), vec!["Harmony 1".to_string()]);

        let a = adjust(&g.styles, &[0, 1], 120.0, 0.0, 0.0);
        let (h, _, _) = a.styles[0].color.to_hsb();
        assert!((h - 120.0).abs() < 1.0, "{h}");
        assert_eq!(a.pairs.len(), 2);
        assert_eq!(a.styles[2].color, blue(), "B is not in it");

        let u = ungroup(&g.styles, &[0]);
        assert_eq!(u.styles[0].harmony, None);
        assert_eq!(u.styles[1].harmony.as_deref(), Some("Harmony 1"));
    }

    #[test]
    fn a_new_rule_works_the_harmony_out_again_from_its_base() {
        let st = vec![
            style("X", green(), None),
            style("H1", red(), Some("H")),
            style("H2", blue(), Some("H")),
        ];
        let c = set_rule(&st, "H", Harmony::Triadic);
        let inside = members(&c.styles, "H");
        assert_eq!(inside.len(), 3);
        assert_eq!(c.styles[inside[0]].color, red(), "the base stays");
        assert_eq!(c.styles[inside[1]].name, "H2", "names kept");
        assert_eq!(c.styles[0].name, "X");
        // H2's objects follow its new colour.
        assert_eq!(c.pairs.len(), 1);
        assert_eq!(c.pairs[0].0, blue());

        // A rule of fewer colours leaves the extra styles out of it.
        let c2 = set_rule(&c.styles, "H", Harmony::Complementary);
        assert_eq!(members(&c2.styles, "H").len(), 2);
        assert_eq!(c2.styles.len(), 4);
    }

    #[test]
    fn gradients_run_light_to_dark_and_sorting_keeps_harmonies() {
        let g = gradient(blue(), 5, 0.8);
        assert_eq!(g.len(), 5);
        let lum: Vec<f32> = g.iter().map(|c| c.luminance()).collect();
        assert!(lum.windows(2).all(|w| w[0] >= w[1]), "{lum:?}");
        assert_eq!(gradient(blue(), 1, 0.5).len(), 2);

        let st = vec![
            style("b", blue(), None),
            style("h2", green(), Some("H")),
            style("a", red(), None),
            style("h1", red(), Some("H")),
        ];
        let by_name: Vec<String> = sort(&st, false).into_iter().map(|s| s.name).collect();
        assert_eq!(by_name, ["a", "b", "h1", "h2"]);
        let by_hue: Vec<String> = sort(&st, true).into_iter().map(|s| s.name).collect();
        assert_eq!(by_hue, ["a", "b", "h1", "h2"]);
    }

    #[test]
    fn styles_follow_the_drawing_and_the_drawing_follows_them() {
        let mut app = App::headless();
        app.engine
            .run(&Command::SetColorStyles {
                styles: vec![style("A", red(), None), style("B", blue(), None)],
            })
            .unwrap();
        let id = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        app.apply_fill(Fill::Solid(red()));
        app.select(Vec::new());
        assert_eq!(unused(&app.doc().color_styles, app.doc()), vec![1]);
        assert_eq!(app.select_color_style_objects(&[0]), 1);
        assert_eq!(app.selection, vec![id]);
        // Swapping recolours the rectangle in one undo step.
        let c = swap(&app.doc().color_styles, 0, 1).unwrap();
        app.change_color_styles(c);
        let fill = app.doc().find_shape(id).unwrap().fill.clone();
        assert_eq!(fill, Fill::Solid(blue()));
        app.undo();
        assert_eq!(app.doc().find_shape(id).unwrap().fill, Fill::Solid(red()));
        assert_eq!(app.doc().color_styles[0].color, red());
        // New from document adds the colours without a style: the red
        // fill and the outline.
        app.engine
            .run(&Command::SetColorStyles {
                styles: vec![style("A", red(), None)],
            })
            .unwrap();
        let outline = app.doc().find_shape(id).unwrap().stroke.clone();
        assert_eq!(
            app.color_styles_from_document(),
            usize::from(outline.is_some())
        );
        // A gradient harmony.
        let h = app.add_color_gradient(blue(), 4, 0.5);
        assert_eq!(members(&app.doc().color_styles, &h).len(), 4);
    }
}
