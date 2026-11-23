//! Formulas (#2183): mathematical notation typed in the widely used
//! backslash syntax (`x^2`, `\frac{a}{b}`, `\sqrt{x}`, `\alpha`,
//! `\sum_{i=1}^{n}`...) laid out and made into one curve object, which
//! keeps the source so that the formula can be edited again.
//!
//! The layout works in millimetres with Y up and the baseline at 0, as
//! the text engine draws; the finished curve is turned to the page's Y
//! down.

use kurbo::{Affine, BezPath, Shape as _};
use tracedraw_core::TextSpan;

/// The object data key that holds a formula's source and settings.
pub const DATA_KEY: &str = "formula";

/// The settings of a formula: its source, font and size.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaSpec {
    pub source: String,
    pub font: String,
    pub size_pt: f64,
}

impl Default for FormulaSpec {
    fn default() -> Self {
        Self {
            source: r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}".into(),
            font: "DejaVu Serif".into(),
            size_pt: 24.0,
        }
    }
}

impl FormulaSpec {
    /// The settings as kept in the object's data: size, font and source
    /// on separate lines (the source may hold more lines).
    pub fn to_data(&self) -> String {
        format!("{}\n{}\n{}", self.size_pt, self.font, self.source)
    }

    pub fn from_data(s: &str) -> Option<FormulaSpec> {
        let mut it = s.splitn(3, '\n');
        let size_pt = it.next()?.trim().parse::<f64>().ok()?;
        let font = it.next()?.to_string();
        let source = it.next().unwrap_or("").to_string();
        Some(FormulaSpec {
            source,
            font,
            size_pt,
        })
    }

    /// The formula settings an object keeps, if it is a formula.
    pub fn of_shape(s: &tracedraw_core::Shape) -> Option<FormulaSpec> {
        s.data
            .iter()
            .find(|(k, _)| k == DATA_KEY)
            .and_then(|(_, v)| FormulaSpec::from_data(v))
    }
}

// ----- parsing ---------------------------------------------------------------

