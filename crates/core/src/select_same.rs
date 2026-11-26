//! Attribute matching for the Select Same panel. Queries never modify a drawing.

use crate::{Affine, Document, Error, LayerId, PageId, Result, Shape, ShapeId, ShapeKind, Symbol};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// How the page-space width and height compare with a reference object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeComparison {
    #[default]
    Equal,
    Larger,
    Smaller,
}

/// Enabled properties are combined with AND for each reference, then references
/// are combined with OR. All geometry and its tolerance use millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectSameOptions {
    pub fill: bool,
    pub kind: bool,
    pub stroke_color: bool,
    pub stroke_width: bool,
    pub size: bool,
    pub font: bool,
    pub font_size: bool,
    pub size_comparison: SizeComparison,
    pub tolerance_mm: f64,
    /// Skip groups and clip frames together with all their descendants.
    pub exclude_containers: bool,
    pub current_layer_only: bool,
}

impl Default for SelectSameOptions {
    fn default() -> Self {
        Self {
            fill: true,
            kind: true,
            stroke_color: false,
            stroke_width: false,
            size: false,
            font: false,
            font_size: false,
            size_comparison: SizeComparison::Equal,
            tolerance_mm: 0.5,
            exclude_containers: false,
            current_layer_only: false,
        }
    }
}

impl SelectSameOptions {
    pub fn has_criteria(&self) -> bool {
        self.fill
            || self.kind
            || self.stroke_color
            || self.stroke_width
            || self.size
            || self.font
            || self.font_size
    }
}

struct Object<'a> {
    shape: &'a Shape,
    parent: Affine,
    layer: LayerId,
    ancestors: Vec<ShapeId>,
    size: Option<(f64, f64)>,
}

fn is_container(shape: &Shape) -> bool {
    matches!(
        shape.kind,
        ShapeKind::Group { .. } | ShapeKind::ClipFrame { .. }
    )
}

/// A container holding a locked object cannot itself be selected. Its unlocked
/// contents remain independently selectable, including when a sibling is hidden.
fn collect_locked_containers(shape: &Shape, out: &mut HashSet<ShapeId>) -> bool {
    if shape.locked {
        return true;
    }
    let holds_locked = match &shape.kind {
        ShapeKind::Group { children } => children.iter().fold(false, |held, child| {
            collect_locked_containers(child, out) | held
        }),
        ShapeKind::ClipFrame { frame, contents } => contents
            .iter()
            .fold(collect_locked_containers(frame, out), |held, child| {
                collect_locked_containers(child, out) | held
            }),
        _ => false,
    };
    if holds_locked {
        out.insert(shape.id);
    }
    holds_locked
}

fn collect<'a>(
    shape: &'a Shape,
    above: Affine,
    layer: LayerId,
    ancestors: &mut Vec<ShapeId>,
    options: &SelectSameOptions,
    symbols: &[Symbol],
    out: &mut Vec<Object<'a>>,
) {
    if !shape.visible || shape.locked || (options.exclude_containers && is_container(shape)) {
        return;
    }
    let transform = above * shape.transform;
    let mut object = Object {
        shape,
        parent: above,
        layer,
        ancestors: ancestors.clone(),
        size: None,
    };
    if options.size {
        object.size = size(&object, symbols);
    }
    out.push(object);
    ancestors.push(shape.id);
    match &shape.kind {
        ShapeKind::Group { children }
        | ShapeKind::ClipFrame {
            contents: children, ..
        } => {
            for child in children {
                collect(child, transform, layer, ancestors, options, symbols, out);
            }
        }
        _ => {}
    }
    ancestors.pop();
}

/// Resolve symbols only in the temporary shape measured by this query. A
/// missing or cyclic definition has no measurable size rather than a unit box.
fn resolved_shape(
    shape: &Shape,
    symbols: &[Symbol],
    seen: &mut HashSet<usize>,
    depth: usize,
) -> Option<Shape> {
    if depth > 128 {
        return None;
    }
    let mut out = shape.clone();
    match &shape.kind {
        ShapeKind::SymbolInstance { index } => {
            if !seen.insert(*index) {
                return None;
            }
            let definition = symbols.get(*index)?;
            let child = resolved_shape(&definition.shape, symbols, seen, depth + 1)?;
            seen.remove(index);
            out.kind = ShapeKind::Group {
                children: vec![child],
            };
        }
        ShapeKind::Group { children } => {
            let children = children
                .iter()
                .map(|child| resolved_shape(child, symbols, seen, depth + 1))
                .collect::<Option<Vec<_>>>()?;
            out.kind = ShapeKind::Group { children };
        }
        ShapeKind::ClipFrame { frame, contents } => {
            let frame = Box::new(resolved_shape(frame, symbols, seen, depth + 1)?);
            let contents = contents
                .iter()
                .map(|child| resolved_shape(child, symbols, seen, depth + 1))
                .collect::<Option<Vec<_>>>()?;
            out.kind = ShapeKind::ClipFrame { frame, contents };
        }
        _ => {}
    }
    Some(out)
}

