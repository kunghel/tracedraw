//! Workflows (Tools > Workflow): steps drawn as boxes on a canvas of their
//! own and joined by wires, each step working on what the step before it
//! made. A picture comes in (the selection or a picture file), is
//! vectorized, recoloured and given bitmap effects, and goes out onto the
//! page or into a file. One output may feed several steps, so branches
//! made with other settings can be compared side by side. A workflow is
//! saved as a `.tdflow` file (JSON) and runs again on another input.
//!
//! This module is the model and the run, with no interface: the window
//! is `ui/workflow.rs`, which gathers the inputs, runs the workflow
//! (off the interface thread on the desktop) and places or saves what
//! the Export steps receive. Other parts of the program add steps of
//! their own with [`register`] (an [`Extension`]); they are saved with
//! their kind and settings and run like the built-in ones. See
//! docs/behavior/workflow.md.

use crate::trace::{Cancel, Preset, Remove, Traced};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tracedraw_core::recolor::Recolor;
use tracedraw_core::{BitmapEffect, Color};

pub type NodeId = u32;

/// The file format's version, written in every saved workflow.
pub const VERSION: u32 = 1;
/// The extension of a saved workflow.
pub const EXTENSION: &str = "tdflow";
/// The most inputs a step takes (a Compare step).
pub const MAX_INPUTS: usize = 4;

/// Where an Input step's picture comes from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum Source {
    /// The objects selected when the workflow runs: a picture as it is,
    /// anything else drawn into one.
    #[default]
    Selection,
    /// A picture file.
    File { path: String },
}

/// Where an Export step sends what it receives.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Target {
    /// Onto the active layer, over the input.
    #[default]
    Page,
    /// Into a file: PNG, or SVG for vectors when the name ends in `.svg`.
    File { path: String },
}

/// What a step does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    Input {
        #[serde(default)]
        source: Source,
    },
    /// Trace a picture into curves (vectors pass through drawn first).
    Vectorize {
        preset: Preset,
        /// The most colours kept; 0 keeps the preset's.
        #[serde(default)]
        colors: u32,
        #[serde(default)]
        remove: Remove,
    },
    /// Turn, saturate, lighten or tint every colour, then invert.
    Recolor {
        #[serde(default)]
        recolor: Recolor,
        #[serde(default)]
        invert: bool,
    },
    /// One bitmap effect (vectors are drawn into a picture first).
    Effect { effect: BitmapEffect },
    /// Up to four results shown side by side.
    Compare,
    Export {
        #[serde(default)]
        target: Target,
    },
    /// A step added by another part of the program ([`register`]).
    Extension {
        kind: String,
        #[serde(default)]
        params: BTreeMap<String, String>,
    },
}

impl Step {
    /// The built-in steps, as the Add list offers them.
    pub fn builtin() -> Vec<Step> {
        vec![
            Step::Input {
                source: Source::Selection,
            },
            Step::Vectorize {
                preset: Preset::Logo,
                colors: 0,
                remove: Remove::Background,
            },
            Step::Recolor {
                recolor: Recolor::default(),
                invert: false,
            },
            Step::Effect {
                effect: crate::fx::new_effect("gaussian_blur"),
            },
            Step::Compare,
            Step::Export {
                target: Target::Page,
            },
        ]
    }

    /// The i18n key of the step's name (an extension's own key).
    pub fn name_key(&self) -> String {
        match self {
            Step::Input { .. } => "flow.step_input".into(),
            Step::Vectorize { .. } => "flow.step_vectorize".into(),
            Step::Recolor { .. } => "flow.step_recolor".into(),
            Step::Effect { .. } => "flow.step_effect".into(),
            Step::Compare => "flow.step_compare".into(),
            Step::Export { .. } => "flow.step_export".into(),
            Step::Extension { kind, .. } => extension(kind)
                .map(|e| e.name_key.to_string())
                .unwrap_or_else(|| kind.clone()),
        }
    }

