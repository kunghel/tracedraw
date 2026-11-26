//! Reading a VBA project for the Macro Editor's compatibility check and
//! turning it into plain JavaScript a person can read and edit
//! ([`to_javascript`]). The JavaScript runs on the same object model as
//! the macros, after [`readable_prelude`] (VBA's built-in functions under
//! their own names). See docs/behavior/vba.md, "Converting macros".

use crate::ast::*;
use crate::{CompileError, Module, ModuleKind};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// The first line of a script written against the macros' object model:
/// the Scripts panel runs such a script as a macro.
pub const JS_MARKER: &str = "// TraceDraw macro object model";

/// A member a module names (`x.Name`, `.Name` in a With block, or a
/// method called on an object), with the line it is on.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberUse {
    pub line: u32,
    pub name: String,
}

/// What a module holds for the compatibility check.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModuleFacts {
    /// Members named after a dot, in source order.
    pub members: Vec<MemberUse>,
    /// Names the module declares (procedures, variables, constants, type
    /// fields, enum members, parameters), as written.
    pub declared: Vec<String>,
    /// Statements the runtime cannot run: (line, what).
    pub unsupported: Vec<(u32, String)>,
    /// Procedures: (name, first line).
    pub procs: Vec<(String, u32)>,
    /// Windows functions it declares.
    pub declares: Vec<String>,
}

/// Read a module for the compatibility check.
pub fn module_facts(m: &Module) -> Result<ModuleFacts, CompileError> {
    let a = crate::parse(m)?;
    let mut f = ModuleFacts::default();
    for v in &a.vars {
        f.declared.push(v.decl.name.clone());
    }
    for c in &a.consts {
        f.declared.push(c.name.clone());
    }
    for t in &a.types {
        f.declared.push(t.name.clone());
        for fl in &t.fields {
            f.declared.push(fl.name.clone());
        }
    }
    for e in &a.enums {
        f.declared.push(e.name.clone());
        for (n, _) in &e.members {
            f.declared.push(n.clone());
        }
    }
    for (n, _, _) in &a.declares {
        f.declares.push(n.clone());
        f.declared.push(n.clone());
    }
    for p in &a.procs {
        f.declared.push(p.name.clone());
        f.procs.push((p.name.clone(), p.line));
        for q in &p.params {
            f.declared.push(q.name.clone());
        }
        walk_stmts(&p.body, &mut f);
    }
    Ok(f)
}

fn walk_stmts(body: &[Stmt], f: &mut ModuleFacts) {
    for s in body {
        walk_stmt(s, f);
    }
}

fn walk_stmt(s: &Stmt, f: &mut ModuleFacts) {
    let line = s.line;
    let e = |x: &Expr, f: &mut ModuleFacts| walk_expr(x, line, f);
    match &s.kind {
        StmtKind::Dim { vars, .. } => {
            for v in vars {
                f.declared.push(v.name.clone());
            }
        }
        StmtKind::Const(list) => {
            for (n, _, v) in list {
                f.declared.push(n.clone());
                e(v, f);
            }
        }
        StmtKind::Assign { target, value, .. } => {
            e(target, f);
            e(value, f);
        }
        StmtKind::Call(x, args) => {
            e(x, f);
            for a in args.iter().flatten() {
                if let Some(v) = &a.value {
                    e(v, f);
                }
            }
        }
        StmtKind::If {
            branches,
            otherwise,
        } => {
            for (c, b) in branches {
                e(c, f);
                walk_stmts(b, f);
            }
            if let Some(b) = otherwise {
                walk_stmts(b, f);
            }
        }
        StmtKind::Select {
            test,
            cases,
            otherwise,
        } => {
            e(test, f);
            for (items, b) in cases {
                for it in items {
                    match it {
                        CaseItem::Value(v) | CaseItem::Is(_, v) => e(v, f),
                        CaseItem::Range(a, b) => {
                            e(a, f);
                            e(b, f);
                        }
                    }
                }
                walk_stmts(b, f);
            }
            if let Some(b) = otherwise {
                walk_stmts(b, f);
            }
        }
        StmtKind::For {
            from,
            to,
            step,
            body,
            ..
        } => {
            e(from, f);
            e(to, f);
            if let Some(s) = step {
                e(s, f);
            }
            walk_stmts(body, f);
        }
        StmtKind::ForEach { coll, body, .. } => {
            e(coll, f);
            walk_stmts(body, f);
        }
        StmtKind::DoLoop { cond, body } => {
            if let Some((_, c)) = cond {
                e(c, f);
            }
            walk_stmts(body, f);
        }
        StmtKind::While { cond, body } => {
            e(cond, f);
            walk_stmts(body, f);
        }
        StmtKind::With { obj, body } => {
            e(obj, f);
            walk_stmts(body, f);
        }
        StmtKind::MidAssign {
            target,
            start,
            len,
            value,
        } => {
            e(target, f);
            e(start, f);
            if let Some(l) = len {
                e(l, f);
            }
            e(value, f);
        }
        StmtKind::DebugPrint(items) => {
            for (x, _) in items {
                e(x, f);
            }
        }
        StmtKind::Unload(x) | StmtKind::Load(x) => e(x, f),
        StmtKind::File {
            args,
            items,
            targets,
            ..
        } => {
            for x in args.iter().chain(items.iter().map(|(x, _)| x)).chain(targets) {
                e(x, f);
            }
        }
        StmtKind::ReDim { vars, .. } => {
            for (n, _, _) in vars {
                f.declared.push(n.clone());
            }
        }
        StmtKind::Unsupported(what) => f.unsupported.push((line, what.clone())),
        _ => {}
    }
}

fn walk_expr(x: &Expr, line: u32, f: &mut ModuleFacts) {
    match x {
        Expr::Member(b, n) => {
            walk_expr(b, line, f);
            f.members.push(MemberUse {
                line,
                name: n.clone(),
            });
        }
        Expr::WithMember(n) => f.members.push(MemberUse {
            line,
            name: n.clone(),
        }),
        Expr::Bang(b, _) => walk_expr(b, line, f),
        Expr::Call(c, args) => {
            walk_expr(c, line, f);
            for a in args {
                if let Some(v) = &a.value {
                    walk_expr(v, line, f);
                }
            }
        }
        Expr::Unary(_, a) | Expr::Paren(a) | Expr::TypeOfIs(a, _) => walk_expr(a, line, f),
        Expr::Binary(_, a, b) => {
            walk_expr(a, line, f);
            walk_expr(b, line, f);
        }
        _ => {}
    }
}

// ----- Readable JavaScript ---------------------------------------------------

/// What the converter knows of a member of the object model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemberHint {
    /// Its spelling.
    pub name: String,
    /// A method (called), not a property.
    pub method: bool,
    /// Its parameter names, lower case (to place named arguments).
    pub params: Vec<String>,
    /// It raises an error here: why (a `// TODO` note goes above the
    /// statement that uses it).
    pub unavailable: Option<String>,
}

/// What the converter knows of the host.
#[derive(Debug, Clone, Default)]
pub struct JsHints {
    /// Members by lower-case name.
    pub members: HashMap<String, MemberHint>,
    /// Global names (objects, constants) by lower-case name.
    pub globals: HashMap<String, String>,
    /// The member another name of a member stands for.
    pub alias: Option<fn(&str) -> Option<&'static str>>,
}

