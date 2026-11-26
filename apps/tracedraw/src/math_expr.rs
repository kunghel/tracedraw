//! Math expressions for the Math Shapes panel: a small parser and
//! evaluator for formulas such as `2x^2 - 3x + 1`, `sin(3t)`,
//! `x^2 + y^2 = 9` or `if(x < 0, -x, sqrt(x))`.
//!
//! The text is compiled once into a list of stack operations (postfix
//! order) and evaluated many times with different variable values, so
//! that drawing a curve with thousands of samples stays fast.
//!
//! Syntax:
//! - numbers `3`, `2.5`, `1e-3`; a comma between digits outside a
//!   function's brackets is a decimal comma (`2,5x`);
//! - operators `+ - * / ^` (also `× · ÷ −`), `%` as modulo, comparisons
//!   `< > <= >= == !=` giving 1 or 0, `²` and `³`, `√x`, `|x|`;
//! - implicit multiplication: `2x`, `3sin(x)`, `(x+1)(x-1)`, `ab`;
//! - variables `x y t` (`θ`, `theta` and `t` are the same angle), the
//!   parameters `a b c d k`, constants `pi π e tau τ phi φ` (the golden
//!   ratio);
//! - functions `sin cos tan cot sec csc asin acos atan sinh cosh tanh
//!   asinh acosh atanh sqrt cbrt abs exp ln log log2 floor ceil round
//!   sign frac fact`, two arguments `atan2 min max mod pow log(b, x)
//!   hypot root(x, n)` (min and max take any number), `if(c, a, b)`;
//! - an equation `lhs = rhs` stands for `lhs - (rhs)`; a leading `y =`,
//!   `f(x) =`, `r =` or `r(θ) =` is dropped.
//!
//! Errors are values ([`ExprError`]), never panics.

/// Variable slots of [`Vars`].
pub const X: usize = 0;
pub const Y: usize = 1;
pub const T: usize = 2;
pub const A: usize = 3;
pub const B: usize = 4;
pub const C: usize = 5;
pub const D: usize = 6;
pub const K: usize = 7;
pub const SLOTS: usize = 8;

/// The values of the variables while evaluating.
pub type Vars = [f64; SLOTS];

/// Why a formula could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprError {
    /// Nothing typed.
    Empty,
    /// A name that is neither a variable, a constant nor a function.
    Unknown(String),
    /// Something unexpected at this character (0-based, in characters).
    Syntax(usize),
    /// A bracket opened and never closed, or closed and never opened.
    Brackets,
    /// A function given the wrong number of arguments.
    Args(String),
    /// Too deep or too long.
    TooLong,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum F1 {
    Sin,
    Cos,
    Tan,
    Cot,
    Sec,
    Csc,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Asinh,
    Acosh,
    Atanh,
    Sqrt,
    Cbrt,
    Abs,
    Exp,
    Ln,
    Log10,
    Log2,
    Floor,
    Ceil,
    Round,
    Sign,
    Frac,
    Fact,
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum F2 {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Mod,
    Atan2,
    Min,
    Max,
    LogB,
    Hypot,
    Root,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Num(f64),
    Var(usize),
    F1(F1),
    F2(F2),
    If,
}

/// A compiled formula.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    ops: Vec<Op>,
    /// Which variables it reads.
    uses: [bool; SLOTS],
}

/// The longest formula read, in characters.
const MAX_LEN: usize = 2000;
/// How deeply brackets may nest.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Op(char),
    /// `<=`, `>=`, `==`, `!=`.
    Cmp2(char),
    LParen,
    RParen,
    Comma,
    Bar,
}