    /// How many wires come in.
    pub fn inputs(&self) -> usize {
        match self {
            Step::Input { .. } => 0,
            Step::Compare => MAX_INPUTS,
            Step::Extension { kind, .. } => extension(kind).map_or(1, |e| e.inputs.min(MAX_INPUTS)),
            _ => 1,
        }
    }

    /// Whether a wire can go out (Compare and Export end a branch).
    pub fn has_output(&self) -> bool {
        !matches!(self, Step::Compare | Step::Export { .. })
    }

    /// A new extension step of `kind` with its default settings.
    pub fn extension_step(e: &Extension) -> Step {
        Step::Extension {
            kind: e.kind.to_string(),
            params: e
                .params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
}

/// A step on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    /// The box's top-left corner on the workflow canvas, points.
    pub pos: [f32; 2],
    pub step: Step,
}

/// A wire from a step's output to input `port` of another step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wire {
    pub from: NodeId,
    pub to: NodeId,
    #[serde(default)]
    pub port: usize,
}

/// A workflow: its steps and wires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    #[serde(default = "version")]
    pub tracedraw_flow: u32,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub wires: Vec<Wire>,
}

fn version() -> u32 {
    VERSION
}

impl Default for Flow {
    /// The workflow a new window starts with: the selection vectorized
    /// and placed on the page.
    fn default() -> Self {
        let mut f = Flow::empty();
        let input = f.add(
            Step::Input {
                source: Source::Selection,
            },
            [20.0, 60.0],
        );
        let trace = f.add(
            Step::Vectorize {
                preset: Preset::Logo,
                colors: 0,
                remove: Remove::Background,
            },
            [220.0, 60.0],
        );
        let out = f.add(
            Step::Export {
                target: Target::Page,
            },
            [420.0, 60.0],
        );
        f.connect(input, trace, 0);
        f.connect(trace, out, 0);
        f
    }
}

impl Flow {
    pub fn empty() -> Self {
        Flow {
            tracedraw_flow: VERSION,
            nodes: Vec::new(),
            wires: Vec::new(),
        }
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Add a step at `pos`; its id.
    pub fn add(&mut self, step: Step, pos: [f32; 2]) -> NodeId {
        let id = self.nodes.iter().map(|n| n.id + 1).max().unwrap_or(1);
        self.nodes.push(Node { id, pos, step });
        id
    }

    /// Take a step out, with its wires.
    pub fn remove(&mut self, id: NodeId) {
        self.nodes.retain(|n| n.id != id);
        self.wires.retain(|w| w.from != id && w.to != id);
    }

    /// Join `from`'s output to input `port` of `to`, replacing the wire
    /// that came in there. Refused (false) when either step is missing,
    /// `from` has no output, `to` has no such input, or the wire would
    /// close a loop.
    pub fn connect(&mut self, from: NodeId, to: NodeId, port: usize) -> bool {
        let (Some(a), Some(b)) = (self.node(from), self.node(to)) else {
            return false;
        };
        if from == to || !a.step.has_output() || port >= b.step.inputs() {
            return false;
        }
        if self.reaches(to, from) {
            return false;
        }
        self.wires.retain(|w| !(w.to == to && w.port == port));
        self.wires.push(Wire { from, to, port });
        true
    }

    /// Take out the wire coming into input `port` of `to`.
    pub fn disconnect(&mut self, to: NodeId, port: usize) {
        self.wires.retain(|w| !(w.to == to && w.port == port));
    }

    /// Whether a path of wires leads from `a` to `b`.
    fn reaches(&self, a: NodeId, b: NodeId) -> bool {
        let mut seen = vec![a];
        let mut todo = vec![a];
        while let Some(n) = todo.pop() {
            if n == b {
                return true;
            }
            for w in self.wires.iter().filter(|w| w.from == n) {
                if !seen.contains(&w.to) {
                    seen.push(w.to);
                    todo.push(w.to);
                }
            }
        }
        false
    }

    /// The step feeding input `port` of `to`.
    pub fn input_of(&self, to: NodeId, port: usize) -> Option<NodeId> {
        self.wires
            .iter()
            .find(|w| w.to == to && w.port == port)
            .map(|w| w.from)
    }

    /// The steps in an order where every step comes after the ones that
    /// feed it (wires out of missing steps are ignored).
    pub fn order(&self) -> Vec<NodeId> {
        let mut need: HashMap<NodeId, usize> = self.nodes.iter().map(|n| (n.id, 0)).collect();
        for w in &self.wires {
            if need.contains_key(&w.from) {
                if let Some(c) = need.get_mut(&w.to) {
                    *c += 1;
                }
            }
        }
        let mut ready: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| need.get(&n.id) == Some(&0))
            .map(|n| n.id)
            .collect();
        let mut out = Vec::new();
        while !ready.is_empty() {
            let n = ready.remove(0);
            out.push(n);
            for w in self.wires.iter().filter(|w| w.from == n) {
                if let Some(c) = need.get_mut(&w.to) {
                    *c = c.saturating_sub(1);
                    if *c == 0 {
                        ready.push(w.to);
                    }
                }
            }
        }
        out
    }

