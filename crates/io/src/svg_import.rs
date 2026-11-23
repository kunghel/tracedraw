//! SVG import: parses with `usvg` (which resolves CSS, `use`, units and
//! nested transforms into a flat tree) and converts the result into
//! document shapes. Groups keep their structure, clip paths become
//! clip frames, images become bitmaps, text becomes editable text
//! objects (read by `svg_text`, since usvg is built without its text
//! engine and drops text elements).
//!
//! Coordinates: SVG is in user units at 96 dpi with Y down; the document is
//! in millimetres with Y up, origin at the bottom-left of the page.

use std::sync::Arc;

use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::{Affine, BezPath, PathEl, Point, Rect, Size},
    id::IdSource,
    style::{Fountain, FountainKind, LineCap, LineJoin, Stop, Stroke},
    Color, Fill, ShapeId,
};

const PX_TO_MM: f64 = 25.4 / 96.0;

/// Result of an import: shapes in page space (mm, Y up) and the page size
/// the SVG declared.
pub struct Imported {
    pub shapes: Vec<Shape>,
    pub size: Size,
}

/// How deep elements may nest in an SVG file. The XML reader walks the
/// elements by calling itself once per level, with no bound of its own,
/// so a file of nothing but opening `<g>` tags (50 000 of them in
/// 350 KB) ran the stack out, which cannot be caught, and closed the
/// program. Drawings nest a few dozen levels at most; a file deeper
/// than this is refused with a clear error before it is read.
pub const MAX_ELEMENT_DEPTH: usize = 256;

/// How deep the elements of XML text nest: the most elements open at
/// once, counted from the text alone (comments, CDATA sections,
/// processing instructions and the document type skipped, self-closing
/// tags not counted). Text that is not XML gives some number; the XML
/// reader then says what is wrong with it.
fn element_depth(xml: &str) -> usize {
    let b = xml.as_bytes();
    let (mut depth, mut deepest) = (0usize, 0usize);
    let mut i = 0;
    // The end of the section starting at `i` with `open`, after `close`.
    let skip_to = |i: usize, close: &[u8]| -> usize {
        b[i..]
            .windows(close.len())
            .position(|w| w == close)
            .map_or(b.len(), |p| i + p + close.len())
    };
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            i = skip_to(i + 4, b"-->");
        } else if rest.starts_with(b"<![CDATA[") {
            i = skip_to(i + 9, b"]]>");
        } else if rest.starts_with(b"<?") {
            i = skip_to(i + 2, b"?>");
        } else if rest.starts_with(b"<!") {
            // The document type, whose internal subset may hold `>`.
            let mut j = i + 2;
            let mut in_subset = false;
            while j < b.len() {
                match b[j] {
                    b'[' => in_subset = true,
                    b']' => in_subset = false,
                    b'>' if !in_subset => break,
                    _ => {}
                }
                j += 1;
            }
            i = j + 1;
        } else if rest.starts_with(b"</") {
            depth = depth.saturating_sub(1);
            i = skip_to(i + 2, b">");
        } else {
            // A start tag: to its `>` outside quoted attribute values;
            // `/>` closes the element at once.
            let mut j = i + 1;
            let mut quote: Option<u8> = None;
            let mut self_closing = false;
            while j < b.len() {
                match (quote, b[j]) {
                    (Some(q), c) if c == q => quote = None,
                    (Some(_), _) => {}
                    (None, c @ (b'"' | b'\'')) => quote = Some(c),
                    (None, b'>') => {
                        self_closing = j > 0 && b[j - 1] == b'/';
                        break;
                    }
                    _ => {}
                }
                j += 1;
            }
            if !self_closing {
                depth += 1;
                deepest = deepest.max(depth);
            }
            i = j + 1;
        }
    }
    deepest
}

/// The stack of the thread an SVG file is read on: groups nested up to
/// [`MAX_ELEMENT_DEPTH`] are read by recursion, whatever thread asks.
#[cfg(not(target_arch = "wasm32"))]
const PARSE_STACK: usize = 32 << 20;

/// Parse SVG text. `ids` allocates shape identifiers so the result can be
/// added to an existing document. Outside the browser the file is read on
/// a thread with a stack of its own ([`PARSE_STACK`]), so deeply nested
/// groups have a known room whatever thread asks for the import; when
/// that thread cannot be started the file is read here.
pub fn parse(svg: &str, ids: &mut IdSource) -> std::result::Result<Imported, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let spawned = std::thread::scope(|s| {
            let handle = std::thread::Builder::new()
                .name("svg-import".into())
                .stack_size(PARSE_STACK)
                .spawn_scoped(s, || parse_here(svg, ids))
                .ok()?;
            Some(
                handle
                    .join()
                    .unwrap_or_else(|_| Err("the SVG reader stopped unexpectedly".into())),
            )
        });
        if let Some(result) = spawned {
            return result;
        }
    }
    parse_here(svg, ids)
}

/// [`parse`] on the current thread.
fn parse_here(svg: &str, ids: &mut IdSource) -> std::result::Result<Imported, String> {
    let depth = element_depth(svg);
    if depth > MAX_ELEMENT_DEPTH {
        return Err(format!(
            "the file nests {depth} elements deep; at most {MAX_ELEMENT_DEPTH} are read"
        ));
    }
    let split = split_style_sheets(svg);
    let svg: &str = &split;
    let opt = usvg::Options::default();
    // Text elements become marker paths that usvg places; should the
    // changed source not read, the file is read as it is, without text.
    let (prepared, mut texts) = crate::svg_text::prepare(svg);
    let (prepared, mut links) = mark_links(&prepared);
    let tree = match usvg::Tree::from_str(&prepared, &opt) {
        Ok(tree) => tree,
        Err(_) if prepared != svg => {
            texts.clear();
            links.clear();
            usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?
        }
        Err(e) => return Err(e.to_string()),
    };
    let size = tree.size();
    let height_px = size.height() as f64;
    // SVG px, Y down -> mm, Y up.
    let to_page = Affine::new([PX_TO_MM, 0.0, 0.0, -PX_TO_MM, 0.0, height_px * PX_TO_MM]);
    let cx = Cx {
        to_page,
        texts: &texts,
        links: &links,
    };
    let mut shapes = Vec::new();
    for node in tree.root().children() {
        if let Some(s) = convert_node(node, &cx, ids) {
            shapes.push(s);
        }
    }
    Ok(Imported {
        shapes,
        size: Size::new(size.width() as f64 * PX_TO_MM, height_px * PX_TO_MM),
    })
}