/// The spelling the readable JavaScript gives a VBA library function.
/// Names JavaScript itself uses get another name.
pub fn builtin_name(lower: &str) -> String {
    const NAMES: &[&str] = &[
        "Abs",
        "AscW",
        "Atn",
        "CallByName",
        "CBool",
        "CByte",
        "CCur",
        "CDate",
        "CDbl",
        "CDec",
        "ChrW",
        "CInt",
        "CLng",
        "CLngLng",
        "CLngPtr",
        "CreateObject",
        "CSng",
        "CStr",
        "CurDir",
        "CVar",
        "CVDate",
        "CVErr",
        "DateAdd",
        "DateDiff",
        "DatePart",
        "DateSerial",
        "DateValue",
        "DeleteSetting",
        "DoEvents",
        "EOF",
        "FileLen",
        "FormatCurrency",
        "FormatDateTime",
        "FormatNumber",
        "FormatPercent",
        "FreeFile",
        "GetAllSettings",
        "GetObject",
        "GetSetting",
        "IIf",
        "InputBox",
        "InStr",
        "InStrRev",
        "IsArray",
        "IsDate",
        "IsEmpty",
        "IsError",
        "IsMissing",
        "IsNull",
        "IsNumeric",
        "IsObject",
        "LBound",
        "LCase",
        "LoadPicture",
        "LOF",
        "LTrim",
        "MonthName",
        "MsgBox",
        "QBColor",
        "RGB",
        "RTrim",
        "SaveSetting",
        "StrComp",
        "StrConv",
        "StrReverse",
        "TimeSerial",
        "TimeValue",
        "TypeName",
        "UBound",
        "UCase",
        "VarType",
        "WeekdayName",
    ];
    match lower {
        "array" => return "VbArray".into(),
        "string" => return "VbString".into(),
        "date" => return "VbDate".into(),
        "error" => return "ErrorText".into(),
        "eval" => return "VbEval".into(),
        _ => {}
    }
    if let Some(n) = NAMES.iter().find(|n| n.eq_ignore_ascii_case(lower)) {
        return (*n).to_string();
    }
    let mut c = lower.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// JavaScript that gives readable converted code VBA's library under the
/// names [`to_javascript`] writes: evaluate it after the runtime.
pub fn readable_prelude() -> String {
    let mut s = String::from(
        "// VBA's library for converted macros.\n\
         function __rv(x) { return Array.isArray(x) ? __v.arr(0, x.map(__rv)) : x; }\n\
         function __rj(x) { return (x && typeof x.items === 'function' && x.dims) ? x.items().map(__rj) : x; }\n",
    );
    for f in crate::builtins::BUILTIN_FUNCTIONS {
        let name = builtin_name(f);
        match *f {
            "ubound" => s.push_str(
                "function UBound(a, d) { return Array.isArray(a) ? a.length - 1 : __b.ubound(a, d); }\n",
            ),
            "lbound" => s.push_str(
                "function LBound(a, d) { return Array.isArray(a) ? 0 : __b.lbound(a, d); }\n",
            ),
            "isarray" => s.push_str(
                "function IsArray(a) { return Array.isArray(a) || __b.isarray(a); }\n",
            ),
            "array" => s.push_str("function VbArray(...a) { return a; }\n"),
            _ => {
                let _ = writeln!(
                    s,
                    "function {name}(...a) {{ return __rj(__b.{f}(...a.map(__rv))); }}"
                );
            }
        }
    }
    s.push_str(
        "function Like(a, b) { return __v.like(a, b, false); }\n\
         function TypeOfIs(x, t) { return String(TypeName(x)).toLowerCase() === String(t).toLowerCase(); }\n\
         const Debug = { Print(...items) { __v.dprint(items.map((x) => [x, ''])); } };\n\
         function End() { throw __v.END; }\n\
         const Err = __v.Err;\n\
         const Collection = __v.Collection;\n\
         const Dictionary = __v.Dictionary;\n\
         try { Collection.prototype[Symbol.iterator] = function () { return __v.each(this)[Symbol.iterator](); }; } catch (e) {}\n\
         try { Object.getPrototypeOf(new Dictionary())[Symbol.iterator] = function () { return __v.each(this)[Symbol.iterator](); }; } catch (e) {}\n",
    );
    s
}

/// The JavaScript that runs procedure `name` of converted code.
pub fn readable_entry(name: &str) -> String {
    format!("__v.run(function(){{ return {name}(); }})")
}

/// The VBA constants the runtime defines, as JavaScript literals, by lower
/// case name.
fn constant_values() -> HashMap<String, String> {
    let mut out = HashMap::new();
    let src = crate::RUNTIME_JS;
    let Some(start) = src.find("const C = {") else {
        return out;
    };
    let body = &src[start + "const C = {".len()..];
    let Some(end) = body.find("\n};") else {
        return out;
    };
    let body = &body[..end];
    // key: value pairs; values are numbers or simple string literals.
    let mut rest = body;
    while let Some(colon) = rest.find(':') {
        let key = rest[..colon]
            .rsplit(|c: char| c == ',' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_string();
        let after = rest[colon + 1..].trim_start();
        let (value, used) = if let Some(s) = after.strip_prefix('"') {
            let mut end = 0;
            let mut esc = false;
            for (i, c) in s.char_indices() {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' {
                    end = i;
                    break;
                }
            }
            (format!("\"{}\"", &s[..end]), end + 2)
        } else {
            let end = after.find([',', '\n']).unwrap_or(after.len());
            (after[..end].trim().to_string(), end)
        };
        if !key.is_empty() {
            out.insert(key, value);
        }
        let consumed = rest.len() - after.len() + used;
        rest = rest.get(consumed.max(colon + 1)..).unwrap_or("");
    }
    out
}

const JS_RESERVED: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "print",
    "Debug",
    "result",
    "it",
    "sel",
    "Array",
    "String",
    "Date",
    "Object",
    "Number",
    "Math",
    "Error",
    "JSON",
    "Map",
    "Set",
    "Symbol",
    "Boolean",
    "Function",
];

fn safe_name(n: &str) -> String {
    if JS_RESERVED.contains(&n) {
        format!("{n}_")
    } else {
        n.to_string()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Var,
    Array,
    Object,
    Field,
}

struct ProcRef {
    js: String,
    params: Vec<String>,
}

struct Ctx<'a> {
    hints: &'a JsHints,
    out: String,
    indent: usize,
    /// Scopes from the procedure out: lower name -> (js, kind).
    locals: Vec<HashMap<String, (String, Kind)>>,
    module_names: HashMap<String, (String, Kind)>,
    /// Every module's public names.
    globals: HashMap<String, (String, Kind)>,
    procs: HashMap<String, ProcRef>,
    /// Procedures of the module being written (preferred).
    own_procs: HashMap<String, ProcRef>,
    forms: HashSet<String>,
    classes: HashMap<String, String>,
    types: HashMap<String, String>,
    /// The function being written: its lower name (assignments to it set
    /// `result`).
    func: Option<String>,
    in_class: bool,
    with_depth: usize,
    with_names: Vec<String>,
    sel_depth: usize,
    constants: HashMap<String, String>,
    used_constants: Vec<(String, String)>,
    comments: HashMap<u32, Vec<String>>,
    last_line: u32,
    todo: usize,
    /// Notes for the statement being written (members not available).
    pending: Vec<String>,
    /// The indent of a procedure's body (Dim statements deeper in are
    /// declared at its top: VBA's variables belong to the procedure).
    proc_indent: usize,
}

/// Comment lines of a module, by line (1-based), without the quote.
fn comment_lines(src: &str) -> HashMap<u32, Vec<String>> {
    let mut out: HashMap<u32, Vec<String>> = HashMap::new();
    for (i, l) in src.lines().enumerate() {
        let t = l.trim();
        let body = t.strip_prefix('\'').or_else(|| {
            t.get(..4)
                .filter(|h| h.eq_ignore_ascii_case("rem "))
                .map(|_| &t[4..])
        });
        if let Some(c) = body {
            out.entry(i as u32 + 1)
                .or_default()
                .push(c.trim_end().to_string());
        }
    }
    out
}