    /// The Input steps, in order of their ids.
    pub fn input_nodes(&self) -> Vec<&Node> {
        let mut v: Vec<&Node> = self
            .nodes
            .iter()
            .filter(|n| matches!(n.step, Step::Input { .. }))
            .collect();
        v.sort_by_key(|n| n.id);
        v
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// A saved workflow; its wires that are not valid any more are left
    /// out (a step of an extension that is missing keeps one input).
    pub fn from_json(text: &str) -> Result<Flow, String> {
        let mut f: Flow = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if f.tracedraw_flow > VERSION {
            return Err("flow.newer_version".into());
        }
        let wires = std::mem::take(&mut f.wires);
        for w in wires {
            f.connect(w.from, w.to, w.port);
        }
        Ok(f)
    }
}

// ----- values --------------------------------------------------------------------

/// Curves in the pixel space of a `w` by `h` picture (y down), as a
/// trace gives them, with the width of open curves' outlines.
#[derive(Debug, Clone, PartialEq)]
pub struct Art {
    pub traced: Vec<Traced>,
    pub w: u32,
    pub h: u32,
    pub stroke_px: f64,
}

/// What travels along a wire.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Picture(Arc<RgbaImage>),
    Art(Arc<Art>),
}

impl Value {
    /// The value as a picture (curves drawn at their picture's size).
    pub fn picture(&self) -> Arc<RgbaImage> {
        match self {
            Value::Picture(p) => p.clone(),
            Value::Art(a) => Arc::new(crate::trace::render_traced(
                &a.traced,
                a.w.max(1),
                a.h.max(1),
                a.stroke_px,
            )),
        }
    }

    pub fn size(&self) -> (u32, u32) {
        match self {
            Value::Picture(p) => p.dimensions(),
            Value::Art(a) => (a.w, a.h),
        }
    }
}

/// A step's result: its value, or why it has none (an i18n key, or a
/// message an extension wrote).
pub type Outcome = Result<Value, String>;

/// Run the workflow on `given`, the pictures of its Input steps by id.
/// Every step gets an outcome; a step whose input failed fails with the
/// same message. `None` once `cancel` is set.
pub fn run(
    flow: &Flow,
    given: &BTreeMap<NodeId, Value>,
    cancel: Option<&Cancel>,
) -> Option<BTreeMap<NodeId, Outcome>> {
    let mut out: BTreeMap<NodeId, Outcome> = BTreeMap::new();
    for id in flow.order() {
        if crate::trace::cancelled(cancel) {
            return None;
        }
        let Some(node) = flow.node(id) else {
            continue;
        };
        let inputs: Vec<Option<Outcome>> = (0..node.step.inputs())
            .map(|p| flow.input_of(id, p).and_then(|f| out.get(&f).cloned()))
            .collect();
        let r = run_step(&node.step, id, &inputs, given, cancel)?;
        out.insert(id, r);
    }
    Some(out)
}

