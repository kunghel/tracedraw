//! PDF PostScript calculator functions (function type 4, ISO 32000-1,
//! 7.10.5): a program of numbers, booleans, `{ }` blocks and the
//! arithmetic, relational, boolean, conditional and stack operators of
//! that section, run on the inputs pushed onto the stack. Used for
//! shading functions and the tint transforms of Separation and DeviceN
//! colour spaces (#4283: a gradient whose function is such a program
//! came in as one grey).

/// One item of a parsed program.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    Num(f64),
    Bool(bool),
    Op(String),
    Block(Vec<Item>),
}

/// A value on the stack.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Val {
    Num(f64),
    Bool(bool),
}

/// The most items the stack holds (the limit the section gives).
const MAX_STACK: usize = 100;
/// The most operators one run carries out.
const MAX_STEPS: usize = 100_000;

/// The program in `src` (its outer braces included), or `None` when it
/// does not parse.
fn parse(src: &[u8]) -> Option<Vec<Item>> {
    let text = String::from_utf8_lossy(src);
    let mut tokens = Vec::new();
    let mut word = String::new();
    for ch in text.chars() {
        if ch == '{' || ch == '}' || ch.is_whitespace() {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
            if !ch.is_whitespace() {
                tokens.push(ch.to_string());
            }
        } else {
            word.push(ch);
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    let mut it = tokens.into_iter();
    // The program is one block.
    if it.next().as_deref() != Some("{") {
        return None;
    }
    let block = parse_block(&mut it, 0)?;
    Some(block)
}

fn parse_block(it: &mut impl Iterator<Item = String>, depth: usize) -> Option<Vec<Item>> {
    if depth > 64 {
        return None;
    }
    let mut out = Vec::new();
    loop {
        let t = it.next()?;
        match t.as_str() {
            "}" => return Some(out),
            "{" => out.push(Item::Block(parse_block(it, depth + 1)?)),
            "true" => out.push(Item::Bool(true)),
            "false" => out.push(Item::Bool(false)),
            _ => match t.parse::<f64>() {
                Ok(v) => out.push(Item::Num(v)),
                Err(_) => out.push(Item::Op(t)),
            },
        }
    }
}

struct Machine {
    stack: Vec<Val>,
    steps: usize,
}

impl Machine {
    fn push(&mut self, v: Val) -> Option<()> {
        if self.stack.len() >= MAX_STACK {
            return None;
        }
        self.stack.push(v);
        Some(())
    }
    fn num(&mut self) -> Option<f64> {
        match self.stack.pop()? {
            Val::Num(v) => Some(v),
            Val::Bool(_) => None,
        }
    }
    fn int(&mut self) -> Option<i64> {
        let v = self.num()?;
        v.is_finite().then_some(v as i64)
    }
    fn boolean(&mut self) -> Option<bool> {
        match self.stack.pop()? {
            Val::Bool(b) => Some(b),
            Val::Num(_) => None,
        }
    }

    fn run(&mut self, prog: &[Item]) -> Option<()> {
        let mut i = 0;
        while i < prog.len() {
            self.steps += 1;
            if self.steps > MAX_STEPS {
                return None;
            }
            match &prog[i] {
                Item::Num(v) => self.push(Val::Num(*v))?,
                Item::Bool(b) => self.push(Val::Bool(*b))?,
                Item::Block(b) => {
                    // `{..} if` and `{..} {..} ifelse`.
                    match (prog.get(i + 1), prog.get(i + 2)) {
                        (Some(Item::Op(op)), _) if op == "if" => {
                            if self.boolean()? {
                                self.run(b)?;
                            }
                            i += 1;
                        }
                        (Some(Item::Block(e)), Some(Item::Op(op))) if op == "ifelse" => {
                            if self.boolean()? {
                                self.run(b)?;
                            } else {
                                self.run(e)?;
                            }
                            i += 2;
                        }
                        _ => return None,
                    }
                }
                Item::Op(op) => self.op(op)?,
            }
            i += 1;
        }
        Some(())
    }

    fn op(&mut self, op: &str) -> Option<()> {
        let n = |v: f64| Val::Num(v);
        match op {
            "add" | "sub" | "mul" | "div" | "atan" | "exp" => {
                let b = self.num()?;
                let a = self.num()?;
                let v = match op {
                    "add" => a + b,
                    "sub" => a - b,
                    "mul" => a * b,
                    "div" => {
                        if b == 0.0 {
                            return None;
                        }
                        a / b
                    }
                    "atan" => {
                        let d = a.atan2(b).to_degrees();
                        if d < 0.0 {
                            d + 360.0
                        } else {
                            d
                        }
                    }
                    _ => a.powf(b),
                };
                self.push(n(v))
            }
            "idiv" | "mod" => {
                let b = self.int()?;
                let a = self.int()?;
                if b == 0 {
                    return None;
                }
                let v = if op == "idiv" {
                    a.wrapping_div(b)
                } else {
                    a.wrapping_rem(b)
                };
                self.push(n(v as f64))
            }
            "abs" | "neg" | "ceiling" | "floor" | "round" | "truncate" | "sqrt" | "sin"
            | "cos" | "ln" | "log" | "cvi" | "cvr" => {
                let a = self.num()?;
                let v = match op {
                    "abs" => a.abs(),
                    "neg" => -a,
                    "ceiling" => a.ceil(),
                    "floor" => a.floor(),
                    "round" => (a + 0.5).floor(),
                    "truncate" | "cvi" => a.trunc(),
                    "sqrt" => a.max(0.0).sqrt(),
                    "sin" => a.to_radians().sin(),
                    "cos" => a.to_radians().cos(),
                    "ln" => a.ln(),
                    "log" => a.log10(),
                    _ => a,
                };
                self.push(n(v))
            }
            "eq" | "ne" => {
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                let same = a == b;
                self.push(Val::Bool(if op == "eq" { same } else { !same }))
            }
            "gt" | "ge" | "lt" | "le" => {
                let b = self.num()?;
                let a = self.num()?;
                let v = match op {
                    "gt" => a > b,
                    "ge" => a >= b,
                    "lt" => a < b,
                    _ => a <= b,
                };
                self.push(Val::Bool(v))
            }
            "and" | "or" | "xor" => match (self.stack.pop()?, self.stack.pop()?) {
                (Val::Bool(b), Val::Bool(a)) => self.push(Val::Bool(match op {
                    "and" => a && b,
                    "or" => a || b,
                    _ => a ^ b,
                })),
                (Val::Num(b), Val::Num(a)) => {
                    let (a, b) = (a as i64, b as i64);
                    self.push(n(match op {
                        "and" => a & b,
                        "or" => a | b,
                        _ => a ^ b,
                    } as f64))
                }
                _ => None,
            },
            "not" => match self.stack.pop()? {
                Val::Bool(b) => self.push(Val::Bool(!b)),
                Val::Num(a) => self.push(n(!(a as i64) as f64)),
            },
            "bitshift" => {
                let s = self.int()?;
                let a = self.int()?;
                let v = if s >= 0 {
                    a.checked_shl(s.min(63) as u32).unwrap_or(0)
                } else {
                    a >> (-s).min(63)
                };
                self.push(n(v as f64))
            }
            "true" => self.push(Val::Bool(true)),
            "false" => self.push(Val::Bool(false)),
            "pop" => self.stack.pop().map(|_| ()),
            "exch" => {
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.push(b)?;
                self.push(a)
            }
            "dup" => {
                let a = *self.stack.last()?;
                self.push(a)
            }
            "copy" => {
                let k = usize::try_from(self.int()?).ok()?;
                let len = self.stack.len();
                if k > len {
                    return None;
                }
                for j in len - k..len {
                    let v = self.stack[j];
                    self.push(v)?;
                }
                Some(())
            }
            "index" => {
                let k = usize::try_from(self.int()?).ok()?;
                let len = self.stack.len();
                let v = *self.stack.get(len.checked_sub(k + 1)?)?;
                self.push(v)
            }
            "roll" => {
                let j = self.int()?;
                let k = usize::try_from(self.int()?).ok()?;
                let len = self.stack.len();
                if k > len {
                    return None;
                }
                if k > 0 {
                    let part = &mut self.stack[len - k..];
                    let s = j.rem_euclid(k as i64) as usize;
                    part.rotate_right(s);
                }
                Some(())
            }
            _ => None,
        }
    }
}

/// The outputs of the calculator program `src` for `inputs`, each
/// output clipped to `range` (pairs of bounds; as many outputs as pairs,
/// the last ones the stack holds). `None` when the program does not
/// parse or fails.
pub(crate) fn eval(src: &[u8], inputs: &[f64], range: &[f64]) -> Option<Vec<f64>> {
    let prog = parse(src)?;
    let mut m = Machine {
        stack: Vec::new(),
        steps: 0,
    };
    for v in inputs {
        m.push(Val::Num(*v))?;
    }
    m.run(&prog)?;
    let nout = range.len() / 2;
    let vals: Vec<f64> = m
        .stack
        .iter()
        .map(|v| match v {
            Val::Num(x) => *x,
            Val::Bool(b) => f64::from(u8::from(*b)),
        })
        .collect();
    let nout = if nout == 0 { vals.len() } else { nout };
    if vals.len() < nout {
        return None;
    }
    Some(
        vals[vals.len() - nout..]
            .iter()
            .enumerate()
            .map(|(k, v)| match (range.get(k * 2), range.get(k * 2 + 1)) {
                (Some(lo), Some(hi)) if v.is_finite() => v.clamp(*lo, *hi),
                _ if v.is_finite() => *v,
                _ => 0.0,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::eval;

    #[test]
    fn a_tint_transform_gives_its_process_mix() {
        // A yellow spot: tint t to C 0, M 0.1t, Y t, K 0.
        let prog = b"{ dup 0 mul exch dup 0.1 mul exch dup 1 mul exch 0 mul }";
        let r = eval(prog, &[0.5], &[0., 1., 0., 1., 0., 1., 0., 1.]).unwrap();
        assert_eq!(r, vec![0.0, 0.05, 0.5, 0.0]);
    }

    #[test]
    fn conditionals_stack_operators_and_limits() {
        let prog = b"{ dup 0.5 gt { pop 1 } { 2 mul } ifelse 3 1 roll }";
        assert_eq!(eval(prog, &[7.0, 8.0, 0.25], &[]).unwrap(), vec![0.5, 7.0, 8.0]);
        assert_eq!(eval(prog, &[7.0, 8.0, 0.9], &[0., 1.]).unwrap(), vec![1.0]);
        let prog = b"{ 2 copy add 1 index sub exch pop { } if }";
        assert!(eval(prog, &[1.0, 2.0], &[]).is_none());
        assert_eq!(eval(b"{ 10 idiv 7 3 mod }", &[25.0], &[]).unwrap(), vec![2.0, 1.0]);
        assert_eq!(eval(b"{ 3 1 copy 1 bitshift }", &[], &[]).unwrap(), vec![3.0, 6.0]);
        // Damaged programs fail instead of looping or panicking.
        assert!(eval(b"{ 1 0 div }", &[], &[]).is_none());
        assert!(eval(b"{ add", &[1.0], &[]).is_none());
        assert!(eval(b"no braces", &[], &[]).is_none());
        assert!(eval(b"{ 1 0 roll 5 index }", &[], &[]).is_none());
    }
}