fn is_name_char(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, ExprError> {
    let chars: Vec<char> = src.chars().collect();
    if chars.len() > MAX_LEN {
        return Err(ExprError::TooLong);
    }
    let mut out: Vec<(Tok, usize)> = Vec::new();
    // For each open bracket: whether it holds a function's arguments.
    let mut calls: Vec<bool> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let at = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let mut s = String::new();
            while i < chars.len() {
                let d = chars[i];
                let decimal_comma = d == ','
                    && !calls.iter().any(|&f| f)
                    && !s.contains('.')
                    && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
                if d.is_ascii_digit() || d == '.' {
                    s.push(d);
                } else if decimal_comma {
                    s.push('.');
                } else if (d == 'e' || d == 'E')
                    && (chars.get(i + 1).is_some_and(|n| n.is_ascii_digit())
                        || (matches!(chars.get(i + 1), Some('+') | Some('-'))
                            && chars.get(i + 2).is_some_and(|n| n.is_ascii_digit())))
                {
                    s.push('e');
                    i += 1;
                    if matches!(chars[i], '+' | '-') {
                        s.push(chars[i]);
                        i += 1;
                    }
                    continue;
                } else {
                    break;
                }
                i += 1;
            }
            let v: f64 = s.parse().map_err(|_| ExprError::Syntax(at))?;
            out.push((Tok::Num(v), at));
            continue;
        }
        if is_name_char(c) {
            let mut s = String::new();
            while i < chars.len() && (is_name_char(chars[i]) || chars[i].is_ascii_digit()) {
                s.push(chars[i]);
                i += 1;
            }
            out.push((Tok::Name(s), at));
            continue;
        }
        i += 1;
        let next = chars.get(i).copied();
        let tok = match c {
            '+' => Tok::Op('+'),
            '-' | '−' | '–' => Tok::Op('-'),
            '*' | '×' | '·' | '\u{22C5}' => Tok::Op('*'),
            '/' | '÷' => Tok::Op('/'),
            '^' => Tok::Op('^'),
            '%' => Tok::Op('%'),
            '²' => {
                out.push((Tok::Op('^'), at));
                Tok::Num(2.0)
            }
            '³' => {
                out.push((Tok::Op('^'), at));
                Tok::Num(3.0)
            }
            '√' => Tok::Name("sqrt".into()),
            '\u{221B}' => Tok::Name("cbrt".into()),
            '≤' => Tok::Cmp2('<'),
            '≥' => Tok::Cmp2('>'),
            '≠' => Tok::Cmp2('!'),
            '<' | '>' | '=' | '!' if next == Some('=') => {
                i += 1;
                Tok::Cmp2(c)
            }
            '<' => Tok::Op('<'),
            '>' => Tok::Op('>'),
            '=' => Tok::Op('='),
            '!' => Tok::Op('!'),
            '(' | '[' | '{' => {
                let call = matches!(out.last(), Some((Tok::Name(n), _)) if function_arity(n).is_some());
                calls.push(call);
                if calls.len() > MAX_DEPTH {
                    return Err(ExprError::TooLong);
                }
                Tok::LParen
            }
            ')' | ']' | '}' => {
                calls.pop();
                Tok::RParen
            }
            ',' | ';' => Tok::Comma,
            '|' => Tok::Bar,
            _ => return Err(ExprError::Syntax(at)),
        };
        out.push((tok, at));
    }
    Ok(out)
}

/// The constants and variables by name.
fn atom_name(n: &str) -> Option<Op> {
    Some(match n {
        "x" => Op::Var(X),
        "y" => Op::Var(Y),
        "t" | "theta" | "θ" | "u" => Op::Var(T),
        "a" => Op::Var(A),
        "b" => Op::Var(B),
        "c" => Op::Var(C),
        "d" => Op::Var(D),
        "k" => Op::Var(K),
        "pi" | "π" => Op::Num(std::f64::consts::PI),
        "e" => Op::Num(std::f64::consts::E),
        "tau" | "τ" => Op::Num(std::f64::consts::TAU),
        "phi" | "φ" => Op::Num(1.618_033_988_749_895),
        _ => return None,
    })
}

/// How many arguments a function takes: (least, most).
fn function_arity(n: &str) -> Option<(usize, usize)> {
    Some(match n {
        "atan2" | "mod" | "pow" | "hypot" | "root" => (2, 2),
        "min" | "max" => (1, 32),
        "log" => (1, 2),
        "if" => (3, 3),
        _ => {
            f1_of(n)?;
            (1, 1)
        }
    })
}

