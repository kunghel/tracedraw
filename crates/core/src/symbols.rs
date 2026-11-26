//! Symbolize Similar: turning every group that copies a sample group
//! into an instance of one symbol, in place (#2343). The similarity
//! measure, the plan and the replacement shapes live here so the Symbols
//! panel's button and the scripting host's `symbolize_groups` share one
//! behaviour; each executor runs the plan as commands of its own.

use crate::command::Command;
use crate::document::{Shape, ShapeKind, Symbol};
use crate::geometry::{Affine, Point, Rect};
use crate::{Document, Fill, ShapeId};

/// What an object is, as the similarity measure reads it: the same tags
/// the Objects docker shows, so a curve and a rectangle never match.
fn kind_tag(k: &ShapeKind) -> &'static str {
    match k {
        ShapeKind::Rect { .. } => "Rectangle",
        ShapeKind::Ellipse { .. } => "Ellipse",
        ShapeKind::Polygon {
            complex: Some(_), ..
        } => "ComplexStar",
        ShapeKind::Polygon { sharpness, .. }
            if crate::geometry::is_star_sharpness(*sharpness) =>
        {
            "Star"
        }
        ShapeKind::Polygon { .. } => "Polygon",
        ShapeKind::Path { .. } => "Curve",
        ShapeKind::Text { .. } => "Text",
        ShapeKind::Group { .. } => "Group",
        ShapeKind::Bitmap { .. } => "Bitmap",
        ShapeKind::ClipFrame { .. } => "ClipFrame",
        ShapeKind::Table(_) => "Table",
        ShapeKind::SymbolInstance { .. } => "Symbol",
    }
}

/// What makes two groups copies of each other: each object that is not
/// text, in order, by its kind, its box from the corner of their common
/// box (to a tenth of a millimetre) and its fill. A copy only moved (by
/// itself or its objects) matches; a recoloured or reshaped one does not.
/// The corner comes back with the key: the instance is placed there.
pub fn fingerprint(objects: &[&Shape]) -> Option<(String, Point)> {
    let boxes: Vec<Rect> = objects.iter().map(|s| s.bounds()).collect();
    let all = boxes.iter().copied().reduce(|a, b| a.union(b))?;
    if !all.is_finite() {
        return None;
    }
    let origin = Point::new(all.x0, all.y0);
    let tenth = |v: f64| (v * 10.0).round() as i64;
    let mut out = String::new();
    for (s, b) in objects.iter().zip(&boxes) {
        let fill = serde_json::to_string(&s.fill).unwrap_or_default();
        out.push_str(&format!(
            "{}|{}|{}|{}|{}|{fill};",
            kind_tag(&s.kind),
            tenth(b.x0 - origin.x),
            tenth(b.y0 - origin.y),
            tenth(b.width()),
            tenth(b.height()),
        ));
    }
    Some((out, origin))
}

fn is_text(s: &Shape) -> bool {
    matches!(s.kind, ShapeKind::Text { .. })
}

/// A group's objects that are not text, and its text objects: the labels
/// stay editable beside the instance rather than baked into the symbol.
pub fn split_group(s: &Shape) -> Option<(Vec<&Shape>, Vec<&Shape>)> {
    let ShapeKind::Group { children } = &s.kind else {
        return None;
    };
    Some(children.iter().partition(|c| !is_text(c)))
}

/// The objects a symbol draws, for matching: a group's objects that are
/// not text, or the one object.
pub fn definition_objects(def: &Shape) -> Vec<&Shape> {
    match split_group(def) {
        Some((graphics, _)) => graphics,
        None => vec![def],
    }
}

/// One group the plan replaces: the group as it stands, its labels, and
/// the corner of its graphics' common box in its own space.
pub struct Match {
    pub group: Shape,
    pub texts: Vec<Shape>,
    pub at: Point,
}

/// What Symbolize Similar will do: the symbol to use (an existing one
/// that draws the same, or a new definition to add), and every group to
/// replace, outermost first so a group inside a matched one is left alone.
pub struct Plan {
    pub index: usize,
    pub name: String,
    pub new_symbol: Option<Symbol>,
    pub matches: Vec<Match>,
    pub with_text: usize,
    pub skipped: usize,
}

/// Plan the symbolize of every group copying `sample`, at any depth of
/// layers, groups and clip frames, master layers included. Each match is
/// replaced where it stands, so an instance keeps the place, the parent
/// and the order its group had.
pub fn plan(doc: &Document, sample: ShapeId, name: Option<&str>) -> Result<Plan, String> {
    let s = doc
        .find_shape(sample)
        .ok_or_else(|| "symbolize needs an object of the drawing".to_string())?;
    let (graphics, _) = split_group(s).ok_or("symbolize needs a group")?;
    plan_graphics(doc, &graphics, name)
}