/// How a symbol sits between its neighbours.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Class {
    /// A letter, digit or other ordinary symbol.
    Ord,
    /// `+`, `−`, `×`...: a medium space on each side.
    Bin,
    /// `=`, `<`, `≤`...: a thick space on each side.
    Rel,
    /// `,` `;`: a thin space after.
    Punct,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Sym {
        text: String,
        italic: bool,
        bold: bool,
        class: Class,
    },
    Row(Vec<Node>),
    Scripts {
        base: Box<Node>,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    Frac(Box<Node>, Box<Node>),
    Sqrt(Box<Node>, Option<Box<Node>>),
    /// A large operator (`∑`, `∫`, `lim`): limits below and above when
    /// `limits`, else at its side.
    BigOp {
        text: String,
        limits: bool,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    /// Delimiters stretched to the height of what they hold (`\left(`
    /// ... `\right)`); an empty text is no delimiter.
    Fenced(String, Box<Node>, String),
    /// A line or an accent above.
    Over(Box<Node>, Accent),
    /// Space, in ems.
    Space(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Accent {
    Line,
    Char(char),
}

fn greek(name: &str) -> Option<char> {
    Some(match name {
        "alpha" => 'α',
        "beta" => 'β',
        "gamma" => 'γ',
        "delta" => 'δ',
        "epsilon" => 'ϵ',
        "varepsilon" => 'ε',
        "zeta" => 'ζ',
        "eta" => 'η',
        "theta" => 'θ',
        "vartheta" => 'ϑ',
        "iota" => 'ι',
        "kappa" => 'κ',
        "lambda" => 'λ',
        "mu" => 'μ',
        "nu" => 'ν',
        "xi" => 'ξ',
        "pi" => 'π',
        "varpi" => 'ϖ',
        "rho" => 'ρ',
        "sigma" => 'σ',
        "varsigma" => 'ς',
        "tau" => 'τ',
        "upsilon" => 'υ',
        "phi" => 'ϕ',
        "varphi" => 'φ',
        "chi" => 'χ',
        "psi" => 'ψ',
        "omega" => 'ω',
        "Gamma" => 'Γ',
        "Delta" => 'Δ',
        "Theta" => 'Θ',
        "Lambda" => 'Λ',
        "Xi" => 'Ξ',
        "Pi" => 'Π',
        "Sigma" => 'Σ',
        "Upsilon" => 'Υ',
        "Phi" => 'Φ',
        "Psi" => 'Ψ',
        "Omega" => 'Ω',
        _ => return None,
    })
}

/// A named symbol and its class.
fn symbol(name: &str) -> Option<(&'static str, Class)> {
    use Class::*;
    Some(match name {
        "pm" => ("±", Bin),
        "mp" => ("∓", Bin),
        "times" => ("×", Bin),
        "cdot" => ("·", Bin),
        "div" => ("÷", Bin),
        "ast" => ("∗", Bin),
        "circ" => ("∘", Bin),
        "cup" => ("∪", Bin),
        "cap" => ("∩", Bin),
        "setminus" => ("∖", Bin),
        "wedge" | "land" => ("∧", Bin),
        "vee" | "lor" => ("∨", Bin),
        "oplus" => ("⊕", Bin),
        "otimes" => ("⊗", Bin),
        "leq" | "le" => ("≤", Rel),
        "geq" | "ge" => ("≥", Rel),
        "neq" | "ne" => ("≠", Rel),
        "approx" => ("≈", Rel),
        "equiv" => ("≡", Rel),
        "sim" => ("∼", Rel),
        "simeq" => ("≃", Rel),
        "cong" => ("≅", Rel),
        "propto" => ("∝", Rel),
        "ll" => ("≪", Rel),
        "gg" => ("≫", Rel),
        "in" => ("∈", Rel),
        "notin" => ("∉", Rel),
        "ni" => ("∋", Rel),
        "subset" => ("⊂", Rel),
        "supset" => ("⊃", Rel),
        "subseteq" => ("⊆", Rel),
        "supseteq" => ("⊇", Rel),
        "to" | "rightarrow" => ("→", Rel),
        "leftarrow" | "gets" => ("←", Rel),
        "leftrightarrow" => ("↔", Rel),
        "Rightarrow" | "implies" => ("⇒", Rel),
        "Leftarrow" => ("⇐", Rel),
        "Leftrightarrow" | "iff" => ("⇔", Rel),
        "mapsto" => ("↦", Rel),
        "perp" => ("⊥", Rel),
        "parallel" => ("∥", Rel),
        "mid" => ("∣", Rel),
        "infty" => ("∞", Ord),
        "partial" => ("∂", Ord),
        "nabla" => ("∇", Ord),
        "forall" => ("∀", Ord),
        "exists" => ("∃", Ord),
        "emptyset" | "varnothing" => ("∅", Ord),
        "neg" | "lnot" => ("¬", Ord),
        "angle" => ("∠", Ord),
        "triangle" => ("△", Ord),
        "degree" => ("°", Ord),
        "prime" => ("′", Ord),
        "hbar" => ("ℏ", Ord),
        "ell" => ("ℓ", Ord),
        "Re" => ("ℜ", Ord),
        "Im" => ("ℑ", Ord),
        "aleph" => ("ℵ", Ord),
        "cdots" => ("⋯", Ord),
        "ldots" | "dots" => ("…", Ord),
        "vdots" => ("⋮", Ord),
        "ddots" => ("⋱", Ord),
        "langle" => ("⟨", Ord),
        "rangle" => ("⟩", Ord),
        "lfloor" => ("⌊", Ord),
        "rfloor" => ("⌋", Ord),
        "lceil" => ("⌈", Ord),
        "rceil" => ("⌉", Ord),
        "vert" => ("|", Ord),
        "Vert" => ("‖", Ord),
        "lbrace" => ("{", Ord),
        "rbrace" => ("}", Ord),
        _ => return None,
    })
}

/// A large operator: its sign and whether its limits go below and above.
fn big_op(name: &str) -> Option<(&'static str, bool)> {
    Some(match name {
        "sum" => ("∑", true),
        "prod" => ("∏", true),
        "coprod" => ("∐", true),
        "bigcup" => ("⋃", true),
        "bigcap" => ("⋂", true),
        "int" => ("∫", false),
        "iint" => ("∬", false),
        "iiint" => ("∭", false),
        "oint" => ("∮", false),
        "lim" => ("lim", true),
        "max" => ("max", true),
        "min" => ("min", true),
        "sup" => ("sup", true),
        "inf" => ("inf", true),
        "det" => ("det", true),
        _ => return None,
    })
}

const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "log", "ln", "lg", "exp", "deg", "dim", "gcd", "ker", "arg", "mod",
];

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    /// Commands not known, shown as typed and listed for the dialog.
    unknown: &'a mut Vec<String>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    /// A command's name after the backslash: letters, or one other sign.
    fn command(&mut self) -> String {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        if self.pos == start {
            if let Some(c) = self.peek() {
                self.pos += 1;
                return c.to_string();
            }
        }
        self.chars[start..self.pos].iter().collect()
    }

    /// A row up to `}` (when `in_group`), `\right` or the end; `\\`
    /// breaks lines at the top only.
    fn row(&mut self, in_group: bool) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::new();
        // Whether the last part is words (`\text{...}` or a word in a
        // script of its own), for the space typed after it (#3793).
        let mut last_words = false;
        loop {
            let before = self.pos;
            self.skip_space();
            let spaced = self.pos > before;
            let Some(c) = self.peek() else { break };
            if c == '}' {
                if in_group {
                    break;
                }
                self.pos += 1;
                continue;
            }
            if c == '^' || c == '_' {
                self.pos += 1;
                let arg = self.argument();
                let base = out.pop().unwrap_or(Node::Row(Vec::new()));
                out.push(attach(base, c == '^', arg));
                continue;
            }
            if c == '\\' {
                let save = self.pos;
                self.pos += 1;
                let name = self.command();
                if name == "right" {
                    self.pos = save;
                    break;
                }
                let words = matches!(name.as_str(), "text" | "textrm");
                if let Some(n) = self.command_node(&name) {
                    push_spaced(&mut out, n, spaced && (words || last_words));
                }
                last_words = words;
                continue;
            }
            let words = in_script_word(c);
            if let Some(n) = self.atom() {
                push_spaced(&mut out, n, spaced && (words || last_words));
            }
            last_words = words;
        }
        out
    }

    /// One argument of `^`, `_` or a command: a `{group}`, a `\command`
    /// or one sign.
    fn argument(&mut self) -> Node {
        self.skip_space();
        match self.peek() {
            Some('{') => {
                self.pos += 1;
                let r = self.row(true);
                if self.peek() == Some('}') {
                    self.pos += 1;
                }
                Node::Row(r)
            }
            Some('\\') => {
                self.pos += 1;
                let name = self.command();
                self.command_node(&name).unwrap_or(Node::Row(Vec::new()))
            }
            Some(c) => {
                self.pos += 1;
                sym_for_char(c)
            }
            None => Node::Row(Vec::new()),
        }
    }

    /// `[...]` after `\sqrt`, if there.
    fn optional(&mut self) -> Option<Node> {
        self.skip_space();
        if self.peek() != Some('[') {
            return None;
        }
        self.pos += 1;
        let mut r = Vec::new();
        while let Some(c) = self.peek() {
            if c == ']' {
                self.pos += 1;
                break;
            }
            if c.is_whitespace() {
                self.pos += 1;
                continue;
            }
            self.pos += 1;
            r.push(sym_for_char(c));
        }
        Some(Node::Row(r))
    }

    /// The raw text of a `{group}` (for `\text`).
    fn raw_group(&mut self) -> String {
        self.skip_space();
        if self.peek() != Some('{') {
            return self
                .peek()
                .map(|c| {
                    self.pos += 1;
                    c.to_string()
                })
                .unwrap_or_default();
        }
        self.pos += 1;
        let mut depth = 0;
        let mut s = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '{' => depth += 1,
                '}' if depth == 0 => break,
                '}' => depth -= 1,
                _ => {}
            }
            s.push(c);
        }
        s
    }

    /// A delimiter after `\left` or `\right` (`.` for none).
    fn delimiter(&mut self) -> String {
        self.skip_space();
        match self.peek() {
            Some('\\') => {
                self.pos += 1;
                let name = self.command();
                match name.as_str() {
                    "{" | "lbrace" => "{".into(),
                    "}" | "rbrace" => "}".into(),
                    "|" | "Vert" => "‖".into(),
                    n => symbol(n).map(|(s, _)| s.to_string()).unwrap_or_default(),
                }
            }
            Some('.') => {
                self.pos += 1;
                String::new()
            }
            Some(c) => {
                self.pos += 1;
                c.to_string()
            }
            None => String::new(),
        }
    }

    fn command_node(&mut self, name: &str) -> Option<Node> {
        if let Some(g) = greek(name) {
            let upper = g.is_uppercase();
            return Some(Node::Sym {
                text: g.to_string(),
                italic: !upper,
                bold: false,
                class: Class::Ord,
            });
        }
        if let Some((s, class)) = symbol(name) {
            return Some(Node::Sym {
                text: s.into(),
                italic: false,
                bold: false,
                class,
            });
        }
        if let Some((s, limits)) = big_op(name) {
            return Some(Node::BigOp {
                text: s.into(),
                limits,
                sub: None,
                sup: None,
            });
        }
        if FUNCTIONS.contains(&name) {
            return Some(upright(name));
        }
        Some(match name {
            "frac" | "dfrac" | "tfrac" => {
                let num = self.argument();
                let den = self.argument();
                Node::Frac(Box::new(num), Box::new(den))
            }
            "sqrt" => {
                let index = self.optional();
                let body = self.argument();
                Node::Sqrt(Box::new(body), index.map(Box::new))
            }
            "left" => {
                let l = self.delimiter();
                let body = self.row(true);
                // `\right` and its delimiter.
                let mut r = String::new();
                if self.peek() == Some('\\') {
                    let save = self.pos;
                    self.pos += 1;
                    if self.command() == "right" {
                        r = self.delimiter();
                    } else {
                        self.pos = save;
                    }
                }
                Node::Fenced(l, Box::new(Node::Row(body)), r)
            }
            "text" | "mathrm" | "textrm" | "operatorname" => {
                let t = self.raw_group();
                upright(&t)
            }
            "mathbf" | "textbf" | "boldsymbol" => {
                let t = self.raw_group();
                Node::Sym {
                    text: t,
                    italic: false,
                    bold: true,
                    class: Class::Ord,
                }
            }
            "mathit" | "textit" => {
                let t = self.raw_group();
                Node::Sym {
                    text: t,
                    italic: true,
                    bold: false,
                    class: Class::Ord,
                }
            }
            "overline" | "bar" => Node::Over(Box::new(self.argument()), Accent::Line),
            "hat" | "widehat" => Node::Over(Box::new(self.argument()), Accent::Char('^')),
            "tilde" | "widetilde" => Node::Over(Box::new(self.argument()), Accent::Char('~')),
            "vec" => Node::Over(Box::new(self.argument()), Accent::Char('→')),
            "dot" => Node::Over(Box::new(self.argument()), Accent::Char('˙')),
            "ddot" => Node::Over(Box::new(self.argument()), Accent::Char('¨')),
            "," => Node::Space(3.0 / 18.0),
            ":" | ">" => Node::Space(4.0 / 18.0),
            ";" => Node::Space(5.0 / 18.0),
            "!" => Node::Space(-3.0 / 18.0),
            " " => Node::Space(0.33),
            "quad" => Node::Space(1.0),
            "qquad" => Node::Space(2.0),
            "{" | "}" | "%" | "$" | "&" | "#" | "_" => Node::Sym {
                text: name.into(),
                italic: false,
                bold: false,
                class: Class::Ord,
            },
            "|" => Node::Sym {
                text: "‖".into(),
                italic: false,
                bold: false,
                class: Class::Ord,
            },
            // `\\` is handled by the caller (lines).
            "\\" => return None,
            "displaystyle" | "limits" | "nolimits" => return None,
            _ => {
                self.unknown.push(format!("\\{name}"));
                upright(&format!("\\{name}"))
            }
        })
    }

    /// One sign outside commands: a number (digits and points together)
    /// or one symbol.
    fn atom(&mut self) -> Option<Node> {
        let c = self.peek()?;
        if c == '{' {
            self.pos += 1;
            let r = self.row(true);
            if self.peek() == Some('}') {
                self.pos += 1;
            }
            return Some(Node::Row(r));
        }
        if c.is_ascii_digit() {
            let start = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                self.pos += 1;
            }
            let t: String = self.chars[start..self.pos].iter().collect();
            return Some(upright(&t));
        }
        // A word in a script of its own (Hindi, Russian, Arabic...): one
        // run, shaped as text with the spaces between its words, as the
        // letters and vowel signs of Devanagari join only when shaped
        // together. One sign at a time left the vowel signs apart from
        // their letters and the spaces out (#3572).
        if in_script_word(c) {
            let start = self.pos;
            loop {
                while self.peek().is_some_and(in_script_word) {
                    self.pos += 1;
                }
                let mut ahead = self.pos;
                while self.chars.get(ahead).is_some_and(|c| *c == ' ') {
                    ahead += 1;
                }
                if ahead > self.pos && self.chars.get(ahead).is_some_and(|c| in_script_word(*c)) {
                    self.pos = ahead;
                } else {
                    break;
                }
            }
            let t: String = self.chars[start..self.pos].iter().collect();
            return Some(upright(&t));
        }
        self.pos += 1;
        Some(sym_for_char(c))
    }
}