fn default_of(ty: &Option<TypeRef>, ctx: &Ctx) -> String {
    let Some(t) = ty else {
        return "undefined".into();
    };
    match t.key().as_str() {
        "integer" | "long" | "single" | "double" | "currency" | "byte" | "longlong" | "longptr"
        | "decimal" | "date" => "0".into(),
        "string" => "\"\"".into(),
        "boolean" => "false".into(),
        "variant" => "undefined".into(),
        k => match ctx.types.get(k) {
            Some(n) => format!("{n}()"),
            None => "null".into(),
        },
    }
}

fn is_value_type(ty: &Option<TypeRef>) -> bool {
    match ty {
        None => true,
        Some(t) => matches!(
            t.key().as_str(),
            "integer"
                | "long"
                | "single"
                | "double"
                | "currency"
                | "byte"
                | "longlong"
                | "longptr"
                | "decimal"
                | "date"
                | "string"
                | "boolean"
                | "variant"
        ),
    }
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// JavaScript operator precedence of what an expression is written as.
fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Binary(op, _, _) => match op {
            BinOp::Pow => 14,
            BinOp::Mul | BinOp::Div | BinOp::Mod => 13,
            BinOp::Add | BinOp::Sub | BinOp::Cat => 12,
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => 10,
            BinOp::Eq | BinOp::Ne | BinOp::Is => 9,
            BinOp::And => 5,
            BinOp::Or => 4,
            _ => 20,
        },
        Expr::Unary(_, _) => 15,
        _ => 20,
    }
}

fn stringy(e: &Expr) -> bool {
    match e {
        Expr::Str(_) => true,
        Expr::Binary(BinOp::Cat, _, _) => true,
        Expr::Call(c, _) => matches!(&**c, Expr::Ident(n) if {
            let l = n.to_ascii_lowercase();
            matches!(l.as_str(), "left" | "right" | "mid" | "trim" | "ltrim" | "rtrim" | "ucase" | "lcase" | "cstr" | "format" | "replace" | "chr" | "space" | "string" | "join" | "str" | "hex" | "oct")
        }),
        _ => false,
    }
}

