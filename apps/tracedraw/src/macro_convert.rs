//! Converting macros in the Macro Editor: the compatibility check (each
//! member and statement a project uses, whether it works here, does
//! nothing, or is not available), the automatic fixes, and the project
//! written as plain JavaScript that runs on the same object model. See
//! docs/behavior/vba.md, "Converting macros".

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::scripting::vg_catalog::{alias_of, catalog, known_words, Status};
use std::collections::HashSet;
use tracedraw_vba as vba;
use tracedraw_vba::convert as cv;
use tracedraw_vba_project as vp;

/// How well something works here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Runs as it is.
    Ok,
    /// Runs, but something does nothing or may not exist here.
    Warn,
    /// Something raises an error here.
    Error,
}

/// What can be done about an issue automatically.
#[derive(Debug, Clone, PartialEq)]
pub enum Fix {
    None,
    /// Another name of a member: write the member's name.
    Rename {
        from: String,
        to: String,
    },
    /// Comment the statement out with a TODO note.
    CommentOut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub module: String,
    /// 1-based; 0 for the module as a whole.
    pub line: u32,
    pub level: Level,
    pub text: String,
    pub fix: Fix,
}

/// A macro (or other procedure) and its worst issue.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroRow {
    pub module: String,
    pub name: String,
    pub line: u32,
    pub level: Level,
    pub issues: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub issues: Vec<Issue>,
    pub macros: Vec<MacroRow>,
}

impl Report {
    pub fn count(&self, level: Level) -> usize {
        self.macros.iter().filter(|m| m.level == level).count()
    }
}

fn vba_module(m: &vp::Module) -> vba::Module {
    let kind = match m.kind {
        vp::ModuleKind::Standard => vba::ModuleKind::Standard,
        vp::ModuleKind::Class => vba::ModuleKind::Class,
        vp::ModuleKind::Form => vba::ModuleKind::Form,
        vp::ModuleKind::Document => vba::ModuleKind::Document,
    };
    vba::Module::new(m.name.clone(), kind, m.source.clone())
}

/// Windows functions the runtime gives (see vba.md, "Not supported").
fn supported_declare(name: &str, line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    let n = name.to_ascii_lowercase();
    [
        "shellexecute",
        "urldownloadtofile",
        "urldownloadtocachefile",
    ]
    .iter()
    .any(|s| n.starts_with(s) || l.contains(&format!("\"{s}")))
}