/// One step on its inputs (`None`: nothing wired in).
fn run_step(
    step: &Step,
    id: NodeId,
    inputs: &[Option<Outcome>],
    given: &BTreeMap<NodeId, Value>,
    cancel: Option<&Cancel>,
) -> Option<Outcome> {
    // The first input, which every step but Input and Compare needs.
    let first = || -> Result<Value, String> {
        match inputs.first() {
            Some(Some(r)) => r.clone(),
            _ => Err("flow.not_connected".into()),
        }
    };
    Some(match step {
        Step::Input { .. } => given
            .get(&id)
            .cloned()
            .ok_or_else(|| "flow.no_input".into()),
        Step::Vectorize {
            preset,
            colors,
            remove,
        } => match first() {
            Ok(v) => {
                let img = v.picture();
                let mut s = preset.settings();
                if *colors > 0 {
                    s.colors = *colors;
                }
                s.remove = *remove;
                let traced = crate::trace::trace_unless(&img, &s, cancel)?;
                let stroke_px = crate::trace::last_stroke_width_px();
                if traced.is_empty() {
                    Err("flow.nothing_traced".into())
                } else {
                    Ok(Value::Art(Arc::new(Art {
                        traced,
                        w: img.width(),
                        h: img.height(),
                        stroke_px,
                    })))
                }
            }
            Err(e) => Err(e),
        },
        Step::Recolor { recolor, invert } => first().map(|v| recolor_value(&v, recolor, *invert)),
        Step::Effect { effect } => first().map(|v| {
            let img = v.picture();
            Value::Picture(Arc::new(crate::fx::apply_effect(&img, effect)))
        }),
        Step::Compare => {
            // The first result wired in; the window shows them all.
            let mut wired = inputs.iter().flatten();
            match wired.next() {
                Some(r) => r.clone(),
                None => Err("flow.not_connected".into()),
            }
        }
        Step::Export { .. } => first(),
        Step::Extension { kind, params } => match extension(kind) {
            None => Err("flow.unknown_step".into()),
            Some(e) if !(e.available)() => Err(e.unavailable_key.into()),
            Some(e) => {
                let mut values = Vec::new();
                let mut failed = None;
                for i in 0..e.inputs.min(MAX_INPUTS) {
                    match inputs.get(i) {
                        Some(Some(Ok(v))) => values.push(v.clone()),
                        Some(Some(Err(m))) => {
                            failed = Some(m.clone());
                            break;
                        }
                        _ => {
                            failed = Some("flow.not_connected".into());
                            break;
                        }
                    }
                }
                match failed {
                    Some(m) => Err(m),
                    None => (e.run)(&values, params, cancel),
                }
            }
        },
    })
}

/// The colour `c` (RGB) as Recolor and Invert make it.
fn recolor_rgb(c: [u8; 3], r: &Recolor, invert: bool) -> [u8; 3] {
    let c = if r.is_identity() {
        c
    } else {
        r.apply(Color::rgb8(c[0], c[1], c[2])).to_rgb8()
    };
    if invert {
        [255 - c[0], 255 - c[1], 255 - c[2]]
    } else {
        c
    }
}

/// A value with every colour recoloured: each pixel of a picture (its
/// opacity kept), each curve's colour and blend of vectors.
pub fn recolor_value(v: &Value, r: &Recolor, invert: bool) -> Value {
    let mut seen: HashMap<[u8; 3], [u8; 3]> = HashMap::new();
    let mut map = |c: [u8; 3]| *seen.entry(c).or_insert_with(|| recolor_rgb(c, r, invert));
    match v {
        Value::Picture(p) => {
            let mut img = p.as_ref().clone();
            for px in img.pixels_mut() {
                if px[3] == 0 {
                    continue;
                }
                let c = map([px[0], px[1], px[2]]);
                px[0] = c[0];
                px[1] = c[1];
                px[2] = c[2];
            }
            Value::Picture(Arc::new(img))
        }
        Value::Art(a) => {
            let mut a = a.as_ref().clone();
            for t in &mut a.traced {
                let c = map(t.color.to_rgb8());
                t.color = Color::rgb8(c[0], c[1], c[2]);
                if let Some(b) = &mut t.blend {
                    b.start = map(b.start);
                    b.end = map(b.end);
                }
            }
            Value::Art(Arc::new(a))
        }
    }
}