impl<'a> Ctx<'a> {
    fn line(&mut self, s: &str) {
        for note in std::mem::take(&mut self.pending) {
            self.todo(&note);
        }
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn todo(&mut self, what: &str) {
        self.todo += 1;
        let l = format!("// TODO: {what}");
        self.line(&l);
    }

    /// The VBA comments from the last statement written up to `line`.
    fn comments_before(&mut self, line: u32) {
        if line <= self.last_line {
            return;
        }
        for l in self.last_line + 1..line {
            if let Some(cs) = self.comments.get(&l).cloned() {
                for c in cs {
                    let t = format!("//{}{}", if c.starts_with(' ') { "" } else { " " }, c);
                    self.line(&t);
                }
            }
        }
        self.last_line = line;
    }

    fn lookup(&self, name: &str) -> Option<(String, Kind)> {
        let k = name.to_ascii_lowercase();
        for s in self.locals.iter().rev() {
            if let Some(v) = s.get(&k) {
                return Some(v.clone());
            }
        }
        if let Some(v) = self.module_names.get(&k) {
            return Some(v.clone());
        }
        self.globals.get(&k).cloned()
    }

    fn proc_ref(&self, name: &str) -> Option<&ProcRef> {
        let k = name.to_ascii_lowercase();
        self.own_procs.get(&k).or_else(|| self.procs.get(&k))
    }

    fn member_name(&mut self, n: &str) -> (String, Option<&'a MemberHint>) {
        let k = n.to_ascii_lowercase();
        let hints = self.hints;
        if let Some(h) = hints.members.get(&k) {
            if let Some(why) = &h.unavailable {
                let note = if why.is_empty() {
                    format!("{} is not available in TraceDraw", h.name)
                } else {
                    format!("{} is not available in TraceDraw: {why}", h.name)
                };
                if !self.pending.contains(&note) {
                    self.pending.push(note);
                }
            }
            return (h.name.clone(), Some(h));
        }
        if let Some(a) = self.hints.alias.and_then(|f| f(n)) {
            let h = hints.members.get(&a.to_ascii_lowercase());
            return (a.to_string(), h);
        }
        (n.to_string(), None)
    }

    fn with_obj(&self) -> String {
        self.with_names
            .last()
            .cloned()
            .unwrap_or_else(|| "it".into())
    }

    fn ident(&mut self, n: &str, as_value: bool) -> String {
        let k = n.to_ascii_lowercase();
        if self.func.as_deref() == Some(k.as_str()) {
            return "result".into();
        }
        if let Some((js, _)) = self.lookup(n) {
            return js;
        }
        if let Some(p) = self.proc_ref(n) {
            let js = p.js.clone();
            return if as_value { format!("{js}()") } else { js };
        }
        if let Some(g) = self.hints.globals.get(&k) {
            return g.clone();
        }
        if k == "err" {
            return "Err".into();
        }
        if crate::builtins::is_function(&k) {
            let b = builtin_name(&k);
            return if as_value { format!("{b}()") } else { b };
        }
        if crate::builtins::is_constant(&k) {
            if let Some(v) = self.constants.get(&k) {
                if !self
                    .used_constants
                    .iter()
                    .any(|(c, _)| c.eq_ignore_ascii_case(n))
                {
                    self.used_constants.push((n.to_string(), v.clone()));
                }
            }
            return self
                .used_constants
                .iter()
                .find(|(c, _)| c.eq_ignore_ascii_case(n))
                .map(|(c, _)| c.clone())
                .unwrap_or_else(|| n.to_string());
        }
        safe_name(n)
    }

    fn args(&mut self, args: &[Arg], params: Option<&[String]>) -> String {
        let mut pos: Vec<String> = Vec::new();
        let mut named: Vec<(String, String)> = Vec::new();
        for a in args {
            let v = match &a.value {
                Some(v) => self.expr(v),
                None => "undefined".into(),
            };
            match &a.name {
                Some(n) => named.push((n.to_ascii_lowercase(), v)),
                None => pos.push(v),
            }
        }
        if !named.is_empty() {
            match params {
                Some(ps) => {
                    for (n, v) in named {
                        match ps.iter().position(|p| p.eq_ignore_ascii_case(&n)) {
                            Some(i) => {
                                while pos.len() <= i {
                                    pos.push("undefined".into());
                                }
                                pos[i] = v;
                            }
                            None => pos.push(format!("/* {n}:= */ {v}")),
                        }
                    }
                }
                None => {
                    for (n, v) in named {
                        pos.push(format!("/* {n}:= */ {v}"));
                    }
                }
            }
        }
        while pos.last().is_some_and(|p| p == "undefined") {
            pos.pop();
        }
        pos.join(", ")
    }

    fn base(&mut self, b: &Expr) -> String {
        let s = self.expr(b);
        match b {
            Expr::Binary(..) | Expr::Unary(..) => format!("({s})"),
            _ => s,
        }
    }

    /// `callee(args)`.
    fn call(&mut self, callee: &Expr, args: &[Arg]) -> String {
        match callee {
            Expr::Ident(n) => {
                let k = n.to_ascii_lowercase();
                if let Some((js, kind)) = self.lookup(n) {
                    return match kind {
                        Kind::Array | Kind::Var | Kind::Field => {
                            let idx: Vec<String> = args
                                .iter()
                                .map(|a| a.value.as_ref().map(|v| self.expr(v)).unwrap_or_default())
                                .collect();
                            format!("{js}[{}]", idx.join("]["))
                        }
                        Kind::Object => format!("{js}.Item({})", self.args(args, None)),
                    };
                }
                if let Some(p) = self.proc_ref(n) {
                    let (js, params) = (p.js.clone(), p.params.clone());
                    return format!("{js}({})", self.args(args, Some(&params)));
                }
                if self.hints.globals.contains_key(&k) {
                    let g = self.ident(n, false);
                    let method = self.hints.members.get(&k).is_some_and(|h| h.method);
                    return if method {
                        format!("{g}({})", self.args(args, None))
                    } else {
                        format!("{g}.Item({})", self.args(args, None))
                    };
                }
                if crate::builtins::is_function(&k) {
                    match k.as_str() {
                        "array" => {
                            return format!("[{}]", self.args(args, None));
                        }
                        "iif" if args.len() == 3 && args.iter().all(|a| a.name.is_none()) => {
                            let v: Vec<String> = args
                                .iter()
                                .map(|a| {
                                    a.value
                                        .as_ref()
                                        .map(|v| self.expr(v))
                                        .unwrap_or_else(|| "undefined".into())
                                })
                                .collect();
                            return format!("({} ? {} : {})", v[0], v[1], v[2]);
                        }
                        "ismissing" if args.len() == 1 => {
                            let v = args[0]
                                .value
                                .as_ref()
                                .map(|v| self.expr(v))
                                .unwrap_or_default();
                            return format!("({v} === undefined)");
                        }
                        _ => {}
                    }
                    return format!("{}({})", builtin_name(&k), self.args(args, None));
                }
                format!("{}({})", self.ident(n, false), self.args(args, None))
            }
            Expr::Member(b, n) => {
                let base = self.base(b);
                self.member_call(base, n, args)
            }
            Expr::WithMember(n) => {
                let base = self.with_obj();
                self.member_call(base, n, args)
            }
            other => {
                let c = self.expr(other);
                format!("{c}.Item({})", self.args(args, None))
            }
        }
    }

    fn member_call(&mut self, base: String, n: &str, args: &[Arg]) -> String {
        let (name, hint) = self.member_name(n);
        let hint = hint.cloned();
        match hint {
            Some(h) if !h.method => format!("{base}.{name}.Item({})", self.args(args, None)),
            Some(h) => format!("{base}.{name}({})", self.args(args, Some(&h.params))),
            None => format!("{base}.{name}({})", self.args(args, None)),
        }
    }

    fn member(&mut self, base: String, n: &str) -> String {
        let (name, hint) = self.member_name(n);
        match hint {
            Some(h) if h.method => format!("{base}.{name}()"),
            _ => format!("{base}.{name}"),
        }
    }

    fn sub(&mut self, e: &Expr, parent: u8, right: bool) -> String {
        let s = self.expr(e);
        let p = prec(e);
        if p < parent || (right && p == parent) {
            format!("({s})")
        } else {
            s
        }
    }

    fn expr(&mut self, e: &Expr) -> String {
        match e {
            Expr::Num(v) => num(*v),
            Expr::Str(s) => crate::js_string(s),
            Expr::Date(v) => format!("{} /* date */", num(*v)),
            Expr::Bool(b) => b.to_string(),
            Expr::Empty => "undefined".into(),
            Expr::Null | Expr::Nothing => "null".into(),
            Expr::Me => "this".into(),
            Expr::Ident(n) => self.ident(n, true),
            Expr::Member(b, n) => {
                let base = self.base(b);
                self.member(base, n)
            }
            Expr::WithMember(n) => {
                let base = self.with_obj();
                self.member(base, n)
            }
            Expr::Bang(b, n) => {
                let base = self.base(b);
                format!("{base}.Item({})", crate::js_string(n))
            }
            Expr::Call(c, args) => self.call(c, args),
            Expr::New(t) => {
                let k = t.rsplit('.').next().unwrap_or(t).to_ascii_lowercase();
                match k.as_str() {
                    "collection" => "new Collection()".into(),
                    "dictionary" => "new Dictionary()".into(),
                    _ => match self.classes.get(&k) {
                        Some(c) => format!("new {c}()"),
                        None => format!("new {}()", safe_name(t)),
                    },
                }
            }
            Expr::Unary(UnOp::Neg, a) => format!("-{}", self.sub(a, 15, false)),
            Expr::Unary(UnOp::Not, a) => format!("!{}", self.sub(a, 15, false)),
            Expr::Binary(op, a, b) => {
                let p = prec(e);
                let js_op = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Pow => "**",
                    BinOp::Cat => "+",
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Le => "<=",
                    BinOp::Ge => ">=",
                    BinOp::Is => "===",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                    BinOp::IntDiv => {
                        let (x, y) = (self.sub(a, 13, false), self.sub(b, 13, true));
                        return format!("Math.trunc({x} / {y})");
                    }
                    BinOp::Like => {
                        let (x, y) = (self.expr(a), self.expr(b));
                        return format!("Like({x}, {y})");
                    }
                    BinOp::Xor => {
                        let (x, y) = (self.expr(a), self.expr(b));
                        return format!("(Boolean({x}) !== Boolean({y}))");
                    }
                    BinOp::Eqv => {
                        let (x, y) = (self.expr(a), self.expr(b));
                        return format!("(Boolean({x}) === Boolean({y}))");
                    }
                    BinOp::Imp => {
                        let (x, y) = (self.sub(a, 15, false), self.sub(b, 4, true));
                        return format!("(!{x} || {y})");
                    }
                };
                let right_assoc = matches!(op, BinOp::Pow);
                let x = self.sub(a, p, right_assoc);
                let y = self.sub(b, p, !right_assoc);
                if *op == BinOp::Cat && !stringy(a) && !stringy(b) {
                    format!("String({x}) + {y}")
                } else {
                    format!("{x} {js_op} {y}")
                }
            }
            Expr::TypeOfIs(a, t) => {
                let x = self.expr(a);
                format!("TypeOfIs({x}, {})", crate::js_string(t))
            }
            Expr::AddressOf(n) => {
                self.todo += 1;
                format!("null /* TODO: AddressOf {n} */")
            }
            Expr::Paren(a) => format!("({})", self.expr(a)),
        }
    }

    fn declare_var(&mut self, v: &VarDecl, keyword: &str) {
        let js = safe_name(&v.name);
        let kind = if v.bounds.is_some() {
            Kind::Array
        } else if !is_value_type(&v.ty)
            && !v
                .ty
                .as_ref()
                .is_some_and(|t| self.types.contains_key(&t.key()))
        {
            Kind::Object
        } else {
            Kind::Var
        };
        let init = if let Some(bounds) = &v.bounds {
            let fill = default_of(&v.ty, self);
            self.array_init(bounds, &fill)
        } else if v.is_new {
            let t = v.ty.as_ref().map(|t| t.name.clone()).unwrap_or_default();
            self.expr(&Expr::New(t))
        } else {
            default_of(&v.ty, self)
        };
        if let Some(s) = self.locals.last_mut() {
            s.insert(v.name.to_ascii_lowercase(), (js.clone(), kind));
        }
        let l = if init == "undefined" {
            format!("{keyword} {js};")
        } else {
            format!("{keyword} {js} = {init};")
        };
        self.line(&l);
    }

