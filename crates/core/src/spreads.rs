//! Facing pages (#1972, #1660): the pages of a drawing shown and drawn
//! in spreads, two pages side by side with the spine between them. The
//! first page stands alone on the right (or starts a spread on the left),
//! then pages 2-3, 4-5 ... pair up. The pages keep their own coordinates;
//! a spread only says which pages sit beside which and where. Objects
//! drawn across the spine belong to one page and reach over onto the
//! other.

use crate::document::{Document, Layer, Page, Shape, ShapeKind};
use crate::geometry::{Affine, Point, Rect, Size};
use serde::{Deserialize, Serialize};

/// The side of a spread a page sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PageSide {
    Left,
    /// The first page alone on the right, as a book opens.
    #[default]
    Right,
}

impl PageSide {
    pub fn is_right(&self) -> bool {
        *self == PageSide::Right
    }
}

/// The pages of one spread, by index (0-based): a left page, a right
/// page, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Spread {
    pub left: Option<usize>,
    pub right: Option<usize>,
}

impl Spread {
    /// The pages of the spread, left first.
    pub fn pages(&self) -> impl Iterator<Item = usize> {
        self.left.into_iter().chain(self.right)
    }

    pub fn contains(&self, index: usize) -> bool {
        self.left == Some(index) || self.right == Some(index)
    }

    /// The other page of the spread, if `index` has one beside it.
    pub fn partner(&self, index: usize) -> Option<usize> {
        match (self.left, self.right) {
            (Some(l), Some(r)) if l == index => Some(r),
            (Some(l), Some(r)) if r == index => Some(l),
            _ => None,
        }
    }
}

/// How many slots come before the first page: one empty left slot when
/// the first page stands on the right.
fn lead(start: PageSide) -> usize {
    match start {
        PageSide::Right => 1,
        PageSide::Left => 0,
    }
}

/// The side page `index` sits on when the first page is on `start`.
pub fn side_of(index: usize, start: PageSide) -> PageSide {
    if (index + lead(start)).is_multiple_of(2) {
        PageSide::Left
    } else {
        PageSide::Right
    }
}

/// The number of the spread page `index` is in (0-based).
pub fn spread_number(index: usize, start: PageSide) -> usize {
    (index + lead(start)) / 2
}

/// The spread holding page `index` of `count` pages.
pub fn spread_of(index: usize, count: usize, start: PageSide) -> Spread {
    let n = spread_number(index, start);
    let slot = |s: usize| {
        (s >= lead(start))
            .then(|| s - lead(start))
            .filter(|i| *i < count)
    };
    Spread {
        left: slot(2 * n),
        right: slot(2 * n + 1),
    }
}

/// Every spread of `count` pages, in order. Adding or removing pages
/// reflows them: the spreads are worked out from the page order alone.
pub fn spreads(count: usize, start: PageSide) -> Vec<Spread> {
    if count == 0 {
        return Vec::new();
    }
    let last = spread_number(count - 1, start);
    (0..=last)
        .map(|n| spread_of((2 * n).saturating_sub(lead(start)), count, start))
        .collect()
}

/// Where the pages of a spread go, with the spine at `x = left width`:
/// the left page from x 0, the right page from the spine, their tops
/// lined up. Each page's bottom left corner, by page index, and the box
/// of the whole spread.
pub fn place(spread: Spread, size: impl Fn(usize) -> Size) -> (Vec<(usize, Point)>, Rect) {
    let dim = |i: Option<usize>| {
        i.map(&size)
            .map(|s| (s.width.max(0.0), s.height.max(0.0)))
            .unwrap_or((0.0, 0.0))
    };
    let (wl, hl) = dim(spread.left);
    let (wr, hr) = dim(spread.right);
    let h = hl.max(hr);
    let mut out = Vec::with_capacity(2);
    if let Some(l) = spread.left {
        out.push((l, Point::new(0.0, h - hl)));
    }
    if let Some(r) = spread.right {
        out.push((r, Point::new(wl, h - hr)));
    }
    (out, Rect::new(0.0, 0.0, wl + wr, h))
}

/// An object moved by `t`, the copies blends keep of their end objects
/// moved along.
fn shift(s: &mut Shape, t: Affine) {
    s.transform_by(t);
    carry_blend_ends(s, t);
}

fn carry_blend_ends(s: &mut Shape, t: Affine) {
    for e in s.effects.iter_mut() {
        if let crate::live::Effect::Blend { end, .. } = e {
            end.transform_by(t);
        }
    }
    match &mut s.kind {
        ShapeKind::Group { children } => children.iter_mut().for_each(|c| carry_blend_ends(c, t)),
        ShapeKind::ClipFrame { frame, contents } => {
            carry_blend_ends(frame, t);
            contents.iter_mut().for_each(|c| carry_blend_ends(c, t));
        }
        _ => {}
    }
}

impl Document {
    /// The side the first page starts on when the drawing shows facing
    /// pages, else `None`.
    pub fn facing(&self) -> Option<PageSide> {
        self.metadata
            .facing_pages
            .then_some(self.metadata.facing_start)
    }

    /// The spread holding page `index`: with facing pages off, the page
    /// alone (on the right).
    pub fn spread_at(&self, index: usize) -> Spread {
        match self.facing() {
            Some(start) => spread_of(index, self.pages.len(), start),
            None => Spread {
                left: None,
                right: (index < self.pages.len()).then_some(index),
            },
        }
    }

    /// The box inside the page margins of page `index` whose box is
    /// `page` (#1660): with facing pages the inside margin lies at the
    /// spine, else on the page's left. `None` without margins.
    pub fn margin_box(&self, index: usize, page: Rect) -> Option<Rect> {
        let m = self.metadata.margins.filter(|m| !m.is_zero())?;
        let side = match self.facing() {
            Some(start) => side_of(index, start),
            None => PageSide::Right,
        };
        Some(m.inner(page, side))
    }