/// The same plan for a set of objects taken as the sample directly: a
/// selection that is not a group (loose objects, or a symbol instance's
/// definition) can be symbolized by similarity too.
pub fn plan_graphics(
    doc: &Document,
    graphics: &[&Shape],
    name: Option<&str>,
) -> Result<Plan, String> {
    let (key, origin) = fingerprint(graphics).ok_or("the group has no objects to match")?;
    // An existing symbol that draws the same, else a new one appended.
    let found = doc.symbols.iter().position(|sym| {
        fingerprint(&definition_objects(&sym.shape)).is_some_and(|(k, _)| k == key)
    });
    let index = found.unwrap_or(doc.symbols.len());
    let name = match found {
        Some(i) => doc.symbols[i].name.clone(),
        None => match name {
            Some(n) if !n.trim().is_empty() => n.to_string(),
            _ => format!("Symbol {}", index + 1),
        },
    };
    let new_symbol = found.is_none().then(|| {
        let children: Vec<Shape> = graphics.iter().map(|c| (*c).clone()).collect();
        let mut def = Shape::new(ShapeId(0), ShapeKind::Group { children });
        def.fill = Fill::None;
        def.stroke = None;
        def.transform_by(Affine::translate((-origin.x, -origin.y)));
        Symbol {
            name: name.clone(),
            shape: def,
        }
    });
    let mut matches = Vec::new();
    let mut skipped = 0usize;
    fn walk(s: &Shape, key: &str, loose: bool, out: &mut Vec<Match>, skipped: &mut usize) {
        match &s.kind {
            ShapeKind::Group { children } => {
                let (graphics, texts) = match split_group(s) {
                    Some(p) => p,
                    None => return,
                };
                match fingerprint(&graphics) {
                    Some((k, at)) if k == key => {
                        out.push(Match {
                            group: s.clone(),
                            texts: texts.into_iter().cloned().collect(),
                            at,
                        });
                        // A matched group's own members are not matched
                        // again: the instance stands for the whole group.
                        return;
                    }
                    _ => *skipped += 1,
                }
                for c in children {
                    walk(c, key, loose, out, skipped);
                }
            }
            ShapeKind::ClipFrame { frame, contents } => {
                walk(frame, key, loose, out, skipped);
                for c in contents {
                    walk(c, key, loose, out, skipped);
                }
            }
            _ => {
                // A one-object sample also matches single congruent objects
                // lying loose, not only groups copying it.
                if loose && !matches!(s.kind, ShapeKind::Text { .. }) {
                    if let Some((k, at)) = fingerprint(&[s]) {
                        if k == key {
                            out.push(Match {
                                group: s.clone(),
                                texts: Vec::new(),
                                at,
                            });
                        }
                    }
                }
            }
        }
    }
    let loose = graphics.len() == 1;
    for l in doc.all_layers() {
        for s in &l.shapes {
            walk(s, &key, loose, &mut matches, &mut skipped);
        }
    }
    let with_text = matches.iter().filter(|m| !m.texts.is_empty()).count();
    Ok(Plan {
        index,
        name,
        new_symbol,
        matches,
        with_text,
        skipped,
    })
}

/// The shape that replaces a matched group: an instance placed at the
/// graphics' corner, or, when the group carried labels, a group of the
/// instance and those labels, keeping the group's transform and name.
/// The group's own transparency and shadow go on what replaces it, so a
/// copy draws as it did (#3497). `inst_id` and `group_id` come from the
/// executor's id source.
pub fn replacement(m: &Match, index: usize, inst_id: ShapeId, group_id: ShapeId) -> Shape {
    let place = Affine::translate((m.at.x, m.at.y));
    let mut inst = Shape::new(inst_id, ShapeKind::SymbolInstance { index });
    if m.texts.is_empty() {
        inst.transform = m.group.transform * place;
        inst.opacity = m.group.opacity;
        inst.shadow = m.group.shadow;
        return inst;
    }
    inst.transform = place;
    let mut children = vec![inst];
    children.extend(m.texts.iter().cloned());
    let mut grouped = Shape::new(group_id, ShapeKind::Group { children });
    grouped.transform = m.group.transform;
    grouped.name = m.group.name.clone();
    grouped.fill = Fill::None;
    grouped.stroke = None;
    grouped.opacity = m.group.opacity;
    grouped.shadow = m.group.shadow;
    grouped
}