fn size(object: &Object<'_>, symbols: &[Symbol]) -> Option<(f64, f64)> {
    let mut shape = resolved_shape(object.shape, symbols, &mut HashSet::new(), 0)?;
    shape.transform_by(object.parent);
    let bounds = shape.own_visual_bounds();
    let (w, h) = (bounds.width().abs(), bounds.height().abs());
    (w.is_finite() && h.is_finite()).then_some((w, h))
}

fn text_matches(reference: &Shape, candidate: &Shape, options: &SelectSameOptions) -> bool {
    let (ShapeKind::Text { spans: a, .. }, ShapeKind::Text { spans: b, .. }) =
        (&reference.kind, &candidate.kind)
    else {
        return false;
    };
    a.iter().filter(|s| !s.text.trim().is_empty()).any(|r| {
        b.iter().filter(|s| !s.text.trim().is_empty()).any(|c| {
            (!options.font
                || r.font_family.trim().to_lowercase() == c.font_family.trim().to_lowercase())
                && (!options.font_size
                    || (r.size_pt.is_finite()
                        && c.size_pt.is_finite()
                        && (r.size_pt - c.size_pt).abs() <= 0.01))
        })
    })
}

fn matches(reference: &Object<'_>, candidate: &Object<'_>, options: &SelectSameOptions) -> bool {
    let (a, b) = (reference.shape, candidate.shape);
    if options.fill && a.fill != b.fill {
        return false;
    }
    if options.kind && crate::find::ObjectType::of(&a.kind) != crate::find::ObjectType::of(&b.kind)
    {
        return false;
    }
    if options.stroke_color
        && a.stroke.as_ref().map(|s| s.color) != b.stroke.as_ref().map(|s| s.color)
    {
        return false;
    }
    if options.stroke_width {
        match (&a.stroke, &b.stroke) {
            (None, None) => {}
            (Some(a), Some(b)) if (a.width - b.width).abs() <= crate::find::WIDTH_TOLERANCE => {}
            _ => return false,
        }
    }
    if (options.font || options.font_size) && !text_matches(a, b, options) {
        return false;
    }
    if options.size {
        let (Some((aw, ah)), Some((bw, bh))) = (reference.size, candidate.size) else {
            return false;
        };
        let t = options.tolerance_mm;
        let fits = match options.size_comparison {
            SizeComparison::Equal => (aw - bw).abs() <= t && (ah - bh).abs() <= t,
            SizeComparison::Larger => bw >= aw - t && bh >= ah - t && (bw > aw + t || bh > ah + t),
            SizeComparison::Smaller => bw <= aw + t && bh <= ah + t && (bw < aw - t || bh < ah - t),
        };
        if !fits {
            return false;
        }
    }
    true
}