/// Style sheets longer than this are cut into several (see
/// [`split_style_sheets`]).
const LONG_STYLE_SHEET: usize = 16 * 1024;

/// Cut each long `<style>` element of `svg` into several in a row, each
/// holding whole rules. The style sheet reader works out the line and
/// column of every place it gives up on a value, as it does at the end
/// of every declaration, by counting from the start of the sheet: a
/// sheet of 20 000 rules took 9 seconds to read, twice per import, and
/// one of 200 000 seemed to hang (#3341). Short sheets cost little, and
/// the rules keep their order, so the cascade is the same. A source
/// that cannot be read, or holds no long sheet, is given back unchanged.
fn split_style_sheets(svg: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    if svg.len() < LONG_STYLE_SHEET || !svg.contains("style") {
        return Cow::Borrowed(svg);
    }
    let opt = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let Ok(doc) = roxmltree::Document::parse_with_options(svg, opt) else {
        return Cow::Borrowed(svg);
    };
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for n in doc.descendants().filter(|n| n.has_tag_name("style")) {
        let mut kids = n.children();
        let (Some(text), None) = (kids.next(), kids.next()) else {
            continue;
        };
        let Some(css) = text
            .text()
            .filter(|t| text.is_text() && t.len() > LONG_STYLE_SHEET)
        else {
            continue;
        };
        let range = n.range();
        let element = &svg[range.clone()];
        let tag_from = n
            .attributes()
            .next_back()
            .map_or(range.start, |a| a.range_value().end);
        let (Some(open_end), Some(close_at)) = (svg[tag_from..].find('>'), element.rfind("</"))
        else {
            continue;
        };
        let open = &svg[range.start..tag_from + open_end + 1];
        let close = &element[close_at..];
        // About the square root of the sheet's length: each piece costs
        // its own length per declaration, and each also sorts the rules
        // read so far.
        let piece = ((2.0 * css.len() as f64).sqrt() as usize).max(4096);
        let mut out = String::with_capacity(element.len() + 64);
        for chunk in css_pieces(css, piece) {
            out.push_str(open);
            for c in chunk.chars() {
                match c {
                    '&' => out.push_str("&amp;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    c => out.push(c),
                }
            }
            out.push_str(close);
        }
        edits.push((range, out));
    }
    if edits.is_empty() {
        return Cow::Borrowed(svg);
    }
    let mut out = String::with_capacity(svg.len() + 1024);
    let mut at = 0;
    for (range, text) in edits {
        out.push_str(&svg[at..range.start]);
        out.push_str(&text);
        at = range.end;
    }
    out.push_str(&svg[at..]);
    Cow::Owned(out)
}

/// `css` cut after rules that close at the outermost level once a piece
/// is at least `piece` bytes long; braces in comments and strings do not
/// count.
fn css_pieces(css: &str, piece: usize) -> Vec<&str> {
    let b = css.as_bytes();
    let (mut pieces, mut start, mut depth, mut i) = (Vec::new(), 0, 0usize, 0);
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = css[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
                continue;
            }
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && i + 1 - start >= piece {
                    pieces.push(&css[start..i + 1]);
                    start = i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if start < b.len() {
        pieces.push(&css[start..]);
    }
    pieces
}

/// What every node is converted with.
struct Cx<'a> {
    /// SVG px, Y down -> mm, Y up.
    to_page: Affine,
    /// The text elements, by the number their marker path carries.
    texts: &'a [crate::svg_text::SvgText],
    /// The links, by the id of the group their `<a>` element became.
    links: &'a std::collections::HashMap<String, Link>,
}

/// A link of the file: an `<a>` element around objects.
struct Link {
    href: String,
    /// The element had no id: the one it carries was made here and is
    /// not a name.
    made_id: bool,
}

const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Turn the `<a>` elements of `svg` that link objects into groups with
/// an id, and give their addresses by that id (#2191). usvg reads an
/// `<a>` as a group but keeps neither its address nor its id, so the
/// links of a file were lost. Links inside text, clips, masks, patterns
/// and markers stay as they are, and so does an element not spelled
/// `<a ...> ... </a>`. A source that cannot be read is given back
/// unchanged.
fn mark_links(svg: &str) -> (String, std::collections::HashMap<String, Link>) {
    let mut links = std::collections::HashMap::new();
    let opt = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let Ok(doc) = roxmltree::Document::parse_with_options(svg, opt) else {
        return (svg.to_string(), links);
    };
    let named = |n: roxmltree::Node, name: &str| {
        n.is_element() && n.tag_name().name() == name && n.tag_name().namespace() == Some(SVG_NS)
    };
    let taken: std::collections::HashSet<&str> = doc
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .collect();
    // What takes the place of which bytes of the source.
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut made = 0usize;
    for node in doc.descendants().filter(|n| named(*n, "a")) {
        let Some(href) = node
            .attribute("href")
            .or_else(|| node.attribute((XLINK_NS, "href")))
            .map(str::trim)
            .filter(|h| !h.is_empty())
        else {
            continue;
        };
        if node.ancestors().skip(1).any(|a| {
            ["text", "clipPath", "mask", "pattern", "marker"]
                .iter()
                .any(|name| named(a, name))
        }) {
            continue;
        }
        let range = node.range();
        let Some(text) = svg.get(range.clone()) else {
            continue;
        };
        let opens = text
            .strip_prefix("<a")
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_whitespace() || c == '>');
        // The end tag: `</a`, blanks, `>` at the very end.
        let close = text
            .rfind("</a")
            .filter(|c| text.get(c + 3..).is_some_and(|rest| rest.trim() == ">"));
        let (true, Some(close)) = (opens, close) else {
            continue;
        };
        let own = node.attribute("id").filter(|id| !id.is_empty());
        let id = match own {
            Some(id) => id.to_string(),
            None => loop {
                made += 1;
                let id = format!("tracedraw-link-{made}");
                if !taken.contains(id.as_str()) {
                    break id;
                }
            },
        };
        let open = match own {
            Some(_) => "g".to_string(),
            None => format!("g id=\"{id}\""),
        };
        edits.push((range.start + 1..range.start + 2, open));
        edits.push((
            range.start + close + 2..range.start + close + 3,
            "g".to_string(),
        ));
        links.insert(
            id,
            Link {
                href: href.to_string(),
                made_id: own.is_none(),
            },
        );
    }
    if edits.is_empty() {
        return (svg.to_string(), links);
    }
    edits.sort_by_key(|(r, _)| r.start);
    let mut out = String::with_capacity(svg.len() + edits.len() * 24);
    let mut at = 0;
    for (range, rep) in &edits {
        out.push_str(&svg[at..range.start]);
        out.push_str(rep);
        at = range.end;
    }
    out.push_str(&svg[at..]);
    (out, links)
}

