//! The enlarging networks of the Real-ESRGAN project (BSD-3-Clause, see
//! `ATTRIBUTION.md`), run on the CPU with the weights read from the
//! project's own `.pth` files as published (a zip holding a pickled
//! state dict and the raw tensors). Nothing of a model ships with
//! TraceDraw: the file is downloaded when the user asks (see
//! `upscale.rs`, which uses the compact "general x4v3" model).
//!
//! Two layouts are read, both four times larger:
//! - compact: 3x3 convolutions of 64 features, each followed by a leaky
//!   ReLU whose slope is learnt per channel, the last convolution giving
//!   16 sub-pixels per colour, put in place (pixel shuffle) and added to
//!   the picture enlarged pixel by pixel;
//! - dense: a first convolution to 64 features, blocks of three dense
//!   blocks each (five convolutions, every one reading all the features
//!   before it, leaky ReLU 0.2 between, the block's result scaled by 0.2
//!   and added to its input), a convolution over the blocks added to the
//!   first features, two nearest-neighbour doublings each followed by a
//!   convolution and leaky ReLU, and two last convolutions to RGB.
//!
//! Colours go in and out as 0..1. Convolutions are matrix products
//! ([`matrixmultiply::sgemm`]) over planes kept with a one-pixel zero
//! border, one product per kernel tap reading the planes shifted in
//! place, so nothing is copied out. Big pictures go in tiles (with a
//! margin of their neighbours' pixels) on as many threads as the machine
//! has.

use image::RgbaImage;
use std::collections::HashMap;
use std::io::Read;

/// The grown picture is this many times larger on each side.
pub const SCALE: u32 = 4;
/// The model's layout.
const FEAT: usize = 64;
const GROW: usize = 32;
/// A tile's own side and the margin of pixels around it read from its
/// neighbours, input pixels.
const TILE: u32 = 64;
const TILE_PAD: u32 = 10;

/// A 3x3 convolution: weights per tap, `out x in` row-major, and biases.
struct Conv {
    cin: usize,
    cout: usize,
    taps: Vec<Vec<f32>>,
    bias: Vec<f32>,
}

/// The network's weights, read from the model file: a dense block
/// network, or a compact one (convolutions with a learnt leaky slope
/// each, the last giving 16 sub-pixels per colour, added to the picture
/// enlarged pixel by pixel).
pub struct Net {
    kind: Kind,
}

enum Kind {
    Dense(Dense),
    Compact {
        /// Each convolution with its per-channel slope below zero.
        convs: Vec<(Conv, Vec<f32>)>,
        last: Conv,
    },
}

struct Dense {
    first: Conv,
    /// Blocks of three dense blocks of five convolutions.
    body: Vec<[[Conv; 5]; 3]>,
    body_conv: Conv,
    up1: Conv,
    up2: Conv,
    hr: Conv,
    last: Conv,
}

// ----- reading the model file --------------------------------------------------

/// What the pickle of a state dict holds, as far as it is read.
#[derive(Debug, Clone)]
enum Val {
    None,
    Bool,
    Int(i64),
    Float,
    Str(String),
    Tuple(Vec<Val>),
    List(Vec<Val>),
    Dict(Vec<(Val, Val)>),
    Global(String),
    Storage { key: String, float: bool },
    Tensor(TensorRef),
    Mark,
    Other,
}

/// A tensor of the file: its storage's key, offset, shape and strides.
#[derive(Debug, Clone)]
struct TensorRef {
    key: String,
    float: bool,
    offset: usize,
    shape: Vec<usize>,
    stride: Vec<usize>,
}