fn f1_of(n: &str) -> Option<F1> {
    Some(match n {
        "sin" | "sen" => F1::Sin,
        "cos" => F1::Cos,
        "tan" | "tg" => F1::Tan,
        "cot" | "cotg" => F1::Cot,
        "sec" => F1::Sec,
        "csc" | "cosec" => F1::Csc,
        "asin" | "arcsin" | "arcsen" => F1::Asin,
        "acos" | "arccos" => F1::Acos,
        "atan" | "arctan" | "arctg" => F1::Atan,
        "sinh" | "senh" => F1::Sinh,
        "cosh" => F1::Cosh,
        "tanh" | "tgh" => F1::Tanh,
        "asinh" => F1::Asinh,
        "acosh" => F1::Acosh,
        "atanh" => F1::Atanh,
        "sqrt" | "raiz" => F1::Sqrt,
        "cbrt" => F1::Cbrt,
        "abs" => F1::Abs,
        "exp" => F1::Exp,
        "ln" => F1::Ln,
        "log10" => F1::Log10,
        "log2" => F1::Log2,
        "floor" => F1::Floor,
        "ceil" => F1::Ceil,
        "round" => F1::Round,
        "sign" | "sgn" => F1::Sign,
        "frac" => F1::Frac,
        "fact" => F1::Fact,
        _ => return None,
    })
}