    /// Where the pages of `spread` sit in it (see [`place`]).
    pub fn place_spread(&self, spread: Spread) -> (Vec<(usize, Point)>, Rect) {
        place(spread, |i| {
            self.pages
                .get(i)
                .map(|p| p.size)
                .unwrap_or(Size::new(0.0, 0.0))
        })
    }

    /// The chosen pages, in order, with the master layers they show
    /// materialized as page layers. Page fields are resolved against the
    /// original drawing before pages are removed or reordered for output.
    pub fn with_master_layers_on_pages(&self, indexes: &[usize]) -> Document {
        use std::collections::{HashMap, HashSet};
        let mut out = self.clone();
        out.pages.clear();
        out.master.clear();
        let mut ids = out.ids().clone();
        let master_ids: HashSet<_> = self.master.iter().map(|l| l.id).collect();
        let mut copied_masters = HashSet::new();
        let mut copied_pages = HashSet::new();
        for &index in indexes {
            let Some(page) = self.pages.get(index) else {
                continue;
            };
            if !copied_pages.insert(index) {
                continue;
            }
            let mut copy = page.clone();
            let mut references = HashMap::new();
            let mut baked_references = HashMap::new();
            copy.layers = self
                .layers_drawn_for_page(page.id)
                .unwrap_or_default()
                .into_iter()
                .map(|layer| {
                    let master = master_ids.contains(&layer.id);
                    let mut layer = layer.into_owned();
                    if master {
                        let original = layer.id;
                        layer.id = ids.layer();
                        if !copied_masters.insert(original) {
                            layer.shapes = layer
                                .shapes
                                .iter()
                                .map(|s| {
                                    let copy = s.with_fresh_ids(&mut || ids.shape());
                                    output_shape_ids(s, &copy, &mut references);
                                    copy
                                })
                                .collect();
                        } else {
                            for shape in &layer.shapes {
                                output_shape_ids(shape, shape, &mut references);
                            }
                        }
                    } else {
                        for shape in &layer.shapes {
                            output_shape_ids(shape, shape, &mut references);
                        }
                    }
                    layer.scope = crate::document::MasterScope::All;
                    layer.page_range = None;
                    layer.in_front = false;
                    layer.desktop = false;
                    for shape in &mut layer.shapes {
                        freeze_page_fields(shape);
                    }
                    layer
                })
                .collect();
            for shape in copy.layers.iter_mut().flat_map(|l| &mut l.shapes) {
                rebind_output_references(shape, &references, &mut baked_references, &mut ids);
            }
            for shape in copy.layers.iter_mut().flat_map(|l| &mut l.shapes) {
                redirect_baked_references(shape, &baked_references);
            }
            out.pages.push(copy);
        }
        let pages: HashSet<_> = out.pages.iter().map(|p| p.id).collect();
        out.comments.retain(|c| pages.contains(&c.page));
        out.page_guides_hidden.retain(|id| pages.contains(id));
        out.selection_groups.clear();
        out.saved_view = None;
        out.set_ids(ids);
        out
    }

    /// Self-contained output pages with identities from the destination
    /// drawing, for combining several drawings into one publication.
    pub fn copy_pages_for_output(
        &self,
        indexes: &[usize],
        ids: &mut crate::id::IdSource,
    ) -> Document {
        let mut out = self.with_master_layers_on_pages(indexes);
        let mut pages = std::collections::HashMap::new();
        for page in &mut out.pages {
            let original = page.id;
            page.id = ids.page();
            pages.insert(original, page.id);
            let mut references = std::collections::HashMap::new();
            let mut baked_references = std::collections::HashMap::new();
            for layer in &mut page.layers {
                layer.id = ids.layer();
                layer.shapes = layer
                    .shapes
                    .iter()
                    .map(|shape| {
                        let copy = shape.with_fresh_ids(&mut || ids.shape());
                        output_shape_ids(shape, &copy, &mut references);
                        copy
                    })
                    .collect();
            }
            for shape in page.layers.iter_mut().flat_map(|l| &mut l.shapes) {
                rebind_output_references(shape, &references, &mut baked_references, ids);
            }
            for shape in page.layers.iter_mut().flat_map(|l| &mut l.shapes) {
                redirect_baked_references(shape, &baked_references);
            }
        }
        let mut references = std::collections::HashMap::new();
        let mut baked_references = std::collections::HashMap::new();
        for symbol in &mut out.symbols {
            let copy = symbol.shape.with_fresh_ids(&mut || ids.shape());
            output_shape_ids(&symbol.shape, &copy, &mut references);
            symbol.shape = copy;
        }
        for symbol in &mut out.symbols {
            rebind_output_references(&mut symbol.shape, &references, &mut baked_references, ids);
        }
        for symbol in &mut out.symbols {
            redirect_baked_references(&mut symbol.shape, &baked_references);
        }
        for comment in &mut out.comments {
            if let Some(page) = pages.get(&comment.page) {
                comment.page = *page;
            }
        }
        out.page_guides_hidden = out
            .page_guides_hidden
            .iter()
            .filter_map(|id| pages.get(id).copied())
            .collect();
        out.set_ids(ids.clone());
        out
    }