/// A character of a word written in a script other than Latin and Greek
/// (whose letters are the formula's variables) and not a mathematical
/// sign: Devanagari, Cyrillic, Arabic and the like, with their marks.
fn in_script_word(c: char) -> bool {
    let u = c as u32;
    u >= 0x0400
        && !c.is_whitespace()
        && !(0x2000..=0x2BFF).contains(&u)
        && !(0x1D400..=0x1D7FF).contains(&u)
}

/// Push `n` onto the row `out`, after a word space when `space` (a space
/// typed between words and the formula, #3793: math ignores spaces, so
/// "Area = " ran into what followed) and neither side is a sign that
/// brings its own space.
fn push_spaced(out: &mut Vec<Node>, n: Node, space: bool) {
    let spaced_sign = |n: &Node| matches!(class_of(n), Class::Bin | Class::Rel | Class::Punct);
    if space && out.last().is_some_and(|l| !spaced_sign(l)) && !spaced_sign(&n) {
        out.push(Node::Space(0.33));
    }
    out.push(n);
}

fn upright(t: &str) -> Node {
    Node::Sym {
        text: t.into(),
        italic: false,
        bold: false,
        class: Class::Ord,
    }
}

fn sym_for_char(c: char) -> Node {
    let (text, class) = match c {
        '-' | '−' => ("−".to_string(), Class::Bin),
        '+' | '±' | '×' | '÷' | '*' | '·' => (
            if c == '*' {
                "∗".into()
            } else {
                c.to_string()
            },
            Class::Bin,
        ),
        '=' | '<' | '>' | '≤' | '≥' | '≠' | '≈' | '→' => (c.to_string(), Class::Rel),
        ',' | ';' => (c.to_string(), Class::Punct),
        '\'' => ("′".to_string(), Class::Ord),
        _ => (c.to_string(), Class::Ord),
    };
    Node::Sym {
        italic: c.is_alphabetic() && class == Class::Ord && c.is_ascii(),
        text,
        bold: false,
        class,
    }
}