fn convert_node(node: &usvg::Node, cx: &Cx, ids: &mut IdSource) -> Option<Shape> {
    match node {
        usvg::Node::Group(g) => convert_group(g, cx, ids),
        _ => convert_leaf(node, cx, ids),
    }
}

/// A node that is not a group. Kept out of the recursion through
/// [`convert_node`] and [`convert_group`], so each level of nested groups
/// takes only a small stack frame (deep files read on small stacks).
#[inline(never)]
fn convert_leaf(node: &usvg::Node, cx: &Cx, ids: &mut IdSource) -> Option<Shape> {
    let to_page = cx.to_page;
    match node {
        usvg::Node::Group(_) => None,
        usvg::Node::Path(p) => match crate::svg_text::marker_index(p) {
            Some(n) => {
                let text = cx.texts.get(n)?;
                crate::svg_text::to_shape(text, to_page * to_affine(p.abs_transform()), ids)
            }
            None => convert_path(p, to_page, ids),
        },
        usvg::Node::Image(img) => convert_image(img, to_page, ids),
        // usvg is built without its text engine: text elements reach
        // the tree as marker paths (above), never as text nodes.
        usvg::Node::Text(_) => None,
    }
}

fn convert_group(g: &usvg::Group, cx: &Cx, ids: &mut IdSource) -> Option<Shape> {
    // usvg already pushed the group transform into the children's
    // absolute transforms, so children are converted with `to_page` only.
    let children: Vec<Shape> = g
        .children()
        .iter()
        .filter_map(|c| convert_node(c, cx, ids))
        .collect();
    if children.is_empty() {
        return None;
    }
    finish_group(g, children, cx, ids)
}

/// The group `g` made of its converted `children`: clip frame, opacity,
/// mask, name and link. Out of the recursion, as [`convert_leaf`] is.
#[inline(never)]
fn finish_group(
    g: &usvg::Group,
    mut children: Vec<Shape>,
    cx: &Cx,
    ids: &mut IdSource,
) -> Option<Shape> {
    let to_page = cx.to_page;
    // A link's group carries the address; an id made for it is no name.
    let link = cx.links.get(g.id());
    let name = match link {
        Some(l) if l.made_id => "",
        _ => g.id(),
    };
    let opacity = g.opacity().get() as f64;
    let mut shape = if let Some(clip) = g.clip_path() {
        // Clip path -> ClipFrame with the union of the clip's paths as frame.
        // Clip content is in the user space of the clipped group: its
        // nodes' absolute transforms start at the clip's root, so the
        // group's own absolute transform and the clip's transform come
        // first.
        let mut frame_path = BezPath::new();
        collect_paths(clip.root(), &mut frame_path);
        let frame_path =
            to_page * to_affine(g.abs_transform()) * to_affine(clip.transform()) * frame_path;
        if frame_path.elements().is_empty() {
            group_of(children, ids)
        } else {
            let mut frame = Shape::new(
                ShapeId(ids.shape().0),
                ShapeKind::Path {
                    path: frame_path,
                    closed: true,
                },
            );
            frame.fill = Fill::None;
            frame.stroke = None;
            Shape::new(
                ShapeId(ids.shape().0),
                ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents: std::mem::take(&mut children),
                },
            )
        }
    } else if children.len() == 1 && name.is_empty() {
        children.pop().unwrap()
    } else {
        group_of(children, ids)
    };
    if opacity < 1.0 {
        shape.opacity *= opacity;
    }
    if let Some(mask) = g
        .mask()
        .and_then(|m| mask_fill(m, &shape, to_page * to_affine(g.abs_transform())))
    {
        shape
            .effects
            .push(tracedraw_core::live::Effect::Transparency {
                mask,
                merge: Default::default(),
                target: 2,
            });
    }
    if !name.is_empty() {
        shape.name = Some(name.to_string());
    }
    if let Some(link) = link {
        shape.link = Some(link.href.clone());
    }
    Some(shape)
}