    /// The drawing with each page also carrying the parts of its facing
    /// page's objects that reach over the spine onto it (or within
    /// `margin` mm of it, for the bleed), so a page exported or printed
    /// alone shows all that lies on it. The copies go beneath the page's
    /// own layers, in a layer of their own, with ids of their own. A
    /// drawing without facing pages comes back as it is.
    pub fn with_spread_overhangs(&self, margin: f64) -> Document {
        let mut out = self.clone();
        let Some(start) = self.facing() else {
            return out;
        };
        let margin = if margin.is_finite() {
            margin.max(0.0)
        } else {
            0.0
        };
        let count = self.pages.len();
        for (index, page) in self.pages.iter().enumerate() {
            let spread = spread_of(index, count, start);
            let Some(other) = spread.partner(index) else {
                continue;
            };
            let (placed, _) = self.place_spread(spread);
            let at = |i: usize| {
                placed
                    .iter()
                    .find(|(j, _)| *j == i)
                    .map(|(_, p)| *p)
                    .unwrap_or(Point::new(0.0, 0.0))
            };
            let (mine, theirs) = (at(index), at(other));
            // The other page's coordinates in this page's.
            let t = Affine::translate((theirs.x - mine.x, theirs.y - mine.y));
            let reach = page.rect().inflate(margin, margin);
            let Some(src) = self.pages.get(other) else {
                continue;
            };
            let mut copies = Vec::new();
            for layer in src.layers.iter().filter(|l| l.visible && l.printable) {
                for s in layer.shapes.iter().filter(|s| s.visible) {
                    let b = s.visual_bounds();
                    let moved = t.transform_rect_bbox(b);
                    if moved.x0 < reach.x1
                        && moved.x1 > reach.x0
                        && moved.y0 < reach.y1
                        && moved.y1 > reach.y0
                    {
                        let mut c = s.clone();
                        shift(&mut c, t);
                        copies.push(c);
                    }
                }
            }
            if copies.is_empty() {
                continue;
            }
            let ids = out.ids_mut();
            let copies: Vec<Shape> = copies
                .iter()
                .map(|c| c.with_fresh_ids(&mut || ids.shape()))
                .collect();
            let mut layer = Layer::new(ids.layer(), "Facing page");
            layer.shapes = copies;
            if let Some(p) = out.pages.get_mut(index) {
                p.layers.insert(0, layer);
            }
        }
        out
    }

    /// The drawing with each spread of the pages at `indexes` (0-based,
    /// in order; facing pages decide the spreads, else each page stands
    /// alone) turned into one page as wide as its pages side by side:
    /// Export > Spreads. The master layers are drawn into each half (with
    /// that page's number in page number fields), as are the pages'
    /// backgrounds and guidelines; the result has no master layers.
    pub fn spreads_document(&self, indexes: &[usize]) -> Document {
        let mut out = self.clone();
        let ranged = self.master.iter().any(|l| l.page_range.is_some());
        out.pages.clear();
        out.master.clear();
        out.master_guides.clear();
        let mut seen: Vec<Spread> = Vec::new();
        for &i in indexes {
            if i >= self.pages.len() {
                continue;
            }
            let full = self.spread_at(i);
            if seen.contains(&full) {
                continue;
            }
            seen.push(full);
            // Only the chosen pages of the spread.
            let spread = Spread {
                left: full.left.filter(|l| indexes.contains(l)),
                right: full.right.filter(|r| indexes.contains(r)),
            };
            let (placed, rect) = self.place_spread(spread);
            let mut layers: Vec<Layer> = Vec::new();
            let mut guides = Vec::new();
            let mut names = Vec::new();
            let mut first_id = None;
            for (index, at) in placed {
                let Some(page) = self.pages.get(index) else {
                    continue;
                };
                first_id.get_or_insert(page.id);
                names.push(if page.name.is_empty() {
                    (index + 1).to_string()
                } else {
                    page.name.clone()
                });
                let t = Affine::translate((at.x, at.y));
                let drawn = self.layers_drawn_for_page(page.id).unwrap_or_default();
                let ids = out.ids_mut();
                if let Some(bg) = &page.background {
                    let mut l = Layer::new(ids.layer(), "Background");
                    let mut s = Shape::new(
                        ids.shape(),
                        ShapeKind::Rect {
                            rect: page.rect(),
                            radius: 0.0,
                            corners: None,
                        },
                    );
                    s.fill = bg.clone();
                    s.stroke = None;
                    shift(&mut s, t);
                    l.shapes.push(s);
                    layers.push(l);
                }
                for l in drawn {
                    let mut l: Layer = l.into_owned();
                    l.id = ids.layer();
                    l.scope = crate::document::MasterScope::All;
                    l.page_range = None;
                    l.in_front = false;
                    l.shapes = l
                        .shapes
                        .iter()
                        .map(|s| {
                            let mut c = s.with_fresh_ids(&mut || ids.shape());
                            shift(&mut c, t);
                            if ranged {
                                freeze_page_fields(&mut c);
                            }
                            c
                        })
                        .collect();
                    layers.push(l);
                }
                for g in self.master_guides.iter().chain(page.guides.iter()) {
                    guides.push(g.with_line(shift_line(g.line, at.x, at.y)));
                }
            }
            let Some(id) = first_id else {
                continue;
            };
            out.pages.push(Page {
                id,
                name: names.join("-"),
                size: Size::new(rect.width(), rect.height()),
                layers,
                guides,
                background: None,
            });
        }
        if out.pages.is_empty() {
            // Never a drawing without pages.
            return self.clone();
        }
        out
    }
}

fn output_shape_ids(
    original: &Shape,
    copy: &Shape,
    references: &mut std::collections::HashMap<crate::ShapeId, crate::ShapeId>,
) {
    references.insert(original.id, copy.id);
    match (&original.kind, &copy.kind) {
        (ShapeKind::Group { children: original }, ShapeKind::Group { children: copy }) => {
            for (original, copy) in original.iter().zip(copy) {
                output_shape_ids(original, copy, references);
            }
        }
        (
            ShapeKind::ClipFrame {
                frame: a,
                contents: ac,
            },
            ShapeKind::ClipFrame {
                frame: b,
                contents: bc,
            },
        ) => {
            output_shape_ids(a, b, references);
            for (a, b) in ac.iter().zip(bc) {
                output_shape_ids(a, b, references);
            }
        }
        _ => {}
    }
}