/// `base` with a superscript (`sup`) or subscript added.
fn attach(base: Node, sup: bool, arg: Node) -> Node {
    match base {
        Node::BigOp {
            text,
            limits,
            sub: s0,
            sup: p0,
        } => {
            let (sub, sp) = if sup {
                (s0, Some(Box::new(arg)))
            } else {
                (Some(Box::new(arg)), p0)
            };
            Node::BigOp {
                text,
                limits,
                sub,
                sup: sp,
            }
        }
        Node::Scripts {
            base,
            sub: s0,
            sup: p0,
        } if (sup && p0.is_none()) || (!sup && s0.is_none()) => {
            let (sub, sp) = if sup {
                (s0, Some(Box::new(arg)))
            } else {
                (Some(Box::new(arg)), p0)
            };
            Node::Scripts { base, sub, sup: sp }
        }
        base => {
            let (sub, sp) = if sup {
                (None, Some(Box::new(arg)))
            } else {
                (Some(Box::new(arg)), None)
            };
            Node::Scripts {
                base: Box::new(base),
                sub,
                sup: sp,
            }
        }
    }
}

/// The lines of `source` (split at `\\`), parsed; and the commands not
/// known.
fn parse(source: &str) -> (Vec<Node>, Vec<String>) {
    let mut unknown = Vec::new();
    let mut lines = Vec::new();
    for line in split_lines(source) {
        let mut p = Parser {
            chars: line.chars().collect(),
            pos: 0,
            unknown: &mut unknown,
        };
        let mut row = p.row(false);
        // A stray `\right`: skip it and go on.
        while p.pos < p.chars.len() {
            p.pos += 1;
            let _ = p.command();
            let _ = p.delimiter();
            row.extend(p.row(false));
        }
        lines.push(Node::Row(row));
    }
    (lines, unknown)
}