/// Check a project.
pub fn check(p: &vp::Project) -> Report {
    let cat = catalog();
    let words = known_words();
    let mut report = Report::default();
    let mut facts = Vec::new();
    let mut declared: HashSet<String> = HashSet::new();
    for f in &p.forms {
        declared.insert(f.name.to_ascii_lowercase());
        for c in f.all_controls() {
            declared.insert(c.name.to_ascii_lowercase());
        }
    }
    for m in &p.modules {
        declared.insert(m.name.to_ascii_lowercase());
        if !m.has_source {
            report.issues.push(Issue {
                module: m.name.clone(),
                line: 0,
                level: Level::Error,
                text: tr("convert.no_source"),
                fix: Fix::None,
            });
            continue;
        }
        match cv::module_facts(&vba_module(m)) {
            Ok(f) => {
                for d in &f.declared {
                    declared.insert(d.to_ascii_lowercase());
                }
                facts.push((m, f));
            }
            Err(e) => report.issues.push(Issue {
                module: m.name.clone(),
                line: e.line,
                level: Level::Error,
                text: trf("convert.compile_error", &[("e", &e.message)]),
                fix: Fix::None,
            }),
        }
    }
    for (m, f) in &facts {
        let lines: Vec<&str> = m.source.lines().collect();
        let mut push = |line: u32, level: Level, text: String, fix: Fix| {
            if !report
                .issues
                .iter()
                .any(|i| i.module == m.name && i.line == line && i.text == text)
            {
                report.issues.push(Issue {
                    module: m.name.clone(),
                    line,
                    level,
                    text,
                    fix,
                });
            }
        };
        for u in &f.members {
            let k = u.name.to_ascii_lowercase();
            match cat.get(&k) {
                Some(c) => match &c.status {
                    Status::Works => {}
                    Status::NoEffect => push(
                        u.line,
                        Level::Warn,
                        trf("convert.no_effect", &[("m", &c.name)]),
                        Fix::None,
                    ),
                    Status::Unavailable(why) => {
                        let text = if why.is_empty() {
                            trf("convert.unavailable", &[("m", &c.name)])
                        } else {
                            trf("convert.unavailable_why", &[("m", &c.name), ("why", why)])
                        };
                        push(u.line, Level::Error, text, Fix::CommentOut)
                    }
                },
                None => {
                    if let Some(to) = alias_of(&u.name) {
                        push(
                            u.line,
                            Level::Warn,
                            trf("convert.other_name", &[("m", &u.name), ("to", to)]),
                            Fix::Rename {
                                from: u.name.clone(),
                                to: to.to_string(),
                            },
                        );
                    } else if !declared.contains(&k) && !words.contains(&k) {
                        push(
                            u.line,
                            Level::Warn,
                            trf("convert.unknown", &[("m", &u.name)]),
                            Fix::None,
                        );
                    }
                }
            }
        }
        for (line, what) in &f.unsupported {
            push(
                *line,
                Level::Error,
                trf("convert.statement", &[("s", what)]),
                Fix::CommentOut,
            );
        }
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim_start();
            if t.starts_with('\'') {
                continue;
            }
            let low = t.to_ascii_lowercase();
            let decl = low
                .strip_prefix("private ")
                .or_else(|| low.strip_prefix("public "))
                .unwrap_or(&low);
            if decl.starts_with("declare ") {
                let name = f
                    .declares
                    .iter()
                    .find(|d| low.contains(&d.to_ascii_lowercase()))
                    .cloned()
                    .unwrap_or_default();
                if !supported_declare(&name, t) {
                    push(
                        i as u32 + 1,
                        Level::Error,
                        trf("convert.declare", &[("f", &name)]),
                        Fix::None,
                    );
                }
            }
            let mut rest = low.as_str();
            while let Some(at) = rest.find("createobject(\"") {
                let after = &rest[at + 14..];
                let name = after.split('"').next().unwrap_or("");
                if name != "scripting.dictionary" && name != "vba.collection" {
                    push(
                        i as u32 + 1,
                        Level::Error,
                        trf("convert.object", &[("o", name)]),
                        Fix::None,
                    );
                }
                rest = after;
            }
            if low.starts_with("shell ") || low.contains("shell(") {
                push(i as u32 + 1, Level::Error, tr("convert.shell"), Fix::None);
            }
        }
        // Each procedure and the issues in its lines.
        let mut procs = f.procs.clone();
        procs.sort_by_key(|(_, l)| *l);
        let macros: HashSet<String> = vba::list_macros(&[vba_module(m)])
            .into_iter()
            .map(|(_, n)| n.to_ascii_lowercase())
            .collect();
        for (i, (name, start)) in procs.iter().enumerate() {
            let end = procs.get(i + 1).map(|(_, l)| *l).unwrap_or(u32::MAX);
            let mine: Vec<&Issue> = report
                .issues
                .iter()
                .filter(|x| x.module == m.name && x.line >= *start && x.line < end)
                .collect();
            if !macros.contains(&name.to_ascii_lowercase()) && mine.is_empty() {
                continue;
            }
            report.macros.push(MacroRow {
                module: m.name.clone(),
                name: name.clone(),
                line: *start,
                level: mine.iter().map(|x| x.level).max().unwrap_or(Level::Ok),
                issues: mine.len(),
            });
        }
    }
    report
        .issues
        .sort_by(|a, b| (a.module.as_str(), a.line).cmp(&(b.module.as_str(), b.line)));
    report
}