/// Each output page links to its own copies. A missing linked blend end
/// gets a hidden local copy of its cache, keeping the original effect
/// stack and its rendering order on the main object.
fn rebind_output_references(
    shape: &mut Shape,
    references: &std::collections::HashMap<crate::ShapeId, crate::ShapeId>,
    baked_references: &mut std::collections::HashMap<crate::ShapeId, crate::ShapeId>,
    ids: &mut crate::id::IdSource,
) {
    let mut cached_ends = Vec::new();
    for effect in &mut shape.effects {
        if let crate::live::Effect::Blend {
            end,
            end_id: Some(id),
            options,
            ..
        } = effect
        {
            if let Some(mapped) = references.get(id) {
                *id = *mapped;
                end.id = *mapped;
            } else {
                let mut cache = end.with_fresh_ids(&mut || ids.shape());
                clear_cached_links(&mut cache);
                cache.visible = false;
                *id = cache.id;
                end.id = cache.id;
                cached_ends.push((cache, options.steps_below));
            }
        }
    }
    let clone_master = shape.data.iter().position(|(key, _)| key == "clone.master");
    if let Some(index) = clone_master {
        let mapped = shape
            .data
            .get(index)
            .and_then(|(_, value)| value.parse::<u64>().ok())
            .and_then(|id| references.get(&crate::ShapeId(id)));
        if let Some(id) = mapped {
            if let Some((_, value)) = shape.data.get_mut(index) {
                *value = id.raw().to_string();
            }
        } else {
            shape
                .data
                .retain(|(key, _)| key != "clone.master" && key != "clone.state");
        }
    }
    match &mut shape.kind {
        ShapeKind::Group { children } => {
            for child in children {
                rebind_output_references(child, references, baked_references, ids);
            }
        }
        ShapeKind::ClipFrame { frame, contents } => {
            rebind_output_references(frame, references, baked_references, ids);
            for child in contents {
                rebind_output_references(child, references, baked_references, ids);
            }
        }
        _ => {}
    }
    if !cached_ends.is_empty() {
        let original = shape.id;
        let bounds = shape.visual_bounds();
        let point = Point::new(bounds.x0, bounds.y0);
        let (below, above): (Vec<_>, Vec<_>) = cached_ends
            .into_iter()
            .partition(|(_, steps_below)| *steps_below);
        let mut container = |ends: Vec<(Shape, bool)>| {
            let frame = Shape::new(
                ids.shape(),
                ShapeKind::Path {
                    path: crate::geometry::BezPath::from_vec(vec![
                        crate::geometry::PathEl::MoveTo(point),
                        crate::geometry::PathEl::LineTo(point),
                    ]),
                    closed: false,
                },
            );
            // A hidden clip's frame owns its bounds, so the reference geometry
            // cannot enlarge the output selection. Its stack position retains
            // the blend step order when an editable output refreshes links.
            let mut container = Shape::new(
                ids.shape(),
                ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents: ends.into_iter().map(|(end, _)| end).collect(),
                },
            );
            container.visible = false;
            container
        };
        let below = (!below.is_empty()).then(|| container(below));
        let above = (!above.is_empty()).then(|| container(above));
        let mut main = shape.clone();
        main.id = ids.shape();
        main.uid = None;
        baked_references.insert(original, main.id);
        let children = below.into_iter().chain([main]).chain(above).collect();
        let mut group = Shape::new(original, ShapeKind::Group { children });
        group.name = shape.name.clone();
        group.visible = shape.visible;
        group.locked = shape.locked;
        group.uid = shape.uid;
        group.link = shape.link.clone();
        group.data = shape
            .data
            .iter()
            .filter(|(key, _)| key != "clone.master" && key != "clone.state")
            .cloned()
            .collect();
        *shape = group;
    }
}

/// Cached endpoints are geometry references, with no independent live links.
fn clear_cached_links(shape: &mut Shape) {
    shape.locked = false;
    shape.effects.clear();
    shape.parked_effects.clear();
    shape.shadow = None;
    shape.page_number = None;
    shape.toc = None;
    shape
        .data
        .retain(|(key, _)| key != "clone.master" && key != "clone.state");
    match &mut shape.kind {
        ShapeKind::Group { children } => children.iter_mut().for_each(clear_cached_links),
        ShapeKind::ClipFrame { frame, contents } => {
            clear_cached_links(frame);
            contents.iter_mut().for_each(clear_cached_links);
        }
        _ => {}
    }
}

fn redirect_baked_references(
    shape: &mut Shape,
    references: &std::collections::HashMap<crate::ShapeId, crate::ShapeId>,
) {
    for effect in &mut shape.effects {
        if let crate::live::Effect::Blend {
            end,
            end_id: Some(id),
            ..
        } = effect
        {
            if let Some(mapped) = references.get(id) {
                *id = *mapped;
                end.id = *mapped;
            }
        }
    }
    for (key, value) in &mut shape.data {
        if key == "clone.master" {
            if let Some(mapped) = value
                .parse::<u64>()
                .ok()
                .and_then(|id| references.get(&crate::ShapeId(id)))
            {
                *value = mapped.raw().to_string();
            }
        }
    }
    match &mut shape.kind {
        ShapeKind::Group { children } => {
            for child in children {
                redirect_baked_references(child, references);
            }
        }
        ShapeKind::ClipFrame { frame, contents } => {
            redirect_baked_references(frame, references);
            for child in contents {
                redirect_baked_references(child, references);
            }
        }
        _ => {}
    }
}

/// Output pages keep the labels calculated in the source drawing.
fn freeze_page_fields(shape: &mut Shape) {
    shape.page_number = None;
    shape.toc = None;
    if let ShapeKind::Group { children } = &mut shape.kind {
        for child in children {
            freeze_page_fields(child);
        }
    }
}