    fn array_init(&mut self, bounds: &[Bound], fill: &str) -> String {
        if bounds.is_empty() {
            return "[]".into();
        }
        let len = |c: &mut Ctx, b: &Bound| match &b.upper {
            Expr::Num(v) => num(v + 1.0),
            e => format!("{} + 1", c.sub(e, 12, false)),
        };
        let last = bounds.len() - 1;
        let mut s = format!("new Array({}).fill({fill})", len(self, &bounds[last]));
        for b in bounds[..last].iter().rev() {
            s = format!("Array.from({{ length: {} }}, () => {s})", len(self, b));
        }
        s
    }

    fn block(&mut self, body: &[Stmt]) {
        self.indent += 1;
        for s in body {
            self.stmt(s);
        }
        self.indent -= 1;
    }

    fn stmt(&mut self, s: &Stmt) {
        self.comments_before(s.line);
        match &s.kind {
            StmtKind::Dim { .. } if self.indent > self.proc_indent => {}
            StmtKind::Dim { vars, is_static } => {
                if *is_static {
                    self.todo("Static: in VBA these keep their values between calls");
                }
                for v in vars {
                    self.declare_var(v, "let");
                }
            }
            StmtKind::Const(list) => {
                for (n, _, v) in list {
                    let js = safe_name(n);
                    let val = self.expr(v);
                    if let Some(sc) = self.locals.last_mut() {
                        sc.insert(n.to_ascii_lowercase(), (js.clone(), Kind::Var));
                    }
                    let l = format!("const {js} = {val};");
                    self.line(&l);
                }
            }
            StmtKind::Assign { target, value, .. } => {
                let v = self.expr(value);
                match self.target(target) {
                    Some(t) => {
                        let l = format!("{t} = {v};");
                        self.line(&l);
                    }
                    None => {
                        self.todo("an indexed property cannot be set like this in JavaScript");
                        let l = format!("// {} = {v};", self.expr(target));
                        self.line(&l);
                    }
                }
            }
            StmtKind::Call(x, args) => {
                let l = match args {
                    Some(a) => self.call(x, a),
                    None => match x {
                        Expr::Ident(n) => {
                            let i = self.ident(n, false);
                            if i.ends_with(')') {
                                i
                            } else {
                                format!("{i}()")
                            }
                        }
                        Expr::Member(b, n) => {
                            let base = self.base(b);
                            let (name, _) = self.member_name(n);
                            format!("{base}.{name}()")
                        }
                        Expr::WithMember(n) => {
                            let base = self.with_obj();
                            let (name, _) = self.member_name(n);
                            format!("{base}.{name}()")
                        }
                        other => self.expr(other),
                    },
                };
                self.form_todo(x);
                let l = format!("{l};");
                self.line(&l);
            }
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (i, (c, b)) in branches.iter().enumerate() {
                    let c = self.expr(c);
                    let l = if i == 0 {
                        format!("if ({c}) {{")
                    } else {
                        format!("}} else if ({c}) {{")
                    };
                    self.line(&l);
                    self.block(b);
                }
                if let Some(b) = otherwise {
                    self.line("} else {");
                    self.block(b);
                }
                self.line("}");
            }
            StmtKind::Select {
                test,
                cases,
                otherwise,
            } => {
                self.sel_depth += 1;
                let sel = if self.sel_depth == 1 {
                    "sel".to_string()
                } else {
                    format!("sel{}", self.sel_depth)
                };
                let t = self.expr(test);
                self.line("{");
                self.indent += 1;
                let l = format!("const {sel} = {t};");
                self.line(&l);
                let mut first = true;
                for (items, b) in cases {
                    let conds: Vec<String> = items
                        .iter()
                        .map(|it| match it {
                            CaseItem::Value(v) => format!("{sel} == {}", self.sub(v, 9, true)),
                            CaseItem::Range(a, b) => format!(
                                "{sel} >= {} && {sel} <= {}",
                                self.sub(a, 10, true),
                                self.sub(b, 10, true)
                            ),
                            CaseItem::Is(op, v) => {
                                let o = match op {
                                    BinOp::Lt => "<",
                                    BinOp::Gt => ">",
                                    BinOp::Le => "<=",
                                    BinOp::Ge => ">=",
                                    BinOp::Ne => "!=",
                                    _ => "==",
                                };
                                format!("{sel} {o} {}", self.sub(v, 10, true))
                            }
                        })
                        .collect();
                    let c = if conds.len() == 1 {
                        conds.join("")
                    } else {
                        conds
                            .iter()
                            .map(|c| {
                                if c.contains("&&") {
                                    format!("({c})")
                                } else {
                                    c.clone()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" || ")
                    };
                    let l = if first {
                        format!("if ({c}) {{")
                    } else {
                        format!("}} else if ({c}) {{")
                    };
                    first = false;
                    self.line(&l);
                    self.block(b);
                }
                if let Some(b) = otherwise {
                    if first {
                        self.line("{");
                    } else {
                        self.line("} else {");
                    }
                    self.block(b);
                    self.line("}");
                } else if !first {
                    self.line("}");
                }
                self.indent -= 1;
                self.line("}");
                self.sel_depth -= 1;
            }
            StmtKind::For {
                var,
                from,
                to,
                step,
                body,
            } => {
                let v = self.ident(var, false);
                let f = self.expr(from);
                let t = self.expr(to);
                let (cmp, inc) = match step {
                    None => ("<=".to_string(), format!("{v}++")),
                    Some(Expr::Num(n)) if *n == 1.0 => ("<=".into(), format!("{v}++")),
                    Some(Expr::Num(n)) if *n == -1.0 => (">=".into(), format!("{v}--")),
                    Some(Expr::Num(n)) if *n < 0.0 => (">=".into(), format!("{v} -= {}", num(-n))),
                    Some(Expr::Unary(UnOp::Neg, n)) => {
                        let n = self.expr(n);
                        (">=".into(), format!("{v} -= {n}"))
                    }
                    Some(s) => {
                        let s = self.expr(s);
                        ("<=".into(), format!("{v} += {s}"))
                    }
                };
                let l = format!("for ({v} = {f}; {v} {cmp} {t}; {inc}) {{");
                self.line(&l);
                self.block(body);
                self.line("}");
            }
            StmtKind::ForEach { var, coll, body } => {
                let v = self.ident(var, false);
                let c = self.expr(coll);
                let l = format!("for ({v} of {c}) {{");
                self.line(&l);
                self.block(body);
                self.line("}");
            }
            StmtKind::DoLoop { cond, body } => match cond {
                None => {
                    self.line("while (true) {");
                    self.block(body);
                    self.line("}");
                }
                Some((LoopCond::PreWhile, c)) => {
                    let c = self.expr(c);
                    let l = format!("while ({c}) {{");
                    self.line(&l);
                    self.block(body);
                    self.line("}");
                }
                Some((LoopCond::PreUntil, c)) => {
                    let c = self.sub(c, 15, false);
                    let l = format!("while (!{c}) {{");
                    self.line(&l);
                    self.block(body);
                    self.line("}");
                }
                Some((LoopCond::PostWhile, c)) => {
                    self.line("do {");
                    self.block(body);
                    let c = self.expr(c);
                    let l = format!("}} while ({c});");
                    self.line(&l);
                }
                Some((LoopCond::PostUntil, c)) => {
                    self.line("do {");
                    self.block(body);
                    let c = self.sub(c, 15, false);
                    let l = format!("}} while (!{c});");
                    self.line(&l);
                }
            },
            StmtKind::While { cond, body } => {
                let c = self.expr(cond);
                let l = format!("while ({c}) {{");
                self.line(&l);
                self.block(body);
                self.line("}");
            }
            StmtKind::With { obj, body } => {
                let o = self.expr(obj);
                self.with_depth += 1;
                let name = if self.with_depth == 1 {
                    "it".to_string()
                } else {
                    format!("it{}", self.with_depth)
                };
                self.line("{");
                self.indent += 1;
                let l = format!("const {name} = {o};");
                self.line(&l);
                self.with_names.push(name);
                for s in body {
                    self.stmt(s);
                }
                self.with_names.pop();
                self.indent -= 1;
                self.line("}");
                self.with_depth -= 1;
            }
            StmtKind::Exit(k) => match k {
                ExitKind::For | ExitKind::Do => self.line("break;"),
                ExitKind::Sub => self.line("return;"),
                ExitKind::Function | ExitKind::Property => self.line("return result;"),
            },
            StmtKind::GoTo(l) => {
                let t = format!("GoTo {l}: JavaScript has no GoTo");
                self.todo(&t);
            }
            StmtKind::Label(l) => {
                let t = format!("// {l}:");
                self.line(&t);
            }
            StmtKind::OnErrorResumeNext => {
                self.todo("On Error Resume Next: JavaScript stops at an error (use try/catch)")
            }
            StmtKind::OnErrorGoTo(l) => {
                let t = format!("On Error GoTo {l}: use try/catch");
                self.todo(&t);
            }
            StmtKind::OnErrorGoTo0 | StmtKind::OnErrorGoToMinus1 => {}
            StmtKind::Resume(_) => self.todo("Resume: use try/catch"),
            StmtKind::ReDim { preserve, vars } => {
                for (n, bounds, ty) in vars {
                    let js = self.ident(n, false);
                    if *preserve {
                        if let Some(b) = bounds.last() {
                            let u = self.sub(&b.upper, 12, false);
                            let l = format!("{js}.length = {u} + 1;");
                            self.line(&l);
                        }
                    } else {
                        let fill = default_of(ty, self);
                        let init = self.array_init(bounds, &fill);
                        let l = format!("{js} = {init};");
                        self.line(&l);
                    }
                }
            }
            StmtKind::Erase(names) => {
                for n in names {
                    let js = self.ident(n, false);
                    let l = format!("{js} = [];");
                    self.line(&l);
                }
            }
            StmtKind::MidAssign { .. } => self.todo("Mid statement: rebuild the string"),
            StmtKind::DebugPrint(items) => {
                let v: Vec<String> = items.iter().map(|(e, _)| self.expr(e)).collect();
                let l = format!("Debug.Print({});", v.join(", "));
                self.line(&l);
            }
            StmtKind::End => self.line("End();"),
            StmtKind::Nop => {}
            StmtKind::RaiseEvent(n, _) => {
                let t = format!("RaiseEvent {n}: events are not converted");
                self.todo(&t);
            }
            StmtKind::Unload(_) | StmtKind::Load(_) => {
                self.todo("Load/Unload: forms stay in the VBA project")
            }
            StmtKind::File { op, .. } => {
                let t = format!("{} statement: files are read and written by the VBA macro", op.word());
                self.todo(&t);
            }
            StmtKind::Unsupported(w) => {
                let t = format!("{w} is not available in TraceDraw");
                self.todo(&t);
            }
        }
    }

    fn form_todo(&mut self, x: &Expr) {
        let mut root = x;
        while let Expr::Member(b, _) | Expr::Call(b, _) | Expr::Bang(b, _) = root {
            root = b;
        }
        if let Expr::Ident(n) = root {
            if self.forms.contains(&n.to_ascii_lowercase()) {
                let t = format!("form {n} stays in the VBA project: forms are not converted");
                self.todo(&t);
            }
        }
    }

    /// An assignment target, or None when JavaScript cannot set it.
    fn target(&mut self, t: &Expr) -> Option<String> {
        match t {
            Expr::Ident(n) => Some(self.ident(n, false)),
            Expr::Member(b, n) => {
                let base = self.base(b);
                let (name, _) = self.member_name(n);
                Some(format!("{base}.{name}"))
            }
            Expr::WithMember(n) => {
                let base = self.with_obj();
                let (name, _) = self.member_name(n);
                Some(format!("{base}.{name}"))
            }
            Expr::Call(c, args) => match &**c {
                Expr::Ident(n)
                    if matches!(
                        self.lookup(n).map(|(_, k)| k),
                        Some(Kind::Array) | Some(Kind::Var) | Some(Kind::Field)
                    ) || (self.lookup(n).is_none() && self.proc_ref(n).is_none()) =>
                {
                    let js = self.ident(n, false);
                    let idx: Vec<String> = args
                        .iter()
                        .map(|a| a.value.as_ref().map(|v| self.expr(v)).unwrap_or_default())
                        .collect();
                    Some(format!("{js}[{}]", idx.join("][")))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Names a procedure assigns without declaring them (no Option
    /// Explicit): they get a `let`.
    fn undeclared(&self, all: &[Stmt], body: &[Stmt], out: &mut Vec<String>) {
        for s in body {
            let check = |n: &str, out: &mut Vec<String>| {
                let k = n.to_ascii_lowercase();
                if self.func.as_deref() != Some(k.as_str())
                    && self.lookup(n).is_none()
                    && self.proc_ref(n).is_none()
                    && !self.hints.globals.contains_key(&k)
                    && !out.iter().any(|o: &String| o.eq_ignore_ascii_case(n))
                    && !declared_in(all, n)
                {
                    out.push(n.to_string());
                }
            };
            match &s.kind {
                StmtKind::Assign {
                    target: Expr::Ident(n),
                    ..
                } => check(n, out),
                StmtKind::For { var, body, .. } | StmtKind::ForEach { var, body, .. } => {
                    check(var, out);
                    self.undeclared(all, body, out);
                }
                StmtKind::If {
                    branches,
                    otherwise,
                } => {
                    for (_, b) in branches {
                        self.undeclared(all, b, out);
                    }
                    if let Some(b) = otherwise {
                        self.undeclared(all, b, out);
                    }
                }
                StmtKind::Select {
                    cases, otherwise, ..
                } => {
                    for (_, b) in cases {
                        self.undeclared(all, b, out);
                    }
                    if let Some(b) = otherwise {
                        self.undeclared(all, b, out);
                    }
                }
                StmtKind::DoLoop { body, .. }
                | StmtKind::While { body, .. }
                | StmtKind::With { body, .. } => self.undeclared(all, body, out),
                _ => {}
            }
        }
    }

    fn procedure(&mut self, p: &Proc, js_name: &str, method: bool) {
        self.comments_before(p.line);
        let mut scope = HashMap::new();
        let mut params = Vec::new();
        let mut byref_set = Vec::new();
        for q in &p.params {
            let js = safe_name(&q.name);
            let kind = if q.is_array {
                Kind::Array
            } else if is_value_type(&q.ty) {
                Kind::Var
            } else {
                Kind::Object
            };
            scope.insert(q.name.to_ascii_lowercase(), (js.clone(), kind));
            let mut s = if q.param_array {
                format!("...{js}")
            } else {
                js.clone()
            };
            if let Some(d) = &q.default {
                let d = self.expr(d);
                s = format!("{s} = {d}");
            }
            if !q.by_val && !q.param_array && assigns(&p.body, &q.name) {
                byref_set.push(q.name.clone());
            }
            params.push(s);
        }
        self.locals.push(scope);
        let is_func = matches!(p.kind, ProcKind::Function | ProcKind::PropertyGet);
        self.func = is_func.then(|| p.name.to_ascii_lowercase());
        let head = match (method, p.kind) {
            (true, ProcKind::PropertyGet) if p.params.is_empty() => format!("get {js_name}() {{"),
            (true, ProcKind::PropertyLet | ProcKind::PropertySet) if p.params.len() == 1 => {
                format!("set {js_name}({}) {{", params.join(", "))
            }
            (true, _) => format!("{js_name}({}) {{", params.join(", ")),
            (false, _) => format!("function {js_name}({}) {{", params.join(", ")),
        };
        self.line(&head);
        self.indent += 1;
        for n in &byref_set {
            let t = format!(
                "\"{n}\" is ByRef in VBA: what this procedure sets in it does not reach the caller"
            );
            self.todo(&t);
        }
        if is_func {
            let d = default_of(&p.ret, self);
            let l = if d == "undefined" {
                "let result;".to_string()
            } else {
                format!("let result = {d};")
            };
            self.line(&l);
        }
        self.proc_indent = self.indent;
        let mut nested = Vec::new();
        nested_dims(&p.body, false, &mut nested);
        for v in &nested {
            self.declare_var(v, "let");
        }
        let mut und = Vec::new();
        self.undeclared(&p.body, &p.body, &mut und);
        for n in und {
            let js = safe_name(&n);
            if let Some(s) = self.locals.last_mut() {
                s.insert(n.to_ascii_lowercase(), (js.clone(), Kind::Var));
            }
            let l = format!("let {js};");
            self.line(&l);
        }
        for s in &p.body {
            self.stmt(s);
        }
        if is_func {
            self.line("return result;");
        }
        self.indent -= 1;
        self.line("}");
        self.locals.pop();
        self.func = None;
    }
}

/// The variables Dim declares inside blocks of a procedure.
fn nested_dims(body: &[Stmt], deep: bool, out: &mut Vec<VarDecl>) {
    for s in body {
        match &s.kind {
            StmtKind::Dim { vars, .. } if deep => out.extend(vars.iter().cloned()),
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (_, b) in branches {
                    nested_dims(b, true, out);
                }
                if let Some(b) = otherwise {
                    nested_dims(b, true, out);
                }
            }
            StmtKind::Select {
                cases, otherwise, ..
            } => {
                for (_, b) in cases {
                    nested_dims(b, true, out);
                }
                if let Some(b) = otherwise {
                    nested_dims(b, true, out);
                }
            }
            StmtKind::For { body, .. }
            | StmtKind::ForEach { body, .. }
            | StmtKind::DoLoop { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::With { body, .. } => nested_dims(body, true, out),
            _ => {}
        }
    }
}

fn declared_in(body: &[Stmt], n: &str) -> bool {
    body.iter().any(|s| match &s.kind {
        StmtKind::Dim { vars, .. } => vars.iter().any(|v| v.name.eq_ignore_ascii_case(n)),
        StmtKind::Const(l) => l.iter().any(|(c, _, _)| c.eq_ignore_ascii_case(n)),
        StmtKind::If {
            branches,
            otherwise,
        } => {
            branches.iter().any(|(_, b)| declared_in(b, n))
                || otherwise.as_ref().is_some_and(|b| declared_in(b, n))
        }
        StmtKind::Select {
            cases, otherwise, ..
        } => {
            cases.iter().any(|(_, b)| declared_in(b, n))
                || otherwise.as_ref().is_some_and(|b| declared_in(b, n))
        }
        StmtKind::For { body, .. }
        | StmtKind::ForEach { body, .. }
        | StmtKind::DoLoop { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::With { body, .. } => declared_in(body, n),
        _ => false,
    })
}

/// Whether `body` assigns to `name` (directly).
fn assigns(body: &[Stmt], name: &str) -> bool {
    body.iter().any(|s| match &s.kind {
        StmtKind::Assign {
            target: Expr::Ident(n),
            ..
        } => n.eq_ignore_ascii_case(name),
        StmtKind::If {
            branches,
            otherwise,
        } => {
            branches.iter().any(|(_, b)| assigns(b, name))
                || otherwise.as_ref().is_some_and(|b| assigns(b, name))
        }
        StmtKind::Select {
            cases, otherwise, ..
        } => {
            cases.iter().any(|(_, b)| assigns(b, name))
                || otherwise.as_ref().is_some_and(|b| assigns(b, name))
        }
        StmtKind::For { var, body, .. } => var.eq_ignore_ascii_case(name) || assigns(body, name),
        StmtKind::ForEach { var, body, .. } => {
            var.eq_ignore_ascii_case(name) || assigns(body, name)
        }
        StmtKind::DoLoop { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::With { body, .. } => assigns(body, name),
        _ => false,
    })
}

/// The result of [`to_javascript`].
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub js: String,
    /// `// TODO` notes written into it.
    pub todos: usize,
    /// Each macro (public Sub without parameters) and the JavaScript
    /// function that runs it: (module, macro, function).
    pub entries: Vec<(String, String, String)>,
}

/// Write a project as plain JavaScript on the macros' object model. What
/// JavaScript cannot do the same way (forms, GoTo, error handlers, Windows
/// functions, ByRef parameters a procedure changes) becomes a `// TODO`
/// note.
pub fn to_javascript(
    modules: &[Module],
    project: &str,
    hints: &JsHints,
) -> Result<Converted, CompileError> {
    let mut asts = Vec::new();
    for m in modules {
        asts.push(crate::parse(m)?);
    }
    let mut ctx = Ctx {
        hints,
        out: String::new(),
        indent: 0,
        locals: Vec::new(),
        module_names: HashMap::new(),
        globals: HashMap::new(),
        procs: HashMap::new(),
        own_procs: HashMap::new(),
        forms: HashSet::new(),
        classes: HashMap::new(),
        types: HashMap::new(),
        func: None,
        in_class: false,
        with_depth: 0,
        with_names: Vec::new(),
        sel_depth: 0,
        constants: constant_values(),
        used_constants: Vec::new(),
        comments: HashMap::new(),
        last_line: 0,
        todo: 0,
        proc_indent: 0,
        pending: Vec::new(),
    };
    // Names every module sees, and each procedure's JavaScript name (the
    // module's name first when two modules use the same one).
    let mut seen: HashSet<String> = HashSet::new();
    let mut proc_js: Vec<Vec<String>> = Vec::new();
    for (m, a) in modules.iter().zip(&asts) {
        match m.kind {
            ModuleKind::Form => {
                ctx.forms.insert(m.name.to_ascii_lowercase());
            }
            ModuleKind::Class => {
                ctx.classes
                    .insert(m.name.to_ascii_lowercase(), safe_name(&m.name));
            }
            _ => {}
        }
        for t in &a.types {
            ctx.types
                .insert(t.name.to_ascii_lowercase(), safe_name(&t.name));
        }
        let mut names = Vec::new();
        for p in &a.procs {
            let k = p.name.to_ascii_lowercase();
            let js = if matches!(m.kind, ModuleKind::Standard | ModuleKind::Document) {
                if seen.insert(k.clone()) {
                    safe_name(&p.name)
                } else {
                    format!("{}_{}", safe_name(&m.name), p.name)
                }
            } else {
                safe_name(&p.name)
            };
            if matches!(m.kind, ModuleKind::Standard | ModuleKind::Document)
                && p.is_public
                && !ctx.procs.contains_key(&k)
            {
                ctx.procs.insert(
                    k,
                    ProcRef {
                        js: js.clone(),
                        params: p
                            .params
                            .iter()
                            .map(|q| q.name.to_ascii_lowercase())
                            .collect(),
                    },
                );
            }
            names.push(js);
        }
        proc_js.push(names);
        if matches!(m.kind, ModuleKind::Standard | ModuleKind::Document) {
            for v in a.vars.iter().filter(|v| v.is_public) {
                let kind = if v.decl.bounds.is_some() {
                    Kind::Array
                } else {
                    Kind::Var
                };
                ctx.globals.insert(
                    v.decl.name.to_ascii_lowercase(),
                    (safe_name(&v.decl.name), kind),
                );
            }
            for c in a.consts.iter().filter(|c| c.is_public) {
                ctx.globals
                    .insert(c.name.to_ascii_lowercase(), (safe_name(&c.name), Kind::Var));
            }
        }
        for e in &a.enums {
            for (n, _) in &e.members {
                ctx.globals
                    .insert(n.to_ascii_lowercase(), (safe_name(n), Kind::Var));
            }
        }
    }
    let mut body = String::new();
    let mut entries = Vec::new();
    for ((m, a), js_names) in modules.iter().zip(&asts).zip(&proc_js) {
        ctx.out.clear();
        ctx.comments = comment_lines(&m.source);
        ctx.last_line = 0;
        ctx.module_names.clear();
        ctx.own_procs.clear();
        let header = format!("// ----- {} -----", m.name);
        ctx.line(&header);
        if m.kind == ModuleKind::Form {
            ctx.todo(&format!(
                "form {} stays in the VBA project: forms and their events are not converted",
                m.name
            ));
            ctx.line("");
            body.push_str(&ctx.out);
            continue;
        }
        for (p, js) in a.procs.iter().zip(js_names) {
            ctx.own_procs.insert(
                p.name.to_ascii_lowercase(),
                ProcRef {
                    js: if m.kind == ModuleKind::Class {
                        format!("this.{js}")
                    } else {
                        js.clone()
                    },
                    params: p
                        .params
                        .iter()
                        .map(|q| q.name.to_ascii_lowercase())
                        .collect(),
                },
            );
        }
        for (n, _, _) in &a.declares {
            ctx.todo(&format!(
                "Declare {n}: Windows functions do not exist in JavaScript"
            ));
        }
        for t in &a.types {
            let fields: Vec<String> = t
                .fields
                .iter()
                .map(|f| format!("{}: {}", safe_name(&f.name), default_of(&f.ty, &ctx)))
                .collect();
            let l = format!(
                "function {}() {{ return {{ {} }}; }}",
                safe_name(&t.name),
                fields.join(", ")
            );
            ctx.line(&l);
        }
        for e in &a.enums {
            let mut next = 0f64;
            for (n, v) in &e.members {
                let val = match v {
                    Some(Expr::Num(x)) => {
                        next = *x + 1.0;
                        num(*x)
                    }
                    Some(x) => ctx.expr(x),
                    None => {
                        let s = num(next);
                        next += 1.0;
                        s
                    }
                };
                let l = format!("const {} = {val};", safe_name(n));
                ctx.line(&l);
            }
        }
        let class = m.kind == ModuleKind::Class;
        ctx.in_class = class;
        ctx.locals.push(HashMap::new());
        for c in &a.consts {
            let js = safe_name(&c.name);
            let v = ctx.expr(&c.value);
            ctx.module_names
                .insert(c.name.to_ascii_lowercase(), (js.clone(), Kind::Var));
            if !class {
                let l = format!("const {js} = {v};");
                ctx.line(&l);
            } else {
                let l = format!("const {}_{js} = {v};", safe_name(&m.name));
                ctx.module_names.insert(
                    c.name.to_ascii_lowercase(),
                    (format!("{}_{js}", safe_name(&m.name)), Kind::Var),
                );
                ctx.line(&l);
            }
        }
        if class {
            let l = format!("class {} {{", safe_name(&m.name));
            ctx.line(&l);
            ctx.indent += 1;
            ctx.line("constructor() {");
            ctx.indent += 1;
            for v in &a.vars {
                let js = safe_name(&v.decl.name);
                let init = if let Some(b) = &v.decl.bounds {
                    let fill = default_of(&v.decl.ty, &ctx);
                    ctx.array_init(b, &fill)
                } else {
                    default_of(&v.decl.ty, &ctx)
                };
                let kind = if v.decl.bounds.is_some() {
                    Kind::Array
                } else {
                    Kind::Field
                };
                ctx.module_names.insert(
                    v.decl.name.to_ascii_lowercase(),
                    (format!("this.{js}"), kind),
                );
                let l = format!("this.{js} = {init};");
                ctx.line(&l);
            }
            if a.procs
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case("Class_Initialize"))
            {
                ctx.line("this.Class_Initialize();");
            }
            ctx.indent -= 1;
            ctx.line("}");
        } else {
            for v in &a.vars {
                let kind = if v.decl.bounds.is_some() {
                    Kind::Array
                } else {
                    Kind::Var
                };
                ctx.module_names.insert(
                    v.decl.name.to_ascii_lowercase(),
                    (safe_name(&v.decl.name), kind),
                );
                ctx.declare_var(&v.decl, "let");
            }
        }
        // Properties of a class: a getter and setter pair keep their names.
        for (p, js) in a.procs.iter().zip(js_names) {
            ctx.procedure(p, js, class);
            if !class
                && p.is_public
                && p.kind == ProcKind::Sub
                && p.params.is_empty()
                && !a.option_private
            {
                entries.push((m.name.clone(), p.name.clone(), js.clone()));
            }
        }
        if class {
            ctx.indent -= 1;
            ctx.line("}");
        }
        ctx.locals.pop();
        ctx.in_class = false;
        // Comments after the last statement.
        let end = m.source.lines().count() as u32 + 1;
        ctx.comments_before(end);
        ctx.line("");
        body.push_str(&ctx.out);
    }
    let mut js = String::new();
    let _ = writeln!(js, "{JS_MARKER}");
    let _ = writeln!(
        js,
        "// Converted from the VBA project \"{}\". Lengths are in ActiveDocument.Unit.",
        project.replace('\n', " ")
    );
    if let Some((_, _, f)) = entries.first() {
        let _ = writeln!(js, "// Run: {f}");
    }
    if !ctx.used_constants.is_empty() {
        js.push('\n');
        for (n, v) in &ctx.used_constants {
            let _ = writeln!(js, "const {n} = {v};");
        }
    }
    js.push('\n');
    js.push_str(&body);
    while js.ends_with("\n\n") {
        js.pop();
    }
    Ok(Converted {
        js,
        todos: ctx.todo,
        entries,
    })
}

/// The function a script names on a `// Run: Name` line, else the first
/// it defines without parameters.
pub fn js_run_name(js: &str) -> Option<String> {
    for l in js.lines().take(20) {
        if let Some(n) = l.trim().strip_prefix("// Run:") {
            let n = n.trim();
            if !n.is_empty() {
                return Some(n.to_string());
            }
        }
    }
    js_entries(js).into_iter().next()
}

/// The functions a converted script defines that a person can run: those
/// declared `function Name()` with no parameters at the start of a line.
pub fn js_entries(js: &str) -> Vec<String> {
    js.lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("function ")?;
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            let after = rest[name.len()..].trim_start();
            (!name.is_empty() && after.starts_with("()")).then_some(name)
        })
        .collect()
}

/// Make the loops of a script check, at each pass, whether the user
/// stopped it (`__chk`).
pub fn with_loop_checks(js: &str) -> String {
    let mut out = String::with_capacity(js.len() + 64);
    for l in js.lines() {
        out.push_str(l);
        let t = l.trim_start();
        if (t.starts_with("for ")
            || t.starts_with("for(")
            || t.starts_with("while ")
            || t.starts_with("while(")
            || t == "do {")
            && t.ends_with('{')
        {
            out.push_str(" __chk();");
        }
        out.push('\n');
    }
    out
}