/// Split a run of letters into known names, longest first (`sinx` is
/// `sin x`, `ab` is `a b`, `2pix` is `2 pi x`). None when a part is
/// unknown.
fn split_name(s: &str) -> Option<Vec<String>> {
    if atom_name(s).is_some() || function_arity(s).is_some() {
        return Some(vec![s.to_string()]);
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let mut found = None;
        for j in (i + 1..=chars.len()).rev() {
            let part: String = chars[i..j].iter().collect();
            if atom_name(&part).is_some() || function_arity(&part).is_some() {
                found = Some((part, j));
                break;
            }
        }
        let (part, j) = found?;
        out.push(part);
        i = j;
    }
    Some(out)
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    ops: Vec<Op>,
    depth: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }

    fn at(&self) -> usize {
        self.toks.get(self.pos).map_or(self.end, |(_, a)| *a)
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|(t, _)| t.clone());
        self.pos += 1;
        t
    }

    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH * 4 {
            return Err(ExprError::TooLong);
        }
        Ok(())
    }

    fn expr(&mut self) -> Result<(), ExprError> {
        self.enter()?;
        self.sum()?;
        let cmp = match self.peek() {
            Some(Tok::Op('<')) => Some(F2::Lt),
            Some(Tok::Op('>')) => Some(F2::Gt),
            Some(Tok::Cmp2('<')) => Some(F2::Le),
            Some(Tok::Cmp2('>')) => Some(F2::Ge),
            Some(Tok::Cmp2('=')) => Some(F2::Eq),
            Some(Tok::Cmp2('!')) => Some(F2::Ne),
            _ => None,
        };
        if let Some(f) = cmp {
            self.bump();
            self.sum()?;
            self.ops.push(Op::F2(f));
        }
        self.depth -= 1;
        Ok(())
    }

    fn sum(&mut self) -> Result<(), ExprError> {
        self.product()?;
        loop {
            let f = match self.peek() {
                Some(Tok::Op('+')) => F2::Add,
                Some(Tok::Op('-')) => F2::Sub,
                _ => return Ok(()),
            };
            self.bump();
            self.product()?;
            self.ops.push(Op::F2(f));
        }
    }

    fn starts_atom(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::Num(_)) | Some(Tok::Name(_)) | Some(Tok::LParen)
        )
    }

    fn product(&mut self) -> Result<(), ExprError> {
        self.unary()?;
        loop {
            let f = match self.peek() {
                Some(Tok::Op('*')) => Some(F2::Mul),
                Some(Tok::Op('/')) => Some(F2::Div),
                Some(Tok::Op('%')) => Some(F2::Mod),
                _ if self.starts_atom() => None,
                _ => return Ok(()),
            };
            if f.is_some() {
                self.bump();
            }
            self.unary()?;
            self.ops.push(Op::F2(f.unwrap_or(F2::Mul)));
        }
    }

    fn unary(&mut self) -> Result<(), ExprError> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.bump();
                self.enter()?;
                self.unary()?;
                self.depth -= 1;
                self.ops.push(Op::F1(F1::Neg));
                Ok(())
            }
            Some(Tok::Op('+')) => {
                self.bump();
                self.enter()?;
                let r = self.unary();
                self.depth -= 1;
                r
            }
            Some(Tok::Op('!')) => {
                self.bump();
                self.enter()?;
                self.unary()?;
                self.depth -= 1;
                self.ops.push(Op::F1(F1::Not));
                Ok(())
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<(), ExprError> {
        self.postfix()?;
        if self.peek() == Some(&Tok::Op('^')) {
            self.bump();
            self.enter()?;
            // Right to left: 2^3^2 = 2^9; the exponent may carry a sign.
            self.unary()?;
            self.depth -= 1;
            self.ops.push(Op::F2(F2::Pow));
        }
        Ok(())
    }

    fn postfix(&mut self) -> Result<(), ExprError> {
        self.atom()?;
        while self.peek() == Some(&Tok::Op('!')) {
            self.bump();
            self.ops.push(Op::F1(F1::Fact));
        }
        Ok(())
    }

    fn atom(&mut self) -> Result<(), ExprError> {
        let at = self.at();
        match self.bump() {
            Some(Tok::Num(v)) => {
                self.ops.push(Op::Num(v));
                Ok(())
            }
            Some(Tok::LParen) => {
                self.expr()?;
                match self.bump() {
                    Some(Tok::RParen) => Ok(()),
                    None => Err(ExprError::Brackets),
                    _ => Err(ExprError::Syntax(self.prev_at())),
                }
            }
            Some(Tok::Bar) => {
                self.expr()?;
                match self.bump() {
                    Some(Tok::Bar) => {
                        self.ops.push(Op::F1(F1::Abs));
                        Ok(())
                    }
                    None => Err(ExprError::Brackets),
                    _ => Err(ExprError::Syntax(self.prev_at())),
                }
            }
            Some(Tok::Name(n)) => self.name(&n, at),
            Some(Tok::RParen) => Err(ExprError::Brackets),
            _ => Err(ExprError::Syntax(at)),
        }
    }

    fn prev_at(&self) -> usize {
        self.toks
            .get(self.pos.saturating_sub(1))
            .map_or(self.end, |(_, a)| *a)
    }

    fn name(&mut self, n: &str, _at: usize) -> Result<(), ExprError> {
        let parts = split_name(n).ok_or_else(|| ExprError::Unknown(n.to_string()))?;
        self.name_parts(&parts)
    }

    /// Letters run together: each constant or variable multiplies the
    /// next (`ab` is `a b`; an exponent binds the last letter only, so
    /// `ab^2` is `a b^2`), a function takes the rest as its argument
    /// (`sinx` is `sin(x)`).
    fn name_parts(&mut self, parts: &[String]) -> Result<(), ExprError> {
        let Some(first) = parts.first() else {
            return Err(ExprError::Syntax(self.at()));
        };
        self.enter()?;
        if let Some(op) = atom_name(first) {
            self.ops.push(op);
            if parts.len() > 1 {
                self.name_parts(&parts[1..])?;
                self.ops.push(Op::F2(F2::Mul));
            } else if self.peek() == Some(&Tok::Op('^')) {
                self.bump();
                self.unary()?;
                self.ops.push(Op::F2(F2::Pow));
            }
        } else if parts.len() == 1 {
            self.call(first)?;
        } else {
            let (lo, _) =
                function_arity(first).ok_or_else(|| ExprError::Unknown(first.to_string()))?;
            if lo != 1 {
                return Err(ExprError::Args(first.to_string()));
            }
            self.name_parts(&parts[1..])?;
            self.emit(first, 1)?;
        }
        self.depth -= 1;
        Ok(())
    }

    /// A function call: `f(a, b)`, or `f x` with no brackets (taking the
    /// next power: `sin 2x` is `sin(2) x`, `sin x^2` is `sin(x^2)`).
    fn call(&mut self, n: &str) -> Result<(), ExprError> {
        let (lo, hi) = function_arity(n).ok_or_else(|| ExprError::Unknown(n.to_string()))?;
        let mut args = 0;
        if self.peek() == Some(&Tok::LParen) {
            self.bump();
            loop {
                self.expr()?;
                args += 1;
                match self.bump() {
                    Some(Tok::Comma) => continue,
                    Some(Tok::RParen) => break,
                    None => return Err(ExprError::Brackets),
                    _ => return Err(ExprError::Syntax(self.prev_at())),
                }
            }
            // `f(x)^2` squares the result.
        } else {
            if lo != 1 {
                return Err(ExprError::Args(n.to_string()));
            }
            self.enter()?;
            self.unary_tight()?;
            self.depth -= 1;
            args = 1;
        }
        if args < lo || args > hi {
            return Err(ExprError::Args(n.to_string()));
        }
        self.emit(n, args)
    }

    /// The operation of function `n` with `args` arguments on the stack.
    fn emit(&mut self, n: &str, args: usize) -> Result<(), ExprError> {
        match n {
            "if" => self.ops.push(Op::If),
            "min" | "max" => {
                let f = if n == "min" { F2::Min } else { F2::Max };
                for _ in 1..args {
                    self.ops.push(Op::F2(f));
                }
            }
            "log" if args == 2 => self.ops.push(Op::F2(F2::LogB)),
            "log" => self.ops.push(Op::F1(F1::Log10)),
            "atan2" => self.ops.push(Op::F2(F2::Atan2)),
            "mod" => self.ops.push(Op::F2(F2::Mod)),
            "pow" => self.ops.push(Op::F2(F2::Pow)),
            "hypot" => self.ops.push(Op::F2(F2::Hypot)),
            "root" => self.ops.push(Op::F2(F2::Root)),
            _ => {
                let f = f1_of(n).ok_or_else(|| ExprError::Unknown(n.to_string()))?;
                self.ops.push(Op::F1(f));
            }
        }
        Ok(())
    }

    /// The argument of a function written without brackets: a sign, then
    /// a power.
    fn unary_tight(&mut self) -> Result<(), ExprError> {
        if self.peek() == Some(&Tok::Op('-')) {
            self.bump();
            self.unary_tight()?;
            self.ops.push(Op::F1(F1::Neg));
            return Ok(());
        }
        self.power()
    }
}