/// A luminance mask that paints one fill, a gradient or a flat colour, over
/// `shape` as that fill: it comes in as the shape's transparency, white
/// keeping it, black clearing it. The fill's frame is the shape's own box.
/// Any other mask is left out, as before.
fn mask_fill(mask: &usvg::Mask, shape: &Shape, to_page: Affine) -> Option<Fill> {
    if mask.kind() != usvg::MaskType::Luminance || mask.mask().is_some() {
        return None;
    }
    fn paths<'a>(g: &'a usvg::Group, out: &mut Vec<&'a usvg::Path>) -> bool {
        for n in g.children() {
            match n {
                usvg::Node::Path(p) => out.push(p),
                usvg::Node::Group(g)
                    if g.clip_path().is_none()
                        && g.mask().is_none()
                        && g.opacity().get() >= 1.0 =>
                {
                    if !paths(g, out) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
    let mut found = Vec::new();
    if !paths(mask.root(), &mut found) {
        return None;
    }
    let [p] = found.as_slice() else {
        return None;
    };
    if p.stroke().is_some() || !p.is_visible() {
        return None;
    }
    let f = p.fill()?;
    if f.opacity().get() < 1.0 {
        return None;
    }
    let fill = convert_paint(
        f.paint(),
        1.0,
        shape.bounds(),
        to_page * to_affine(p.abs_transform()),
    );
    matches!(fill, Fill::Solid(_) | Fill::Fountain(_)).then_some(fill)
}

fn group_of(children: Vec<Shape>, ids: &mut IdSource) -> Shape {
    Shape::new(ShapeId(ids.shape().0), ShapeKind::Group { children })
}

fn collect_paths(group: &usvg::Group, out: &mut BezPath) {
    for n in group.children() {
        match n {
            usvg::Node::Path(p) => {
                let t = p.abs_transform();
                let path = bez_path(p.data(), to_affine(t));
                out.extend(path.elements().iter().copied());
            }
            usvg::Node::Group(g) => collect_paths(g, out),
            _ => {}
        }
    }
}

fn to_affine(t: usvg::Transform) -> Affine {
    Affine::new([
        t.sx as f64,
        t.ky as f64,
        t.kx as f64,
        t.sy as f64,
        t.tx as f64,
        t.ty as f64,
    ])
}

fn bez_path(data: &usvg::tiny_skia_path::Path, t: Affine) -> BezPath {
    use usvg::tiny_skia_path::PathSegment;
    let mut out = BezPath::new();
    for seg in data.segments() {
        match seg {
            PathSegment::MoveTo(p) => out.move_to(t * Point::new(p.x as f64, p.y as f64)),
            PathSegment::LineTo(p) => out.line_to(t * Point::new(p.x as f64, p.y as f64)),
            PathSegment::QuadTo(a, p) => out.quad_to(
                t * Point::new(a.x as f64, a.y as f64),
                t * Point::new(p.x as f64, p.y as f64),
            ),
            PathSegment::CubicTo(a, b, p) => out.curve_to(
                t * Point::new(a.x as f64, a.y as f64),
                t * Point::new(b.x as f64, b.y as f64),
                t * Point::new(p.x as f64, p.y as f64),
            ),
            PathSegment::Close => out.close_path(),
        }
    }
    out
}

fn convert_path(p: &usvg::Path, to_page: Affine, ids: &mut IdSource) -> Option<Shape> {
    if !p.is_visible() {
        return None;
    }
    let t = to_page * to_affine(p.abs_transform());
    let mut path = bez_path(p.data(), t);
    if path.elements().is_empty() {
        return None;
    }
    // A filled path with no outline whose shapes are left open is filled
    // all the same, as if each were closed: schematic programs write the
    // filled pin shapes so, which came in with no fill (#3322). With an
    // outline the open ends stay open, the outline not drawn across them.
    if p.fill().is_some() && p.stroke().is_none() {
        path = closed_subpaths(&path);
    }
    let closed = path
        .elements()
        .iter()
        .any(|e| matches!(e, PathEl::ClosePath));
    let bounds = bbox(&path);
    let mut shape = Shape::new(ShapeId(ids.shape().0), ShapeKind::Path { path, closed });
    shape.fill = match p.fill() {
        // A gradient's points are in the path's user space: placed on the
        // page by the path's own transform as well (the viewBox scale and
        // the groups'), not by the page flip alone.
        Some(f) => convert_paint(f.paint(), f.opacity().get() as f64, bounds, t),
        None => Fill::None,
    };
    shape.stroke = p.stroke().map(|s| convert_stroke(s, t));
    if !p.id().is_empty() {
        shape.name = Some(p.id().to_string());
    }
    Some(shape)
}

/// `path` with every shape in it closed.
fn closed_subpaths(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut open = false;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(_) if open => {
                out.close_path();
                open = false;
            }
            PathEl::ClosePath => open = false,
            _ => {}
        }
        out.push(*el);
        if !matches!(el, PathEl::ClosePath) {
            open = true;
        }
    }
    if open {
        out.close_path();
    }
    out
}

fn bbox(path: &BezPath) -> Rect {
    use tracedraw_core::geometry::Shape as _;
    path.bounding_box()
}

fn rgb(c: usvg::Color) -> Color {
    Color::rgb8(c.red, c.green, c.blue)
}

fn convert_paint(paint: &usvg::Paint, opacity: f64, bounds: Rect, to_page: Affine) -> Fill {
    let _ = opacity; // per-shape fill opacity is folded into the colour below
    match paint {
        usvg::Paint::Color(c) => Fill::Solid(rgb(*c)),
        usvg::Paint::LinearGradient(g) => {
            let stops = convert_stops(g.stops());
            // Angle from the gradient vector, mapped through the SVG transform
            // and the page flip (which negates the Y component).
            let t = to_page * to_affine(g.transform());
            let a = t * Point::new(g.x1() as f64, g.y1() as f64);
            let b = t * Point::new(g.x2() as f64, g.y2() as f64);
            let angle = (b - a).atan2().to_degrees();
            Fill::Fountain(Fountain {
                kind: FountainKind::Linear,
                stops,
                angle,
                offset: Point::ZERO,
                edge_pad: 0.0,
                ..Default::default()
            })
        }
        usvg::Paint::RadialGradient(g) => {
            let stops = convert_stops(g.stops());
            let t = to_page * to_affine(g.transform());
            let c = t * Point::new(g.cx() as f64, g.cy() as f64);
            let centre = bounds.center();
            let off = if bounds.width() > 0.0 && bounds.height() > 0.0 {
                Point::new(
                    ((c.x - centre.x) / (bounds.width() / 2.0)).clamp(-1.0, 1.0),
                    ((c.y - centre.y) / (bounds.height() / 2.0)).clamp(-1.0, 1.0),
                )
            } else {
                Point::ZERO
            };
            Fill::Fountain(Fountain {
                kind: FountainKind::Radial,
                stops,
                angle: 0.0,
                offset: off,
                edge_pad: 0.0,
                ..Default::default()
            })
        }
        usvg::Paint::Pattern(_) => Fill::Solid(Color::rgb8(128, 128, 128)),
    }
}

fn convert_stops(stops: &[usvg::Stop]) -> Vec<Stop> {
    let mut out: Vec<Stop> = stops
        .iter()
        .map(|s| Stop {
            opacity: s.opacity().get(),
            ..Stop::new(s.offset().get() as f64, rgb(s.color()))
        })
        .collect();
    if out.len() < 2 {
        let c = out.first().map(|s| s.color).unwrap_or(Color::BLACK);
        out = vec![Stop::new(0.0, c), Stop::new(1.0, c)];
    }
    out
}

fn convert_stroke(s: &usvg::Stroke, t: Affine) -> Stroke {
    let color = match s.paint() {
        usvg::Paint::Color(c) => rgb(*c),
        usvg::Paint::LinearGradient(g) => g
            .stops()
            .first()
            .map(|st| rgb(st.color()))
            .unwrap_or(Color::BLACK),
        usvg::Paint::RadialGradient(g) => g
            .stops()
            .first()
            .map(|st| rgb(st.color()))
            .unwrap_or(Color::BLACK),
        usvg::Paint::Pattern(_) => Color::BLACK,
    };
    // Uniform scale of the transform, for the stroke width.
    let scale = (t.as_coeffs()[0].abs() * t.as_coeffs()[3].abs()).sqrt();
    let width = (s.width().get() as f64 * scale).max(Stroke::HAIRLINE);
    let dash = s
        .dasharray()
        .map(|d| {
            d.iter()
                .map(|v| *v as f64 / s.width().get() as f64)
                .collect()
        })
        .unwrap_or_default();
    Stroke {
        color,
        width,
        cap: match s.linecap() {
            usvg::LineCap::Butt => LineCap::Butt,
            usvg::LineCap::Round => LineCap::Round,
            usvg::LineCap::Square => LineCap::Square,
        },
        join: match s.linejoin() {
            usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
            usvg::LineJoin::Round => LineJoin::Round,
            usvg::LineJoin::Bevel => LineJoin::Bevel,
        },
        dash,
        miter_limit: Stroke::miter_angle_from_ratio(s.miterlimit().get() as f64),
        ..Default::default()
    }
}

fn convert_image(img: &usvg::Image, to_page: Affine, ids: &mut IdSource) -> Option<Shape> {
    if !img.is_visible() {
        return None;
    }
    let (png, width_px, height_px) = match img.kind() {
        usvg::ImageKind::PNG(data) => {
            let data: Arc<Vec<u8>> = data.clone();
            let (w, h) = png_size(&data)?;
            (data.as_ref().clone(), w, h)
        }
        // Other encodings are left to the host application's image decoder;
        // here only PNG is embedded directly.
        _ => return None,
    };
    let t = to_page * to_affine(img.abs_transform());
    let sz = img.size();
    let (w, h) = (sz.width() as f64, sz.height() as f64);
    let r = Rect::new(0.0, 0.0, w, h);
    // Upright (the page's Y flip and scales alone): the box on the page.
    // Turned, slanted or mirrored: the picture in its own box, placed by
    // the transform, top row up; the box round it lost the turn.
    let c = t.as_coeffs();
    let upright = c[1].abs() < 1e-9 && c[2].abs() < 1e-9 && c[0] > 0.0 && c[3] < 0.0;
    let (rect, place) = if upright {
        (t.transform_rect_bbox(r), Affine::IDENTITY)
    } else {
        (r, t * Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, h]))
    };
    let mut shape = Shape::new(
        ShapeId(ids.shape().0),
        ShapeKind::Bitmap {
            rect,
            width_px,
            height_px,
            png: png.into(),
            fx: None,
        },
    );
    shape.transform = place;
    shape.stroke = None;
    if !img.id().is_empty() {
        shape.name = Some(img.id().to_string());
    }
    Some(shape)
}