/// Objects available on `page`, including group and clip contents. Invisible or
/// locked layers and branches are excluded. A matching container represents its
/// contents, so a selection never contains both an ancestor and its descendant.
/// With container exclusion enabled, their complete branches are skipped before
/// collecting either candidates or references.
pub fn matching_ids(
    doc: &Document,
    page: PageId,
    references: &[ShapeId],
    active_layer: Option<LayerId>,
    options: &SelectSameOptions,
) -> Result<Vec<ShapeId>> {
    if !options.tolerance_mm.is_finite() || options.tolerance_mm < 0.0 {
        return Err(Error::BadSelectionTolerance);
    }
    let layers = doc.layers_for_page(page)?;
    if references.is_empty() || !options.has_criteria() {
        return Ok(Vec::new());
    }
    let mut objects = Vec::new();
    let mut locked_containers = HashSet::new();
    for layer in layers.into_iter().filter(|l| l.visible && !l.locked) {
        for shape in &layer.shapes {
            collect_locked_containers(shape, &mut locked_containers);
            collect(
                shape,
                Affine::IDENTITY,
                layer.id,
                &mut Vec::new(),
                options,
                &doc.symbols,
                &mut objects,
            );
        }
    }
    let requested: HashSet<_> = references.iter().copied().collect();
    let selected: HashSet<_> = objects
        .iter()
        .filter(|o| requested.contains(&o.shape.id) && !locked_containers.contains(&o.shape.id))
        .map(|o| o.shape.id)
        .collect();
    let mut reference_ids = selected.clone();
    // A group has no meaningful appearance of its own. With type disabled,
    // its editable leaves supply the reference attributes. Excluded branches
    // have already been removed from the query.
    if !options.kind {
        for object in &objects {
            if object.ancestors.iter().any(|id| selected.contains(id)) {
                reference_ids.insert(object.shape.id);
            }
        }
    }
    let refs: Vec<_> = objects
        .iter()
        .filter(|o| {
            reference_ids.contains(&o.shape.id)
                && !locked_containers.contains(&o.shape.id)
                && (options.kind || !is_container(o.shape))
        })
        .collect();
    let mut matched = HashSet::new();
    let mut out = Vec::new();
    for candidate in &objects {
        if locked_containers.contains(&candidate.shape.id) {
            continue;
        }
        if options.current_layer_only && Some(candidate.layer) != active_layer {
            continue;
        }
        if !options.kind && is_container(candidate.shape) {
            continue;
        }
        if candidate.ancestors.iter().any(|id| matched.contains(id)) {
            continue;
        }
        if refs
            .iter()
            .any(|reference| matches(reference, candidate, options))
        {
            out.push(candidate.shape.id);
            matched.insert(candidate.shape.id);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Command, Engine, Fill, Layer, MasterScope, Point, Rect, Stroke, TextSpan};

    fn rect(id: u64, w: f64, h: f64, red: bool) -> Shape {
        let mut shape = Shape::new(
            ShapeId(id),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, w, h),
                radius: 0.0,
                corners: None,
            },
        );
        shape.fill = Fill::Solid(if red {
            Color::rgb8(220, 20, 20)
        } else {
            Color::rgb8(20, 20, 220)
        });
        shape
    }

    fn text(id: u64, spans: Vec<TextSpan>) -> Shape {
        Shape::new(
            ShapeId(id),
            ShapeKind::Text {
                spans,
                origin: Point::ZERO,
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            },
        )
    }

    fn document(shapes: Vec<Shape>) -> Document {
        let mut doc = Document::default();
        doc.pages[0].layers[0].shapes = shapes;
        doc
    }

    fn query(doc: &Document, refs: &[u64], options: &SelectSameOptions) -> Vec<ShapeId> {
        matching_ids(
            doc,
            doc.pages[0].id,
            &refs.iter().map(|id| ShapeId(*id)).collect::<Vec<_>>(),
            Some(doc.pages[0].layers[0].id),
            options,
        )
        .expect("valid query")
    }

    fn only_size(comparison: SizeComparison) -> SelectSameOptions {
        SelectSameOptions {
            fill: false,
            kind: false,
            size: true,
            size_comparison: comparison,
            ..Default::default()
        }
    }

    #[test]
    fn combined_properties_must_match_one_reference() {
        let mut ellipse = rect(12, 10.0, 10.0, false);
        ellipse.kind = ShapeKind::Ellipse {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            arc: None,
        };
        let mut crossed = ellipse.clone();
        crossed.id = ShapeId(13);
        crossed.fill = rect(10, 1.0, 1.0, true).fill;
        let doc = document(vec![
            rect(10, 10.0, 10.0, true),
            rect(11, 20.0, 10.0, true),
            ellipse,
            crossed,
        ]);
        assert_eq!(
            query(&doc, &[10, 12], &Default::default()),
            vec![ShapeId(10), ShapeId(11), ShapeId(12)]
        );
    }

    #[test]
    fn outline_color_and_width_are_independent_criteria() {
        let mut a = rect(10, 10.0, 10.0, true);
        a.stroke = Some(Stroke::new(Color::BLACK, 1.0));
        let mut b = a.clone();
        b.id = ShapeId(11);
        b.stroke.as_mut().expect("stroke").width = 2.0;
        let mut c = a.clone();
        c.id = ShapeId(12);
        c.stroke.as_mut().expect("stroke").color = Color::WHITE;
        let mut no_outline = rect(13, 10.0, 10.0, true);
        no_outline.stroke = None;
        let doc = document(vec![a, b, c, no_outline]);
        let mut options = SelectSameOptions {
            stroke_color: true,
            ..Default::default()
        };
        assert_eq!(query(&doc, &[10], &options), vec![ShapeId(10), ShapeId(11)]);
        options.stroke_color = false;
        options.stroke_width = true;
        assert_eq!(query(&doc, &[10], &options), vec![ShapeId(10), ShapeId(12)]);
        options.stroke_color = true;
        assert_eq!(query(&doc, &[10], &options), vec![ShapeId(10)]);
        assert_eq!(query(&doc, &[13], &options), vec![ShapeId(13)]);
    }

    #[test]
    fn size_uses_parent_transforms_and_both_dimensions_with_tolerance() {
        let group = Shape::new(
            ShapeId(20),
            ShapeKind::Group {
                children: vec![rect(21, 5.0, 5.0, true)],
            },
        );
        let mut group = group;
        group.transform = Affine::scale(2.0);
        let doc = document(vec![
            rect(10, 10.0, 10.0, true),
            rect(11, 10.4, 9.6, true),
            rect(12, 10.4, 9.4, true),
            group,
        ]);
        let options = only_size(SizeComparison::Equal);
        assert_eq!(
            query(&doc, &[21], &options),
            vec![ShapeId(10), ShapeId(11), ShapeId(21)]
        );
    }

    #[test]
    fn symbol_dimensions_use_definitions_and_nested_parent_transforms() {
        let mut nested_instance = Shape::new(ShapeId(102), ShapeKind::SymbolInstance { index: 0 });
        nested_instance.transform = Affine::scale(2.0);
        let mut instance = Shape::new(ShapeId(21), ShapeKind::SymbolInstance { index: 2 });
        instance.transform = Affine::translate((5.0, 5.0));
        let mut group = Shape::new(
            ShapeId(20),
            ShapeKind::Group {
                children: vec![instance],
            },
        );
        group.transform = Affine::scale(2.0);
        let small = Shape::new(ShapeId(11), ShapeKind::SymbolInstance { index: 0 });
        let large = Shape::new(ShapeId(12), ShapeKind::SymbolInstance { index: 1 });
        let mut same = small.clone();
        same.id = ShapeId(13);
        same.transform = Affine::scale(4.0);
        let mut doc = document(vec![rect(10, 40.0, 40.0, true), small, large, same, group]);
        doc.symbols = vec![
            Symbol {
                name: "Small".into(),
                shape: rect(100, 10.0, 10.0, true),
            },
            Symbol {
                name: "Large".into(),
                shape: rect(101, 50.0, 50.0, true),
            },
            Symbol {
                name: "Nested".into(),
                shape: Shape::new(
                    ShapeId(103),
                    ShapeKind::Group {
                        children: vec![nested_instance],
                    },
                ),
            },
        ];
        let options = only_size(SizeComparison::Equal);
        assert_eq!(
            query(&doc, &[10], &options),
            vec![ShapeId(10), ShapeId(13), ShapeId(21)]
        );
        assert_eq!(query(&doc, &[11], &options), vec![ShapeId(11)]);
        assert_eq!(query(&doc, &[12], &options), vec![ShapeId(12)]);
    }

    #[test]
    fn missing_and_cyclic_symbol_definitions_do_not_use_placeholder_dimensions() {
        let a = Shape::new(ShapeId(11), ShapeKind::SymbolInstance { index: 0 });
        let b = Shape::new(ShapeId(12), ShapeKind::SymbolInstance { index: 2 });
        let mut doc = document(vec![rect(10, 10.0, 10.0, true), a, b]);
        doc.symbols = vec![
            Symbol {
                name: "First".into(),
                shape: Shape::new(ShapeId(100), ShapeKind::SymbolInstance { index: 1 }),
            },
            Symbol {
                name: "Second".into(),
                shape: Shape::new(ShapeId(101), ShapeKind::SymbolInstance { index: 0 }),
            },
        ];
        assert_eq!(
            query(&doc, &[10], &only_size(SizeComparison::Equal)),
            vec![ShapeId(10)]
        );
        assert!(query(&doc, &[11], &only_size(SizeComparison::Equal)).is_empty());
    }

    #[test]
    fn dimensions_include_live_geometry_but_not_linked_blend_steps() {
        use crate::live::{BlendOptions, Effect};
        let plain = rect(10, 10.0, 10.0, true);
        let mut warped = plain.clone();
        warped.id = ShapeId(11);
        let mut nodes = crate::envelope::default_nodes(warped.bounds());
        nodes[5].y += 10.0;
        let frame = crate::envelope::Frame::from_parts(&nodes, &[], &[]).expect("envelope frame");
        warped.effects.push(Effect::from_envelope(
            &frame,
            false,
            crate::envelope::Mapping::Putty,
        ));
        let mut linked = plain.clone();
        linked.id = ShapeId(13);
        let mut end = plain.clone();
        end.id = ShapeId(14);
        end.transform = Affine::translate((100.0, 0.0));
        linked.effects.push(Effect::Blend {
            end: Box::new(end.clone()),
            steps: 5,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: None,
            rotate_on_path: false,
            end_id: Some(end.id),
            options: BlendOptions::default(),
        });
        let doc = document(vec![plain, warped, rect(12, 10.0, 20.0, true), linked, end]);
        let options = only_size(SizeComparison::Equal);
        assert_eq!(
            query(&doc, &[10], &options),
            vec![ShapeId(10), ShapeId(13), ShapeId(14)]
        );
        assert_eq!(query(&doc, &[12], &options), vec![ShapeId(11), ShapeId(12)]);
    }

    #[test]
    fn larger_and_smaller_need_one_dimension_beyond_tolerance() {
        let doc = document(vec![
            rect(10, 10.0, 10.0, true),
            rect(11, 10.4, 10.4, true),
            rect(12, 11.0, 10.0, true),
            rect(13, 9.0, 10.0, true),
            rect(14, 11.0, 9.0, true),
        ]);
        assert_eq!(
            query(&doc, &[10], &only_size(SizeComparison::Larger)),
            vec![ShapeId(12)]
        );
        assert_eq!(
            query(&doc, &[10], &only_size(SizeComparison::Smaller)),
            vec![ShapeId(13)]
        );
    }

    #[test]
    fn font_and_size_match_the_same_nonempty_text_span() {
        let a = text(10, vec![TextSpan::new("A", " Sans ", 12.0)]);
        let b = text(11, vec![TextSpan::new("B", "sans", 14.0)]);
        let c = text(12, vec![TextSpan::new("C", "serif", 12.0)]);
        let d = text(
            13,
            vec![
                TextSpan::new("D", "sans", 14.0),
                TextSpan::new("E", "serif", 12.0),
            ],
        );
        let e = text(14, vec![TextSpan::new(" ", "sans", 12.0)]);
        let doc = document(vec![a, b, c, d, e]);
        let mut options = SelectSameOptions {
            fill: false,
            kind: false,
            font: true,
            ..Default::default()
        };
        assert_eq!(
            query(&doc, &[10], &options),
            vec![ShapeId(10), ShapeId(11), ShapeId(13)]
        );
        options.font = false;
        options.font_size = true;
        assert_eq!(
            query(&doc, &[10], &options),
            vec![ShapeId(10), ShapeId(12), ShapeId(13)]
        );
        options.font = true;
        assert_eq!(query(&doc, &[10], &options), vec![ShapeId(10)]);
    }

    #[test]
    fn container_exclusion_skips_complete_branches_independently_of_object_type() {
        let nested = Shape::new(
            ShapeId(22),
            ShapeKind::ClipFrame {
                frame: Box::new(rect(23, 10.0, 10.0, true)),
                contents: vec![rect(24, 10.0, 10.0, true)],
            },
        );
        let group = Shape::new(
            ShapeId(20),
            ShapeKind::Group {
                children: vec![rect(21, 10.0, 10.0, true), nested],
            },
        );
        let frame = Shape::new(
            ShapeId(30),
            ShapeKind::ClipFrame {
                frame: Box::new(rect(31, 10.0, 10.0, true)),
                contents: vec![
                    rect(32, 10.0, 10.0, true),
                    Shape::new(
                        ShapeId(33),
                        ShapeKind::Group {
                            children: vec![rect(34, 10.0, 10.0, true)],
                        },
                    ),
                ],
            },
        );
        let doc = document(vec![rect(10, 10.0, 10.0, true), group, frame]);
        for kind in [false, true] {
            let mut options = SelectSameOptions {
                kind,
                ..Default::default()
            };
            assert_eq!(
                query(&doc, &[10], &options),
                vec![
                    ShapeId(10),
                    ShapeId(21),
                    ShapeId(24),
                    ShapeId(32),
                    ShapeId(34)
                ]
            );
            options.exclude_containers = true;
            assert_eq!(query(&doc, &[10], &options), vec![ShapeId(10)]);
            assert_eq!(
                query(&doc, &[10, 20, 24, 30, 34], &options),
                vec![ShapeId(10)]
            );
        }
    }

    #[test]
    fn excluded_group_and_clip_references_cannot_match_their_contents() {
        let group = Shape::new(
            ShapeId(20),
            ShapeKind::Group {
                children: vec![rect(21, 10.0, 10.0, true)],
            },
        );
        let frame = Shape::new(
            ShapeId(30),
            ShapeKind::ClipFrame {
                frame: Box::new(rect(31, 30.0, 30.0, false)),
                contents: vec![rect(32, 10.0, 10.0, true)],
            },
        );
        let doc = document(vec![group, rect(10, 10.0, 10.0, true), frame]);
        let options = SelectSameOptions {
            exclude_containers: true,
            ..Default::default()
        };
        for reference in [20, 21, 30, 31, 32] {
            assert!(query(&doc, &[reference], &options).is_empty());
        }
        assert_eq!(query(&doc, &[10], &options), vec![ShapeId(10)]);
        assert_eq!(query(&doc, &[20], &Default::default()), vec![ShapeId(20)]);
        assert_eq!(
            query(
                &doc,
                &[20],
                &SelectSameOptions {
                    kind: false,
                    ..Default::default()
                }
            ),
            vec![ShapeId(21), ShapeId(10), ShapeId(32)]
        );
    }

    #[test]
    fn selected_container_prevents_selecting_its_descendants_too() {
        let a = Shape::new(
            ShapeId(20),
            ShapeKind::Group {
                children: vec![rect(21, 10.0, 10.0, true)],
            },
        );
        let b = Shape::new(
            ShapeId(30),
            ShapeKind::Group {
                children: vec![a.clone()],
            },
        );
        let doc = document(vec![b]);
        let options = SelectSameOptions {
            fill: false,
            ..Default::default()
        };
        assert_eq!(query(&doc, &[20, 21, 30], &options), vec![ShapeId(30)]);
    }

    #[test]
    fn groups_holding_locked_descendants_do_not_suppress_selectable_children() {
        let group = |id, children| Shape::new(ShapeId(id), ShapeKind::Group { children });
        let mut locked = rect(31, 10.0, 10.0, true);
        locked.locked = true;
        let mut hidden_locked = rect(42, 10.0, 10.0, true);
        hidden_locked.locked = true;
        hidden_locked.visible = false;
        let doc = document(vec![
            rect(10, 10.0, 10.0, true),
            group(20, vec![rect(21, 10.0, 10.0, true)]),
            group(30, vec![locked, rect(32, 10.0, 10.0, true)]),
            group(
                40,
                vec![group(41, vec![hidden_locked]), rect(43, 10.0, 10.0, true)],
            ),
            group(50, vec![rect(51, 10.0, 10.0, true)]),
        ]);
        assert_eq!(
            query(&doc, &[20], &SelectSameOptions::default()),
            vec![ShapeId(20), ShapeId(50)]
        );
        let mut options = SelectSameOptions {
            fill: false,
            ..Default::default()
        };
        assert_eq!(
            query(&doc, &[10], &options),
            vec![
                ShapeId(10),
                ShapeId(21),
                ShapeId(32),
                ShapeId(43),
                ShapeId(51)
            ]
        );
        options.fill = true;
        options.kind = false;
        for reference in [30, 40, 41] {
            assert!(query(&doc, &[reference], &options).is_empty());
        }
        assert_eq!(
            query(&doc, &[20], &options),
            vec![
                ShapeId(10),
                ShapeId(21),
                ShapeId(32),
                ShapeId(43),
                ShapeId(51)
            ]
        );
    }

    #[test]
    fn clip_frames_respect_locks_on_the_frame_and_nested_contents() {
        let clip = |id, frame, contents| {
            Shape::new(
                ShapeId(id),
                ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents,
                },
            )
        };
        let mut locked_frame = rect(31, 10.0, 10.0, false);
        locked_frame.locked = true;
        let mut locked_content = rect(43, 10.0, 10.0, true);
        locked_content.locked = true;
        let doc = document(vec![
            rect(10, 10.0, 10.0, true),
            clip(
                20,
                rect(21, 10.0, 10.0, false),
                vec![rect(22, 10.0, 10.0, true)],
            ),
            clip(30, locked_frame, vec![rect(32, 10.0, 10.0, true)]),
            clip(
                40,
                rect(41, 10.0, 10.0, false),
                vec![Shape::new(
                    ShapeId(42),
                    ShapeKind::Group {
                        children: vec![locked_content, rect(44, 10.0, 10.0, true)],
                    },
                )],
            ),
            clip(
                50,
                rect(51, 10.0, 10.0, false),
                vec![rect(52, 10.0, 10.0, true)],
            ),
        ]);
        assert_eq!(
            query(&doc, &[20], &SelectSameOptions::default()),
            vec![ShapeId(20), ShapeId(50)]
        );
        let mut options = SelectSameOptions {
            fill: false,
            ..Default::default()
        };
        assert_eq!(
            query(&doc, &[10], &options),
            vec![
                ShapeId(10),
                ShapeId(22),
                ShapeId(32),
                ShapeId(44),
                ShapeId(52)
            ]
        );
        options.fill = true;
        options.kind = false;
        for reference in [30, 40, 42] {
            assert!(query(&doc, &[reference], &options).is_empty());
        }
    }

    #[test]
    fn scope_skips_hidden_locked_layers_objects_and_other_pages() {
        let mut hidden = rect(11, 10.0, 10.0, true);
        hidden.visible = false;
        let mut locked = rect(12, 10.0, 10.0, true);
        locked.locked = true;
        let mut doc = document(vec![rect(10, 10.0, 10.0, true), hidden, locked]);
        let mut hidden_layer = Layer::new(LayerId(50), "Hidden");
        hidden_layer.visible = false;
        hidden_layer.shapes.push(rect(51, 10.0, 10.0, true));
        let mut locked_layer = Layer::new(LayerId(60), "Locked");
        locked_layer.locked = true;
        locked_layer.shapes.push(rect(61, 10.0, 10.0, true));
        let mut other_layer = Layer::new(LayerId(70), "Other");
        other_layer.shapes.push(rect(71, 10.0, 10.0, true));
        doc.pages[0]
            .layers
            .extend([hidden_layer, locked_layer, other_layer]);
        let mut page = doc.pages[0].clone();
        page.id = PageId(90);
        page.layers.clear();
        let mut layer = Layer::new(LayerId(91), "Other page");
        layer.shapes.push(rect(92, 10.0, 10.0, true));
        page.layers.push(layer);
        doc.pages.push(page);
        let mut master = Layer::new(LayerId(80), "Master");
        master.shapes.push(rect(81, 10.0, 10.0, true));
        master.scope = MasterScope::Even;
        doc.master.push(master);
        assert_eq!(
            query(&doc, &[10], &Default::default()),
            vec![ShapeId(10), ShapeId(71)]
        );
        assert_eq!(
            query(
                &doc,
                &[10],
                &SelectSameOptions {
                    current_layer_only: true,
                    ..Default::default()
                }
            ),
            vec![ShapeId(10)]
        );
    }

    #[test]
    fn empty_missing_and_invalid_inputs_never_match_everything() {
        let doc = document(vec![rect(10, 10.0, 10.0, true)]);
        assert!(query(&doc, &[], &Default::default()).is_empty());
        assert!(query(&doc, &[999], &Default::default()).is_empty());
        assert!(query(
            &doc,
            &[10],
            &SelectSameOptions {
                fill: false,
                kind: false,
                ..Default::default()
            }
        )
        .is_empty());
        for tolerance in [f64::NAN, f64::INFINITY, -0.1] {
            let options = SelectSameOptions {
                tolerance_mm: tolerance,
                ..Default::default()
            };
            assert!(matching_ids(&doc, doc.pages[0].id, &[ShapeId(10)], None, &options).is_err());
        }
    }

    #[test]
    fn query_command_preserves_drawing_dirty_history_and_redo() {
        let doc = document(vec![rect(10, 10.0, 10.0, true), rect(11, 10.0, 10.0, true)]);
        let mut engine = Engine::new(doc);
        engine
            .run(&Command::SetTitle {
                title: "Changed".into(),
            })
            .expect("rename");
        engine.undo().expect("undo rename");
        engine.mark_saved();
        let before = engine.document().clone();
        let revision = engine.revision();
        let command = Command::SelectSame {
            page: engine.document().pages[0].id,
            references: vec![ShapeId(10)],
            active_layer: None,
            options: Default::default(),
        };
        let encoded = serde_json::to_string(&command).expect("encode");
        assert_eq!(
            serde_json::from_str::<Command>(&encoded).expect("decode"),
            command
        );
        engine.run(&command).expect("query");
        assert_eq!(
            engine.take_selection_result(),
            Some(vec![ShapeId(10), ShapeId(11)])
        );
        assert!(engine.take_selection_result().is_none());
        assert_eq!(engine.document(), &before);
        assert_eq!(engine.revision(), revision);
        assert!(!engine.is_dirty());
        assert!(engine.undo_label().is_none());
        assert_eq!(engine.redo_label(), Some("Rename Document"));
        engine.run(&command).expect("query again");
        let bad = Command::SelectSame {
            page: PageId(999),
            references: vec![ShapeId(10)],
            active_layer: None,
            options: Default::default(),
        };
        assert!(engine.run(&bad).is_err());
        assert!(engine.take_selection_result().is_none());
        assert_eq!(engine.revision(), revision);
        engine.redo().expect("query retains redo");
        assert_eq!(engine.document().title, "Changed");
    }

    #[test]
    fn query_cannot_dirty_batches_and_does_not_disturb_preview_history() {
        let mut engine = Engine::new(document(vec![rect(10, 10.0, 10.0, true)]));
        let command = Command::SelectSame {
            page: engine.document().pages[0].id,
            references: vec![ShapeId(10)],
            active_layer: None,
            options: Default::default(),
        };
        let before = engine.document().clone();
        assert!(engine
            .run_batch("Query", std::slice::from_ref(&command))
            .is_err());
        assert!(engine
            .amend_batch("Query", std::slice::from_ref(&command))
            .is_err());
        assert_eq!(engine.document(), &before);
        assert!(!engine.is_dirty());
        assert_eq!(engine.revision(), 0);
        engine.begin_preview();
        engine.amend(&command).expect("query in preview");
        assert_eq!(engine.take_selection_result(), Some(vec![ShapeId(10)]));
        engine.withdraw_preview();
        assert_eq!(engine.document(), &before);
        assert!(!engine.is_dirty());
        assert!(engine.undo_label().is_none());
    }

    #[test]
    fn object_type_distinguishes_artistic_and_paragraph_text() {
        let artistic = text(10, vec![TextSpan::new("A", "Sans", 12.0)]);
        let mut paragraph = artistic.clone();
        paragraph.id = ShapeId(11);
        if let ShapeKind::Text { frame, .. } = &mut paragraph.kind {
            *frame = Some(crate::Size::new(20.0, 10.0));
        }
        let doc = document(vec![artistic, paragraph]);
        assert_eq!(
            query(
                &doc,
                &[10],
                &SelectSameOptions {
                    fill: false,
                    ..Default::default()
                }
            ),
            vec![ShapeId(10)]
        );
    }

    #[test]
    fn fill_only_skips_container_appearance_and_inaccessible_branches() {
        let mut leaf = rect(21, 10.0, 10.0, true);
        leaf.fill = Fill::None;
        let group = Shape::new(
            ShapeId(20),
            ShapeKind::Group {
                children: vec![leaf],
            },
        );
        let mut locked_group = Shape::new(
            ShapeId(30),
            ShapeKind::Group {
                children: vec![rect(31, 10.0, 10.0, true)],
            },
        );
        locked_group.locked = true;
        let mut hidden_group = Shape::new(
            ShapeId(40),
            ShapeKind::Group {
                children: vec![rect(41, 10.0, 10.0, true)],
            },
        );
        hidden_group.visible = false;
        let mut reference = rect(10, 10.0, 10.0, true);
        reference.fill = Fill::None;
        let doc = document(vec![reference, group, locked_group, hidden_group]);
        let options = SelectSameOptions {
            kind: false,
            ..Default::default()
        };
        assert_eq!(query(&doc, &[10], &options), vec![ShapeId(10), ShapeId(21)]);
    }
}