/// The commands a plan runs, in order: the new definition when there is
/// one, then every replacement in place. Ids are minted by `mint`, twice
/// per match with labels (instance and group) and once otherwise.
pub fn commands(plan: &Plan, mut mint: impl FnMut() -> ShapeId) -> Vec<Command> {
    let mut cmds = Vec::new();
    if let Some(sym) = &plan.new_symbol {
        cmds.push(Command::AddSymbol { symbol: sym.clone() });
    }
    let replacements: Vec<(ShapeId, Shape)> = plan
        .matches
        .iter()
        .map(|m| {
            let inst = mint();
            let group = if m.texts.is_empty() { inst } else { mint() };
            (m.group.id, replacement(m, plan.index, inst, group))
        })
        .collect();
    if !replacements.is_empty() {
        cmds.push(Command::ReplaceShapes { replacements });
    }
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;

    fn rect(id: u64, x: f64, y: f64, color: crate::Color) -> Shape {
        let mut s = Shape::new(
            ShapeId(id),
            ShapeKind::Rect {
                rect: Rect::new(x, y, x + 5.0, y + 5.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(color);
        s
    }
    fn red() -> crate::Color {
        crate::Color::rgb8(200, 30, 30)
    }
    fn blue() -> crate::Color {
        crate::Color::rgb8(30, 30, 200)
    }

    /// A drawing with the sample group, a moved copy, a copy nested in
    /// another group, a copy with a label, and a recoloured non-copy.
    fn drawing() -> (Engine, ShapeId) {
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let sample = Shape::new(
            ShapeId(1),
            ShapeKind::Group {
                children: vec![rect(2, 0.0, 0.0, red()), rect(3, 10.0, 0.0, red())],
            },
        );
        e.run(&Command::AddShape { layer, shape: sample }).unwrap();
        let mut moved = Shape::new(
            ShapeId(4),
            ShapeKind::Group {
                children: vec![rect(5, 0.0, 0.0, red()), rect(6, 10.0, 0.0, red())],
            },
        );
        moved.transform = Affine::translate((50.0, 50.0));
        e.run(&Command::AddShape { layer, shape: moved }).unwrap();
        let nested = Shape::new(
            ShapeId(7),
            ShapeKind::Group {
                children: vec![rect(8, 0.0, 0.0, red()), rect(9, 10.0, 0.0, red())],
            },
        );
        let holder = Shape::new(ShapeId(10), ShapeKind::Group { children: vec![nested] });
        e.run(&Command::AddShape { layer, shape: holder }).unwrap();
        let mut label = Shape::new(
            ShapeId(12),
            ShapeKind::Text {
                spans: vec![serde_json::from_str(
                    r#"{"text":"tag","font_family":"Arial","size_pt":10,"bold":false,"italic":false}"#,
                )
                .expect("span")],
                origin: Point::new(0.0, 8.0),
                frame: None,
                align: crate::TextAlign::default(),
                para: crate::ParagraphStyle::default(),
                on_path: None,
            },
        );
        label.transform = Affine::translate((0.0, 8.0));
        let with_label = Shape::new(
            ShapeId(11),
            ShapeKind::Group {
                children: vec![rect(13, 0.0, 0.0, red()), rect(14, 10.0, 0.0, red()), label],
            },
        );
        e.run(&Command::AddShape { layer, shape: with_label }).unwrap();
        let recoloured = Shape::new(
            ShapeId(15),
            ShapeKind::Group {
                children: vec![rect(16, 0.0, 0.0, blue()), rect(17, 10.0, 0.0, blue())],
            },
        );
        e.run(&Command::AddShape { layer, shape: recoloured }).unwrap();
        (e, ShapeId(1))
    }

    /// A one-object sample matches single congruent objects lying loose,
    /// not only groups: same kind, box and fill match; a recoloured copy
    /// does not.
    #[test]
    fn a_one_object_sample_matches_loose_copies() {
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        for (id, x, color) in [(1u64, 0.0, red()), (2, 40.0, red()), (3, 80.0, blue())] {
            let mut s = rect(id, x, 0.0, color);
            s.kind = ShapeKind::Rect {
                rect: Rect::new(x, 0.0, x + 20.0, 10.0),
                radius: 0.0,
                corners: None,
            };
            e.run(&Command::AddShape { layer, shape: s }).unwrap();
        }
        let s1 = e.document().find_shape(ShapeId(1)).expect("sample");
        let plan = plan_graphics(e.document(), &[s1], None).expect("plan");
        assert_eq!(plan.matches.len(), 2, "the sample and its congruent copy");
        assert!(plan.new_symbol.is_some());
    }

    /// The plan matches the sample, the moved copy, the nested copy and
    /// the labelled copy, but not the recoloured one; the nested match is
    /// replaced inside its holder, and the label stays text beside the
    /// instance. One undo puts every group back.
    #[test]
    fn symbolize_matches_at_every_depth_and_keeps_labels() {
        let (mut e, sample) = drawing();
        let plan = plan(e.document(), sample, None).expect("plan");
        assert_eq!(plan.matches.len(), 4, "sample, moved, nested, labelled");
        assert_eq!(plan.with_text, 1);
        assert!(plan.skipped >= 1, "the recoloured group is skipped");
        assert!(plan.new_symbol.is_some());
        assert_eq!(plan.index, 0);
        let cmds = commands(&plan, || e.new_shape_id());
        let mark = e.begin_group();
        for c in &cmds {
            e.run(c).unwrap();
        }
        e.end_group(mark, "Symbolize Similar");
        assert_eq!(e.document().symbols.len(), 1);
        // The nested copy became an instance inside its holder.
        let holder = e.document().find_shape(ShapeId(10)).unwrap();
        let ShapeKind::Group { children } = &holder.kind else {
            panic!("holder")
        };
        assert_eq!(children.len(), 1);
        assert!(matches!(children[0].kind, ShapeKind::SymbolInstance { index: 0 }));
        // The labelled copy kept its label as text beside the instance:
        // the replacement stands where the group stood (fourth on the
        // layer), under a fresh id.
        let labelled = &e.document().pages[0].layers[0].shapes[3];
        let ShapeKind::Group { children } = &labelled.kind else {
            panic!("labelled")
        };
        assert_eq!(children.len(), 2);
        assert!(matches!(children[0].kind, ShapeKind::SymbolInstance { index: 0 }));
        assert!(matches!(children[1].kind, ShapeKind::Text { .. }));
        // The recoloured group is untouched.
        let rec = e.document().find_shape(ShapeId(15)).unwrap();
        assert!(matches!(rec.kind, ShapeKind::Group { .. }));
        e.undo();
        assert_eq!(e.document().symbols.len(), 0);
        let holder = e.document().find_shape(ShapeId(10)).unwrap();
        let ShapeKind::Group { children } = &holder.kind else {
            panic!("holder")
        };
        assert!(matches!(children[0].kind, ShapeKind::Group { .. }));
    }

    /// An existing symbol that draws the same is reused: no new definition,
    /// and the instances point at it.
    #[test]
    fn symbolize_reuses_a_symbol_that_draws_the_same() {
        let (mut e, sample) = drawing();
        let first = plan(e.document(), sample, None).expect("plan");
        for c in &commands(&first, || e.new_shape_id()) {
            e.run(c).unwrap();
        }
        // A fresh moved copy of the sample, after the symbol exists.
        let layer = e.document().pages[0].layers[0].id;
        let mut copy = Shape::new(
            ShapeId(100),
            ShapeKind::Group {
                children: vec![rect(101, 0.0, 0.0, red()), rect(102, 10.0, 0.0, red())],
            },
        );
        copy.transform = Affine::translate((80.0, 80.0));
        e.run(&Command::AddShape { layer, shape: copy }).unwrap();
        let again = plan(e.document(), ShapeId(100), Some("Other")).expect("plan");
        assert!(again.new_symbol.is_none(), "the existing symbol is reused");
        assert_eq!(again.index, 0);
        assert_eq!(again.name, "Symbol 1", "the existing name stays");
        assert_eq!(again.matches.len(), 1, "only the new copy matches");
    }
    /// A copy keeps how it drew as a whole: the group's transparency and
    /// shadow go on the instance that replaces it, or on the group of the
    /// instance and its labels (#3497).
    #[test]
    fn the_replacement_keeps_the_groups_opacity_and_shadow() {
        let mut group = Shape::new(
            ShapeId(1),
            ShapeKind::Group {
                children: vec![rect(2, 0.0, 0.0, red())],
            },
        );
        group.opacity = 0.4;
        group.shadow = Some(crate::Shadow::default());
        let m = Match {
            group: group.clone(),
            texts: Vec::new(),
            at: Point::new(0.0, 0.0),
        };
        let inst = replacement(&m, 0, ShapeId(10), ShapeId(11));
        assert!(matches!(inst.kind, ShapeKind::SymbolInstance { index: 0 }));
        assert_eq!(inst.opacity, 0.4);
        assert_eq!(inst.shadow, group.shadow);
        let labelled = Match {
            texts: vec![rect(3, 0.0, 8.0, blue())],
            ..m
        };
        let grouped = replacement(&labelled, 0, ShapeId(12), ShapeId(13));
        assert!(matches!(grouped.kind, ShapeKind::Group { .. }));
        assert_eq!(grouped.opacity, 0.4);
        assert_eq!(grouped.shadow, group.shadow);
    }
}