/// Split `src` at a lone `=` (not part of `<=`, `>=`, `==`, `!=`).
fn split_equation(src: &str) -> Option<(&str, &str)> {
    let b = src.as_bytes();
    let mut found = None;
    for (i, &c) in b.iter().enumerate() {
        if c != b'=' {
            continue;
        }
        let prev = if i > 0 { b[i - 1] } else { b' ' };
        let next = b.get(i + 1).copied().unwrap_or(b' ');
        if matches!(prev, b'<' | b'>' | b'=' | b'!') || next == b'=' {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(i);
    }
    let i = found?;
    Some((&src[..i], &src[i + 1..]))
}

/// The left side names the result (`y`, `f(x)`, `r`, `r(θ)`, `x(t)`,
/// `y(t)`): it is dropped.
fn is_result_name(lhs: &str) -> bool {
    let s: String = lhs.chars().filter(|c| !c.is_whitespace()).collect();
    let base = match s.find('(') {
        Some(i) if s.ends_with(')') => {
            let arg = &s[i + 1..s.len() - 1];
            if !matches!(arg, "x" | "t" | "θ" | "theta" | "u") {
                return false;
            }
            &s[..i]
        }
        Some(_) => return false,
        None => s.as_str(),
    };
    matches!(base, "y" | "f" | "g" | "h" | "r" | "ρ" | "z" | "x" | "F")
}

impl Expr {
    /// Compile `src`. An equation `lhs = rhs` becomes `lhs - (rhs)`,
    /// unless its left side only names the result (`y = ...`).
    pub fn parse(src: &str) -> Result<Expr, ExprError> {
        let trimmed = src.trim();
        if trimmed.is_empty() {
            return Err(ExprError::Empty);
        }
        if let Some((lhs, rhs)) = split_equation(trimmed) {
            if is_result_name(lhs) {
                return Self::parse_plain(rhs);
            }
            if lhs.trim().is_empty() || rhs.trim().is_empty() {
                return Err(ExprError::Syntax(lhs.chars().count()));
            }
            return Self::parse_plain(&format!("({lhs})-({rhs})"));
        }
        Self::parse_plain(trimmed)
    }

    fn parse_plain(src: &str) -> Result<Expr, ExprError> {
        if src.trim().is_empty() {
            return Err(ExprError::Empty);
        }
        let toks = lex(src)?;
        let end = src.chars().count();
        let mut p = Parser {
            toks,
            pos: 0,
            ops: Vec::new(),
            depth: 0,
            end,
        };
        p.expr()?;
        match p.peek() {
            None => {}
            Some(Tok::RParen) => return Err(ExprError::Brackets),
            Some(_) => return Err(ExprError::Syntax(p.at())),
        }
        let mut uses = [false; SLOTS];
        for op in &p.ops {
            if let Op::Var(i) = op {
                uses[*i] = true;
            }
        }
        Ok(Expr { ops: p.ops, uses })
    }

    /// Whether the formula reads variable `slot`.
    pub fn uses(&self, slot: usize) -> bool {
        self.uses.get(slot).copied().unwrap_or(false)
    }

    /// The value with `vars`; trigonometric functions take and give
    /// degrees when `degrees` is set. NaN where it is not defined.
    pub fn eval(&self, vars: &Vars, degrees: bool) -> f64 {
        let mut stack: [f64; 64] = [0.0; 64];
        let mut heap: Vec<f64> = Vec::new();
        let mut n = 0usize;
        // A stack on the stack for usual formulas, a Vec for long ones.
        macro_rules! push {
            ($v:expr) => {{
                let v = $v;
                if n < stack.len() {
                    stack[n] = v;
                } else {
                    heap.push(v);
                }
                n += 1;
            }};
        }
        macro_rules! pop {
            () => {{
                if n == 0 {
                    return f64::NAN;
                }
                n -= 1;
                if n < stack.len() {
                    stack[n]
                } else {
                    heap.pop().unwrap_or(f64::NAN)
                }
            }};
        }
        let (to_rad, from_rad) = if degrees {
            (std::f64::consts::PI / 180.0, 180.0 / std::f64::consts::PI)
        } else {
            (1.0, 1.0)
        };
        for op in &self.ops {
            match *op {
                Op::Num(v) => push!(v),
                Op::Var(i) => push!(vars.get(i).copied().unwrap_or(0.0)),
                Op::F1(f) => {
                    let a = pop!();
                    push!(f1(f, a, to_rad, from_rad));
                }
                Op::F2(f) => {
                    let b = pop!();
                    let a = pop!();
                    push!(f2(f, a, b));
                }
                Op::If => {
                    let e = pop!();
                    let t = pop!();
                    let c = pop!();
                    push!(if c.is_nan() {
                        f64::NAN
                    } else if c != 0.0 {
                        t
                    } else {
                        e
                    });
                }
            }
        }
        if n == 1 {
            pop!()
        } else {
            f64::NAN
        }
    }
}

fn factorial(a: f64) -> f64 {
    if a < 0.0 || a.fract() != 0.0 {
        return f64::NAN;
    }
    if a > 170.0 {
        return f64::INFINITY;
    }
    let mut r = 1.0;
    let mut i = 2.0;
    while i <= a {
        r *= i;
        i += 1.0;
    }
    r
}

fn f1(f: F1, a: f64, to_rad: f64, from_rad: f64) -> f64 {
    match f {
        F1::Sin => (a * to_rad).sin(),
        F1::Cos => (a * to_rad).cos(),
        F1::Tan => (a * to_rad).tan(),
        F1::Cot => 1.0 / (a * to_rad).tan(),
        F1::Sec => 1.0 / (a * to_rad).cos(),
        F1::Csc => 1.0 / (a * to_rad).sin(),
        F1::Asin => a.asin() * from_rad,
        F1::Acos => a.acos() * from_rad,
        F1::Atan => a.atan() * from_rad,
        F1::Sinh => a.sinh(),
        F1::Cosh => a.cosh(),
        F1::Tanh => a.tanh(),
        F1::Asinh => a.asinh(),
        F1::Acosh => a.acosh(),
        F1::Atanh => a.atanh(),
        F1::Sqrt => a.sqrt(),
        F1::Cbrt => a.cbrt(),
        F1::Abs => a.abs(),
        F1::Exp => a.exp(),
        F1::Ln => a.ln(),
        F1::Log10 => a.log10(),
        F1::Log2 => a.log2(),
        F1::Floor => a.floor(),
        F1::Ceil => a.ceil(),
        F1::Round => a.round(),
        F1::Sign => {
            if a.is_nan() {
                f64::NAN
            } else if a == 0.0 {
                0.0
            } else {
                a.signum()
            }
        }
        F1::Frac => a - a.floor(),
        F1::Fact => factorial(a),
        F1::Neg => -a,
        F1::Not => {
            if a.is_nan() {
                f64::NAN
            } else if a == 0.0 {
                1.0
            } else {
                0.0
            }
        }
    }
}

fn truth(v: bool) -> f64 {
    if v {
        1.0
    } else {
        0.0
    }
}

fn f2(f: F2, a: f64, b: f64) -> f64 {
    match f {
        F2::Add => a + b,
        F2::Sub => a - b,
        F2::Mul => a * b,
        F2::Div => a / b,
        F2::Pow => {
            // A negative base with an odd root as the exponent (x^(1/3))
            // keeps its sign, as on a calculator.
            if a < 0.0 && b.fract() != 0.0 {
                let inv = 1.0 / b;
                if (inv - inv.round()).abs() < 1e-9 && (inv.round() as i64) % 2 != 0 {
                    return -((-a).powf(b));
                }
            }
            a.powf(b)
        }
        F2::Mod => {
            if b == 0.0 {
                f64::NAN
            } else {
                a - b * (a / b).floor()
            }
        }
        F2::Atan2 => a.atan2(b),
        F2::Min => a.min(b),
        F2::Max => a.max(b),
        F2::LogB => b.ln() / a.ln(),
        F2::Hypot => a.hypot(b),
        F2::Root => {
            if a < 0.0 && b.fract() == 0.0 && (b as i64) % 2 != 0 {
                -((-a).powf(1.0 / b))
            } else {
                a.powf(1.0 / b)
            }
        }
        F2::Lt => truth(a < b),
        F2::Gt => truth(a > b),
        F2::Le => truth(a <= b),
        F2::Ge => truth(a >= b),
        F2::Eq => truth((a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1.0)),
        F2::Ne => truth((a - b).abs() > 1e-12 * a.abs().max(b.abs()).max(1.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(src: &str, x: f64) -> f64 {
        let mut vars = [0.0; SLOTS];
        vars[X] = x;
        vars[T] = x;
        vars[A] = 2.0;
        vars[B] = 3.0;
        Expr::parse(src).expect(src).eval(&vars, false)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn arithmetic_follows_the_usual_order() {
        assert!(close(v("1 + 2 * 3", 0.0), 7.0));
        assert!(close(v("(1 + 2) * 3", 0.0), 9.0));
        assert!(close(v("2^3^2", 0.0), 512.0));
        assert!(close(v("-x^2", 3.0), -9.0));
        assert!(close(v("2^-1", 0.0), 0.5));
        assert!(close(v("10 / 4", 0.0), 2.5));
        assert!(close(v("7 % 3", 0.0), 1.0));
        assert!(close(v("1e-3 * 1000", 0.0), 1.0));
        assert!(close(v("2 − 5 × 2 ÷ 4", 0.0), -0.5));
    }

    #[test]
    fn implicit_multiplication_and_names_run_together() {
        assert!(close(v("2x", 4.0), 8.0));
        assert!(close(v("2x^2 - 3x + 1", 2.0), 3.0));
        assert!(close(v("(x+1)(x-1)", 3.0), 8.0));
        assert!(close(v("3sin(x)", std::f64::consts::FRAC_PI_2), 3.0));
        assert!(close(v("sinx", std::f64::consts::FRAC_PI_2), 1.0));
        assert!(close(v("ab", 0.0), 6.0));
        assert!(close(v("2pix", 1.0), std::f64::consts::TAU));
        assert!(close(v("x²", 5.0), 25.0));
        assert!(close(v("√x", 16.0), 4.0));
        assert!(close(v("|x - 5|", 2.0), 3.0));
        assert!(close(v("2,5x", 2.0), 5.0));
    }

    #[test]
    fn functions_and_constants() {
        assert!(close(v("sqrt(2)^2", 0.0), 2.0));
        assert!(close(v("log(1000)", 0.0), 3.0));
        assert!(close(v("log(2, 8)", 0.0), 3.0));
        assert!(close(v("ln(e)", 0.0), 1.0));
        assert!(close(v("max(1, 5, 3)", 0.0), 5.0));
        assert!(close(v("min(4, -2)", 0.0), -2.0));
        assert!(close(v("atan2(1, 1)", 0.0), std::f64::consts::FRAC_PI_4));
        assert!(close(v("root(-8, 3)", 0.0), -2.0));
        assert!(close(v("x^(1/3)", -27.0), -3.0));
        assert!(close(v("5!", 0.0), 120.0));
        assert!(close(v("fact(4)", 0.0), 24.0));
        assert!(close(v("if(x < 0, -x, x)", -4.0), 4.0));
        assert!(close(v("if(x < 0, -x, x)", 4.0), 4.0));
        assert!(close(v("π", 0.0), std::f64::consts::PI));
        assert!(close(v("sen(θ)", std::f64::consts::FRAC_PI_2), 1.0));
        assert!(v("sqrt(-1)", 0.0).is_nan());
        assert!(v("ln(0)", 0.0).is_infinite());
    }

    #[test]
    fn degrees_change_the_trigonometry() {
        let e = Expr::parse("sin(x)").expect("sin");
        let mut vars = [0.0; SLOTS];
        vars[X] = 90.0;
        assert!(close(e.eval(&vars, true), 1.0));
        let e = Expr::parse("acos(0)").expect("acos");
        assert!(close(e.eval(&vars, true), 90.0));
    }

    #[test]
    fn equations_and_result_names() {
        assert!(close(v("y = 2x + 1", 3.0), 7.0));
        assert!(close(v("f(x) = x^2", 3.0), 9.0));
        assert!(close(v("r(θ) = 2", 0.0), 2.0));
        // An equation in x and y: lhs - rhs.
        let e = Expr::parse("x^2 + y^2 = 25").expect("circle");
        let mut vars = [0.0; SLOTS];
        vars[X] = 3.0;
        vars[Y] = 4.0;
        assert!(close(e.eval(&vars, false), 0.0));
        assert!(e.uses(X) && e.uses(Y) && !e.uses(T));
        assert!(close(v("x <= 2", 2.0), 1.0));
        assert!(close(v("x != 2", 2.0), 0.0));
    }

    #[test]
    fn mistakes_are_errors_not_panics() {
        assert_eq!(Expr::parse(""), Err(ExprError::Empty));
        assert_eq!(Expr::parse("  "), Err(ExprError::Empty));
        assert_eq!(
            Expr::parse("foo(x)"),
            Err(ExprError::Unknown("foo".into()))
        );
        assert_eq!(Expr::parse("(x + 1"), Err(ExprError::Brackets));
        assert_eq!(Expr::parse("x + 1)"), Err(ExprError::Brackets));
        assert!(matches!(Expr::parse("x +"), Err(ExprError::Syntax(_))));
        assert!(matches!(Expr::parse("2 $ 3"), Err(ExprError::Syntax(2))));
        assert_eq!(Expr::parse("atan2(1)"), Err(ExprError::Args("atan2".into())));
        assert_eq!(Expr::parse("if(1, 2)"), Err(ExprError::Args("if".into())));
        let deep = "(".repeat(500) + "x" + &")".repeat(500);
        assert_eq!(Expr::parse(&deep), Err(ExprError::TooLong));
        assert_eq!(Expr::parse(&"x+".repeat(1500)), Err(ExprError::TooLong));
        // Long but legal: evaluated past the fixed stack.
        let long = vec!["1"; 100].join("^");
        assert!(close(v(&long, 0.0), 1.0));
        let nested = (0..50).map(|_| "(1+").collect::<String>() + "0" + &")".repeat(50);
        assert!(close(v(&nested, 0.0), 50.0));
    }

    #[test]
    fn random_text_never_panics() {
        let alphabet: Vec<char> = "x y t ( ) | , ; + - * / ^ ! = < > 2 . e π sin log if abc ²"
            .chars()
            .collect();
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..3000 {
            let mut s = String::new();
            for _ in 0..(seed % 24) {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                s.push(alphabet[(seed % alphabet.len() as u64) as usize]);
            }
            if let Ok(e) = Expr::parse(&s) {
                let _ = e.eval(&[1.0; SLOTS], false);
            }
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        }
    }
}