/// Width and height from a PNG header (IHDR).
fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 || &data[1..4] != b"PNG" {
        return None;
    }
    let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    Some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_and_gradient_round_trip_to_page_space() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="192" viewBox="0 0 96 192">
            <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient></defs>
            <g id="layer"><rect id="r" x="0" y="0" width="48" height="48" fill="url(#g)" stroke="#000" stroke-width="2"/>
            <circle cx="48" cy="144" r="24" fill="#0f0"/></g>
        </svg>"##;
        let mut ids = IdSource::default();
        let out = parse(svg, &mut ids).unwrap();
        assert!((out.size.width - 25.4).abs() < 1e-6 && (out.size.height - 50.8).abs() < 1e-6);
        assert_eq!(out.shapes.len(), 1);
        let ShapeKind::Group { children } = &out.shapes[0].kind else {
            panic!("expected a group");
        };
        assert_eq!(children.len(), 2);
        // The rect sits at the top-left of the SVG: in page space its top is at y = 50.8.
        let b = children[0].bounds();
        assert!((b.x0).abs() < 1e-6 && (b.y1 - 50.8).abs() < 1e-6, "{b:?}");
        assert!((b.width() - 12.7).abs() < 1e-6);
        assert!(matches!(children[0].fill, Fill::Fountain(_)));
        let st = children[0].stroke.as_ref().unwrap();
        assert!((st.width - 2.0 * PX_TO_MM).abs() < 1e-6);
        assert_eq!(children[0].name.as_deref(), Some("r"));
        assert_eq!(children[1].fill, Fill::Solid(Color::rgb8(0, 255, 0)));
    }

    /// A radial gradient centred on its object comes in centred, in a
    /// file measured in millimetres and in one whose object is moved by
    /// a transform: the centre was placed without the viewBox scale and
    /// the transforms, and landed in a corner of the object.
    #[test]
    fn a_radial_gradient_keeps_its_centre() {
        let grad = r##"<defs><radialGradient id="g" gradientUnits="userSpaceOnUse" cx="50" cy="40" r="30"><stop offset="0" stop-color="#ff0"/><stop offset="1" stop-color="#080"/></radialGradient></defs>"##;
        for body in [
            r##"<path d="M20 60L80 60L80 20L20 20Z" fill="url(#g)"/>"##,
            r##"<g transform="translate(30 10) scale(0.5 2)"><path d="M20 60L80 60L80 20L20 20Z" fill="url(#g)"/></g>"##,
        ] {
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="200mm" height="150mm" viewBox="0 0 200 150">{grad}{body}</svg>"##
            );
            let mut ids = IdSource::default();
            let out = parse(&svg, &mut ids).unwrap();
            let mut s = &out.shapes[0];
            while let ShapeKind::Group { children } = &s.kind {
                s = &children[0];
            }
            let Fill::Fountain(f) = &s.fill else {
                panic!("{:?}", s.fill);
            };
            assert_eq!(f.kind, FountainKind::Radial);
            assert!(
                f.offset.x.abs() < 0.01 && f.offset.y.abs() < 0.01,
                "{body}: centre off by {:?}",
                f.offset
            );
        }
        // Off centre: a quarter of the width to the right of the middle.
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="200mm" height="150mm" viewBox="0 0 200 150">{}<path d="M5 60L65 60L65 20L5 20Z" fill="url(#g)"/></svg>"##,
            grad
        );
        let mut ids = IdSource::default();
        let out = parse(&svg, &mut ids).unwrap();
        let Fill::Fountain(f) = &out.shapes[0].fill else {
            panic!("{:?}", out.shapes[0].fill);
        };
        assert!(
            (f.offset.x - 0.5).abs() < 0.01 && f.offset.y.abs() < 0.01,
            "{:?}",
            f.offset
        );
    }

    /// Objects inside `<a>` come in with the link's address, a lone
    /// object carrying it itself, several as a linked group; what the
    /// SVG export writes for linked objects reads back (#2191).
    #[test]
    fn links_around_objects_are_read() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="96" height="96">
            <a href="https://example.com/one"><rect x="0" y="0" width="10" height="10" fill="#f00"/></a>
            <a xlink:href="mailto:a@b.c" id="mail">
              <rect x="20" y="0" width="10" height="10" fill="#0f0"/>
              <rect x="40" y="0" width="10" height="10" fill="#00f"/>
            </a >
            <rect x="60" y="0" width="10" height="10" fill="#000"/>
        </svg>"##;
        let mut ids = IdSource::default();
        let out = parse(svg, &mut ids).unwrap();
        assert_eq!(out.shapes.len(), 3);
        let one = &out.shapes[0];
        assert!(matches!(one.kind, ShapeKind::Path { .. }), "{:?}", one.kind);
        assert_eq!(one.link.as_deref(), Some("https://example.com/one"));
        assert_eq!(one.name, None, "no name from the id made for the link");
        let two = &out.shapes[1];
        assert!(matches!(&two.kind, ShapeKind::Group { children } if children.len() == 2));
        assert_eq!(two.link.as_deref(), Some("mailto:a@b.c"));
        assert_eq!(two.name.as_deref(), Some("mail"));
        assert_eq!(out.shapes[2].link, None);
    }

    /// A file of 50 000 nested groups is refused before the XML reader
    /// sees it: the reader's recursion ran the stack out, which cannot
    /// be caught, and closed the program. Drawings nested a hundred
    /// deep are read whole, and the count skips what is not an element.
    #[test]
    fn deeply_nested_elements_are_refused_not_a_crash() {
        let nested = |depth: usize| {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">{}<rect x="1" y="1" width="10" height="10"/>{}</svg>"#,
                "<g>".repeat(depth),
                "</g>".repeat(depth)
            )
        };
        let mut ids = IdSource::default();
        let Err(err) = parse(&nested(50_000), &mut ids) else {
            panic!("a file nested 50 000 deep was read");
        };
        assert!(err.contains("50001 elements deep"), "{err}");
        let imp = parse(&nested(100), &mut ids).expect("a hundred deep is fine");
        let mut shape = &imp.shapes[0];
        while let ShapeKind::Group { children } = &shape.kind {
            shape = &children[0];
        }
        assert!(
            matches!(shape.kind, ShapeKind::Path { .. }),
            "{:?}",
            shape.kind
        );
        // Comments, CDATA, the prologue, the document type, attribute
        // values and self-closing tags hold no elements.
        let text = r#"<?xml version="1.0"?><!DOCTYPE svg [ <!ENTITY a "<g><g>"> ]>