/// `source` cut at the `\\` outside braces, and at line breaks.
fn split_lines(source: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut depth = 0i32;
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth <= 0 && (c == '\n' || (c == '\\' && chars.get(i + 1) == Some(&'\\'))) {
            out.push(String::new());
            i += if c == '\n' { 1 } else { 2 };
            continue;
        }
        if c == '\\' && i + 1 < chars.len() {
            // An escaped sign stays with its backslash.
            out.last_mut().unwrap().push(c);
            out.last_mut().unwrap().push(chars[i + 1]);
            i += 2;
            continue;
        }
        out.last_mut().unwrap().push(c);
        i += 1;
    }
    out.retain(|l| !l.trim().is_empty());
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

// ----- layout ----------------------------------------------------------------

/// A laid-out part: its outline (Y up, baseline at 0, starting at x 0),
/// its advance and its height above and below the baseline, mm.
#[derive(Debug, Clone, Default)]
struct Bx {
    path: BezPath,
    w: f64,
    asc: f64,
    desc: f64,
}

impl Bx {
    fn place(&mut self, other: &Bx, dx: f64, dy: f64) {
        self.path
            .extend(Affine::translate((dx, dy)) * other.path.clone());
    }
}

fn rect_path(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
    kurbo::Rect::new(x0, y0, x1, y1).to_path(0.01)
}

struct Layout<'a> {
    font: &'a str,
}