/// Whether a line is a statement on its own that can be commented out
/// without breaking a block.
fn simple_statement(lines: &[String], i: usize) -> bool {
    let Some(l) = lines.get(i) else {
        return false;
    };
    if l.trim_end().ends_with(" _") || (i > 0 && lines[i - 1].trim_end().ends_with(" _")) {
        return false;
    }
    // The line without its strings and comment.
    let mut code = String::new();
    let mut in_str = false;
    for c in l.trim_start().chars() {
        match c {
            '"' => in_str = !in_str,
            '\'' if !in_str => break,
            c if !in_str => code.push(c.to_ascii_lowercase()),
            _ => {}
        }
    }
    let low = code;
    let first = low.split_whitespace().next().unwrap_or("");
    !matches!(
        first,
        "if" | "elseif"
            | "else"
            | "end"
            | "select"
            | "case"
            | "for"
            | "next"
            | "do"
            | "loop"
            | "while"
            | "wend"
            | "with"
            | "sub"
            | "function"
            | "property"
            | "private"
            | "public"
            | "dim"
            | "static"
            | "set"
    ) && !low.contains(" then ")
        && !low.contains(':')
}

/// Replace whole-word `.from` with `.to` in a line.
fn rename_member(line: &str, from: &str, to: &str) -> String {
    let low = line.to_ascii_lowercase();
    let pat = format!(".{}", from.to_ascii_lowercase());
    let mut out = String::new();
    let mut last = 0;
    let mut at = 0;
    while let Some(i) = low[at..].find(&pat) {
        let s = at + i;
        let e = s + pat.len();
        let boundary = low[e..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if boundary {
            out.push_str(&line[last..s]);
            out.push('.');
            out.push_str(to);
            last = e;
        }
        at = e;
    }
    out.push_str(&line[last..]);
    out
}

/// What [`fix_project`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fixed {
    pub renamed: usize,
    pub commented: usize,
    pub notes: usize,
}

/// Apply the automatic fixes of `report` to the project's code: other
/// names of members become the member's name; a statement that raises an
/// error here is commented out with a `' TODO` note when it stands on its
/// own line, and gets a note above it otherwise. The code still compiles
/// afterwards (otherwise only the notes are added).
pub fn fix_project(p: &mut vp::Project, report: &Report) -> Fixed {
    let mut done = Fixed::default();
    for m in p.modules.iter_mut().filter(|m| m.has_source) {
        let issues: Vec<&Issue> = report
            .issues
            .iter()
            .filter(|i| i.module.eq_ignore_ascii_case(&m.name) && i.line > 0)
            .collect();
        if issues.is_empty() {
            continue;
        }
        let nl = if m.source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let original: Vec<String> = m.source.lines().map(str::to_string).collect();
        let attempt = |comment_out: bool, done: &mut Fixed| -> Vec<String> {
            let mut lines = original.clone();
            // Notes above lines, applied from the bottom so numbers hold.
            let mut notes: Vec<(usize, String)> = Vec::new();
            for is in &issues {
                let i = is.line as usize - 1;
                if i >= lines.len() {
                    continue;
                }
                match &is.fix {
                    Fix::Rename { from, to } => {
                        let new = rename_member(&lines[i], from, to);
                        if new != lines[i] {
                            lines[i] = new;
                            done.renamed += 1;
                        }
                    }
                    Fix::CommentOut | Fix::None if is.level == Level::Error => {
                        let indent: String =
                            lines[i].chars().take_while(|c| c.is_whitespace()).collect();
                        let note = format!("{indent}' TODO: {}", is.text);
                        let already = i > 0 && lines[i - 1].trim() == note.trim();
                        if is.fix == Fix::CommentOut
                            && comment_out
                            && simple_statement(&original, i)
                            && !lines[i].trim_start().starts_with('\'')
                        {
                            lines[i] = format!(
                                "{indent}' TODO: {}: {}",
                                is.text.trim_end_matches('.'),
                                lines[i].trim()
                            );
                            done.commented += 1;
                        } else if !already && !notes.iter().any(|(j, n)| *j == i && *n == note) {
                            notes.push((i, note));
                        }
                    }
                    _ => {}
                }
            }
            notes.sort_by_key(|n| std::cmp::Reverse(n.0));
            for (i, n) in notes {
                lines.insert(i, n);
                done.notes += 1;
            }
            lines
        };
        let mut d = Fixed::default();
        let mut lines = attempt(true, &mut d);
        let mut trial = vba_module(m);
        trial.source = lines.join(nl);
        if vba::compile(&[trial]).is_err() {
            d = Fixed::default();
            lines = attempt(false, &mut d);
        }
        done.renamed += d.renamed;
        done.commented += d.commented;
        done.notes += d.notes;
        let mut s = lines.join(nl);
        if m.source.ends_with('\n') {
            s.push_str(nl);
        }
        m.source = s;
    }
    done
}