fn shift_line(line: crate::document::GuideLine, dx: f64, dy: f64) -> crate::document::GuideLine {
    use crate::document::GuideLine;
    match line {
        GuideLine::Horizontal { y } => GuideLine::Horizontal { y: y + dy },
        GuideLine::Vertical { x } => GuideLine::Vertical { x: x + dx },
        GuideLine::Angled { x, y, angle } => GuideLine::Angled {
            x: x + dx,
            y: y + dy,
            angle,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ShapeKind;
    use crate::geometry::Rect;

    fn range_book() -> Document {
        let mut doc = Document::new("book", Size::new(100.0, 200.0));
        doc.pages[0].name = "1".into();
        doc.pages[0].layers[0].name = "Page".into();
        for number in 2..=3 {
            let page = Page {
                id: doc.ids_mut().page(),
                name: number.to_string(),
                size: Size::new(100.0, 200.0),
                layers: vec![Layer::new(doc.ids_mut().layer(), "Page")],
                guides: Vec::new(),
                background: None,
            };
            doc.pages.push(page);
        }
        for (name, range, front) in [
            ("A", None, false),
            ("Range", Some("2-3"), false),
            ("B", None, false),
            ("Front", Some("2"), true),
        ] {
            let mut layer = Layer::master(doc.ids_mut().layer(), name, crate::MasterScope::All);
            layer.page_range = range.map(|r| crate::PageRanges::parse(r).expect("range"));
            layer.in_front = front;
            let mut shape = Shape::new(
                doc.ids_mut().shape(),
                ShapeKind::Rect {
                    rect: Rect::new(1.0, 1.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            shape.name = Some(name.into());
            layer.shapes.push(shape);
            doc.master.push(layer);
        }
        doc
    }

    #[test]
    fn range_output_pages_keep_source_membership_order_and_unique_ids() {
        let mut doc = range_book();
        doc.master[2].printable = false;
        let original = doc.clone();
        let out = doc.with_master_layers_on_pages(&[2, 0, 1, 2, usize::MAX]);
        assert!(out.master.is_empty());
        let expected = [
            vec!["A", "Range", "B", "Page"],
            vec!["A", "B", "Page"],
            vec!["A", "Range", "B", "Page", "Front"],
        ];
        assert_eq!(out.pages.len(), 3);
        for (i, page) in out.pages.iter().enumerate() {
            let names: Vec<_> = page.layers.iter().map(|l| l.name.as_str()).collect();
            assert_eq!(names, expected[i]);
            assert!(page
                .layers
                .iter()
                .all(|l| l.page_range.is_none() && !l.in_front));
            assert!(
                !page
                    .layers
                    .iter()
                    .find(|l| l.name == "B")
                    .expect("B")
                    .printable
            );
        }
        assert_eq!(
            out.pages
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["3", "1", "2"]
        );
        let shapes: Vec<_> = out
            .pages
            .iter()
            .flat_map(|p| &p.layers)
            .flat_map(|l| &l.shapes)
            .map(|s| s.id)
            .collect();
        let unique: std::collections::HashSet<_> = shapes.iter().copied().collect();
        assert_eq!(shapes.len(), unique.len());
        assert_eq!(
            out.pages[0].layers[1].shapes[0].id,
            doc.master[1].shapes[0].id
        );
        assert_ne!(
            out.pages[2].layers[1].shapes[0].id,
            doc.master[1].shapes[0].id
        );
        assert_eq!(doc, original);
    }

    #[test]
    fn range_output_pages_freeze_original_page_numbers_and_contents() {
        use crate::document::{PageNumberField, TextSpan};
        let mut doc = range_book();
        let text = |id, value: &str| {
            Shape::new(
                id,
                ShapeKind::Text {
                    spans: vec![TextSpan::new(value, "Sans", 12.0)],
                    origin: Point::new(1.0, 1.0),
                    frame: None,
                    align: Default::default(),
                    para: Default::default(),
                    on_path: None,
                },
            )
        };
        let mut number = text(doc.ids_mut().shape(), "number");
        number.page_number = Some(PageNumberField {
            start_at: 1,
            first_page: 1,
            style: 0,
            prefix: String::new(),
            suffix: String::new(),
            ..Default::default()
        });
        let mut contents = text(doc.ids_mut().shape(), "contents");
        contents.toc = Some(crate::toc::TocField {
            title: "Contents".into(),
            ..Default::default()
        });
        doc.master[1].shapes = vec![number, contents];
        let mut heading = text(doc.ids_mut().shape(), "Chapter");
        heading.toc_level = Some(1);
        doc.pages[1].layers[0].shapes.push(heading);
        let out = doc.with_master_layers_on_pages(&[2]);
        let drawn = out.layers_drawn_for_page(out.pages[0].id).expect("drawn");
        let range = drawn.iter().find(|l| l.name == "Range").expect("range");
        let words: Vec<_> = range
            .shapes
            .iter()
            .filter_map(|s| match &s.kind {
                ShapeKind::Text { spans, .. } => Some(crate::document::spans_text(spans)),
                _ => None,
            })
            .collect();
        assert_eq!(words, ["3", "Contents\nChapter\t2"]);
        assert!(range
            .shapes
            .iter()
            .all(|s| s.page_number.is_none() && s.toc.is_none()));
        assert!(doc.master[1].shapes[0].page_number.is_some());
        assert!(doc.master[1].shapes[1].toc.is_some());
    }

    #[test]
    fn range_output_rebinds_master_blends_and_clones_inside_groups() {
        let mut doc = range_book();
        let mut start = doc.master[1].shapes[0].clone();
        let mut end = start.with_fresh_ids(&mut || doc.ids_mut().shape());
        end.transform_by(Affine::translate((20.0, 0.0)));
        start.name = Some("start".into());
        end.name = Some("end".into());
        start.effects.push(crate::live::Effect::Blend {
            end: Box::new(end.clone()),
            steps: 2,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: None,
            rotate_on_path: false,
            end_id: Some(end.id),
            options: Default::default(),
        });
        start
            .data
            .push(("clone.master".into(), end.id.raw().to_string()));
        let group = Shape::new(
            doc.ids_mut().shape(),
            ShapeKind::Group {
                children: vec![start, end],
            },
        );
        doc.master[1].shapes = vec![group];
        let out = doc.with_master_layers_on_pages(&[1, 2]);
        let mut previous = None;
        for page in &out.pages {
            let layer = page
                .layers
                .iter()
                .find(|l| l.name == "Range")
                .expect("range");
            let ShapeKind::Group { children } = &layer.shapes[0].kind else {
                panic!("group")
            };
            let (start, end) = (&children[0], &children[1]);
            let crate::live::Effect::Blend {
                end_id,
                end: cached,
                ..
            } = &start.effects[0]
            else {
                panic!("blend")
            };
            assert_eq!(*end_id, Some(end.id));
            assert_eq!(cached.id, end.id);
            assert_eq!(
                start
                    .data
                    .iter()
                    .find(|(key, _)| key == "clone.master")
                    .expect("clone")
                    .1,
                end.id.raw().to_string()
            );
            assert_ne!(previous, Some(end.id));
            previous = Some(end.id);
        }
    }

    #[test]
    fn range_output_freezes_links_to_objects_on_excluded_pages() {
        let mut doc = range_book();
        let mut start = doc.master[1].shapes[0].clone();
        let mut end = start.with_fresh_ids(&mut || doc.ids_mut().shape());
        end.transform_by(Affine::translate((20.0, 0.0)));
        start.effects.push(crate::live::Effect::Blend {
            end: Box::new(end.clone()),
            steps: 2,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: None,
            rotate_on_path: false,
            end_id: Some(end.id),
            options: Default::default(),
        });
        start.data = vec![
            ("clone.master".into(), end.id.raw().to_string()),
            ("clone.state".into(), "shape:own".into()),
        ];
        let mut follower = start.with_fresh_ids(&mut || doc.ids_mut().shape());
        follower.effects.clear();
        follower.data = vec![("clone.master".into(), start.id.raw().to_string())];
        doc.master[1].shapes = vec![start.clone(), follower];
        doc.master[3].shapes = vec![end];
        let out = doc.with_master_layers_on_pages(&[2]);
        let range = out.pages[0]
            .layers
            .iter()
            .find(|l| l.name == "Range")
            .expect("range");
        let group = &range.shapes[0];
        let ShapeKind::Group { children } = &group.kind else {
            panic!("cached group")
        };
        assert_eq!(children.len(), 2, "main and hidden local references");
        assert_eq!(group.id, start.id);
        assert!(group.effects.is_empty());
        assert!(group.data.iter().all(|(key, _)| !key.starts_with("clone.")));
        let logical = range.shapes[1]
            .data
            .iter()
            .find(|(key, _)| key == "clone.master")
            .expect("follower")
            .1
            .parse::<u64>()
            .expect("id");
        let target = out
            .find_shape(crate::ShapeId(logical))
            .expect("logical endpoint");
        assert!(matches!(target.kind, ShapeKind::Rect { .. }));
        assert_ne!(target.id, group.id);
        let crate::live::Effect::Blend {
            end_id: Some(id), ..
        } = &target.effects[0]
        else {
            panic!("linked cached endpoint")
        };
        assert!(!out.find_shape(*id).expect("local cached end").visible);
        let actual = crate::live::break_apart(target, || crate::ShapeId(0));
        let expected = crate::live::break_apart(&start, || crate::ShapeId(0));
        assert_eq!(
            actual.iter().map(Shape::page_path).collect::<Vec<_>>(),
            expected.iter().map(Shape::page_path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn range_output_keeps_missing_blend_effects_on_the_original_object() {
        use crate::live::{Effect, Lens, MergeMode};
        use crate::{Color, Fill};
        let mut doc = range_book();
        let mut start = doc.master[1].shapes[0].clone();
        start.transform = Affine::translate((50.0, 40.0));
        start.opacity = 0.7;
        start.shadow = Some(crate::Shadow::default());
        let mut end = start.with_fresh_ids(&mut || doc.ids_mut().shape());
        end.transform_by(Affine::translate((20.0, 0.0)));
        start.effects = vec![
            Effect::Blend {
                end: Box::new(end.clone()),
                steps: 2,
                accel_objects: 0.0,
                accel_colors: 0.0,
                rotation: 0.0,
                path: None,
                rotate_on_path: false,
                end_id: Some(end.id),
                options: Default::default(),
            },
            Effect::FillOpacity { opacity: 0.5 },
            Effect::Transparency {
                mask: Fill::Solid(Color::Gray { v: 0.4 }),
                merge: MergeMode::Normal,
                target: 2,
            },
            Effect::Lens(Lens::Invert),
            Effect::Glow {
                inner: false,
                color: Color::BLACK,
                opacity: 0.6,
                blur: 2.0,
                merge: MergeMode::Normal,
            },
            Effect::GaussianBlur {
                radius: 1.0,
                pins: Vec::new(),
            },
        ];
        doc.master[1].shapes = vec![start.clone()];
        doc.master[3].shapes = vec![end];
        let source = doc.clone();
        let out = doc.with_master_layers_on_pages(&[2]);
        let range = out.pages[0]
            .layers
            .iter()
            .find(|l| l.name == "Range")
            .expect("range");
        let group = &range.shapes[0];
        let ShapeKind::Group { children } = &group.kind else {
            panic!("reference group")
        };
        let main = children.iter().find(|s| s.visible).expect("original main");
        let references = children
            .iter()
            .find(|s| !s.visible)
            .expect("hidden references");
        assert!(group.effects.is_empty());
        assert!(group.shadow.is_none());
        assert_eq!(main.opacity, start.opacity);
        assert_eq!(main.shadow, start.shadow);
        assert_eq!(main.transform, start.transform);
        assert_eq!(main.effects[1..], start.effects[1..]);
        let expected = start.visual_bounds();
        assert_eq!(group.visual_bounds(), expected);
        let bounds = references.bounds();
        assert_eq!(bounds.width(), 0.0);
        assert_eq!(bounds.height(), 0.0);
        assert!(bounds.x0 >= expected.x0 && bounds.x0 <= expected.x1);
        assert!(bounds.y0 >= expected.y0 && bounds.y0 <= expected.y1);
        let Effect::Blend {
            end_id: Some(id), ..
        } = &main.effects[0]
        else {
            panic!("blend")
        };
        assert!(!out.find_shape(*id).expect("cached end").visible);
        let loaded =
            Document::from_json(&out.to_json().expect("write output")).expect("read output");
        let loaded_main = loaded.find_shape(main.id).expect("reloaded main");
        assert_eq!(loaded_main.effects, main.effects);
        assert!(!loaded.find_shape(*id).expect("reloaded cached end").visible);
        assert_eq!(
            loaded
                .find_shape(group.id)
                .expect("reloaded group")
                .visual_bounds(),
            expected
        );
        fn collect(shape: &Shape, ids: &mut Vec<crate::ShapeId>) {
            ids.push(shape.id);
            match &shape.kind {
                ShapeKind::Group { children } => children.iter().for_each(|s| collect(s, ids)),
                ShapeKind::ClipFrame { frame, contents } => {
                    collect(frame, ids);
                    contents.iter().for_each(|s| collect(s, ids));
                }
                _ => {}
            }
        }
        let mut ids = Vec::new();
        for shape in out.all_layers().flat_map(|l| &l.shapes) {
            collect(shape, &mut ids);
        }
        assert_eq!(
            ids.len(),
            ids.iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len()
        );
        assert_eq!(doc, source);
    }

    fn pages_of(spreads: &[Spread]) -> Vec<Vec<usize>> {
        spreads.iter().map(|s| s.pages().collect()).collect()
    }

    /// #1660: page margins for books, the inside margin at the spine
    /// (right of a left page, left of a right page), saved with the
    /// drawing only when set.
    #[test]
    fn page_margins_mirror_at_the_spine() {
        use crate::document::{Metadata, PageMargins};
        let mut doc = Document::new("d", Size::new(100.0, 200.0));
        let page = Rect::new(0.0, 0.0, 100.0, 200.0);
        assert_eq!(doc.margin_box(0, page), None);
        let json = serde_json::to_string(&doc.metadata).unwrap();
        assert!(!json.contains("margins"), "{json}");
        doc.metadata.margins = Some(PageMargins {
            top: 15.0,
            bottom: 25.0,
            inside: 20.0,
            outside: 10.0,
        });
        // A page alone: the inside margin on its left.
        assert_eq!(
            doc.margin_box(0, page),
            Some(Rect::new(20.0, 25.0, 90.0, 185.0))
        );
        doc.metadata.facing_pages = true;
        // Page 1 on the right, page 2 on the left, page 3 on the right.
        assert_eq!(
            doc.margin_box(0, page),
            Some(Rect::new(20.0, 25.0, 90.0, 185.0))
        );
        let left = Rect::new(-100.0, 0.0, 0.0, 200.0);
        assert_eq!(
            doc.margin_box(1, left),
            Some(Rect::new(-90.0, 25.0, -20.0, 185.0))
        );
        assert_eq!(
            doc.margin_box(2, page),
            Some(Rect::new(20.0, 25.0, 90.0, 185.0))
        );
        doc.metadata.facing_start = PageSide::Left;
        assert_eq!(
            doc.margin_box(0, page),
            Some(Rect::new(10.0, 25.0, 80.0, 185.0))
        );
        // Saved and read back.
        let back: Metadata =
            serde_json::from_str(&serde_json::to_string(&doc.metadata).unwrap()).unwrap();
        assert_eq!(back.margins, doc.metadata.margins);
        // Margins wider than the page leave a box of no width; bad values
        // count as none.
        let wide = PageMargins::uniform(80.0).inner(page, PageSide::Right);
        assert_eq!((wide.x0, wide.x1), (80.0, 80.0));
        assert!(PageMargins::uniform(f64::NAN).is_zero());
        doc.metadata.margins = Some(PageMargins::uniform(0.0));
        assert_eq!(doc.margin_box(0, page), None);
    }

    /// Starting on the right, page 1 stands alone, then 2-3, 4-5 ...;
    /// starting on the left, 1-2, 3-4 ...
    #[test]
    fn pages_pair_up_after_the_first_page() {
        let right = spreads(6, PageSide::Right);
        assert_eq!(
            pages_of(&right),
            vec![vec![0], vec![1, 2], vec![3, 4], vec![5]]
        );
        assert_eq!(
            right[0],
            Spread {
                left: None,
                right: Some(0)
            }
        );
        assert_eq!(
            right[3],
            Spread {
                left: Some(5),
                right: None
            }
        );
        assert_eq!(
            pages_of(&spreads(5, PageSide::Left)),
            vec![vec![0, 1], vec![2, 3], vec![4]]
        );
        assert!(spreads(0, PageSide::Right).is_empty());
        assert_eq!(side_of(0, PageSide::Right), PageSide::Right);
        assert_eq!(side_of(1, PageSide::Right), PageSide::Left);
        assert_eq!(side_of(0, PageSide::Left), PageSide::Left);
        assert_eq!(spread_of(2, 6, PageSide::Right).partner(2), Some(1));
        assert_eq!(spread_of(0, 6, PageSide::Right).partner(0), None);
    }

    /// Adding or removing a page reflows the spreads from the page order.
    #[test]
    fn adding_and_removing_pages_reflows_the_spreads() {
        for n in 1..12 {
            for start in [PageSide::Left, PageSide::Right] {
                let all = spreads(n, start);
                let flat: Vec<usize> = all.iter().flat_map(|s| s.pages()).collect();
                assert_eq!(flat, (0..n).collect::<Vec<_>>(), "{n} {start:?}");
                for i in 0..n {
                    assert!(all[spread_number(i, start)].contains(i));
                }
            }
        }
        let mut doc = Document::new("d", Size::new(100.0, 200.0));
        doc.metadata.facing_pages = true;
        for _ in 0..3 {
            let id = doc.ids_mut().page();
            let layer = doc.ids_mut().layer();
            doc.pages.push(Page {
                id,
                name: String::new(),
                size: Size::new(100.0, 200.0),
                layers: vec![Layer::new(layer, "L")],
                guides: Vec::new(),
                background: None,
            });
        }
        assert_eq!(doc.spread_at(3).pages().collect::<Vec<_>>(), vec![3]);
        doc.pages.remove(0);
        assert_eq!(doc.spread_at(0).pages().collect::<Vec<_>>(), vec![0]);
        assert_eq!(doc.spread_at(2).pages().collect::<Vec<_>>(), vec![1, 2]);
        doc.metadata.facing_start = PageSide::Left;
        assert_eq!(doc.spread_at(0).pages().collect::<Vec<_>>(), vec![0, 1]);
        doc.metadata.facing_pages = false;
        assert_eq!(doc.spread_at(1).pages().collect::<Vec<_>>(), vec![1]);
        assert_eq!(doc.facing(), None);
    }

    #[test]
    fn a_spread_lines_up_the_tops_with_the_spine_between() {
        let sizes = [Size::new(100.0, 200.0), Size::new(120.0, 150.0)];
        let (placed, rect) = place(
            Spread {
                left: Some(0),
                right: Some(1),
            },
            |i| sizes[i],
        );
        assert_eq!(
            placed,
            vec![(0, Point::new(0.0, 0.0)), (1, Point::new(100.0, 50.0))]
        );
        assert_eq!(rect, Rect::new(0.0, 0.0, 220.0, 200.0));
    }

    fn facing_doc() -> (Document, crate::ShapeId) {
        let mut doc = Document::new("d", Size::new(100.0, 200.0));
        doc.metadata.facing_pages = true;
        for _ in 0..2 {
            let id = doc.ids_mut().page();
            let layer = doc.ids_mut().layer();
            doc.pages.push(Page {
                id,
                name: String::new(),
                size: Size::new(100.0, 200.0),
                layers: vec![Layer::new(layer, "L")],
                guides: Vec::new(),
                background: None,
            });
        }
        // On page 2 (the left page of spread 2-3), across the spine.
        let sid = doc.ids_mut().shape();
        let mut s = Shape::new(
            sid,
            ShapeKind::Rect {
                rect: Rect::new(80.0, 10.0, 130.0, 50.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.stroke = None;
        doc.pages[1].layers[0].shapes.push(s);
        (doc, sid)
    }

    /// The right page exported alone carries the part of the left
    /// page's object that reaches over the spine onto it, moved into its
    /// own coordinates, beneath its own layers and with ids of its own.
    #[test]
    fn a_page_alone_carries_what_reaches_over_the_spine() {
        let (doc, sid) = facing_doc();
        let out = doc.with_spread_overhangs(0.0);
        let right = &out.pages[2];
        assert_eq!(right.layers.len(), 2);
        let copy = &right.layers[0].shapes[0];
        assert_ne!(copy.id, sid);
        let b = copy.visual_bounds();
        assert!(
            (b.x0 + 20.0).abs() < 1e-9 && (b.x1 - 30.0).abs() < 1e-9,
            "{b:?}"
        );
        // The left page itself and page 1 (alone) are as they were.
        assert_eq!(out.pages[1], doc.pages[1]);
        assert_eq!(out.pages[0], doc.pages[0]);
        // Facing pages off: nothing added.
        let mut off = doc.clone();
        off.metadata.facing_pages = false;
        assert_eq!(off.with_spread_overhangs(0.0), off);
        // An object clear of the spine (and of the bleed) is not copied.
        let mut clear = doc.clone();
        if let Some(s) = clear.pages[1].layers[0].shapes.first_mut() {
            s.transform_by(Affine::translate((-60.0, 0.0)));
        }
        assert_eq!(clear.with_spread_overhangs(3.0).pages[2].layers.len(), 1);
        assert_eq!(clear.with_spread_overhangs(35.0).pages[2].layers.len(), 2);
    }

    /// Export > Spreads: each spread one page as wide as its two pages,
    /// the right page's objects moved past the spine, the master layers
    /// drawn into both halves; page 1 alone stays a page of its own.
    #[test]
    fn spreads_become_pages_as_wide_as_both() {
        let (mut doc, _) = facing_doc();
        let sid = doc.ids_mut().shape();
        let master = doc.ids_mut().layer();
        let mut l = Layer::master(master, "M", crate::MasterScope::All);
        l.shapes.push(Shape::new(
            sid,
            ShapeKind::Rect {
                rect: Rect::new(1.0, 1.0, 2.0, 2.0),
                radius: 0.0,
                corners: None,
            },
        ));
        doc.master.push(l);
        let sid = doc.ids_mut().shape();
        doc.pages[2].layers[0].shapes.push(Shape::new(
            sid,
            ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            },
        ));
        let out = doc.spreads_document(&[0, 1, 2]);
        assert_eq!(out.pages.len(), 2);
        assert!(out.master.is_empty());
        assert_eq!(out.pages[0].size, Size::new(100.0, 200.0));
        assert_eq!(out.pages[1].size, Size::new(200.0, 200.0));
        assert_eq!(out.pages[1].name, "2-3");
        let all: Vec<&Shape> = out.pages[1]
            .layers
            .iter()
            .flat_map(|l| l.shapes.iter())
            .collect();
        // Master copy twice, the spine-crossing rect, the right page's rect.
        assert_eq!(all.len(), 4);
        let mut ids: Vec<_> = all.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 4, "ids of their own");
        let moved = all
            .iter()
            .map(|s| s.visual_bounds())
            .any(|b| (b.x0 - 110.0).abs() < 1e-9 && (b.y0 - 10.0).abs() < 1e-9);
        assert!(moved, "the right page's rect past the spine");
        // Only page 3 chosen: it stands alone.
        let one = doc.spreads_document(&[2]);
        assert_eq!(one.pages.len(), 1);
        assert_eq!(one.pages[0].size, Size::new(100.0, 200.0));
        // Nothing chosen: the drawing as it is, never without pages.
        assert_eq!(doc.spreads_document(&[]).pages.len(), 3);
    }

    /// Facing pages are saved with the drawing and left out while off.
    #[test]
    fn facing_pages_round_trip_in_the_file() {
        let mut m = crate::Metadata::default();
        let off = serde_json::to_string(&m).expect("json");
        assert!(!off.contains("facing"));
        m.facing_pages = true;
        m.facing_start = PageSide::Left;
        let on = serde_json::to_string(&m).expect("json");
        assert!(on.contains("\"facing_pages\":true") && on.contains("\"facing_start\":\"left\""));
        let back: crate::Metadata = serde_json::from_str(&on).expect("read");
        assert_eq!(back, m);
        let old: crate::Metadata = serde_json::from_str("{}").expect("read");
        assert!(!old.facing_pages && old.facing_start == PageSide::Right);
    }
}