<!-- <g><g><g> --><svg title="<g><g>" xmlns="http://www.w3.org/2000/svg"><g><![CDATA[<g><g><g>]]><rect/><path d='M 0 0'/></g></svg>"#;
        assert_eq!(element_depth(text), 2);
        // The svg, three groups and a self-closing rect.
        assert_eq!(element_depth(&nested(3)), 4);
    }

    /// A turned, slanted or mirrored picture comes in turned: it came in
    /// upright, stretched over the box round it. An upright one keeps
    /// its box on the page with no transform.
    #[test]
    fn a_turned_picture_keeps_its_turn() {
        use tracedraw_core::Document;
        let png = crate::svg::encode_png(
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
            ],
        );
        for place in [
            Affine::IDENTITY,
            Affine::rotate_about(0.4, Point::new(60.0, 55.0)),
            Affine::new([1.0, 0.0, 0.5, 1.0, 0.0, 0.0]),
            Affine::new([-1.0, 0.0, 0.0, 1.0, 150.0, 0.0]),
        ] {
            let mut doc = Document::new("t", tracedraw_core::Size::new(200.0, 150.0));
            let mut b = Shape::new(
                ShapeId(1),
                ShapeKind::Bitmap {
                    rect: Rect::new(30.0, 30.0, 90.0, 80.0),
                    width_px: 2,
                    height_px: 2,
                    png: png.clone().into(),
                    fx: None,
                },
            );
            b.stroke = None;
            b.transform = place;
            let want = b.page_path();
            doc.pages[0].layers[0].shapes.push(b);
            let svg = crate::svg::page_to_svg(&doc, 0);
            let mut ids = IdSource::default();
            let out = parse(&svg, &mut ids).unwrap();
            let mut s = &out.shapes[0];
            while let ShapeKind::Group { children } = &s.kind {
                s = &children[0];
            }
            assert!(matches!(s.kind, ShapeKind::Bitmap { .. }), "{:?}", s.kind);
            if place == Affine::IDENTITY {
                assert_eq!(s.transform, Affine::IDENTITY);
            }
            // The same corners, the picture's top-left one first.
            let corners = |p: &BezPath| -> Vec<Point> {
                p.elements()
                    .iter()
                    .filter_map(|e| e.end_point())
                    .take(4)
                    .collect()
            };
            let got = s.page_path();
            let (a, g) = (corners(&want), corners(&got));
            let near = |p: &Point| g.iter().any(|q| (*p - *q).hypot() < 0.01);
            assert!(a.iter().all(near), "{place:?}: {a:?} vs {g:?}");
            // The top-left pixel (red) where it was: the corner the
            // picture's local top-left goes to.
            let top_left = |s: &Shape| {
                let ShapeKind::Bitmap { rect, .. } = &s.kind else {
                    unreachable!()
                };
                s.transform * Point::new(rect.x0, rect.y1)
            };
            let tl = (top_left(s) - place * Point::new(30.0, 80.0)).hypot();
            assert!(tl < 0.01, "{place:?}: top-left off by {tl}");
        }
    }

    /// A long style sheet reads in a moment and still styles the drawing:
    /// it is read in pieces of whole rules, in order, so a later rule
    /// still beats an earlier one. A sheet of 20 000 rules took 9 seconds
    /// to read, twice per import.
    #[test]
    fn a_long_style_sheet_reads_quickly_and_keeps_its_cascade() {
        let mut css = String::from("@media print { .c1 { fill: #ff0000 } }\n/* } { */\n");
        for i in 0..20_000 {
            css += &format!(".c{i} {{ fill: #{:06x} }}\n", i * 7);
        }
        css += "svg > rect.k { fill: #abcdef }\n.c5 { fill: #123456 }\n";
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><style><![CDATA[{css}]]></style>
            <rect class="c5" width="10" height="10"/><rect class="c19999" x="20" width="10" height="10"/>
            <rect class="c1" x="40" width="10" height="10"/><rect class="k" x="60" width="10" height="10"/></svg>"#
        );
        let t = std::time::Instant::now();
        let out = parse(&svg, &mut IdSource::default()).unwrap();
        assert!(t.elapsed().as_secs() < 30, "{:?}", t.elapsed());
        let fills: Vec<&Fill> = out.shapes.iter().map(|s| &s.fill).collect();
        assert_eq!(
            fills,
            [
                &Fill::Solid(Color::rgb8(0x12, 0x34, 0x56)),
                &Fill::Solid(Color::rgb8(0x02, 0x22, 0xd9)),
                &Fill::Solid(Color::rgb8(0, 0, 7)),
                &Fill::Solid(Color::rgb8(0xab, 0xcd, 0xef)),
            ]
        );
    }

    /// Style sheets are cut only after rules that close at the outermost
    /// level, never inside a block, a comment or a string.
    #[test]
    fn style_sheets_are_cut_between_whole_rules() {
        let css = "a { x: 1 } @media p { b { y: 2 } } /* } */ c[d=\"}\"] { z: 3 } e { w: 4 }";
        let pieces = css_pieces(css, 1);
        assert_eq!(pieces.concat(), css);
        assert_eq!(
            pieces,
            [
                "a { x: 1 }",
                " @media p { b { y: 2 } }",
                " /* } */ c[d=\"}\"] { z: 3 }",
                " e { w: 4 }"
            ]
        );
        assert_eq!(css_pieces(css, 1000), [css]);
        let short = "<svg xmlns=\"http://www.w3.org/2000/svg\"><style>a{fill:red}</style></svg>";
        assert!(matches!(
            split_style_sheets(short),
            std::borrow::Cow::Borrowed(_)
        ));
    }
}