fn as_usizes(v: &Val) -> Option<Vec<usize>> {
    match v {
        Val::Tuple(items) | Val::List(items) => items
            .iter()
            .map(|i| match i {
                Val::Int(n) if *n >= 0 => Some(*n as usize),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Read the pickle `data` far enough to give back its top object: the
/// opcodes of protocols 2 to 4 a saved state dict uses, the tensors
/// rebuilt as [`TensorRef`]s, every other call as `Other`.
fn unpickle(data: &[u8]) -> Result<Val, String> {
    let mut stack: Vec<Val> = Vec::new();
    let mut memo: HashMap<u32, Val> = HashMap::new();
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Result<&[u8], String> {
        let end = at
            .checked_add(n)
            .filter(|e| *e <= data.len())
            .ok_or("pickle cut short")?;
        let s = &data[*at..end];
        *at = end;
        Ok(s)
    };
    let u32le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let pop = |stack: &mut Vec<Val>| stack.pop().ok_or_else(|| "pickle stack empty".to_string());
    let pop_mark = |stack: &mut Vec<Val>| -> Result<Vec<Val>, String> {
        let m = stack
            .iter()
            .rposition(|v| matches!(v, Val::Mark))
            .ok_or("pickle mark missing")?;
        let items = stack.split_off(m + 1);
        stack.pop();
        Ok(items)
    };
    loop {
        let op = *take(&mut at, 1)?.first().ok_or("pickle cut short")?;
        match op {
            0x80 => {
                take(&mut at, 1)?;
            }
            0x95 => {
                take(&mut at, 8)?;
            }
            b'.' => return pop(&mut stack),
            b'}' => stack.push(Val::Dict(Vec::new())),
            b']' => stack.push(Val::List(Vec::new())),
            b')' => stack.push(Val::Tuple(Vec::new())),
            b'N' => stack.push(Val::None),
            0x88 => stack.push(Val::Bool),
            0x89 => stack.push(Val::Bool),
            b'(' => stack.push(Val::Mark),
            b'K' => {
                let b = take(&mut at, 1)?;
                stack.push(Val::Int(b[0] as i64));
            }
            b'M' => {
                let b = take(&mut at, 2)?;
                stack.push(Val::Int(u16::from_le_bytes([b[0], b[1]]) as i64));
            }
            b'J' => {
                let b = take(&mut at, 4)?;
                stack.push(Val::Int(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64));
            }
            0x8a => {
                let n = take(&mut at, 1)?[0] as usize;
                let b = take(&mut at, n)?;
                let mut v: i64 = 0;
                for (i, byte) in b.iter().enumerate().take(8) {
                    v |= (*byte as i64) << (8 * i);
                }
                if n > 0 && n < 8 && b[n - 1] & 0x80 != 0 {
                    v -= 1i64 << (8 * n);
                }
                stack.push(Val::Int(v));
            }
            b'G' => {
                let b = take(&mut at, 8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                let _ = f64::from_be_bytes(a);
                stack.push(Val::Float);
            }
            b'X' => {
                let n = u32le(take(&mut at, 4)?) as usize;
                let s = String::from_utf8_lossy(take(&mut at, n)?).into_owned();
                stack.push(Val::Str(s));
            }
            0x8c => {
                let n = take(&mut at, 1)?[0] as usize;
                let s = String::from_utf8_lossy(take(&mut at, n)?).into_owned();
                stack.push(Val::Str(s));
            }
            b'c' => {
                let line = |at: &mut usize| -> Result<String, String> {
                    let rest = data.get(*at..).ok_or("pickle cut short")?;
                    let n = rest
                        .iter()
                        .position(|b| *b == b'\n')
                        .ok_or("pickle cut short")?;
                    let s = String::from_utf8_lossy(&rest[..n]).into_owned();
                    *at += n + 1;
                    Ok(s)
                };
                let module = line(&mut at)?;
                let name = line(&mut at)?;
                stack.push(Val::Global(format!("{module} {name}")));
            }
            0x93 => {
                let name = pop(&mut stack)?;
                let module = pop(&mut stack)?;
                match (module, name) {
                    (Val::Str(m), Val::Str(n)) => stack.push(Val::Global(format!("{m} {n}"))),
                    _ => stack.push(Val::Other),
                }
            }
            b'q' => {
                let k = take(&mut at, 1)?[0] as u32;
                memo.insert(k, stack.last().cloned().ok_or("pickle stack empty")?);
            }
            b'r' => {
                let k = u32le(take(&mut at, 4)?);
                memo.insert(k, stack.last().cloned().ok_or("pickle stack empty")?);
            }
            0x94 => {
                let k = memo.len() as u32;
                memo.insert(k, stack.last().cloned().ok_or("pickle stack empty")?);
            }
            b'h' => {
                let k = take(&mut at, 1)?[0] as u32;
                stack.push(memo.get(&k).cloned().ok_or("pickle memo missing")?);
            }
            b'j' => {
                let k = u32le(take(&mut at, 4)?);
                stack.push(memo.get(&k).cloned().ok_or("pickle memo missing")?);
            }
            b't' => {
                let items = pop_mark(&mut stack)?;
                stack.push(Val::Tuple(items));
            }
            0x85..=0x87 => {
                let n = (op - 0x84) as usize;
                if stack.len() < n {
                    return Err("pickle stack empty".into());
                }
                let items = stack.split_off(stack.len() - n);
                stack.push(Val::Tuple(items));
            }
            b'a' => {
                let v = pop(&mut stack)?;
                if let Some(Val::List(l)) = stack.last_mut() {
                    l.push(v);
                }
            }
            b'e' => {
                let items = pop_mark(&mut stack)?;
                if let Some(Val::List(l)) = stack.last_mut() {
                    l.extend(items);
                }
            }
            b's' => {
                let v = pop(&mut stack)?;
                let k = pop(&mut stack)?;
                if let Some(Val::Dict(d)) = stack.last_mut() {
                    d.push((k, v));
                }
            }
            b'u' => {
                let items = pop_mark(&mut stack)?;
                if let Some(Val::Dict(d)) = stack.last_mut() {
                    let mut it = items.into_iter();
                    while let (Some(k), Some(v)) = (it.next(), it.next()) {
                        d.push((k, v));
                    }
                }
            }
            b'b' => {
                // The state set on an object (an ordered dict's metadata):
                // not needed.
                pop(&mut stack)?;
            }
            b'Q' => {
                let pid = pop(&mut stack)?;
                let v = match pid {
                    Val::Tuple(items) if items.len() >= 3 => {
                        let float =
                            matches!(&items[1], Val::Global(g) if g == "torch FloatStorage");
                        match &items[2] {
                            Val::Str(key) => Val::Storage {
                                key: key.clone(),
                                float,
                            },
                            _ => Val::Other,
                        }
                    }
                    _ => Val::Other,
                };
                stack.push(v);
            }
            b'R' => {
                let args = pop(&mut stack)?;
                let func = pop(&mut stack)?;
                let v = match (func, args) {
                    (Val::Global(g), _) if g == "collections OrderedDict" => Val::Dict(Vec::new()),
                    (Val::Global(g), Val::Tuple(a)) if g == "torch._utils _rebuild_tensor_v2" => {
                        match (
                            a.first(),
                            a.get(1),
                            a.get(2).and_then(as_usizes),
                            a.get(3).and_then(as_usizes),
                        ) {
                            (
                                Some(Val::Storage { key, float }),
                                Some(Val::Int(off)),
                                Some(shape),
                                Some(stride),
                            ) if *off >= 0 => Val::Tensor(TensorRef {
                                key: key.clone(),
                                float: *float,
                                offset: *off as usize,
                                shape,
                                stride,
                            }),
                            _ => Val::Other,
                        }
                    }
                    _ => Val::Other,
                };
                stack.push(v);
            }
            _ => return Err(format!("pickle opcode 0x{op:02x} not read")),
        }
    }
}

/// The state dict of the model file `bytes`: each weight's name and
/// tensor (the averaged weights, `params_ema`, where the file has them).
fn state_dict(bytes: &[u8]) -> Result<HashMap<String, (Vec<usize>, Vec<f32>)>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let names: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok().map(|c| c.into_owned()))
        .collect();
    let pkl = names
        .iter()
        .find(|n| n.ends_with("data.pkl"))
        .ok_or("no data.pkl in the model file")?
        .clone();
    let prefix = pkl.trim_end_matches("data.pkl").to_string();
    let mut data = Vec::new();
    zip.by_name(&pkl)
        .map_err(|e| e.to_string())?
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    let top = unpickle(&data)?;
    let Val::Dict(top) = top else {
        return Err("the model file holds no state dict".into());
    };
    let field = |name: &str| {
        top.iter()
            .find(|(k, _)| matches!(k, Val::Str(s) if s == name))
            .map(|(_, v)| v.clone())
    };
    let params = match field("params_ema").or_else(|| field("params")) {
        Some(Val::Dict(d)) => d,
        _ => top.clone(),
    };
    let mut out = HashMap::new();
    let mut storages: HashMap<String, Vec<f32>> = HashMap::new();
    for (k, v) in params {
        let (Val::Str(name), Val::Tensor(t)) = (k, v) else {
            continue;
        };
        if !t.float {
            return Err(format!("{name}: not 32-bit floats"));
        }
        if !storages.contains_key(&t.key) {
            let mut raw = Vec::new();
            zip.by_name(&format!("{prefix}data/{}", t.key))
                .map_err(|e| format!("{name}: {e}"))?
                .read_to_end(&mut raw)
                .map_err(|e| e.to_string())?;
            let floats = raw
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            storages.insert(t.key.clone(), floats);
        }
        let storage = &storages[&t.key];
        // Only dense tensors in their natural order.
        let mut want = 1usize;
        for (d, s) in t.shape.iter().zip(&t.stride).rev() {
            if *d > 1 && *s != want {
                return Err(format!("{name}: strided tensor"));
            }
            want = want.saturating_mul(*d);
        }
        let n: usize = t.shape.iter().product();
        let values = storage
            .get(t.offset..t.offset.saturating_add(n))
            .ok_or_else(|| format!("{name}: storage too short"))?
            .to_vec();
        out.insert(name, (t.shape.clone(), values));
    }
    Ok(out)
}

impl Net {
    /// The network of the model file `bytes`, or what is wrong with it.
    pub fn from_pth(bytes: &[u8]) -> Result<Net, String> {
        let dict = state_dict(bytes)?;
        let conv = |name: &str, cin: usize, cout: usize| -> Result<Conv, String> {
            let (shape, w) = dict
                .get(&format!("{name}.weight"))
                .ok_or_else(|| format!("{name}.weight missing"))?;
            if shape.as_slice() != [cout, cin, 3, 3] {
                return Err(format!("{name}.weight: shape {shape:?}"));
            }
            let (bshape, bias) = dict
                .get(&format!("{name}.bias"))
                .ok_or_else(|| format!("{name}.bias missing"))?;
            if bshape.as_slice() != [cout] {
                return Err(format!("{name}.bias: shape {bshape:?}"));
            }
            let mut taps = vec![vec![0.0f32; cout * cin]; 9];
            for o in 0..cout {
                for i in 0..cin {
                    for t in 0..9 {
                        taps[t][o * cin + i] = w[(o * cin + i) * 9 + t];
                    }
                }
            }
            Ok(Conv {
                cin,
                cout,
                taps,
                bias: bias.clone(),
            })
        };
        if dict.contains_key("body.0.weight") {
            // The compact network: body.0 conv, body.1 slopes, ... and
            // the last convolution to 48 channels.
            let mut convs = Vec::new();
            let mut i = 0;
            loop {
                let w = dict
                    .get(&format!("body.{i}.weight"))
                    .ok_or("compact model cut short")?;
                let slope = dict.get(&format!("body.{}.weight", i + 1));
                match slope {
                    Some((shape, a)) if shape.len() == 1 => {
                        let cin = if i == 0 { 3 } else { FEAT };
                        if w.0.as_slice() != [FEAT, cin, 3, 3] || a.len() != FEAT {
                            return Err(format!("body.{i}: shape {:?}", w.0));
                        }
                        convs.push((conv(&format!("body.{i}"), cin, FEAT)?, a.clone()));
                        i += 2;
                    }
                    _ => {
                        let last = conv(&format!("body.{i}"), FEAT, 3 * (SCALE * SCALE) as usize)?;
                        if convs.is_empty() {
                            return Err("compact model without layers".into());
                        }
                        return Ok(Net {
                            kind: Kind::Compact { convs, last },
                        });
                    }
                }
            }
        }
        let mut blocks = 0;
        while dict.contains_key(&format!("body.{blocks}.rdb1.conv1.weight")) {
            blocks += 1;
        }
        if blocks == 0 || blocks > 64 {
            return Err("no blocks in the model".into());
        }
        let mut body = Vec::with_capacity(blocks);
        for b in 0..blocks {
            let rdb = |r: usize| -> Result<[Conv; 5], String> {
                let p = format!("body.{b}.rdb{r}");
                Ok([
                    conv(&format!("{p}.conv1"), FEAT, GROW)?,
                    conv(&format!("{p}.conv2"), FEAT + GROW, GROW)?,
                    conv(&format!("{p}.conv3"), FEAT + 2 * GROW, GROW)?,
                    conv(&format!("{p}.conv4"), FEAT + 3 * GROW, GROW)?,
                    conv(&format!("{p}.conv5"), FEAT + 4 * GROW, FEAT)?,
                ])
            };
            body.push([rdb(1)?, rdb(2)?, rdb(3)?]);
        }
        Ok(Net {
            kind: Kind::Dense(Dense {
                first: conv("conv_first", 3, FEAT)?,
                body,
                body_conv: conv("conv_body", FEAT, FEAT)?,
                up1: conv("conv_up1", FEAT, FEAT)?,
                up2: conv("conv_up2", FEAT, FEAT)?,
                hr: conv("conv_hr", FEAT, FEAT)?,
                last: conv("conv_last", FEAT, 3)?,
            }),
        })
    }
}

// ----- running it ----------------------------------------------------------------

/// Feature planes `w x h` with a one-pixel zero border and one spare
/// value before and after, so the shifted reads of a 3x3 kernel stay
/// inside the buffer.
struct Planes {
    w: usize,
    h: usize,
    ch: usize,
    data: Vec<f32>,
}

impl Planes {
    fn new(w: usize, h: usize, ch: usize) -> Planes {
        let plane = (w + 2) * (h + 2);
        Planes {
            w,
            h,
            ch,
            data: vec![0.0; ch * plane + 2],
        }
    }
    fn plane(&self) -> usize {
        (self.w + 2) * (self.h + 2)
    }
    /// Index of pixel (x, y) of channel c.
    fn at(&self, c: usize, x: usize, y: usize) -> usize {
        1 + c * self.plane() + (y + 1) * (self.w + 2) + x + 1
    }
}

fn lrelu(v: f32) -> f32 {
    if v >= 0.0 {
        v
    } else {
        0.2 * v
    }
}

/// `conv` over the first `conv.cin` channels of `src`, into channels
/// `at..at + conv.cout` of `dst` (the same size), leaky ReLU after when
/// `relu`.
fn conv(src: &Planes, conv: &Conv, dst: &mut Planes, at: usize, relu: bool) {
    if src.w != dst.w || src.h != dst.h || conv.cin > src.ch || at + conv.cout > dst.ch {
        return;
    }
    // SAFETY: `src` and `dst` are distinct buffers of the same layout,
    // checked above to hold the channels read and written.
    unsafe {
        conv_raw(
            src.w,
            src.h,
            src.data.as_ptr(),
            dst.data.as_mut_ptr(),
            conv,
            at,
            relu,
        )
    }
}

/// [`conv`] within one set of planes: it reads channels `0..conv.cin`
/// and writes `at..at + conv.cout`, which must not overlap them.
fn conv_within(p: &mut Planes, conv: &Conv, at: usize, relu: bool) {
    if conv.cin > at || at + conv.cout > p.ch {
        return;
    }
    let ptr = p.data.as_mut_ptr();
    // SAFETY: the channels read (0..cin, plus one spare value before)
    // end before the first channel written (at >= cin); the values read
    // past the last read channel lie in the plane border of channel cin,
    // which is only written with zeros by the clean-up after all taps.
    unsafe { conv_raw(p.w, p.h, ptr, ptr, conv, at, relu) }
}

/// The convolution on raw planes `w x h` (see [`Planes`]).
///
/// # Safety
/// `src` must hold `conv.cin` planes and `dst` `at + conv.cout`, in the
/// [`Planes`] layout, and the channels written must not be read.
unsafe fn conv_raw(
    w: usize,
    h: usize,
    src: *const f32,
    dst: *mut f32,
    conv: &Conv,
    at: usize,
    relu: bool,
) {
    let row = w + 2;
    let plane = row * (h + 2);
    // The rows 1..=h of the padded grid, borders included: the border
    // columns are computed too and cleared after.
    let n = h * row;
    let base = 1 + row;
    let out = 1 + at * plane + row;
    for (t, a) in conv.taps.iter().enumerate() {
        let dy = t as isize / 3 - 1;
        let dx = t as isize % 3 - 1;
        let from = (base as isize + dy * row as isize + dx) as usize;
        matrixmultiply::sgemm(
            conv.cout,
            conv.cin,
            n,
            1.0,
            a.as_ptr(),
            conv.cin as isize,
            1,
            src.add(from),
            plane as isize,
            1,
            if t == 0 { 0.0 } else { 1.0 },
            dst.add(out),
            plane as isize,
            1,
        );
    }
    for o in 0..conv.cout {
        let b = conv.bias[o];
        let ch = std::slice::from_raw_parts_mut(dst.add(out + o * plane), n);
        for (i, v) in ch.iter_mut().enumerate() {
            let col = i % row;
            *v = if col == 0 || col == row - 1 {
                0.0
            } else if relu {
                lrelu(*v + b)
            } else {
                *v + b
            };
        }
    }
}

/// The planes twice as large, each pixel repeated.
fn double(src: &Planes) -> Planes {
    let mut dst = Planes::new(src.w * 2, src.h * 2, src.ch);
    for c in 0..src.ch {
        for y in 0..dst.h {
            for x in 0..dst.w {
                let v = src.data[src.at(c, x / 2, y / 2)];
                let i = dst.at(c, x, y);
                dst.data[i] = v;
            }
        }
    }
    dst
}

impl Net {
    /// The RGB planes `rgb` (0..1, `w x h`) four times larger.
    fn run(&self, rgb: &Planes, cancel: &dyn Fn() -> bool) -> Option<Planes> {
        match &self.kind {
            Kind::Dense(d) => d.run(rgb, cancel),
            Kind::Compact { convs, last } => compact(convs, last, rgb, cancel),
        }
    }
}

/// The compact network on `rgb`.
fn compact(
    convs: &[(Conv, Vec<f32>)],
    last: &Conv,
    rgb: &Planes,
    cancel: &dyn Fn() -> bool,
) -> Option<Planes> {
    let (w, h) = (rgb.w, rgb.h);
    let mut a = Planes::new(w, h, FEAT);
    let mut b = Planes::new(w, h, FEAT);
    for (i, (c, slope)) in convs.iter().enumerate() {
        if cancel() {
            return None;
        }
        let src = if i == 0 { rgb } else { &a };
        conv(src, c, &mut b, 0, false);
        let plane = b.plane();
        for (ch, s) in slope.iter().enumerate() {
            let start = 1 + ch * plane;
            for v in &mut b.data[start..start + plane] {
                if *v < 0.0 {
                    *v *= s;
                }
            }
        }
        std::mem::swap(&mut a, &mut b);
    }
    let n = (SCALE * SCALE) as usize;
    let mut sub = Planes::new(w, h, 3 * n);
    conv(&a, last, &mut sub, 0, false);
    let s = SCALE as usize;
    let mut out = Planes::new(w * s, h * s, 3);
    for c in 0..3 {
        for y in 0..h {
            for x in 0..w {
                let base = rgb.data[rgb.at(c, x, y)];
                for i in 0..s {
                    for j in 0..s {
                        let v = sub.data[sub.at(c * n + i * s + j, x, y)];
                        let o = out.at(c, x * s + j, y * s + i);
                        out.data[o] = v + base;
                    }
                }
            }
        }
    }
    Some(out)
}

impl Dense {
    /// The RGB planes `rgb` (0..1, `w x h`) four times larger.
    fn run(&self, rgb: &Planes, cancel: &dyn Fn() -> bool) -> Option<Planes> {
        let (w, h) = (rgb.w, rgb.h);
        let mut feat = Planes::new(w, h, FEAT);
        conv(rgb, &self.first, &mut feat, 0, false);
        // The dense block's features: its input then each convolution's.
        let mut dense = Planes::new(w, h, FEAT + 4 * GROW);
        let mut five = Planes::new(w, h, FEAT);
        let mut block_in = Planes::new(w, h, FEAT);
        let plane = feat.plane();
        let feat_len = FEAT * plane;
        dense.data[1..1 + feat_len].copy_from_slice(&feat.data[1..1 + feat_len]);
        for block in &self.body {
            if cancel() {
                return None;
            }
            block_in.data[1..1 + feat_len].copy_from_slice(&dense.data[1..1 + feat_len]);
            for rdb in block {
                for (k, c) in rdb.iter().enumerate().take(4) {
                    conv_within(&mut dense, c, FEAT + k * GROW, true);
                }
                conv(&dense, &rdb[4], &mut five, 0, false);
                for i in 1..1 + feat_len {
                    dense.data[i] += 0.2 * five.data[i];
                }
            }
            for i in 1..1 + feat_len {
                dense.data[i] = block_in.data[i] + 0.2 * dense.data[i];
            }
        }
        // The blocks' convolution, added to the first features.
        let mut body = Planes::new(w, h, FEAT);
        body.data[1..1 + feat_len].copy_from_slice(&dense.data[1..1 + feat_len]);
        drop(dense);
        conv(&body, &self.body_conv, &mut five, 0, false);
        for i in 1..1 + feat_len {
            feat.data[i] += five.data[i];
        }
        drop((body, five, block_in));
        if cancel() {
            return None;
        }
        let big = double(&feat);
        let mut up = Planes::new(big.w, big.h, FEAT);
        conv(&big, &self.up1, &mut up, 0, true);
        let big = double(&up);
        let mut up = Planes::new(big.w, big.h, FEAT);
        conv(&big, &self.up2, &mut up, 0, true);
        let mut hr = Planes::new(big.w, big.h, FEAT);
        conv(&up, &self.hr, &mut hr, 0, true);
        drop(up);
        let mut out = Planes::new(big.w, big.h, 3);
        conv(&hr, &self.last, &mut out, 0, false);
        Some(out)
    }
}

impl Net {
    /// `img` four times larger on each side; `None` once `cancel` says
    /// so. `progress` hears the share done (0..1) after each tile.
    /// Transparency is enlarged smoothly, apart from the network.
    pub fn upscale(
        &self,
        img: &RgbaImage,
        cancel: &(dyn Fn() -> bool + Sync),
        progress: &(dyn Fn(f32) + Sync),
    ) -> Option<RgbaImage> {
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            return None;
        }
        let (bw, bh) = (w * SCALE, h * SCALE);
        let mut out = RgbaImage::new(bw, bh);
        // The tiles, in rows.
        let mut tiles = Vec::new();
        let mut y = 0;
        while y < h {
            let mut x = 0;
            while x < w {
                tiles.push((x, y, TILE.min(w - x), TILE.min(h - y)));
                x += TILE;
            }
            y += TILE;
        }
        let total = tiles.len();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let done = std::sync::atomic::AtomicUsize::new(0);
        let results = std::sync::Mutex::new(Vec::with_capacity(total));
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .clamp(1, 8)
            .min(total);
        let work = || loop {
            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some(&(tx, ty, tw, th)) = tiles.get(i) else {
                break;
            };
            if cancel() {
                break;
            }
            // The tile with its margin, clipped to the picture.
            let x0 = tx.saturating_sub(TILE_PAD);
            let y0 = ty.saturating_sub(TILE_PAD);
            let x1 = (tx + tw + TILE_PAD).min(w);
            let y1 = (ty + th + TILE_PAD).min(h);
            let (pw, ph) = ((x1 - x0) as usize, (y1 - y0) as usize);
            let mut rgb = Planes::new(pw, ph, 3);
            for yy in 0..ph {
                for xx in 0..pw {
                    let p = img.get_pixel(x0 + xx as u32, y0 + yy as u32).0;
                    for c in 0..3 {
                        let i = rgb.at(c, xx, yy);
                        rgb.data[i] = p[c] as f32 / 255.0;
                    }
                }
            }
            let Some(big) = self.run(&rgb, cancel) else {
                break;
            };
            // The tile's own part of the result.
            let ox = ((tx - x0) * SCALE) as usize;
            let oy = ((ty - y0) * SCALE) as usize;
            let mut px = Vec::with_capacity((tw * th * SCALE * SCALE) as usize * 3);
            for yy in 0..(th * SCALE) as usize {
                for xx in 0..(tw * SCALE) as usize {
                    for c in 0..3 {
                        let v = big.data[big.at(c, ox + xx, oy + yy)];
                        px.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
                    }
                }
            }
            if let Ok(mut r) = results.lock() {
                r.push((tx, ty, tw, th, px));
            }
            let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            progress(d as f32 / total as f32);
        };
        std::thread::scope(|s| {
            for _ in 1..threads {
                s.spawn(work);
            }
            work();
        });
        if cancel() {
            return None;
        }
        let results = results.into_inner().ok()?;
        if results.len() != total {
            return None;
        }
        // Transparency, smoothly enlarged (opaque pictures stay opaque).
        let opaque = img.pixels().all(|p| p.0[3] == 255);
        let alpha = (!opaque).then(|| {
            let a = image::GrayImage::from_fn(w, h, |x, y| image::Luma([img.get_pixel(x, y).0[3]]));
            image::imageops::resize(&a, bw, bh, image::imageops::FilterType::CatmullRom)
        });
        for (tx, ty, tw, th, px) in results {
            let (sw, sh) = (tw * SCALE, th * SCALE);
            for yy in 0..sh {
                for xx in 0..sw {
                    let i = ((yy * sw + xx) * 3) as usize;
                    let (x, y) = (tx * SCALE + xx, ty * SCALE + yy);
                    let a = alpha.as_ref().map_or(255, |a| a.get_pixel(x, y).0[0]);
                    out.put_pixel(x, y, image::Rgba([px[i], px[i + 1], px[i + 2], a]));
                }
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pickle reader reads a small state dict as a saved one is
    /// written: an ordered dict of rebuilt tensors.
    #[test]
    fn a_pickled_state_dict_is_read() {
        let mut p = vec![0x80, 2, b'}', b'q', 0];
        let s = |p: &mut Vec<u8>, t: &str| {
            p.push(b'X');
            p.extend((t.len() as u32).to_le_bytes());
            p.extend(t.as_bytes());
        };
        s(&mut p, "params_ema");
        p.extend(b"ccollections\nOrderedDict\n)R(");
        s(&mut p, "a.weight");
        p.extend(b"ctorch._utils\n_rebuild_tensor_v2\n((");
        s(&mut p, "storage");
        p.extend(b"ctorch\nFloatStorage\n");
        s(&mut p, "123");
        s(&mut p, "cpu");
        p.extend([
            b'K', 4, b't', b'Q', b'K', 0, b'K', 2, b'K', 2, 0x86, b'K', 2, b'K', 1, 0x86, 0x89,
        ]);
        p.extend(b"ccollections\nOrderedDict\n)Rtq\x01R");
        p.extend(b"u");
        p.extend(b"s.");
        let v = unpickle(&p).unwrap();
        let Val::Dict(top) = v else { panic!("{v:?}") };
        let Val::Dict(params) = &top[0].1 else {
            panic!()
        };
        let Val::Tensor(t) = &params[0].1 else {
            panic!("{params:?}")
        };
        assert_eq!(t.key, "123");
        assert_eq!(t.shape, vec![2, 2]);
        assert_eq!(t.stride, vec![2, 1]);
        assert!(t.float);
    }

    /// Broken files give an error, never a panic.
    #[test]
    fn broken_model_files_are_refused() {
        assert!(Net::from_pth(b"").is_err());
        assert!(Net::from_pth(b"PK\x03\x04garbage").is_err());
        for n in 0..40 {
            let cut: Vec<u8> = b"\x80\x02}q\x00X\x0a\x00\x00\x00params_emaq\x01"
                .iter()
                .take(n)
                .copied()
                .collect();
            assert!(unpickle(&cut).is_err());
        }
    }

    /// A convolution with a centre tap of one and the others zero copies
    /// its input; one that sums the 3x3 neighbourhood sees the zero
    /// border at the edge.
    #[test]
    fn a_convolution_reads_its_neighbours_and_the_zero_border() {
        let mut src = Planes::new(3, 2, 1);
        for y in 0..2 {
            for x in 0..3 {
                let i = src.at(0, x, y);
                src.data[i] = (1 + x + 3 * y) as f32;
            }
        }
        let mut taps = vec![vec![0.0]; 9];
        taps[4] = vec![1.0];
        let id = Conv {
            cin: 1,
            cout: 1,
            taps,
            bias: vec![0.5],
        };
        let mut dst = Planes::new(3, 2, 1);
        conv(&src, &id, &mut dst, 0, false);
        assert_eq!(dst.data[dst.at(0, 2, 1)], 6.5);
        let sum = Conv {
            cin: 1,
            cout: 1,
            taps: vec![vec![1.0]; 9],
            bias: vec![0.0],
        };
        conv(&src, &sum, &mut dst, 0, false);
        // Corner (0,0): 1+2+4+5.
        assert_eq!(dst.data[dst.at(0, 0, 0)], 12.0);
        // (1,0): 1+2+3+4+5+6.
        assert_eq!(dst.data[dst.at(0, 1, 0)], 21.0);
        // The border stays zero.
        assert!(dst.data[..dst.at(0, 0, 0)].iter().all(|v| *v == 0.0));
    }

    /// A model file written as a saved state dict is (zip, pickle, raw
    /// tensors) is read and run end to end: all weights zero but the
    /// last bias, so every pixel of the four times larger picture is that
    /// bias's grey, transparency kept.
    #[test]
    fn a_model_file_is_read_and_run() {
        use std::io::Write;
        let mut names: Vec<(String, Vec<usize>)> = Vec::new();
        let mut conv = |n: &str, cin: usize, cout: usize| {
            names.push((format!("{n}.weight"), vec![cout, cin, 3, 3]));
            names.push((format!("{n}.bias"), vec![cout]));
        };
        conv("conv_first", 3, FEAT);
        for b in 0..6 {
            for r in 1..=3 {
                for k in 0..5 {
                    let cout = if k == 4 { FEAT } else { GROW };
                    conv(
                        &format!("body.{b}.rdb{r}.conv{}", k + 1),
                        FEAT + k * GROW,
                        cout,
                    );
                }
            }
        }
        for n in ["conv_body", "conv_up1", "conv_up2", "conv_hr"] {
            conv(n, FEAT, FEAT);
        }
        conv("conv_last", FEAT, 3);
        let mut p: Vec<u8> = vec![0x80, 2, b'}'];
        let s = |p: &mut Vec<u8>, t: &str| {
            p.push(b'X');
            p.extend((t.len() as u32).to_le_bytes());
            p.extend(t.as_bytes());
        };
        let int = |p: &mut Vec<u8>, v: usize| {
            p.push(b'J');
            p.extend((v as i32).to_le_bytes());
        };
        s(&mut p, "params_ema");
        p.extend(b"ccollections\nOrderedDict\n)R(");
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (i, (name, shape)) in names.iter().enumerate() {
            let n: usize = shape.iter().product();
            s(&mut p, name);
            p.extend(b"ctorch._utils\n_rebuild_tensor_v2\n((");
            s(&mut p, "storage");
            p.extend(b"ctorch\nFloatStorage\n");
            s(&mut p, &i.to_string());
            s(&mut p, "cpu");
            int(&mut p, n);
            p.extend(b"tQ");
            int(&mut p, 0);
            p.push(b'(');
            for d in shape {
                int(&mut p, *d);
            }
            p.extend(b"t(");
            let mut stride = vec![1usize; shape.len()];
            for k in (0..shape.len().saturating_sub(1)).rev() {
                stride[k] = stride[k + 1] * shape[k + 1];
            }
            for d in &stride {
                int(&mut p, *d);
            }
            p.extend(b"t\x89ccollections\nOrderedDict\n)RtR");
            let value = if name == "conv_last.bias" {
                0.5f32
            } else {
                0.0
            };
            let bytes: Vec<u8> = (0..n).flat_map(|_| value.to_le_bytes()).collect();
            zip.start_file(format!("archive/data/{i}"), opts).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        p.extend(b"us.");
        zip.start_file("archive/data.pkl", opts).unwrap();
        zip.write_all(&p).unwrap();
        let file = zip.finish().unwrap().into_inner();
        let net = Net::from_pth(&file).unwrap();
        let mut img = RgbaImage::from_pixel(5, 3, image::Rgba([10, 200, 30, 255]));
        img.put_pixel(0, 0, image::Rgba([0, 0, 0, 0]));
        let seen = std::sync::Mutex::new(Vec::new());
        let big = net
            .upscale(&img, &|| false, &|p| seen.lock().unwrap().push(p))
            .unwrap();
        assert_eq!(big.dimensions(), (20, 12));
        assert!(big.pixels().all(|p| p.0[..3] == [128, 128, 128]));
        assert_eq!(big.get_pixel(19, 11).0[3], 255);
        assert!(big.get_pixel(0, 0).0[3] < 128);
        assert_eq!(seen.lock().unwrap().last().copied(), Some(1.0));
        // Cancelled at once: nothing.
        assert!(net.upscale(&img, &|| true, &|_| {}).is_none());
        // A model of another layout is refused.
        assert!(Net::from_pth(&file[..file.len() / 2]).is_err());
    }

    /// A compact model's last convolution gives 16 sub-pixels per colour:
    /// sub-pixel (i, j) of colour c is its channel c*16 + i*4 + j, put at
    /// row i, column j of the pixel's 4x4 square, added to the pixel.
    #[test]
    fn a_compact_model_puts_its_sub_pixels_in_place() {
        let zero = |cin: usize, cout: usize| Conv {
            cin,
            cout,
            taps: vec![vec![0.0; cin * cout]; 9],
            bias: vec![0.0; cout],
        };
        let mut last = zero(FEAT, 48);
        for (k, b) in last.bias.iter_mut().enumerate() {
            *b = k as f32 / 255.0;
        }
        let net = Net {
            kind: Kind::Compact {
                convs: vec![(zero(3, FEAT), vec![0.1; FEAT])],
                last,
            },
        };
        let img = RgbaImage::from_pixel(2, 1, image::Rgba([100, 50, 20, 255]));
        let big = net.upscale(&img, &|| false, &|_| {}).unwrap();
        assert_eq!(big.dimensions(), (8, 4));
        for (x, y) in [(0, 0), (1, 2), (3, 1), (6, 3)] {
            let (i, j) = ((y % 4) as u8, (x % 4) as u8);
            let p = big.get_pixel(x, y).0;
            assert_eq!(p[0], 100 + i * 4 + j, "{x},{y}");
            assert_eq!(p[1], 50 + 16 + i * 4 + j, "{x},{y}");
            assert_eq!(p[2], 20 + 32 + i * 4 + j, "{x},{y}");
        }
    }
}