/// What the converter knows of the object model.
pub fn hints() -> cv::JsHints {
    let mut h = cv::JsHints::default();
    for (k, m) in catalog() {
        h.members.insert(
            k.clone(),
            cv::MemberHint {
                name: m.name.clone(),
                method: m.method,
                params: m.params.clone(),
                unavailable: match &m.status {
                    Status::Unavailable(why) => Some(why.clone()),
                    _ => None,
                },
            },
        );
    }
    for g in crate::scripting::vg_global_names() {
        h.globals.insert(g.to_ascii_lowercase(), g);
    }
    h.alias = Some(alias_of);
    h
}

/// The project as plain JavaScript.
pub fn to_javascript(p: &vp::Project) -> Result<cv::Converted, vba::CompileError> {
    let modules: Vec<vba::Module> = p
        .modules
        .iter()
        .filter(|m| m.has_source)
        .map(vba_module)
        .collect();
    cv::to_javascript(&modules, &p.name, &hints())
}

/// Everything converted JavaScript needs before it: VBA's library under
/// its own names, and `for ... of` over the object model's collections.
pub fn js_program(js: &str) -> String {
    format!(
        "{}\n{}\n{}",
        cv::readable_prelude(),
        crate::scripting::vg_catalog::iterable_js(),
        cv::with_loop_checks(js)
    )
}

impl App {
    /// Run function `name` of JavaScript written against the macros'
    /// object model, as a macro (messages, output, one undo step).
    /// `label` names the run ("Scripts" sends errors to the Scripts
    /// panel's output).
    pub fn run_macro_js(&mut self, js: &str, name: &str, label: String) {
        if self.macros_desktop_only() {
            return;
        }
        if self.macros.run.is_some() {
            self.status = tr("macros.already_running");
            return;
        }
        let program = js_program(js);
        let entry = cv::readable_entry(name);
        match crate::scripting::vba_run::start(self, &program, &entry, Vec::new(), label) {
            Ok(run) => {
                self.macros.run = Some(run);
                self.macros.applied = None;
            }
            Err(e) => self.status = trf("macros.load_failed", &[("e", &e)]),
        }
    }

    /// Check the editor's project and show the report.
    pub fn check_macro_editor(&mut self) {
        if self.macros.editor.as_ref().is_some_and(|e| e.dirty) {
            self.save_macro_editor();
        }
        let Some(ed) = &self.macros.editor else {
            return;
        };
        let Some(p) = self.macros.projects.get(ed.project) else {
            return;
        };
        let report = check(&p.project);
        if let Some(ed) = &mut self.macros.editor {
            ed.report = Some(report);
            ed.show_report = true;
        }
    }