impl Layout<'_> {
    /// `text` in the font at `em` mm.
    fn glyphs(&self, text: &str, em: f64, italic: bool, bold: bool) -> Bx {
        let size_pt = em * 72.0 / 25.4;
        let span = TextSpan {
            italic,
            bold,
            ..TextSpan::new(text, self.font.to_string(), size_pt)
        };
        let l = tracedraw_text::fonts().outline(&[span]);
        let ink = l.path.bounding_box();
        let has_ink = !l.path.elements().is_empty();
        Bx {
            asc: if has_ink { ink.y1.max(0.0) } else { 0.0 },
            desc: if has_ink { (-ink.y0).max(0.0) } else { 0.0 },
            w: l.bounds.x1.max(0.0),
            path: l.path,
        }
    }

    /// The height of the math axis (where fraction bars sit), mm.
    fn axis(&self, em: f64) -> f64 {
        0.25 * em
    }

    fn rule(&self, em: f64) -> f64 {
        0.055 * em
    }

    fn node(&self, n: &Node, em: f64) -> Bx {
        match n {
            Node::Sym {
                text, italic, bold, ..
            } => {
                let mut g = self.glyphs(text, em, *italic, *bold);
                // Room for the slant of an italic letter.
                if *italic {
                    g.w += 0.04 * em;
                }
                g
            }
            Node::Row(items) => self.row(items, em),
            Node::Space(s) => Bx {
                w: s * em,
                ..Default::default()
            },
            Node::Scripts { base, sub, sup } => {
                let b = self.node(base, em);
                self.scripts(b, sub.as_deref(), sup.as_deref(), em)
            }
            Node::Frac(num, den) => self.frac(num, den, em),
            Node::Sqrt(body, index) => self.sqrt(body, index.as_deref(), em),
            Node::BigOp {
                text,
                limits,
                sub,
                sup,
            } => {
                let word = text.chars().count() > 1;
                let big = if word { em } else { em * 1.45 };
                let mut op = self.glyphs(text, big, false, false);
                if !word {
                    // Centred on the axis.
                    let mid = (op.asc - op.desc) / 2.0;
                    let dy = self.axis(em) - mid;
                    op.path.apply_affine(Affine::translate((0.0, dy)));
                    op.asc += dy;
                    op.desc -= dy;
                }
                if *limits {
                    self.limits(op, sub.as_deref(), sup.as_deref(), em)
                } else {
                    self.scripts(op, sub.as_deref(), sup.as_deref(), em)
                }
            }
            Node::Fenced(l, body, r) => {
                let b = self.node(body, em);
                let mut out = Bx::default();
                let gap = 0.08 * em;
                let mut x = 0.0;
                let (asc, desc) = (b.asc.max(0.7 * em), b.desc.max(0.2 * em));
                if !l.is_empty() {
                    let d = self.delim(l, asc, desc, em);
                    out.place(&d, x, 0.0);
                    x += d.w + gap;
                }
                out.place(&b, x, 0.0);
                x += b.w;
                if !r.is_empty() {
                    x += gap;
                    let d = self.delim(r, asc, desc, em);
                    out.place(&d, x, 0.0);
                    x += d.w;
                }
                out.w = x;
                out.asc = asc + 0.05 * em;
                out.desc = desc + 0.05 * em;
                out
            }
            Node::Over(body, accent) => {
                let mut b = self.node(body, em);
                let gap = 0.08 * em;
                match accent {
                    Accent::Line => {
                        let t = self.rule(em);
                        let y = b.asc.max(0.68 * em) + gap;
                        b.path.extend(rect_path(0.0, y, b.w, y + t));
                        b.asc = y + t;
                    }
                    Accent::Char(c) => {
                        let scale = if *c == '→' { 0.6 } else { 0.9 };
                        let a = self.glyphs(&c.to_string(), em * scale, false, false);
                        let ink = a.path.bounding_box();
                        let y = b.asc.max(0.5 * em) + gap - ink.y0;
                        let x = (b.w - (ink.x1 + ink.x0)) / 2.0;
                        b.place(&a, x, y);
                        b.asc = y + ink.y1;
                    }
                }
                b
            }
        }
    }

    /// A delimiter reaching `asc` above and `desc` below the baseline:
    /// the glyph scaled up and down when the content is taller.
    fn delim(&self, d: &str, asc: f64, desc: f64, em: f64) -> Bx {
        let mut g = self.glyphs(d, em, false, false);
        let h = g.asc + g.desc;
        let want = asc + desc;
        if h > 0.0 && want > h * 1.05 {
            let s = want / h;
            // Scaled about its own bottom, then set from -desc; a little
            // wider too, so tall ones do not look thin.
            let sx = s.sqrt().min(1.6);
            let t = Affine::translate((0.0, -desc))
                * Affine::scale_non_uniform(sx, s)
                * Affine::translate((0.0, g.desc));
            g.path.apply_affine(t);
            g.w *= sx;
            g.asc = asc;
            g.desc = desc;
        }
        g
    }

    fn row(&self, items: &[Node], em: f64) -> Bx {
        let mut out = Bx::default();
        let mut x = 0.0;
        let mut prev: Option<Class> = None;
        for (i, n) in items.iter().enumerate() {
            let class = class_of(n);
            // A minus or plus at the start, or after another operator,
            // is a sign: no spaces.
            let sign = class == Class::Bin
                && matches!(
                    prev,
                    None | Some(Class::Bin) | Some(Class::Rel) | Some(Class::Punct)
                );
            let class = if sign { Class::Ord } else { class };
            let space = match (prev, class) {
                (Some(_), Class::Rel) | (Some(Class::Rel), _) => 5.0 / 18.0,
                (Some(_), Class::Bin) | (Some(Class::Bin), _) => 4.0 / 18.0,
                (Some(Class::Punct), _) => 3.0 / 18.0,
                _ => 0.0,
            };
            // A function name before an argument: a thin space.
            let fn_space = match (i.checked_sub(1).map(|j| &items[j]), n) {
                (
                    Some(Node::Sym { text, .. }),
                    Node::Sym { .. } | Node::Fenced(..) | Node::Scripts { .. },
                ) if FUNCTIONS.contains(&text.as_str()) => 3.0 / 18.0,
                (Some(Node::BigOp { .. }), _) => 2.0 / 18.0,
                _ => 0.0,
            };
            x += (space + fn_space) * em;
            let b = self.node(n, em);
            out.place(&b, x, 0.0);
            x += b.w;
            out.asc = out.asc.max(b.asc);
            out.desc = out.desc.max(b.desc);
            prev = Some(class);
        }
        out.w = x;
        out
    }

    fn scripts(&self, b: Bx, sub: Option<&Node>, sup: Option<&Node>, em: f64) -> Bx {
        let small = (em * 0.7).max(em.min(1.0) * 0.5);
        let mut out = b.clone();
        let mut w = b.w;
        let mut right = b.w;
        if let Some(sp) = sup {
            let s = self.node(sp, small);
            let shift = (0.42 * em).max(b.asc - 0.25 * em).max(s.desc + 0.25 * em);
            out.place(&s, b.w + 0.03 * em, shift);
            right = right.max(b.w + 0.03 * em + s.w);
            out.asc = out.asc.max(shift + s.asc);
        }
        if let Some(sb) = sub {
            let s = self.node(sb, small);
            let shift = (0.22 * em).max(b.desc - 0.1 * em).max(s.asc - 0.45 * em);
            out.place(&s, b.w, -shift);
            right = right.max(b.w + s.w);
            out.desc = out.desc.max(shift + s.desc);
        }
        w = w.max(right) + 0.04 * em;
        out.w = w;
        out
    }

    fn limits(&self, op: Bx, sub: Option<&Node>, sup: Option<&Node>, em: f64) -> Bx {
        let small = em * 0.7;
        let gap = 0.12 * em;
        let up = sup.map(|n| self.node(n, small));
        let down = sub.map(|n| self.node(n, small));
        let w = [
            Some(op.w),
            up.as_ref().map(|b| b.w),
            down.as_ref().map(|b| b.w),
        ]
        .into_iter()
        .flatten()
        .fold(0.0, f64::max);
        let mut out = Bx {
            w,
            asc: op.asc,
            desc: op.desc,
            ..Default::default()
        };
        out.place(&op, (w - op.w) / 2.0, 0.0);
        if let Some(u) = up {
            let y = op.asc + gap + u.desc;
            out.place(&u, (w - u.w) / 2.0, y);
            out.asc = y + u.asc;
        }
        if let Some(d) = down {
            let y = op.desc + gap + d.asc;
            out.place(&d, (w - d.w) / 2.0, -y);
            out.desc = y + d.desc;
        }
        out
    }

    fn frac(&self, num: &Node, den: &Node, em: f64) -> Bx {
        let small = (em * 0.85).max(em.min(1.0) * 0.5);
        let n = self.node(num, small);
        let d = self.node(den, small);
        let t = self.rule(em);
        let gap = 0.18 * em;
        let pad = 0.12 * em;
        let w = n.w.max(d.w) + 2.0 * pad;
        let a = self.axis(em);
        let mut out = Bx {
            w: w + 0.1 * em,
            ..Default::default()
        };
        let ny = a + t / 2.0 + gap + n.desc;
        let dy = a - t / 2.0 - gap - d.asc;
        out.place(&n, 0.05 * em + (w - n.w) / 2.0, ny);
        out.place(&d, 0.05 * em + (w - d.w) / 2.0, dy);
        out.path.extend(rect_path(
            0.05 * em,
            a - t / 2.0,
            0.05 * em + w,
            a + t / 2.0,
        ));
        out.asc = ny + n.asc;
        out.desc = (-(dy - d.desc)).max(0.0);
        out
    }

    fn sqrt(&self, body: &Node, index: Option<&Node>, em: f64) -> Bx {
        let b = self.node(body, em);
        let t = self.rule(em);
        let gap = 0.12 * em;
        let top = b.asc.max(0.7 * em) + gap;
        let bottom = b.desc.max(0.15 * em);
        let h = top + bottom;
        // The sign: a short rise, a long stroke down and up to the bar.
        let hook = 0.28 * em;
        let wsign = 0.55 * em + 0.08 * h;
        let mut sign = BezPath::new();
        sign.move_to((0.0, -bottom + 0.45 * h));
        sign.line_to((hook * 0.6, -bottom + 0.52 * h));
        sign.line_to((wsign * 0.45, -bottom));
        sign.line_to((wsign, top));
        sign.line_to((wsign + b.w + 0.1 * em, top));
        let stroke = kurbo::Stroke::new(t)
            .with_join(kurbo::Join::Miter)
            .with_caps(kurbo::Cap::Butt);
        let sign = kurbo::stroke(sign.iter(), &stroke, &kurbo::StrokeOpts::default(), 0.005);
        let mut out = Bx {
            path: sign,
            w: wsign + b.w + 0.15 * em,
            asc: top + t / 2.0,
            desc: bottom,
        };
        out.place(&b, wsign + 0.05 * em, 0.0);
        if let Some(i) = index {
            let small = em * 0.55;
            let ib = self.node(i, small);
            // Above the short rise, moving the rest right when wide.
            let x = (hook * 0.6 - ib.w).min(0.0);
            if x < 0.0 {
                let dx = -x;
                out.path.apply_affine(Affine::translate((dx, 0.0)));
                out.w += dx;
            }
            let ix = (hook * 0.6 - ib.w).max(0.0);
            let iy = -bottom + 0.55 * h + ib.desc + 0.05 * em;
            out.place(&ib, ix, iy);
            out.asc = out.asc.max(iy + ib.asc);
        }
        out
    }
}