#[cfg(test)]
mod clip_tests {
    use super::*;
    use tracedraw_core::document::ShapeKind;

    #[test]
    fn clip_paths_follow_the_root_scale_and_become_clip_frames() {
        // Millimetre page with a mm viewBox: the root scale is 96/25.4.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 100 50">
            <defs><clipPath id="c"><path d="M 60 10 H 80 V 30 H 60 Z"/></clipPath></defs>
            <g clip-path="url(#c)"><rect x="0" y="0" width="100" height="50" fill="#f00"/></g>
        </svg>"##;
        let mut ids = IdSource::default();
        let imp = parse(svg, &mut ids).expect("parse");
        assert_eq!(imp.shapes.len(), 1);
        match &imp.shapes[0].kind {
            ShapeKind::ClipFrame { frame, .. } => {
                let b = frame.bounds();
                // y is flipped: SVG y 10..30 on a 50 mm page is 20..40 up.
                assert!(
                    (b.x0 - 60.0).abs() < 1e-4 && (b.x1 - 80.0).abs() < 1e-4,
                    "{b:?}"
                );
                assert!(
                    (b.y0 - 20.0).abs() < 1e-4 && (b.y1 - 40.0).abs() < 1e-4,
                    "{b:?}"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_filled_open_path_with_no_outline_is_closed() {
        // The pin shapes of the schematic attached to #3322: filled, no
        // outline, no `Z`.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M0 0V5H3L5 2.5 3 0" fill="#ff0000"/><path d="M0 0L5 5" fill="#00ff00" stroke="#000"/></svg>"##;
        let out = parse(svg, &mut IdSource::default()).unwrap();
        assert_eq!(out.shapes.len(), 2);
        let ShapeKind::Path { path, closed } = &out.shapes[0].kind else {
            panic!("expected a path");
        };
        assert!(*closed);
        assert!(matches!(path.elements().last(), Some(PathEl::ClosePath)));
        assert_eq!(out.shapes[0].fill, Fill::Solid(Color::rgb8(255, 0, 0)));
        // With an outline the open end stays open.
        let ShapeKind::Path { path, .. } = &out.shapes[1].kind else {
            panic!("expected a path");
        };
        assert!(!path.elements().iter().any(|e| matches!(e, PathEl::ClosePath)));
    }
}

#[cfg(test)]
mod mask_tests {
    use super::*;
    use tracedraw_core::live::Effect;

    fn faded(imp: &Imported) -> &Shape {
        let [first] = imp.shapes.as_slice() else {
            panic!("{:#?}", imp.shapes);
        };
        let mut s = first;
        // The export's group round an object with effects.
        while let ShapeKind::Group { children } = &s.kind {
            let [c] = children.as_slice() else {
                panic!("{:#?}", imp.shapes);
            };
            s = c;
        }
        s
    }

    /// A luminance mask painted with a gradient fades the object it
    /// masks: it comes in as the object's transparency. The object came
    /// in solid, the mask left out.
    #[test]
    fn a_gradient_mask_comes_in_as_a_transparency() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 100 50">
            <defs>
              <linearGradient id="g" gradientUnits="userSpaceOnUse" x1="10" y1="0" x2="90" y2="0">
                <stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/>
              </linearGradient>
              <mask id="m" maskUnits="userSpaceOnUse" x="-100" y="-100" width="300" height="300">
                <rect x="-100" y="-100" width="300" height="300" fill="url(#g)"/>
              </mask>
            </defs>
            <g mask="url(#m)"><rect x="10" y="10" width="80" height="30" fill="#c81e1e"/></g>
        </svg>"##;
        let mut ids = IdSource::default();
        let imp = parse(svg, &mut ids).expect("parse");
        let s = faded(&imp);
        let Some(Effect::Transparency { mask, target, .. }) = s.effects.first() else {
            panic!("{:?}", s.effects);
        };
        assert_eq!(*target, 2);
        let Fill::Fountain(f) = mask else {
            panic!("{mask:?}");
        };
        assert!(f.angle.abs() < 1.0, "{}", f.angle);
        assert_eq!(f.stops.first().unwrap().color.to_rgb8(), [255, 255, 255]);
        assert_eq!(f.stops.last().unwrap().color.to_rgb8(), [0, 0, 0]);
    }

    /// A transparency that fades across an object goes out to SVG as a
    /// mask, not as one flat opacity, and comes back as the same fade.
    #[test]
    fn a_fading_transparency_goes_out_and_back() {
        let mut doc = tracedraw_core::Document::default();
        let mut sq = Shape::new(
            ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(20.0, 20.0, 120.0, 80.0),
                radius: 0.0,
                corners: None,
            },
        );
        sq.fill = Fill::Solid(tracedraw_core::Color::rgb8(200, 30, 30));
        sq.stroke = None;
        sq.effects.push(Effect::Transparency {
            mask: Fill::Fountain(Fountain {
                kind: FountainKind::Linear,
                stops: vec![
                    tracedraw_core::Stop::new(0.0, tracedraw_core::Color::rgb8(255, 255, 255)),
                    tracedraw_core::Stop::new(1.0, tracedraw_core::Color::rgb8(0, 0, 0)),
                ],
                angle: 90.0,
                ..Default::default()
            }),
            merge: Default::default(),
            target: 2,
        });
        doc.pages[0].layers[0].shapes.push(sq);
        let svg = crate::svg::page_to_svg(&doc, 0);
        assert!(svg.contains("<mask id=\"tmask"), "{svg}");
        assert!(!svg.contains("opacity=\"0.5"), "{svg}");
        let mut ids = IdSource::default();
        let imp = parse(&svg, &mut ids).expect("parse");
        let s = faded(&imp);
        assert_eq!(
            s.fill,
            Fill::Solid(tracedraw_core::Color::rgb8(200, 30, 30))
        );
        assert!((s.opacity - 1.0).abs() < 1e-9, "{}", s.opacity);
        let Some(Effect::Transparency {
            mask: Fill::Fountain(f),
            ..
        }) = s.effects.first()
        else {
            panic!("{:?}", s.effects);
        };
        assert!((f.angle - 90.0).abs() < 1.0, "{}", f.angle);
        assert_eq!(f.stops.first().unwrap().color.to_rgb8(), [255, 255, 255]);
        assert_eq!(f.stops.last().unwrap().color.to_rgb8(), [0, 0, 0]);
    }
}