// ----- extensions ----------------------------------------------------------------

/// A step another part of the program adds to workflows (the optional AI
/// steps come this way): it is listed under Add, saved with its `kind`
/// and settings, and run with the values wired into it.
pub struct Extension {
    /// The stable name saved in workflow files ("ai.generate").
    pub kind: &'static str,
    /// The i18n key of its name.
    pub name_key: &'static str,
    /// How many wires come in (0 for a step that makes its own picture).
    pub inputs: usize,
    /// Its settings, as text: (i18n key of the label, default).
    pub params: &'static [(&'static str, &'static str)],
    /// Whether it can run now (an AI step needs its service set up);
    /// a step that cannot is listed greyed and fails with
    /// `unavailable_key`.
    pub available: fn() -> bool,
    pub unavailable_key: &'static str,
    pub run: fn(&[Value], &BTreeMap<String, String>, Option<&Cancel>) -> Outcome,
}

static EXTENSIONS: Mutex<Vec<&'static Extension>> = Mutex::new(Vec::new());

/// Add a step kind to workflows (once per kind; a second one of the same
/// kind replaces the first). Nothing in the program registers one yet:
/// the optional AI steps will.
#[allow(dead_code)]
pub fn register(e: &'static Extension) {
    let mut list = EXTENSIONS.lock().unwrap_or_else(|p| p.into_inner());
    list.retain(|x| x.kind != e.kind);
    list.push(e);
}

/// The step kinds added with [`register`].
pub fn extensions() -> Vec<&'static Extension> {
    EXTENSIONS.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