fn class_of(n: &Node) -> Class {
    match n {
        Node::Sym { class, .. } => *class,
        Node::Scripts { base, .. } => class_of(base),
        _ => Class::Ord,
    }
}

/// A formula laid out: its outline in page orientation (Y down, its box's
/// top left at 0, 0) and the commands not known.
pub struct Laid {
    pub path: BezPath,
    pub unknown: Vec<String>,
}

/// Lay out `spec`: the curve, its top left at the origin, Y down.
pub fn layout(spec: &FormulaSpec) -> Laid {
    let em = spec.size_pt.max(1.0) * 25.4 / 72.0;
    let (lines, unknown) = parse(&spec.source);
    let lay = Layout { font: &spec.font };
    let mut all = BezPath::new();
    let mut y = 0.0;
    for (i, line) in lines.iter().enumerate() {
        let b = lay.node(line, em);
        if i > 0 {
            y -= b.asc + 0.35 * em;
        }
        all.extend(Affine::translate((0.0, y)) * b.path);
        y -= b.desc;
    }
    // Y down, the box's corner at the origin.
    let mut path = Affine::scale_non_uniform(1.0, -1.0) * all;
    let bb = path.bounding_box();
    if !path.elements().is_empty() && bb.is_finite() {
        path.apply_affine(Affine::translate((-bb.x0, -bb.y0)));
    }
    Laid { path, unknown }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(src: &str) -> FormulaSpec {
        FormulaSpec {
            source: src.into(),
            ..Default::default()
        }
    }

    /// #3572: Hindi in a formula is one word shaped as text, its vowel
    /// signs with their letters and the spaces between its words kept;
    /// Greek letters and signs stay one at a time.
    #[test]
    fn a_hindi_word_is_one_run_with_its_spaces() {
        let (lines, _) = parse("मेरा नाम = \\alpha + β");
        let Node::Row(items) = &lines[0] else {
            panic!()
        };
        let texts: Vec<&str> = items
            .iter()
            .filter_map(|n| match n {
                Node::Sym { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["मेरा नाम", "=", "α", "+", "β"]);
    }

    /// #3793: a space typed between words and the formula is kept as a
    /// word space; between math symbols spaces still count for nothing.
    #[test]
    fn a_space_after_words_is_kept() {
        let spaces = |src: &str| -> usize {
            let (lines, _) = parse(src);
            let Node::Row(items) = &lines[0] else {
                panic!()
            };
            items.iter().filter(|n| matches!(n, Node::Space(_))).count()
        };
        assert_eq!(spaces(r"\text{Area} \frac{a}{b}"), 1);
        assert_eq!(spaces(r"\text{Area}\frac{a}{b}"), 0);
        assert_eq!(spaces(r"\frac{a}{b} \text{cm}"), 1);
        assert_eq!(spaces("मेरा x"), 1);
        assert_eq!(spaces("a b"), 0, "math ignores spaces");
        assert_eq!(spaces(r"\text{Area} = x"), 0, "the sign brings its own");
        let with = layout(&spec(r"\text{A} x")).path.bounding_box();
        let without = layout(&spec(r"\text{A}x")).path.bounding_box();
        assert!(with.width() > without.width());
    }

    #[test]
    fn the_settings_go_through_the_object_data() {
        let s = FormulaSpec {
            source: "a^2\\\\\nb".into(),
            font: "Liberation Serif".into(),
            size_pt: 18.5,
        };
        assert_eq!(FormulaSpec::from_data(&s.to_data()), Some(s));
        assert_eq!(FormulaSpec::from_data("x"), None);
    }

    #[test]
    fn parsing_knows_scripts_fractions_roots_and_symbols() {
        let (lines, unknown) = parse(r"\frac{a}{b} + x_i^2 - \sqrt[3]{y} \leq \alpha \foo");
        assert_eq!(unknown, vec!["\\foo".to_string()]);
        let Node::Row(items) = &lines[0] else {
            panic!()
        };
        assert!(matches!(items[0], Node::Frac(..)));
        assert!(matches!(
            &items[2],
            Node::Scripts {
                sub: Some(_),
                sup: Some(_),
                ..
            }
        ));
        assert!(matches!(&items[4], Node::Sqrt(_, Some(_))));
        assert!(matches!(&items[5], Node::Sym { text, class: Class::Rel, .. } if text == "≤"));
        assert!(matches!(&items[6], Node::Sym { text, italic: true, .. } if text == "α"));
        // Lines at `\\`, not inside braces.
        assert_eq!(parse(r"a \\ b").0.len(), 2);
        assert_eq!(split_lines(r"\frac{a \\ b}{c}").len(), 1);
    }

    #[test]
    fn a_fraction_stacks_its_parts_and_scripts_rise_and_fall() {
        let x = layout(&spec("x")).path.bounding_box();
        let frac = layout(&spec(r"\frac{x}{y}")).path.bounding_box();
        assert!(frac.height() > 1.8 * x.height(), "{frac:?} {x:?}");
        let sq = layout(&spec("x^2")).path.bounding_box();
        assert!(sq.width() > x.width() * 1.3);
        assert!(sq.height() > x.height() * 1.2);
        // Empty or broken sources still give a result.
        assert!(layout(&spec("")).path.elements().is_empty());
        let _ = layout(&spec(r"\frac{\sqrt"));
        let _ = layout(&spec(r"}\right) ^_ {"));
        let _ = layout(&spec(r"\left( \frac{1}{2} \right."));
    }

    #[test]
    fn a_root_and_a_big_operator_reach_over_what_they_hold() {
        let body = layout(&spec("ab")).path.bounding_box();
        let root = layout(&spec(r"\sqrt{ab}")).path.bounding_box();
        assert!(root.width() > body.width() * 1.4);
        assert!(root.height() > body.height());
        let sum = layout(&spec(r"\sum_{i=1}^{n} i")).path.bounding_box();
        let plain = layout(&spec("i")).path.bounding_box();
        assert!(sum.height() > 3.0 * plain.height(), "{sum:?}");
        // Two lines are taller than one.
        let two = layout(&spec(r"a = b \\ c = d")).path.bounding_box();
        let one = layout(&spec(r"a = b")).path.bounding_box();
        assert!(two.height() > 1.8 * one.height());
        // The top left is at the origin.
        assert!(two.x0.abs() < 1e-9 && two.y0.abs() < 1e-9);
    }
}