    /// Apply the automatic fixes to the editor's project, save it and
    /// check it again.
    pub fn fix_macro_editor(&mut self) {
        if self.macros.editor.as_ref().is_some_and(|e| e.dirty) {
            self.save_macro_editor();
        }
        let Some(ed) = &self.macros.editor else {
            return;
        };
        let (pi, module) = (ed.project, ed.module.clone());
        let Some(p) = self.macros.projects.get_mut(pi) else {
            return;
        };
        let report = check(&p.project);
        let done = fix_project(&mut p.project, &report);
        p.warnings = crate::macros::scan(&p.project);
        let saved = p.save();
        let source = p
            .project
            .module(&module)
            .map(|m| m.source.clone())
            .unwrap_or_default();
        let report = check(&p.project);
        if let Some(ed) = &mut self.macros.editor {
            ed.source = source;
            ed.dirty = false;
            ed.report = Some(report);
            ed.show_report = true;
            ed.message = Some(match saved {
                Err(e) => trf("macros.save_failed", &[("e", &e)]),
                Ok(()) => trf(
                    "convert.fixed",
                    &[
                        ("r", &done.renamed.to_string()),
                        ("c", &done.commented.to_string()),
                        ("n", &done.notes.to_string()),
                    ],
                ),
            });
        }
    }

    /// Write the editor's project as JavaScript in its JavaScript tab.
    pub fn convert_macro_editor(&mut self) {
        if self.macros.editor.as_ref().is_some_and(|e| e.dirty) {
            self.save_macro_editor();
        }
        let Some(ed) = &self.macros.editor else {
            return;
        };
        let Some(p) = self.macros.projects.get(ed.project) else {
            return;
        };
        let result = to_javascript(&p.project);
        if let Some(ed) = &mut self.macros.editor {
            match result {
                Ok(c) => {
                    ed.js_run = c
                        .entries
                        .first()
                        .map(|(_, _, f)| f.clone())
                        .unwrap_or_default();
                    ed.js = c.js;
                    ed.message = Some(trf("convert.converted", &[("n", &c.todos.to_string())]));
                }
                Err(e) => {
                    ed.message = Some(trf(
                        "macros.compile_error_at",
                        &[
                            ("m", &e.module),
                            ("l", &e.line.to_string()),
                            ("e", &e.message),
                        ],
                    ))
                }
            }
            ed.js_tab = true;
        }
    }

    /// Put the editor's JavaScript in the Scripts panel.
    pub fn macro_js_to_scripts(&mut self) {
        let Some(ed) = &self.macros.editor else {
            return;
        };
        let mut js = ed.js.clone();
        if !ed.js_run.is_empty() {
            js = set_run_line(&js, &ed.js_run);
        }
        if self.macros.scripts_vba {
            std::mem::swap(&mut self.script_source, &mut self.macros.other_source);
            self.macros.scripts_vba = false;
        }
        self.script_source = js;
        self.open_docker(crate::app::DockerTab::Scripts);
    }

    /// Save the editor's JavaScript as a `.js` file.
    pub fn save_macro_js(&mut self) {
        let Some(ed) = &self.macros.editor else {
            return;
        };
        let js = set_run_line(&ed.js, &ed.js_run);
        let name = self
            .macros
            .projects
            .get(ed.project)
            .map(|p| p.name().to_string())
            .unwrap_or_else(|| "macro".into());
        crate::files::Dialog::new()
            .add_filter("JavaScript", &["js"])
            .set_file_name(format!("{name}.js"))
            .save_bytes(self, js);
    }
}

/// The script with its `// Run:` line naming `name`.
pub fn set_run_line(js: &str, name: &str) -> String {
    if name.is_empty() {
        return js.to_string();
    }
    let mut found = false;
    let mut out: Vec<String> = js
        .lines()
        .map(|l| {
            if !found && l.trim().starts_with("// Run:") {
                found = true;
                format!("// Run: {name}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        let at = usize::from(out.first().is_some_and(|l| l.starts_with(cv::JS_MARKER)));
        out.insert(at, format!("// Run: {name}"));
    }
    let mut s = out.join("\n");
    if js.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
#[path = "macro_convert_tests.rs"]
mod tests;