pub fn extension(kind: &str) -> Option<&'static Extension> {
    extensions().into_iter().find(|e| e.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// A white picture with a red square in the middle.
    fn square() -> Value {
        let mut img = RgbaImage::from_pixel(60, 60, Rgba([255, 255, 255, 255]));
        for y in 20..40 {
            for x in 20..40 {
                img.put_pixel(x, y, Rgba([220, 20, 20, 255]));
            }
        }
        Value::Picture(Arc::new(img))
    }

    fn given(f: &Flow, v: Value) -> BTreeMap<NodeId, Value> {
        f.input_nodes().iter().map(|n| (n.id, v.clone())).collect()
    }

    #[test]
    fn wires_refuse_loops_missing_ports_and_dead_ends() {
        let mut f = Flow::empty();
        let a = f.add(
            Step::Input {
                source: Source::Selection,
            },
            [0.0; 2],
        );
        let b = f.add(
            Step::Recolor {
                recolor: Recolor::default(),
                invert: true,
            },
            [0.0; 2],
        );
        let c = f.add(
            Step::Effect {
                effect: crate::fx::new_effect("gaussian_blur"),
            },
            [0.0; 2],
        );
        let out = f.add(
            Step::Export {
                target: Target::Page,
            },
            [0.0; 2],
        );
        assert!(f.connect(a, b, 0));
        assert!(f.connect(b, c, 0));
        // c back into b would close a loop.
        assert!(!f.connect(c, b, 0));
        // An Input takes nothing in; a step takes one input.
        assert!(!f.connect(b, a, 0));
        assert!(!f.connect(a, c, 1));
        assert!(!f.connect(b, b, 0));
        assert!(f.connect(c, out, 0));
        // An Export ends the branch.
        assert!(!f.connect(out, c, 0));
        // A new wire into a port replaces the old one.
        assert!(f.connect(a, c, 0));
        assert_eq!(f.input_of(c, 0), Some(a));
        assert_eq!(f.wires.iter().filter(|w| w.to == c).count(), 1);
        f.remove(a);
        assert!(f.wires.iter().all(|w| w.from != a && w.to != a));
    }

    #[test]
    fn steps_run_after_the_steps_that_feed_them() {
        let mut f = Flow::empty();
        let out = f.add(
            Step::Export {
                target: Target::Page,
            },
            [0.0; 2],
        );
        let mid = f.add(
            Step::Recolor {
                recolor: Recolor::default(),
                invert: true,
            },
            [0.0; 2],
        );
        let a = f.add(
            Step::Input {
                source: Source::Selection,
            },
            [0.0; 2],
        );
        f.connect(a, mid, 0);
        f.connect(mid, out, 0);
        assert_eq!(f.order(), vec![a, mid, out]);
    }

    #[test]
    fn a_picture_is_inverted_and_blurred_along_the_wires() {
        let mut f = Flow::empty();
        let a = f.add(
            Step::Input {
                source: Source::Selection,
            },
            [0.0; 2],
        );
        let inv = f.add(
            Step::Recolor {
                recolor: Recolor::default(),
                invert: true,
            },
            [0.0; 2],
        );
        let blur = f.add(
            Step::Effect {
                effect: crate::fx::new_effect("gaussian_blur"),
            },
            [0.0; 2],
        );
        let out = f.add(
            Step::Export {
                target: Target::Page,
            },
            [0.0; 2],
        );
        f.connect(a, inv, 0);
        f.connect(inv, blur, 0);
        f.connect(blur, out, 0);
        let r = run(&f, &given(&f, square()), None).expect("not cancelled");
        let Ok(Value::Picture(p)) = &r[&inv] else {
            panic!("a picture");
        };
        assert_eq!(p.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(p.get_pixel(30, 30).0, [35, 235, 235, 255]);
        let Ok(Value::Picture(b)) = &r[&out] else {
            panic!("a picture");
        };
        // The blur softens the square's edge.
        let edge = b.get_pixel(20, 30).0;
        assert!(edge[0] > 0 && edge[0] < 220, "{edge:?}");
    }

    #[test]
    fn a_branch_traces_the_picture_and_recolours_the_curves() {
        let mut f = Flow::empty();
        let a = f.add(
            Step::Input {
                source: Source::Selection,
            },
            [0.0; 2],
        );
        let t = f.add(
            Step::Vectorize {
                preset: Preset::Logo,
                colors: 0,
                remove: Remove::Background,
            },
            [0.0; 2],
        );
        let gray = f.add(
            Step::Recolor {
                recolor: Recolor {
                    saturation: -100.0,
                    ..Recolor::default()
                },
                invert: false,
            },
            [0.0; 2],
        );
        let cmp = f.add(Step::Compare, [0.0; 2]);
        f.connect(a, t, 0);
        f.connect(t, gray, 0);
        // The picture and its grey trace side by side.
        f.connect(a, cmp, 0);
        f.connect(gray, cmp, 1);
        let r = run(&f, &given(&f, square()), None).expect("not cancelled");
        let Ok(Value::Art(art)) = &r[&t] else {
            panic!("curves");
        };
        assert_eq!((art.w, art.h), (60, 60));
        assert!(!art.traced.is_empty());
        let Ok(Value::Art(g)) = &r[&gray] else {
            panic!("curves");
        };
        for c in g.traced.iter().map(|t| t.color.to_rgb8()) {
            assert!(
                c[0].abs_diff(c[1]) <= 2 && c[1].abs_diff(c[2]) <= 2,
                "{c:?}"
            );
        }
        // The Compare step passes its first input on.
        assert!(matches!(&r[&cmp], Ok(Value::Picture(_))));
        // The curves draw back into a picture of their size.
        let p = Value::Art(art.clone()).picture();
        assert_eq!(p.dimensions(), (60, 60));
        let mid = p.get_pixel(30, 30).0;
        assert!(mid[0] > 150 && mid[1] < 90, "{mid:?}");
    }

    #[test]
    fn missing_inputs_and_wires_say_so_down_the_branch() {
        let mut f = Flow::empty();
        let a = f.add(
            Step::Input {
                source: Source::Selection,
            },
            [0.0; 2],
        );
        let e = f.add(
            Step::Effect {
                effect: crate::fx::new_effect("gaussian_blur"),
            },
            [0.0; 2],
        );
        let out = f.add(
            Step::Export {
                target: Target::Page,
            },
            [0.0; 2],
        );
        let lone = f.add(
            Step::Export {
                target: Target::Page,
            },
            [0.0; 2],
        );
        f.connect(a, e, 0);
        f.connect(e, out, 0);
        let r = run(&f, &BTreeMap::new(), None).expect("not cancelled");
        assert_eq!(r[&a], Err("flow.no_input".to_string()));
        assert_eq!(r[&out], Err("flow.no_input".to_string()));
        assert_eq!(r[&lone], Err("flow.not_connected".to_string()));
    }

    #[test]
    fn a_cancelled_run_gives_nothing() {
        let f = Flow::default();
        let cancel = Cancel::new(true);
        assert!(run(&f, &given(&f, square()), Some(&cancel)).is_none());
    }

    #[test]
    fn a_saved_workflow_reads_back_the_same() {
        let mut f = Flow::default();
        let e = f.add(
            Step::Effect {
                effect: crate::fx::new_effect("gaussian_blur"),
            },
            [5.0, 6.0],
        );
        let input = f.input_nodes()[0].id;
        f.connect(input, e, 0);
        if let Some(n) = f.node_mut(input) {
            n.step = Step::Input {
                source: Source::File {
                    path: "a.png".into(),
                },
            };
        }
        let back = Flow::from_json(&f.to_json()).expect("reads");
        assert_eq!(back, f);
        // A newer file is refused; a wire that is not valid is dropped.
        let newer = f
            .to_json()
            .replace("\"tracedraw_flow\": 1", "\"tracedraw_flow\": 99");
        assert_eq!(
            Flow::from_json(&newer),
            Err("flow.newer_version".to_string())
        );
        let mut bad = f.clone();
        bad.wires.push(Wire {
            from: e,
            to: input,
            port: 0,
        });
        assert_eq!(Flow::from_json(&bad.to_json()).expect("reads"), f);
        assert!(Flow::from_json("not json").is_err());
    }

    fn halve(values: &[Value], params: &BTreeMap<String, String>, _: Option<&Cancel>) -> Outcome {
        let n: u8 = params
            .get("flow.test_level")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mut img = values.first().ok_or("none")?.picture().as_ref().clone();
        for px in img.pixels_mut() {
            px[0] = n;
        }
        Ok(Value::Picture(Arc::new(img)))
    }

    static TEST_STEP: Extension = Extension {
        kind: "test.level",
        name_key: "flow.step_effect",
        inputs: 1,
        params: &[("flow.test_level", "7")],
        available: || true,
        unavailable_key: "flow.unknown_step",
        run: halve,
    };

    #[test]
    fn steps_added_by_other_parts_run_with_their_settings() {
        register(&TEST_STEP);
        register(&TEST_STEP);
        assert_eq!(
            extensions()
                .iter()
                .filter(|e| e.kind == "test.level")
                .count(),
            1
        );
        let mut f = Flow::empty();
        let a = f.add(
            Step::Input {
                source: Source::Selection,
            },
            [0.0; 2],
        );
        let x = f.add(Step::extension_step(&TEST_STEP), [0.0; 2]);
        let unknown = f.add(
            Step::Extension {
                kind: "gone".into(),
                params: BTreeMap::new(),
            },
            [0.0; 2],
        );
        assert!(f.connect(a, x, 0));
        assert!(f.connect(a, unknown, 0));
        let r = run(&f, &given(&f, square()), None).expect("not cancelled");
        let Ok(Value::Picture(p)) = &r[&x] else {
            panic!("a picture");
        };
        assert_eq!(p.get_pixel(0, 0).0[0], 7);
        assert_eq!(r[&unknown], Err("flow.unknown_step".to_string()));
        // Saved and read back with its kind and settings.
        let back = Flow::from_json(&f.to_json()).expect("reads");
        assert_eq!(back, f);
    }
}
