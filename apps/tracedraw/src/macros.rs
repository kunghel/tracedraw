//! VBA macro projects in the app: loading `.gms` files and exported
//! modules, keeping a copy of each in the settings folder (`macros/`) so
//! they load again at start-up, the warnings a project gets before it
//! runs, and running a macro (`scripting::vba_run`) with what the app does
//! with its messages, forms, changes and errors. See docs/behavior/vba.md.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::scripting::vba_run::{Changes, FormEvent, Input, MacroRun, Request, RunError, Snapshot};
use std::path::{Path, PathBuf};
use tracedraw_vba as vba;
use tracedraw_vba_project as vp;

/// A loaded macro project.
#[derive(Debug, Clone)]
pub struct MacroProject {
    /// Its copy in the settings folder.
    pub file: PathBuf,
    pub project: vp::Project,
    /// What may not work here, found by reading its code.
    pub warnings: Vec<String>,
}

fn vba_kind(k: vp::ModuleKind) -> vba::ModuleKind {
    match k {
        vp::ModuleKind::Standard => vba::ModuleKind::Standard,
        vp::ModuleKind::Class => vba::ModuleKind::Class,
        vp::ModuleKind::Form => vba::ModuleKind::Form,
        vp::ModuleKind::Document => vba::ModuleKind::Document,
    }
}

impl MacroProject {
    pub fn new(file: PathBuf, project: vp::Project) -> MacroProject {
        let warnings = scan(&project);
        MacroProject {
            file,
            project,
            warnings,
        }
    }

    pub fn name(&self) -> &str {
        &self.project.name
    }

    /// The modules for the compiler (those with code).
    pub fn modules(&self) -> Vec<vba::Module> {
        self.project
            .modules
            .iter()
            .filter(|m| m.has_source)
            .map(|m| vba::Module::new(m.name.clone(), vba_kind(m.kind), m.source.clone()))
            .collect()
    }

    /// The macros it offers: (module, macro).
    pub fn macros(&self) -> Vec<(String, String)> {
        vba::list_macros(&self.modules())
    }

    /// What the compiler knows of the host and the forms.
    pub fn options(&self) -> vba::Options {
        vba::Options {
            host_names: crate::scripting::vg_global_names(),
            form_controls: self
                .project
                .forms
                .iter()
                .map(|f| {
                    (
                        f.name.clone(),
                        f.all_controls().iter().map(|c| c.name.clone()).collect(),
                    )
                })
                .collect(),
            keep_going: false,
        }
    }

    pub fn compile(&self) -> Result<vba::Program, vba::CompileError> {
        vba::compile_with(&self.modules(), &self.options())
    }

    /// [`Self::compile`] for a run: keeping going past what TraceDraw
    /// cannot do, or not.
    pub fn compile_for_run(&self, keep_going: bool) -> Result<vba::Program, vba::CompileError> {
        let mut o = self.options();
        o.keep_going = keep_going;
        vba::compile_with(&self.modules(), &o)
    }

    /// Write the project to its copy (the code as it is now).
    pub fn save(&self) -> Result<(), String> {
        let bytes = vp::write_gms(&self.project).map_err(|e| e.to_string())?;
        std::fs::write(&self.file, bytes).map_err(|e| e.to_string())
    }
}

/// What may not work in a project, read from its code: Windows functions
/// (`Declare`), file statements, objects of other programs, modules saved
/// without code, a protected project.
pub fn scan(p: &vp::Project) -> Vec<String> {
    let mut out = Vec::new();
    let mut declares = Vec::new();
    let mut objects = Vec::new();
    let mut shell = false;
    for m in &p.modules {
        for line in m.source.lines() {
            let l = line.trim();
            if l.starts_with('\'') {
                continue;
            }
            let low = l.to_ascii_lowercase();
            let decl = low
                .strip_prefix("private ")
                .or_else(|| low.strip_prefix("public "))
                .unwrap_or(&low);
            if decl.starts_with("declare ") {
                let name = l
                    .split_whitespace()
                    .skip_while(|w| {
                        !w.eq_ignore_ascii_case("sub") && !w.eq_ignore_ascii_case("function")
                    })
                    .nth(1)
                    .unwrap_or("")
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>();
                if !name.is_empty() && !declares.contains(&name) {
                    declares.push(name);
                }
                continue;
            }
            let starts = |w: &str| low.starts_with(w);
            if low.contains("shell(") || starts("shell ") {
                shell = true;
            }
            let mut rest = low.as_str();
            while let Some(i) = rest.find("createobject(\"") {
                let after = &rest[i + 14..];
                let name = after.split('"').next().unwrap_or("");
                let known = [
                    "scripting.dictionary",
                    "scripting.filesystemobject",
                    "wscript.shell",
                    "wscript.network",
                    "vba.collection",
                ];
                if !known.iter().any(|k| name.eq_ignore_ascii_case(k))
                    && !objects
                        .iter()
                        .any(|o: &String| o.eq_ignore_ascii_case(name))
                {
                    // The name as written in the code.
                    let at = l.to_ascii_lowercase().find(name).unwrap_or(0);
                    objects.push(l.get(at..at + name.len()).unwrap_or(name).to_string());
                }
                rest = after;
            }
        }
    }
    if p.protected {
        out.push(tr("macros.warn_protected"));
    }
    let without = p.modules_without_source();
    if !without.is_empty() {
        out.push(trf("macros.warn_no_source", &[("m", &without.join(", "))]));
    }
    if !declares.is_empty() {
        out.push(trf("macros.warn_declare", &[("f", &declares.join(", "))]));
    }
    if shell {
        out.push(tr("macros.warn_shell"));
    }
    if !objects.is_empty() {
        out.push(trf("macros.warn_objects", &[("o", &objects.join(", "))]));
    }
    for w in &p.warnings {
        out.push(w.clone());
    }
    out
}

/// Where loaded projects are kept.
pub fn default_dir() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("macros"))
}

/// Read a project from files the user chose: one `.gms` (or any file with
/// a VBA project inside), or module files (`.bas`, `.cls`, `.frm` with
/// their `.frx`).
pub fn read_project(paths: &[PathBuf]) -> Result<vp::Project, String> {
    let is_module = |p: &Path| {
        p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            ["bas", "cls", "frm", "frx"].contains(&e.to_ascii_lowercase().as_str())
        })
    };
    if paths.iter().all(|p| is_module(p)) {
        let mut files = Vec::new();
        for p in paths {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            files.push((name, crate::files::read(p).map_err(|e| e.to_string())?));
        }
        let name = paths
            .iter()
            .find(|p| !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("frx")))
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Macros".into());
        return vp::read_named(&name, &files).map_err(|e| e.to_string());
    }
    let first = paths.first().ok_or("no file")?;
    let bytes = crate::files::read(first).map_err(|e| e.to_string())?;
    vp::read_gms(&bytes).map_err(|e| e.to_string())
}

/// The line (1-based) where `Sub name` or `Function name` starts.
pub fn sub_line(source: &str, name: &str) -> Option<u32> {
    source.lines().enumerate().find_map(|(i, l)| {
        (proc_name(l).is_some_and(|n| n.eq_ignore_ascii_case(name))).then_some(i as u32 + 1)
    })
}

/// `source` without the `Sub` or `Function` called `name`: from its
/// first line (with the comment lines right above it) to its `End Sub`
/// or `End Function`. `None` when no procedure has that name.
pub fn without_procedure(source: &str, name: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let start = lines
        .iter()
        .position(|l| proc_name(l).is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
    let end = (start..lines.len())
        .find(|&i| {
            let l = lines[i].trim().to_ascii_lowercase();
            l.starts_with("end sub") || l.starts_with("end function")
        })
        .unwrap_or(lines.len() - 1);
    let mut from = start;
    while from > 0 && lines[from - 1].trim_start().starts_with('\'') {
        from -= 1;
    }
    let mut to = end + 1;
    // One blank line that separated it from the next goes too.
    if lines.get(to).is_some_and(|l| l.trim().is_empty()) {
        to += 1;
    }
    let mut out: Vec<&str> = lines[..from].to_vec();
    out.extend_from_slice(&lines[to..]);
    let eol = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let mut text = out.join(eol);
    if source.ends_with('\n') && !text.is_empty() {
        text.push_str(eol);
    }
    Some(text)
}

/// The procedure a line declares: `[Public|Private] [Static] Sub|Function
/// Name(`.
fn proc_name(line: &str) -> Option<String> {
    let mut words = line.split_whitespace().peekable();
    while let Some(w) = words.peek() {
        let l = w.to_ascii_lowercase();
        if matches!(l.as_str(), "public" | "private" | "friend" | "static") {
            words.next();
        } else {
            break;
        }
    }
    let kw = words.next()?.to_ascii_lowercase();
    if kw != "sub" && kw != "function" {
        return None;
    }
    let rest = words.next()?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The Sub the caret is in (line 1-based), for Run Sub (F5).
pub fn sub_at(source: &str, line: u32) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut i = (line.max(1) as usize - 1).min(lines.len().saturating_sub(1));
    loop {
        let l = lines.get(i)?.trim().to_ascii_lowercase();
        if (l.starts_with("end sub") || l.starts_with("end function")) && i + 1 != line as usize {
            return None;
        }
        if let Some(n) = proc_name(lines[i]) {
            return Some(n);
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

/// The macros side of the app: projects, the running macro and what it
/// shows.
#[derive(Default)]
pub struct MacroState {
    pub projects: Vec<MacroProject>,
    /// The folder the copies live in (`None`: the settings folder).
    pub dir: Option<PathBuf>,
    pub loaded: bool,
    pub run: Option<MacroRun>,
    /// A message box or input box the macro waits for.
    pub prompt: Option<Prompt>,
    /// A run that ended with an error: the error and the project.
    pub error: Option<(RunError, usize)>,
    /// What the last run kept going past: the macro's name, the steps
    /// (with how many times each) and the project (#4315).
    pub skipped: Option<(String, Vec<(RunError, u32)>, usize)>,
    /// Warnings shown before running a project for the first time.
    pub warned: std::collections::BTreeSet<String>,
    /// The drawing's revision after the run's last changes (to add later
    /// ones to the same undo step).
    pub applied: Option<u64>,
    /// Run Macro (Alt+Shift+F8).
    pub run_dialog: Option<RunDialog>,
    /// The macro editor (Alt+F11).
    pub editor: Option<Editor>,
    /// The Macro Manager's chosen row: (project, module, macro).
    pub chosen: Option<(usize, Option<String>, Option<String>)>,
    /// Renaming a project in the manager: (project, the name typed).
    pub renaming: Option<(usize, String)>,
    /// The manager asks before deleting its chosen row: (project,
    /// module, macro) (#4409).
    pub deleting: Option<(usize, Option<String>, Option<String>)>,
    /// Scripts panel: the snippet is VBA, and the other language's text
    /// (swapped in when the language changes).
    pub scripts_vba: bool,
    pub other_source: String,
    /// A warning dialog before a run: (project, module, macro).
    pub confirm: Option<(usize, String, String)>,
    /// The web addresses macros opened, in tests (see [`Request::Open`]).
    pub opened: Vec<String>,
}

/// A message or input box on screen.
pub enum Prompt {
    MsgBox {
        prompt: String,
        buttons: i32,
        title: String,
    },
    InputBox {
        prompt: String,
        title: String,
        text: String,
    },
}

/// Run Macro's state.
#[derive(Default)]
pub struct RunDialog {
    pub filter: String,
    /// "Project|Module|Macro" chosen.
    pub chosen: Option<(usize, String, String)>,
    /// The search field got the focus.
    pub focused: bool,
}

/// The macro editor's state.
#[derive(Default)]
pub struct Editor {
    pub project: usize,
    pub module: String,
    pub source: String,
    pub dirty: bool,
    /// A line to show (an error's), 1-based.
    pub goto_line: Option<u32>,
    pub find: String,
    pub message: Option<String>,
    /// The caret's line, for F5 (Run Sub).
    pub caret_line: u32,
    /// The compatibility check's last report, and whether its panel shows.
    pub report: Option<crate::macro_convert::Report>,
    pub show_report: bool,
    /// The JavaScript tab: shown, its text, and the function Run starts.
    pub js_tab: bool,
    pub js: String,
    pub js_run: String,
}

impl MacroState {
    fn folder(&self) -> Option<PathBuf> {
        self.dir.clone().or_else(default_dir)
    }

    /// Load the copies kept in the settings folder (once).
    pub fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if crate::files::WEB {
            return;
        }
        let Some(dir) = self.folder() else {
            return;
        };
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gms")))
            .collect();
        paths.sort();
        for p in paths {
            if let Ok(bytes) = std::fs::read(&p) {
                if let Ok(project) = vp::read_gms(&bytes) {
                    self.projects.push(MacroProject::new(p, project));
                }
            }
        }
    }

    /// Keep a copy of `project` in the settings folder and add it. A project
    /// of the same name is replaced.
    pub fn install(
        &mut self,
        project: vp::Project,
        original: Option<&[u8]>,
    ) -> Result<usize, String> {
        self.ensure_loaded();
        let dir = self.folder().ok_or("no settings folder")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stem: String = project
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let stem = if stem.is_empty() {
            "Macros".to_string()
        } else {
            stem
        };
        let file = dir.join(format!("{stem}.gms"));
        let bytes = match original {
            Some(b) => b.to_vec(),
            None => vp::write_gms(&project).map_err(|e| e.to_string())?,
        };
        std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
        let mp = MacroProject::new(file.clone(), project);
        if let Some(i) = self.projects.iter().position(|p| p.file == file) {
            self.projects[i] = mp;
            Ok(i)
        } else {
            self.projects.push(mp);
            Ok(self.projects.len() - 1)
        }
    }

    /// Unload project `i` and delete its copy.
    pub fn remove(&mut self, i: usize) {
        if i < self.projects.len() {
            let p = self.projects.remove(i);
            let _ = std::fs::remove_file(&p.file);
        }
    }

    /// Delete a module, or one macro of a module, from project `i` and
    /// save the project's copy (#4409). A form module takes its form
    /// along.
    pub fn delete_part(&mut self, i: usize, module: &str, name: Option<&str>) -> Result<(), String> {
        let p = self.projects.get_mut(i).ok_or("no project")?;
        let at = p
            .project
            .modules
            .iter()
            .position(|m| m.name.eq_ignore_ascii_case(module))
            .ok_or("no module")?;
        match name {
            None => {
                p.project.modules.remove(at);
                p.project.forms.retain(|f| !f.name.eq_ignore_ascii_case(module));
            }
            Some(n) => {
                let m = &mut p.project.modules[at];
                m.source = without_procedure(&m.source, n).ok_or("no macro")?;
            }
        }
        p.warnings = scan(&p.project);
        p.save()
    }

    /// Rename project `i` (its name and its copy's file).
    pub fn rename(&mut self, i: usize, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(tr("macros.name_empty"));
        }
        let dir = self.folder().ok_or("no settings folder")?;
        let p = self.projects.get_mut(i).ok_or("no project")?;
        p.project.name = name.to_string();
        let stem: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let file = dir.join(format!("{stem}.gms"));
        let old = std::mem::replace(&mut p.file, file);
        p.save()?;
        if old != p.file {
            let _ = std::fs::remove_file(old);
        }
        Ok(())
    }

    /// Whether something of the macros blocks the rest of the app: a
    /// message, an input box, an error, Run Macro or a modal form.
    pub fn modal(&self) -> bool {
        self.prompt.is_some()
            || self.error.is_some()
            || self.run_dialog.is_some()
            || self.confirm.is_some()
            || self
                .run
                .as_ref()
                .and_then(|r| r.forms.lock().ok().map(|f| f.modal_visible()))
                .unwrap_or(false)
    }
}

/// Every macro of every project: (project, module, macro).
pub fn all_macros(state: &MacroState) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for (i, p) in state.projects.iter().enumerate() {
        for (m, s) in p.macros() {
            out.push((i, m, s));
        }
    }
    out
}

/// Dots would end the key's last part early (see `i18n::tr`).
fn key_part(s: &str) -> String {
    s.replace('.', " ")
}

/// The command key of a macro for menus and shortcuts.
pub fn command_key(project: &str, module: &str, name: &str) -> String {
    format!("menu.macro.{}|{module}|{name}", key_part(project))
}

/// The key of a project's submenu in Tools > Macros (shown as its name).
pub fn project_menu_key(project: &str) -> String {
    format!("menu.macros.project.{}", key_part(project))
}

/// A macro command's name ("Module.Macro") and project, for
/// Options > Customization.
pub fn command_name(key: &str) -> Option<(String, String)> {
    // A plugin's command is named the same way (#3519).
    if let Some(named) = crate::plugins::command_name(key) {
        return Some(named);
    }
    let rest = key.strip_prefix("menu.macro.")?;
    let mut parts = rest.splitn(3, '|');
    let project = parts.next()?.to_string();
    let module = parts.next()?;
    let name = parts.next()?;
    Some((format!("{module}.{name}"), project))
}

impl App {
    /// The browser version has no macros (they run on a thread of their
    /// own, and the browser build has none): any macro command shows a
    /// message saying so, with the desktop app's downloads. True there.
    pub fn macros_desktop_only(&mut self) -> bool {
        if crate::files::WEB {
            self.dialog = crate::ui::dialogs::Dialog::MacrosDesktopOnly;
        }
        crate::files::WEB
    }

    /// Tools > Macros > Load Macro Project: pick the files and install them.
    pub fn load_macro_project(&mut self) {
        if self.macros_desktop_only() {
            return;
        }
        crate::files::Dialog::new()
            .add_filter(tr("macros.filter"), &["gms", "bas", "cls", "frm"])
            .pick_files(self, |app, paths| app.install_macro_files(&paths));
    }

    /// Install the macro project in `paths`.
    pub fn install_macro_files(&mut self, paths: &[PathBuf]) {
        let mut paths: Vec<PathBuf> = paths.to_vec();
        // A form's .frx goes with its .frm.
        let frms: Vec<PathBuf> = paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("frm")))
            .cloned()
            .collect();
        for f in frms {
            let frx = f.with_extension("frx");
            if frx.exists() && !paths.contains(&frx) {
                paths.push(frx);
            }
        }
        let original = if paths.len() == 1
            && paths[0]
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gms"))
        {
            crate::files::read(&paths[0]).ok()
        } else {
            None
        };
        match read_project(&paths).and_then(|p| self.macros.install(p, original.as_deref())) {
            Ok(i) => {
                let p = &self.macros.projects[i];
                self.status = trf(
                    "macros.loaded",
                    &[("name", p.name()), ("n", &p.macros().len().to_string())],
                );
                self.macros.chosen = Some((i, None, None));
                self.show_macro_manager();
                // The compatibility report of what was loaded.
                self.edit_macro(i, "", None);
                self.check_macro_editor();
            }
            Err(e) => self.status = trf("macros.load_failed", &[("e", &e)]),
        }
    }

    pub fn show_macro_manager(&mut self) {
        if self.macros_desktop_only() {
            return;
        }
        self.macros.ensure_loaded();
        self.open_docker(crate::app::DockerTab::Macros);
    }

    /// Run `module.name` of project `project`, after showing its warnings
    /// once.
    pub fn request_macro(&mut self, project: usize, module: &str, name: &str) {
        let Some(p) = self.macros.projects.get(project) else {
            return;
        };
        let key = p.file.display().to_string();
        if !p.warnings.is_empty() && !self.macros.warned.contains(&key) {
            self.macros.confirm = Some((project, module.to_string(), name.to_string()));
            return;
        }
        self.run_macro(project, module, name);
    }

    /// Start a macro now.
    pub fn run_macro(&mut self, project: usize, module: &str, name: &str) {
        if self.macros_desktop_only() {
            return;
        }
        if self.macros.run.is_some() {
            self.status = tr("macros.already_running");
            return;
        }
        let Some(p) = self.macros.projects.get(project) else {
            return;
        };
        let program = match p.compile_for_run(self.settings.macros_keep_going) {
            Ok(prog) => prog,
            Err(e) => {
                self.macros.error = Some((
                    RunError {
                        number: 0,
                        description: trf("macros.compile_error", &[("e", &e.message)]),
                        module: e.module.clone(),
                        procedure: String::new(),
                        line: e.line,
                    },
                    project,
                ));
                return;
            }
        };
        let entry = vba::entry_call(&program, module, name);
        let label = format!("{}.{module}.{name}", p.name());
        let models = p.project.forms.clone();
        match crate::scripting::vba_run::start(self, &program.js, &entry, models, label.clone()) {
            Ok(run) => {
                self.macros.run = Some(run);
                self.macros.applied = None;
                self.status = trf("macros.running", &[("name", &label)]);
            }
            Err(e) => self.status = trf("macros.load_failed", &[("e", &e)]),
        }
    }

    /// Run a VBA snippet from the Scripts panel: its first public Sub
    /// without arguments, or the code as the body of one.
    pub fn run_vba_snippet(&mut self, src: &str) {
        let mut module = vba::Module::new("Snippet", vba::ModuleKind::Standard, src);
        let macros = vba::list_macros(std::slice::from_ref(&module));
        let name = match macros.first() {
            Some((_, n)) => n.clone(),
            None => {
                module.source = format!("Sub Main()\r\n{src}\r\nEnd Sub\r\n");
                "Main".into()
            }
        };
        let project = vp::Project {
            name: "Scripts".into(),
            modules: vec![vp::Module {
                name: "Snippet".into(),
                kind: vp::ModuleKind::Standard,
                source: module.source.clone(),
                has_source: true,
                read_only: false,
                private: false,
            }],
            code_page: 1252,
            ..Default::default()
        };
        let mp = MacroProject::new(PathBuf::new(), project);
        let program = match mp.compile() {
            Ok(p) => p,
            Err(e) => {
                self.scripts_output.push(format!("Error: {e}"));
                return;
            }
        };
        let entry = vba::entry_call(&program, "Snippet", &name);
        match crate::scripting::vba_run::start(
            self,
            &program.js,
            &entry,
            Vec::new(),
            "Scripts".into(),
        ) {
            Ok(run) => {
                self.macros.run = Some(run);
                self.macros.applied = None;
            }
            Err(e) => self.scripts_output.push(format!("Error: {e}")),
        }
    }

    /// Stop the running macro.
    pub fn stop_macro(&mut self) {
        if let Some(run) = &self.macros.run {
            run.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if self.macros.prompt.take().is_some() {
            // The macro waits for the dialog: it sees the stop.
        }
    }

    /// Apply a batch of the macro's changes: the first starts an undo step
    /// named "Macro", the next ones join it while nothing else happened.
    pub fn apply_macro_changes(&mut self, c: Changes) {
        if !c.commands.is_empty() {
            let joined = self.macros.applied == Some(self.engine.revision());
            let r = if joined {
                self.engine.amend_batch("Macro", &c.commands)
            } else {
                self.engine.run_batch("Macro", &c.commands)
            };
            if let Err(e) = r {
                self.status = e.to_string();
            }
            self.macros.applied = Some(self.engine.revision());
            self.raster.borrow_mut().invalidate();
        }
        if let Some(page) = c.page {
            if self.doc().pages.iter().any(|p| p.id == page) {
                self.page = page;
            }
        }
        self.selection = c
            .selection
            .into_iter()
            .filter(|id| self.doc().find_shape(*id).is_some())
            .collect();
        self.scripts_output.extend(c.output);
        if let Some(s) = c.status {
            self.status = s;
        }
        if let Some(on) = c.simulate_overprints {
            self.simulate_overprints = on;
        }
        for (step, selection) in c.view_steps {
            self.run_view_step_with(step, &selection);
        }
    }

    /// The system's file dialog a macro asks for (`ScriptTools.GetFileBox`,
    /// #4315): `filter` is "Name (*.csv)|*.csv|...", `mode` 0 opens, 1
    /// saves, 2 picks a folder. The macro gets the path, "" for Cancel.
    fn macro_file_box(&mut self, filter: &str, title: &str, mode: u8, file: &str, folder: &str) {
        let mut dialog = crate::files::Dialog::new();
        let parts: Vec<&str> = filter.split('|').collect();
        for pair in parts.chunks(2) {
            let [name, pattern] = pair else { continue };
            let exts: Vec<&str> = pattern
                .split(';')
                .filter_map(|p| p.trim().strip_prefix("*."))
                .filter(|e| !e.is_empty() && *e != "*")
                .collect();
            if !exts.is_empty() {
                dialog = dialog.add_filter(name.trim(), &exts);
            }
        }
        if !title.is_empty() {
            dialog = dialog.set_title(title);
        }
        let start = std::path::Path::new(file);
        let dir = if !folder.is_empty() {
            Some(std::path::PathBuf::from(folder))
        } else {
            start.parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.to_path_buf())
        };
        if let Some(dir) = dir.filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = start.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy().to_string());
        }
        dialog.choose(self, mode, |app, path| {
            let text = path.map(|p| p.display().to_string()).unwrap_or_default();
            app.answer_macro(Input::Text(Some(text)));
        });
    }

    /// Answer the message or input box the macro waits for.
    pub fn answer_macro(&mut self, input: Input) {
        self.macros.prompt = None;
        if let Some(run) = &self.macros.run {
            let _ = run.input.send(input);
        }
    }

    /// Something happened on a macro's form: the macro handles it.
    pub fn macro_form_event(
        &mut self,
        form: u32,
        control: &str,
        event: &str,
        args: Vec<serde_json::Value>,
    ) {
        let snap = Snapshot::of(self);
        if let Some(run) = &self.macros.run {
            let _ = run.input.send(Input::Event(
                FormEvent {
                    form,
                    control: control.to_string(),
                    event: event.to_string(),
                    args,
                },
                Box::new(snap),
            ));
        }
    }

    /// Each frame: what the running macro asks for; when it ends, its last
    /// changes and its error. Whether it still runs.
    pub fn macro_tick(&mut self) -> bool {
        let Some(run) = &self.macros.run else {
            return false;
        };
        let mut reqs = Vec::new();
        while let Ok(r) = run.requests.try_recv() {
            reqs.push(r);
        }
        let finished = run.is_finished();
        for r in reqs {
            match r {
                Request::Flush(c) => self.apply_macro_changes(c),
                Request::MsgBox {
                    prompt,
                    buttons,
                    title,
                } => {
                    self.macros.prompt = Some(Prompt::MsgBox {
                        prompt,
                        buttons,
                        title,
                    })
                }
                Request::InputBox {
                    prompt,
                    title,
                    default,
                } => {
                    self.macros.prompt = Some(Prompt::InputBox {
                        prompt,
                        title,
                        text: default,
                    })
                }
                // A page the macro opens (ShellExecute, #2165); tests
                // keep the address instead of starting a browser.
                Request::Open(url) => {
                    if cfg!(test) {
                        self.macros.opened.push(url);
                    } else {
                        self.open_url(&url);
                    }
                }
                Request::FileBox {
                    filter,
                    title,
                    mode,
                    file,
                    folder,
                } => self.macro_file_box(&filter, &title, mode, &file, &folder),
            }
        }
        if !finished {
            return true;
        }
        let Some(run) = self.macros.run.take() else {
            return false;
        };
        let name = run.name.clone();
        let done = run.finish();
        self.apply_macro_changes(done.changes);
        self.macros.prompt = None;
        if !done.skipped.is_empty() && name != "Scripts" && !done.stopped {
            let project = self
                .macros
                .projects
                .iter()
                .position(|p| name.starts_with(&format!("{}.", p.name())))
                .unwrap_or(usize::MAX);
            self.macros.skipped = Some((name.clone(), done.skipped, project));
        }
        if done.stopped {
            self.status = trf("macros.stopped", &[("name", &name)]);
        } else if let Some(e) = done.error {
            let project = self
                .macros
                .projects
                .iter()
                .position(|p| name.starts_with(&format!("{}.", p.name())))
                .unwrap_or(usize::MAX);
            if name == "Scripts" && e.number == 0 && e.module.is_empty() {
                // A JavaScript script's error (#4139).
                self.scripts_output.push(format!("Error: {}", e.description));
                self.status = trf("status.script_error", &[("e", &e.description)]);
            } else if name == "Scripts" {
                self.scripts_output.push(format!(
                    "Error: Run-time error '{}': {} ({}, line {})",
                    e.number, e.description, e.procedure, e.line
                ));
            } else {
                self.macros.error = Some((e, project));
            }
        } else if name != "Scripts" {
            self.status = match &self.macros.skipped {
                Some((_, steps, _)) => trf(
                    "macros.done_skipped",
                    &[("name", &name), ("n", &steps.len().to_string())],
                ),
                None => trf("macros.done", &[("name", &name)]),
            };
        }
        false
    }

    /// Tools > Macros > Run Macro (Alt+Shift+F8).
    pub fn open_run_macro(&mut self) {
        if self.macros_desktop_only() {
            return;
        }
        self.macros.ensure_loaded();
        self.macros.run_dialog = Some(RunDialog::default());
    }

    /// Tools > Macros > Macro Editor (Alt+F11): the chosen project's first
    /// module, else the first project's; with no project, a new one.
    pub fn open_macro_editor(&mut self) {
        if self.macros_desktop_only() {
            return;
        }
        self.macros.ensure_loaded();
        if self.macros.editor.is_some() {
            return;
        }
        if self.macros.projects.is_empty() {
            let project = vp::Project {
                name: "Macros".into(),
                modules: vec![vp::Module {
                    name: "Module1".into(),
                    kind: vp::ModuleKind::Standard,
                    source: "Attribute VB_Name = \"Module1\"\r\nSub Hello()\r\n    MsgBox \"Objects on the page: \" & ActivePage.Shapes.Count\r\nEnd Sub\r\n".into(),
                    has_source: true,
                    read_only: false,
                    private: false,
                }],
                code_page: 1252,
                ..Default::default()
            };
            if let Err(e) = self.macros.install(project, None) {
                self.status = trf("macros.save_failed", &[("e", &e)]);
                return;
            }
        }
        let (p, m) = match &self.macros.chosen {
            Some((p, m, _)) if *p < self.macros.projects.len() => {
                (*p, m.clone().unwrap_or_default())
            }
            _ => (0, String::new()),
        };
        self.edit_macro(p, &m, None);
    }

    /// Export a module (`.bas`, `.cls`, `.frm` with its `.frx`) or the
    /// whole project as a `.gms` file.
    pub fn export_macros(&mut self, project: usize, module: Option<&str>) {
        let Some(p) = self.macros.projects.get(project) else {
            return;
        };
        let files = match module.and_then(|m| p.project.module(m)) {
            Some(m) => vp::export_module(&p.project, m).map_err(|e| e.to_string()),
            None => vp::write_gms(&p.project)
                .map(|bytes| {
                    vec![vp::ExportedFile {
                        name: p
                            .file
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| format!("{}.gms", p.name())),
                        bytes,
                    }]
                })
                .map_err(|e| e.to_string()),
        };
        let files = match files {
            Ok(f) if !f.is_empty() => f,
            Ok(_) => return,
            Err(e) => {
                self.status = trf("macros.export_failed", &[("e", &e)]);
                return;
            }
        };
        let files: Vec<(String, Vec<u8>)> = files.into_iter().map(|f| (f.name, f.bytes)).collect();
        let first = files[0].0.clone();
        let ext = Path::new(&first)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        // The save dialog, a download, or the system's save picker on
        // Android (#1402).
        crate::files::Dialog::new()
            .add_filter(ext.to_uppercase(), &[ext.as_str()])
            .set_file_name(first)
            .save_file(self, move |app, path| {
                app.write_exported_macros(&path, files)
            });
    }

    /// Write the exported files: the first to `path`, the others beside
    /// it (a form's `.frx` with the name chosen). A document the Android
    /// picker made has nothing beside it: the picker asks for each of the
    /// others in turn.
    fn write_exported_macros(&mut self, path: &Path, files: Vec<(String, Vec<u8>)>) {
        let mut files = std::collections::VecDeque::from(files);
        let Some((_, bytes)) = files.pop_front() else {
            return;
        };
        let shown = path.display().to_string();
        if let Err(e) = crate::files::write(path, &bytes) {
            self.status = trf("macros.export_failed", &[("e", &e.to_string())]);
            return;
        }
        // The form's .frx goes beside its .frm, with the name chosen.
        let named = |name: String| {
            if name.to_ascii_lowercase().ends_with(".frx") {
                path.with_extension("frx")
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or(name)
            } else {
                name
            }
        };
        let rest: std::collections::VecDeque<(String, Vec<u8>)> =
            files.into_iter().map(|(n, b)| (named(n), b)).collect();
        if crate::files::is_document_path(path) {
            save_each_macro_file(self, rest, shown);
            return;
        }
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut result = Ok(());
        for (name, bytes) in rest {
            if result.is_ok() {
                result = crate::files::write(dir.join(&name), &bytes);
            }
        }
        self.status = match result {
            Ok(()) => trf("macros.exported", &[("f", &shown)]),
            Err(e) => trf("macros.export_failed", &[("e", &e.to_string())]),
        };
    }

    /// Open the macro editor at `module` of project `project`, at `line`.
    pub fn edit_macro(&mut self, project: usize, module: &str, line: Option<u32>) {
        let Some(p) = self.macros.projects.get(project) else {
            return;
        };
        let m = p
            .project
            .module(module)
            .or_else(|| p.project.modules.iter().find(|m| m.has_source));
        let (name, source) = m
            .map(|m| (m.name.clone(), m.source.clone()))
            .unwrap_or_default();
        // The report and the JavaScript tab stay while moving around the
        // same project.
        let kept = self
            .macros
            .editor
            .take()
            .filter(|e| e.project == project)
            .map(|e| (e.report, e.show_report, e.js_tab, e.js, e.js_run, e.find))
            .unwrap_or_default();
        self.macros.editor = Some(Editor {
            project,
            module: name,
            source,
            goto_line: line,
            report: kept.0,
            show_report: kept.1,
            js_tab: kept.2 && line.is_none(),
            js: kept.3,
            js_run: kept.4,
            find: kept.5,
            ..Default::default()
        });
    }

    /// The editor's text goes back into its module, the project is saved,
    /// and the code is compiled to show its errors.
    pub fn save_macro_editor(&mut self) {
        let Some(ed) = &mut self.macros.editor else {
            return;
        };
        let Some(p) = self.macros.projects.get_mut(ed.project) else {
            return;
        };
        if let Some(m) = p
            .project
            .modules
            .iter_mut()
            .find(|m| m.name.eq_ignore_ascii_case(&ed.module))
        {
            m.source = ed.source.clone();
            m.has_source = true;
        }
        p.warnings = scan(&p.project);
        let saved = p.save();
        ed.dirty = false;
        ed.message = Some(match (saved, p.compile()) {
            (Err(e), _) => trf("macros.save_failed", &[("e", &e)]),
            (Ok(()), Err(e)) => {
                ed.goto_line = Some(e.line);
                trf(
                    "macros.compile_error_at",
                    &[
                        ("m", &e.module),
                        ("l", &e.line.to_string()),
                        ("e", &e.message),
                    ],
                )
            }
            (Ok(()), Ok(_)) => tr("macros.saved"),
        });
    }
}

/// The system's save picker for each of `rest` in turn (Android, #1402);
/// the status line names `shown` once all are written.
fn save_each_macro_file(
    app: &mut App,
    mut rest: std::collections::VecDeque<(String, Vec<u8>)>,
    shown: String,
) {
    let Some((name, bytes)) = rest.pop_front() else {
        app.status = trf("macros.exported", &[("f", &shown)]);
        return;
    };
    let ext = Path::new(&name)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    crate::files::Dialog::new()
        .add_filter(ext.to_uppercase(), &[ext.as_str()])
        .set_file_name(name)
        .save_file(app, move |app, path| {
            match crate::files::write(&path, &bytes) {
                Ok(()) => save_each_macro_file(app, rest, shown),
                Err(e) => app.status = trf("macros.export_failed", &[("e", &e.to_string())]),
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripting::vba_run::Input;

    fn project(name: &str, code: &str, forms: Vec<vp::Form>) -> vp::Project {
        let mut modules = vec![vp::Module {
            name: "Main".into(),
            kind: vp::ModuleKind::Standard,
            source: code.into(),
            has_source: true,
            read_only: false,
            private: false,
        }];
        for f in &forms {
            modules.push(vp::Module {
                name: f.name.clone(),
                kind: vp::ModuleKind::Form,
                source: String::new(),
                has_source: true,
                read_only: false,
                private: false,
            });
        }
        vp::Project {
            name: name.into(),
            modules,
            forms,
            code_page: 1252,
            ..Default::default()
        }
    }

    /// The Macro Manager deletes one macro or a whole module, and the
    /// project's copy keeps the change (#4409).
    #[test]
    fn the_manager_deletes_a_macro_and_a_module() {
        let code = "Attribute VB_Name = \"Main\"\r\n\
                    ' Says hello.\r\n\
                    Sub Hello()\r\n\
                    MsgBox \"hi\"\r\n\
                    End Sub\r\n\
                    \r\n\
                    Public Function Twice(x)\r\n\
                    Twice = x * 2\r\n\
                    End Function\r\n";
        assert_eq!(
            without_procedure(code, "hello").as_deref(),
            Some(
                "Attribute VB_Name = \"Main\"\r\n\
                 Public Function Twice(x)\r\n\
                 Twice = x * 2\r\n\
                 End Function\r\n"
            )
        );
        assert!(without_procedure(code, "Nope").is_none());
        let dir = temp("delete");
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = project("Del", code, Vec::new());
        p.modules.push(vp::Module {
            name: "Extra".into(),
            kind: vp::ModuleKind::Standard,
            source: "Sub Other()\r\nEnd Sub\r\n".into(),
            has_source: true,
            read_only: false,
            private: false,
        });
        let mut state = MacroState::default();
        state.projects.push(MacroProject::new(dir.join("Del.gms"), p));
        state.delete_part(0, "Main", Some("Hello")).unwrap();
        let names: Vec<String> = state.projects[0].macros().into_iter().map(|(_, n)| n).collect();
        assert!(!names.iter().any(|n| n == "Hello"), "{names:?}");
        assert!(names.iter().any(|n| n == "Other"), "{names:?}");
        state.delete_part(0, "extra", None).unwrap();
        assert_eq!(state.projects[0].project.modules.len(), 1);
        assert!(state.delete_part(0, "Extra", None).is_err());
        // The copy on disk has the change.
        let bytes = std::fs::read(dir.join("Del.gms")).unwrap();
        let back = read_project(&[dir.join("Del.gms")]).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(back.modules.len(), 1);
        assert!(!back.modules[0].source.contains("Sub Hello"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn temp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("tracedraw-macros-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Run frames until the macro ends, answering with `answer` when it
    /// asks; at most a few seconds.
    fn drive(app: &mut App, mut answer: impl FnMut(&mut App)) {
        let t = std::time::Instant::now();
        while app.macro_tick() {
            answer(app);
            std::thread::sleep(std::time::Duration::from_millis(5));
            assert!(t.elapsed().as_secs() < 20, "the macro did not end");
        }
    }

    #[test]
    fn a_macro_writes_and_reads_a_text_file_and_keeps_going() {
        // #4315: files work, and what TraceDraw cannot do is skipped and
        // listed instead of stopping the macro.
        let dir = temp("files");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ids.txt");
        let code = format!(
            "Sub Go()\r\n\
             Dim i As Long, s As String\r\n\
             Open \"{p}\" For Output As #1\r\n\
             For i = 1 To 3: Print #1, \"ID-\" & i: Next\r\n\
             Close #1\r\n\
             ActiveDocument.Frobnicate\r\n\
             Open \"{p}\" For Input As #1\r\n\
             Do Until EOF(1)\r\n\
             Line Input #1, s\r\n\
             ActiveLayer.CreateArtisticText 0, i, s\r\n\
             i = i + 1\r\n\
             Loop\r\n\
             Close #1\r\n\
             End Sub\r\n",
            p = path.display()
        );
        let mut app = app_with(project("Ids", &code, Vec::new()));
        let shapes = |app: &App| app.doc().all_layers().map(|l| l.shapes.len()).sum::<usize>();
        let before = shapes(&app);
        app.settings.macros_keep_going = true;
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "ID-1\r\nID-2\r\nID-3\r\n"
        );
        assert_eq!(shapes(&app), before + 3);
        let (_, steps, project) = app.macros.skipped.clone().expect("skipped steps");
        assert_eq!(project, 0);
        assert_eq!(steps.len(), 1);
        assert!(steps[0].0.description.contains("Frobnicate"), "{steps:?}");
        assert_eq!(steps[0].0.line, 6);
        // Off: the same macro stops there with the error.
        app.macros.skipped = None;
        app.settings.macros_keep_going = false;
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        let (e, _) = app.macros.error.clone().expect("an error");
        assert_eq!(e.number, 438);
        assert!(app.macros.skipped.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_members_published_macros_use_work() {
        // #4315: the file box picks the list, texts are found and
        // replaced, ranges are centred and placed, and guidelines are
        // added and removed; the macro writes what it saw to a file.
        let dir = temp("members");
        std::fs::create_dir_all(&dir).unwrap();
        let csv = dir.join("names.csv");
        std::fs::write(&csv, "Ann\r\n").unwrap();
        let log = dir.join("log.txt");
        let code = format!(
            "Sub Go()\r\n\
             Dim f As String, s As String, t As Shape, r As Shape, b\r\n\
             f = ScriptTools.GetFileBox(\"CSV (*.csv)|*.csv\", \"Pick\", 0)\r\n\
             Open f For Input As #1: Line Input #1, s: Close #1\r\n\
             Set t = ActiveLayer.CreateArtisticText(10, 10, \"Hello NAME\")\r\n\
             Open \"{log}\" For Output As #2\r\n\
             Print #2, s; ActivePage.TextFind(\"name\"); ActivePage.TextReplace(\"NAME\", s, True, True)\r\n\
             Print #2, t.Text.Story.Text\r\n\
             Set r = ActiveLayer.CreateRectangleRect(CreateRect(0, 0, 2, 1))\r\n\
             r.SetPositionEx cdrBottomLeft, 1, 1\r\n\
             Set b = r.BoundingBox\r\n\
             Print #2, b.Left; b.Bottom; b.Width; b.Height\r\n\
             ActiveDocument.Unit = cdrInch\r\n\
             r.AlignRangeToPageCenter cdrAlignHCenter + cdrAlignVCenter\r\n\
             Print #2, Round(r.CenterX - ActivePage.SizeWidth / 2, 3); Round(r.CenterY - ActivePage.SizeHeight / 2, 3)\r\n\
             ActivePage.GuidesLayer.CreateGuideAngle 1, 1, 45\r\n\
             Print #2, ActivePage.GuidesLayer.Shapes.Count;\r\n\
             ActivePage.GuidesLayer.SelectableShapes.All.Delete\r\n\
             Print #2, ActivePage.GuidesLayer.Shapes.Count\r\n\
             Close #2\r\n\
             End Sub\r\n",
            log = log.display()
        );
        let mut app = app_with(project("Members", &code, Vec::new()));
        crate::files::test_pick::answer(vec![csv.clone()]);
        app.settings.macros_keep_going = false;
        app.run_macro(0, "Main", "Go");
        drive(&mut app, crate::files::test_save::finish);
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let out = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines,
            vec!["AnnTrueTrue", "Hello Ann", " 1  1  2  1 ", " 0  0 ", " 1  0 "],
            "{out}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn projects_are_kept_and_loaded_again() {
        let dir = temp("keep");
        let mut st = MacroState {
            dir: Some(dir.clone()),
            ..Default::default()
        };
        let p = project(
            "Tools",
            "Sub One()\r\nEnd Sub\r\nSub Two()\r\nEnd Sub\r\n",
            Vec::new(),
        );
        let i = st.install(p, None).unwrap();
        assert_eq!(st.projects[i].macros().len(), 2);
        st.rename(i, "Renamed Tools").unwrap();
        // A new session loads the copy.
        let mut again = MacroState {
            dir: Some(dir.clone()),
            ..Default::default()
        };
        again.ensure_loaded();
        assert_eq!(again.projects.len(), 1);
        assert_eq!(again.projects[0].name(), "Renamed Tools");
        assert_eq!(
            again.projects[0].macros(),
            vec![
                ("Main".to_string(), "One".to_string()),
                ("Main".to_string(), "Two".to_string())
            ]
        );
        again.remove(0);
        let mut third = MacroState {
            dir: Some(dir),
            ..Default::default()
        };
        third.ensure_loaded();
        assert!(third.projects.is_empty());
    }

    #[test]
    fn warnings_name_what_cannot_work_here() {
        let p = project(
            "W",
            "Private Declare Function FindWindow Lib \"user32\" (ByVal a As String) As Long\r\nSub Go()\r\n  Open \"x\" For Input As #1\r\n  Set o = CreateObject(\"Excel.Application\")\r\n  Set d = CreateObject(\"Scripting.Dictionary\")\r\nEnd Sub\r\n",
            Vec::new(),
        );
        let w = scan(&p).join("\n");
        assert!(w.contains("FindWindow"), "{w}");
        assert!(w.contains("Excel.Application"), "{w}");
        assert!(!w.contains("Scripting.Dictionary"), "{w}");
        assert_eq!(scan(&p).len(), 2, "{w}");
    }

    fn app_with(p: vp::Project) -> App {
        let mut app = App::headless();
        app.macros.dir = Some(temp(&p.name.clone()));
        app.macros.loaded = true;
        app.macros.install(p, None).unwrap();
        app
    }

    #[test]
    fn a_javascript_script_asks_with_inputbox_and_prompt() {
        // #4139: the Scripts panel's JavaScript waits for the input box.
        let mut app = App::headless();
        let before = app
            .doc()
            .all_layers()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        app.run_script(
            "const n = InputBox('How many?', 'Grid', '2');\n\
             const w = prompt('Width', '10');\n\
             const c = prompt('Cancelled?');\n\
             for (let i = 0; i < Number(n); i++) ActiveLayer.CreateRectangle(i * 20, 0, Number(w), 5);\n\
             print('made ' + n + ' ' + w + ' ' + c);",
        );
        let mut asked = Vec::new();
        drive(&mut app, |app| {
            if let Some(Prompt::InputBox {
                prompt,
                title,
                text,
            }) = app.macros.prompt.take()
            {
                asked.push(format!("{title}: {prompt} [{text}]"));
                let answer = match asked.len() {
                    1 => Some("3".to_string()),
                    2 => Some("15".to_string()),
                    _ => None,
                };
                app.answer_macro(Input::Text(answer));
            }
        });
        assert_eq!(
            asked,
            vec![
                "Grid: How many? [2]".to_string(),
                ": Width [10]".to_string(),
                ": Cancelled? []".to_string()
            ]
        );
        assert!(
            app.scripts_output.iter().any(|l| l == "made 3 15 null"),
            "{:?}",
            app.scripts_output
        );
        let after = app
            .doc()
            .all_layers()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        assert_eq!(after, before + 3);
        // Where nobody can answer, the default comes back.
        let out = crate::scripting::run(&mut app, "print(InputBox('n', 't', '7') + prompt('x'))")
            .unwrap();
        assert_eq!(out, vec!["7".to_string()]);
    }

    #[test]
    fn a_javascript_script_error_after_inputbox_is_shown() {
        let mut app = App::headless();
        app.run_script("const n = InputBox('n'); nothing_here();");
        drive(&mut app, |app| {
            if app.macros.prompt.take().is_some() {
                app.answer_macro(Input::Text(Some("1".into())));
            }
        });
        assert!(
            app.scripts_output
                .iter()
                .any(|l| l.starts_with("Error: ") && l.contains("nothing_here")),
            "{:?}",
            app.scripts_output
        );
        assert!(app.macros.error.is_none());
    }

    #[test]
    fn a_macro_asks_with_msgbox_and_inputbox_and_draws() {
        let code = "Sub Go()\r\n\
            Dim n As String, r As Long\r\n\
            ActiveDocument.Unit = cdrMillimeter\r\n\
            r = MsgBox(\"Draw?\", vbYesNo + vbQuestion, \"Ask\")\r\n\
            If r <> vbYes Then Exit Sub\r\n\
            n = InputBox(\"Name\", \"Ask\", \"box\")\r\n\
            Dim s As Shape\r\n\
            Set s = ActiveLayer.CreateRectangle2(10, 10, 40, 20)\r\n\
            s.Name = n\r\n\
            Debug.Print \"made \" & s.Name\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Ask", code, Vec::new()));
        let before = app
            .doc()
            .all_layers()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        app.run_macro(0, "Main", "Go");
        let mut asked = Vec::new();
        drive(&mut app, |app| match app.macros.prompt.take() {
            Some(Prompt::MsgBox {
                prompt,
                buttons,
                title,
            }) => {
                asked.push(format!("{title}: {prompt} {buttons}"));
                app.answer_macro(Input::Answer(6));
            }
            Some(Prompt::InputBox { prompt, text, .. }) => {
                asked.push(format!("{prompt} [{text}]"));
                app.answer_macro(Input::Text(Some("Frame".into())));
            }
            None => {}
        });
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        assert_eq!(
            asked,
            vec!["Ask: Draw? 36".to_string(), "Name [box]".to_string()]
        );
        let after = app
            .doc()
            .all_layers()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        assert_eq!(after, before + 1);
        assert!(
            app.scripts_output.iter().any(|l| l == "made Frame"),
            "{:?}",
            app.scripts_output
        );
        // The whole run is one undo step.
        assert_eq!(app.engine.undo_label(), Some("Macro"));
        app.undo();
        assert_eq!(
            app.doc()
                .all_layers()
                .map(|l| l.shapes.len())
                .sum::<usize>(),
            before
        );
    }

    #[test]
    fn a_macro_finds_a_palette_spot_colour_by_name_and_fills_with_it() {
        // The pattern of underlay macros: look up an ink by name in every
        // open palette, fill with it and set the fill to overprint.
        let code = "Function FindSpot(nm As String) As Color\r\n\
            Dim pal As Palette, i As Long\r\n\
            For Each pal In Application.Palettes\r\n\
            For i = 1 To pal.ColorCount\r\n\
            If StrComp(pal.Color(i).Name, nm, vbTextCompare) = 0 Then\r\n\
            Set FindSpot = pal.Color(i)\r\n\
            Exit Function\r\n\
            End If\r\n\
            Next i\r\n\
            Next pal\r\n\
            End Function\r\n\
            Sub Go()\r\n\
            Dim c As Color, s As Shape\r\n\
            Set c = FindSpot(\"Spot_1_W\")\r\n\
            If c Is Nothing Then Debug.Print \"missing\": Exit Sub\r\n\
            Debug.Print \"found \" & c.Name & \" \" & c.IsSpot\r\n\
            Set s = ActiveLayer.CreateRectangle2(10, 10, 40, 20)\r\n\
            s.Fill.ApplyUniformFill c\r\n\
            s.OverprintFill = True\r\n\
            Debug.Print \"fill \" & s.Fill.UniformColor.Name\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Spots", code, Vec::new()));
        let white = crate::palette::spot_color(
            "Spot_1_W",
            tracedraw_core::Color::cmyk_pct(0.0, 0.0, 0.0, 20.0),
        );
        app.palettes.push(crate::palette::Palette {
            name: "UV".into(),
            key: "",
            colors: vec![
                ("Spot_1_W".into(), white),
                (
                    "Spot_2_L".into(),
                    crate::palette::spot_color(
                        "Spot_2_L",
                        tracedraw_core::Color::cmyk_pct(50.0, 0.0, 0.0, 0.0),
                    ),
                ),
            ],
            builtin: false,
            path: None,
        });
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        assert!(
            app.scripts_output
                .iter()
                .any(|l| l == "found Spot_1_W True"),
            "{:?}",
            app.scripts_output
        );
        assert!(
            app.scripts_output.iter().any(|l| l == "fill Spot_1_W"),
            "{:?}",
            app.scripts_output
        );
        // The ink joined the drawing under its palette name, and the
        // rectangle is filled with it and overprints.
        let tracedraw_core::Color::Spot { ink, .. } = white else {
            panic!("not a spot colour");
        };
        assert_eq!(
            app.doc().ink(ink).map(|i| i.name.as_str()),
            Some("Spot_1_W")
        );
        let filled = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .find(|s| matches!(s.fill, tracedraw_core::Fill::Solid(c) if c == white))
            .expect("a shape filled with the ink");
        assert!(filled.overprint_fill);
    }

    #[test]
    fn an_underlay_macro_builds_offset_spot_underlays_that_overprint() {
        // The steps of a print underlay macro: inks found by name in the
        // open palettes, a silhouette of the selection, then for each ink
        // a copy contoured outside or inside, separated, filled with the
        // ink and set to overprint.
        let code = "Function FindSpot(nm As String) As Color\r\n\
            Dim pal As Palette, i As Long\r\n\
            On Error Resume Next\r\n\
            For Each pal In Application.Palettes\r\n\
            For i = 1 To pal.ColorCount\r\n\
            If StrComp(pal.Color(i).Name, nm, vbTextCompare) = 0 Then\r\n\
            Set FindSpot = pal.Color(i)\r\n\
            Exit Function\r\n\
            End If\r\n\
            Next i\r\n\
            Next pal\r\n\
            End Function\r\n\
            Function Offset(src As Shape, mm As Double) As Shape\r\n\
            Dim d As Shape, eff As Effect, sep As ShapeRange, res As New ShapeRange, i As Long\r\n\
            Set d = src.Duplicate\r\n\
            If Abs(mm) < 0.001 Then Set Offset = d: Exit Function\r\n\
            If mm > 0 Then\r\n\
            Set eff = d.CreateContour(cdrContourOutside, mm, 1)\r\n\
            Else\r\n\
            Set eff = d.CreateContour(cdrContourInside, -mm, 1)\r\n\
            End If\r\n\
            Set sep = eff.Separate\r\n\
            For i = 1 To sep.Count\r\n\
            If sep(i).StaticID <> d.StaticID Then res.Add sep(i)\r\n\
            Next i\r\n\
            d.Delete\r\n\
            Set Offset = res(1)\r\n\
            End Function\r\n\
            Sub Underlay(sil As Shape, mm As Double, c As Color, nm As String)\r\n\
            Dim u As Shape\r\n\
            Set u = Offset(sil, mm)\r\n\
            u.Fill.ApplyUniformFill c\r\n\
            u.Outline.SetNoOutline\r\n\
            u.OverprintFill = True\r\n\
            u.Name = nm\r\n\
            End Sub\r\n\
            Sub Go()\r\n\
            ActiveDocument.Unit = cdrMillimeter\r\n\
            Dim art As Shape, sil As Shape\r\n\
            Set art = ActiveLayer.CreateRectangle2(10, 10, 40, 20)\r\n\
            art.Fill.UniformColor.CMYKAssign 0, 100, 100, 0\r\n\
            Set sil = art.Duplicate\r\n\
            Underlay sil, -0.1, FindSpot(\"Spot_1_W\"), \"UNDERLAY_W\"\r\n\
            Underlay sil, 0.2, FindSpot(\"Spot_2_L\"), \"UNDERLAY_L\"\r\n\
            Underlay sil, 0, FindSpot(\"Spot_3_K\"), \"UNDERLAY_K\"\r\n\
            sil.Delete\r\n\
            Debug.Print \"done \" & ActiveLayer.Shapes.Count\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Underlay", code, Vec::new()));
        let ink = |n: &str, k: f32| {
            crate::palette::spot_color(n, tracedraw_core::Color::cmyk_pct(0.0, 0.0, 0.0, k))
        };
        let (w, l, k) = (
            ink("Spot_1_W", 10.0),
            ink("Spot_2_L", 20.0),
            ink("Spot_3_K", 30.0),
        );
        app.palettes.push(crate::palette::Palette {
            name: "UV".into(),
            key: "",
            colors: vec![
                ("Spot_1_W".into(), w),
                ("Spot_2_L".into(), l),
                ("Spot_3_K".into(), k),
            ],
            builtin: false,
            path: None,
        });
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        assert!(
            app.scripts_output.iter().any(|x| x == "done 4"),
            "{:?}",
            app.scripts_output
        );
        let shapes: Vec<tracedraw_core::document::Shape> = app
            .doc()
            .all_layers()
            .flat_map(|x| x.shapes.iter().cloned())
            .collect();
        let get = |n: &str| {
            shapes
                .iter()
                .find(|s| s.name.as_deref() == Some(n))
                .unwrap_or_else(|| panic!("{n}"))
                .clone()
        };
        let art = shapes.iter().find(|s| s.name.is_none()).expect("artwork");
        let ab = app.doc().shape_on_page(art.id).expect("art").bounds();
        for (n, c, grow) in [
            ("UNDERLAY_W", w, -0.1),
            ("UNDERLAY_L", l, 0.2),
            ("UNDERLAY_K", k, 0.0),
        ] {
            let u = get(n);
            assert_eq!(u.fill, tracedraw_core::Fill::Solid(c), "{n}");
            assert!(u.overprint_fill, "{n}");
            let b = app.doc().shape_on_page(u.id).expect("underlay").bounds();
            assert!(
                (b.width() - (ab.width() + 2.0 * grow)).abs() < 0.01
                    && (b.height() - (ab.height() + 2.0 * grow)).abs() < 0.01,
                "{n}: {b:?} {ab:?}"
            );
        }
        // Each ink joined the drawing, so a PDF prints it on its own plate.
        for name in ["Spot_1_W", "Spot_2_L", "Spot_3_K"] {
            assert!(app.doc().inks.iter().any(|i| i.name == name), "{name}");
        }
        let pdf = tracedraw_io::pdf::document_to_pdf_with(app.doc(), &Default::default());
        let text = String::from_utf8_lossy(&pdf);
        for name in ["Spot_1_W", "Spot_2_L", "Spot_3_K"] {
            assert!(
                text.contains(&format!("/Separation /{name} ")),
                "{name} is not a separation in the PDF"
            );
        }
        assert!(text.contains("/op true"), "no overprint in the PDF");
    }

    #[test]
    fn artistic_text_takes_its_font_and_size_by_name() {
        // A label written with named arguments, as layout macros do.
        let code = "Sub Go()\r\n\
            Dim t As Shape, pg As Page\r\n\
            Set pg = ActivePage\r\n\
            Set t = pg.ActiveLayer.CreateArtisticText(pg.LeftX, pg.BottomY, \"label\", Font:=\"Arial\", Size:=10)\r\n\
            t.Fill.UniformColor.CMYKAssign 0, 0, 0, 100\r\n\
            Debug.Print \"made \" & t.Text.Story.Text & \" \" & t.Text.Story.Size\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Label", code, Vec::new()));
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        assert!(
            app.scripts_output.iter().any(|l| l == "made label 10"),
            "{:?}",
            app.scripts_output
        );
    }

    /// A VBA macro's ActiveWindow.ActiveView.ToFitAllObjects zooms to
    /// fit what it drew, as View > Zoom To Fit does (#4020).
    #[test]
    fn a_macro_zooms_to_fit_what_it_drew() {
        let code = "Sub Go()\r\n\
            ActiveDocument.Unit = cdrMillimeter\r\n\
            ActiveLayer.CreateRectangle2 150, 120, 20, 20\r\n\
            ActiveWindow.ActiveView.ToFitAllObjects\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Fit", code, Vec::new()));
        app.canvas_rect =
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        app.fit_pending = false;
        app.view.fit(
            tracedraw_core::geometry::Rect::new(0.0, 0.0, 600.0, 600.0),
            app.canvas_rect,
        );
        let before = app.view.zoom;
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        assert!(app.view.zoom > before * 4.0, "{before} {}", app.view.zoom);
    }

    #[test]
    fn a_macro_turns_on_simulated_overprints_and_grouping_keeps_overprint() {
        let code = "Sub Go()\r\n\
            Dim a As Shape, b As Shape, sr As New ShapeRange, g As Shape\r\n\
            Set a = ActiveLayer.CreateRectangle2(10, 10, 40, 20)\r\n\
            Set b = ActiveLayer.CreateRectangle2(12, 12, 36, 16)\r\n\
            b.OverprintFill = True\r\n\
            sr.Add a: sr.Add b\r\n\
            Set g = sr.Group\r\n\
            g.Name = \"UVDTF\"\r\n\
            On Error Resume Next\r\n\
            ActiveWindow.ActiveView.Type = cdrEnhancedViewWithOverprints\r\n\
            Debug.Print \"view \" & (ActiveWindow.ActiveView.Type = cdrEnhancedViewWithOverprints) & \" \" & Err.Number\r\n\
            End Sub\r\n";
        let mut app = app_with(project("View", code, Vec::new()));
        assert!(!app.simulate_overprints);
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        assert!(
            app.scripts_output.iter().any(|l| l == "view True 0"),
            "{:?}",
            app.scripts_output
        );
        assert!(app.simulate_overprints);
        let group = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .find(|s| s.name.as_deref() == Some("UVDTF"))
            .expect("the group")
            .clone();
        let tracedraw_core::document::ShapeKind::Group { children } = &group.kind else {
            panic!("not a group");
        };
        assert_eq!(children.iter().filter(|c| c.overprint_fill).count(), 1);
    }

    #[test]
    fn dir_tells_whether_a_file_exists() {
        let dir = std::env::temp_dir().join(format!("td_dir_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("layout_15.pdf"), b"x").unwrap();
        let here = dir.join("layout_15.pdf").display().to_string();
        let gone = dir.join("nothing.pdf").display().to_string();
        let star = dir.join("lay*.pdf").display().to_string();
        let code = format!(
            "Sub Go()\r\nDebug.Print \"[\" & Dir(\"{here}\") & \"][\" & Dir(\"{gone}\") & \"][\" & Dir(\"{star}\") & \"]\"\r\nEnd Sub\r\n"
        );
        let mut app = app_with(project("Dir", &code, Vec::new()));
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        assert!(
            app.scripts_output
                .iter()
                .any(|l| l == "[layout_15.pdf][][layout_15.pdf]"),
            "{:?}",
            app.scripts_output
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_form_button_draws_and_reads_its_text_box() {
        let mut form = vp::Form {
            name: "Box".into(),
            caption: "Make a box".into(),
            width: 200.0,
            height: 100.0,
            enabled: true,
            ..Default::default()
        };
        let mut tb = vp::Control::new(vp::ControlKind::TextBox, "tbName");
        tb.value = "first".into();
        let ok = vp::Control::new(vp::ControlKind::CommandButton, "cbOK");
        form.controls = vec![tb, ok];
        let mut p = project(
            "Forms",
            "Sub Go()\r\nBox.Show\r\nDebug.Print \"closed\"\r\nEnd Sub\r\n",
            vec![form],
        );
        p.modules[1].source = "Private Sub UserForm_Initialize()\r\n\
            Me.Caption = \"Box maker\"\r\n\
            End Sub\r\n\
            Private Sub cbOK_Click()\r\n\
            Dim s As Shape\r\n\
            Set s = ActiveLayer.CreateEllipse2(1, 1, 0.5)\r\n\
            s.Name = tbName.Text\r\n\
            Unload Me\r\n\
            End Sub\r\n"
            .into();
        let mut app = app_with(p);
        app.run_macro(0, "Main", "Go");
        let mut stage = 0;
        drive(&mut app, |app| {
            let Some(run) = &app.macros.run else { return };
            let shown = run
                .forms
                .lock()
                .unwrap()
                .list
                .iter()
                .find(|f| f.visible)
                .map(|f| (f.id, f.caption.clone()));
            if let (0, Some((id, caption))) = (stage, shown) {
                assert_eq!(caption, "Box maker");
                // The user types and clicks OK.
                run.forms
                    .lock()
                    .unwrap()
                    .set_prop(id, "tbName", "Text", &serde_json::json!("Round"))
                    .unwrap();
                app.macro_form_event(id, "tbName", "Change", Vec::new());
                app.macro_form_event(id, "cbOK", "Click", Vec::new());
                stage = 1;
            }
        });
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let names: Vec<String> = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .filter_map(|s| s.name.clone())
            .collect();
        assert!(names.contains(&"Round".to_string()), "{names:?}");
        assert!(app.scripts_output.iter().any(|l| l == "closed"));
    }

    /// What a published tiling macro reads and sets (#1255, #2165): the
    /// page's paper by name, a paper set by name, and an outline set with
    /// `OutlineStyles`, `ArrowHeads` and named arguments; it stopped with
    /// error 438 on `Paper`, then with "Sub or Function not defined" on
    /// `OutlineStyles`.
    #[test]
    fn a_tiling_macro_reads_the_paper_and_sets_outlines_by_style() {
        let code = "Sub Go()\r\n\
            ActiveDocument.Unit = cdrMillimeter\r\n\
            Debug.Print \"paper \" & ActiveDocument.ActivePage.Paper\r\n\
            Dim r As Shape\r\n\
            Set r = ActiveLayer.CreateRectangle(0#, 30#, 40#, 0#)\r\n\
            r.Outline.SetProperties 0.5, OutlineStyles(0), CreateCMYKColor(0, 0, 0, 100), ArrowHeads(0), ArrowHeads(0), cdrFalse, cdrFalse, cdrOutlineRoundLineCaps, cdrOutlineRoundLineJoin, 0#, 100, MiterLimit:=5#\r\n\
            Dim d As Shape\r\n\
            Set d = ActiveLayer.CreateLineSegment(0#, 0#, 50#, 0#)\r\n\
            d.Outline.SetProperties 0.3, OutlineStyles(1)\r\n\
            Debug.Print \"styles \" & OutlineStyles.Count\r\n\
            ActivePage.Paper = \"A5\"\r\n\
            Debug.Print \"now \" & ActivePage.Paper & \" \" & ActivePage.SizeWidth\r\n\
            ActivePage.SizeWidth = 123\r\n\
            Debug.Print \"custom [\" & ActivePage.Paper & \"]\"\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Tiling", code, Vec::new()));
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let out = &app.scripts_output;
        assert!(out.iter().any(|l| l == "paper A4"), "{out:?}");
        assert!(out.iter().any(|l| l == "now A5 148"), "{out:?}");
        assert!(out.iter().any(|l| l == "custom []"), "{out:?}");
        assert!(out.iter().any(|l| l.starts_with("styles ")), "{out:?}");
        let strokes: Vec<tracedraw_core::Stroke> = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .filter_map(|s| s.stroke.clone())
            .collect();
        let rect = strokes
            .iter()
            .find(|s| (s.width - 0.5).abs() < 1e-9)
            .expect("the rectangle's outline");
        assert!(rect.dash.is_empty());
        assert_eq!(rect.cap, tracedraw_core::style::LineCap::Round);
        assert_eq!(rect.join, tracedraw_core::style::LineJoin::Round);
        let line = strokes
            .iter()
            .find(|s| (s.width - 0.3).abs() < 1e-9)
            .expect("the line's outline");
        assert!(!line.dash.is_empty(), "style 1 is dashed");
    }

    /// Copy and Paste in a macro (#2165): the tiling macro of the same
    /// collection copies the crop marks of one side, pastes them and
    /// moves what was pasted (the selection) to the other side. It
    /// stopped with error 438, "Object doesn't support this property or
    /// method: Copy". The macro's clipboard is its own.
    #[test]
    fn a_macro_copies_and_pastes_shapes() {
        let code = "Sub Go()\r\n\
            ActiveDocument.Unit = cdrMillimeter\r\n\
            Dim a As Shape, b As Shape, g As Shape, r As ShapeRange\r\n\
            Set a = ActiveLayer.CreateLineSegment(0#, 0#, 5#, 0#)\r\n\
            Set b = ActiveLayer.CreateLineSegment(0#, 10#, 5#, 10#)\r\n\
            Set g = ActiveDocument.CreateShapeRangeFromArray(a, b).Group\r\n\
            g.Copy\r\n\
            ActiveLayer.Paste\r\n\
            Set r = ActiveSelectionRange\r\n\
            Debug.Print \"pasted \" & r.Count & \" \" & (r.Item(1).StaticID <> g.StaticID) & \" \" & r.LeftX\r\n\
            r.Move -20#, 0#\r\n\
            Debug.Print \"moved \" & r.LeftX & \" kept \" & g.LeftX\r\n\
            g.Cut\r\n\
            Set r = ActiveLayer.Paste\r\n\
            Debug.Print \"cut \" & ActiveLayer.Shapes.Count & \" \" & r.Count\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Clip", code, Vec::new()));
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let out = &app.scripts_output;
        for want in ["pasted 1 True 0", "moved -20 kept 0", "cut 2 1"] {
            assert!(out.iter().any(|l| l == want), "{want}: {out:?}");
        }
        // The app's clipboard is not touched.
        assert!(app.clipboard.is_none());
    }

    /// A macro that puts each selected object into a clip frame of its
    /// own (#2968), and one that centres the contents on the frame. The
    /// macro of the report calls these members by other names, which
    /// reach them through `__member_alias` (see vg_tests).
    #[test]
    fn a_macro_puts_each_selected_object_into_its_own_clip_frame() {
        let code = "Sub Go()\r\n\
            ActiveDocument.Unit = cdrMillimeter\r\n\
            Dim s As Shape, r As Shape, f As Shape, c As Shape\r\n\
            ActiveDocument.CreateShapeRangeFromArray(ActiveLayer.CreateEllipse2(10#, 10#, 5#), ActiveLayer.CreateEllipse2(40#, 10#, 5#)).CreateSelection\r\n\
            For Each s In ActiveSelectionRange\r\n\
            Set r = ActiveLayer.CreateRectangle2(s.LeftX - 2#, s.BottomY - 2#, s.SizeWidth + 4#, s.SizeHeight + 4#)\r\n\
            s.AddToClipFrame r\r\n\
            Next s\r\n\
            Debug.Print \"frames \" & ActiveLayer.Shapes.Count\r\n\
            For Each f In ActiveLayer.Shapes\r\n\
            Debug.Print \"holds \" & f.ClipFrame.Shapes.Count & \" \" & (f.ClipFrame.Shapes.Item(1).ClipFrameParent.StaticID = f.StaticID)\r\n\
            Next f\r\n\
            Set f = ActiveLayer.CreateRectangle2(0#, 0#, 20#, 20#)\r\n\
            Set c = ActiveLayer.CreateEllipse2(100#, 100#, 3#)\r\n\
            c.AddToClipFrame f, cdrTrue\r\n\
            Set c = f.ClipFrame.Shapes.Item(1)\r\n\
            Debug.Print \"centred \" & Round(c.CenterX, 3) & \" \" & Round(c.CenterY, 3)\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Frames", code, Vec::new()));
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let out = &app.scripts_output;
        let holds = out.iter().filter(|l| *l == "holds 1 True").count();
        assert!(out.iter().any(|l| l == "frames 2"), "{out:?}");
        assert_eq!(holds, 2, "{out:?}");
        assert!(out.iter().any(|l| l == "centred 10 10"), "{out:?}");
    }

    /// What the macros of a published collection use that stopped them
    /// (#2165): `FontList` (For Each over the installed families),
    /// `ActiveVirtualLayer`, `InsertPagesEx` (its argument named `Page`
    /// hid the page class: "not a constructor"), `LogCreateShapeRange`,
    /// and `ShellExecute` declared from the Windows library, which they
    /// call to open their maker's page when a form closes: a web address
    /// is opened, anything else is not and returns 2.
    #[test]
    fn collection_macros_find_fonts_pages_and_open_their_pages() {
        let code = "Private Declare PtrSafe Function OpenIt Lib \"shell32.dll\" Alias \"ShellExecuteA\" (ByVal hwnd As Long, ByVal op As String, ByVal file As String, ByVal params As String, ByVal dir As String, ByVal show As Long) As Long\r\n\
            Sub Go()\r\n\
            Dim v As Variant, n As Long\r\n\
            For Each v In FontList\r\n\
            n = n + 1\r\n\
            Next v\r\n\
            Debug.Print \"fonts \" & (n = FontList.Count)\r\n\
            ActiveDocument.Unit = cdrMillimeter\r\n\
            Dim s As Shape, sr As New ShapeRange\r\n\
            Set s = ActiveVirtualLayer.CreateRectangle(0#, 10#, 10#, 0#)\r\n\
            sr.Add s\r\n\
            Set sr = ActiveDocument.LogCreateShapeRange(sr)\r\n\
            Debug.Print \"made \" & sr.Count\r\n\
            Dim p As Page\r\n\
            Set p = ActiveDocument.InsertPagesEx(1, False, ActivePage.Index, 100#, 50#)\r\n\
            Debug.Print \"pages \" & ActiveDocument.Pages.Count & \" \" & p.SizeWidth & \"x\" & p.SizeHeight\r\n\
            Debug.Print \"web \" & (OpenIt(0, vbNullString, \"http://example.com/page\", \"\", \"\", 1) > 32)\r\n\
            Debug.Print \"file \" & OpenIt(0, \"open\", \"C:\\Windows\\notepad.exe\", \"\", \"\", 1)\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Collection", code, Vec::new()));
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let out = &app.scripts_output;
        for want in [
            "fonts True",
            "made 1",
            "pages 2 100x50",
            "web True",
            "file 2",
        ] {
            assert!(out.iter().any(|l| l == want), "{want}: {out:?}");
        }
        assert_eq!(
            app.macros.opened,
            vec!["http://example.com/page".to_string()]
        );
    }

    /// Forms of the same collection (#2165): one whose Initialize sets its
    /// controls through the form's own name (`Crop.ctlLength = 5`, its
    /// default property; making the form there made it again, without
    /// end) and sets the drop-down's `ListWidth`, and one that unloads
    /// itself in Activate and then goes on setting a control (error 5
    /// "the form was unloaded").
    #[test]
    fn a_form_names_itself_in_initialize_and_unloads_itself_in_activate() {
        let mut crop = vp::Form {
            name: "Crop".into(),
            caption: "Crop".into(),
            width: 200.0,
            height: 100.0,
            enabled: true,
            ..Default::default()
        };
        crop.controls = vec![
            vp::Control::new(vp::ControlKind::TextBox, "ctlLength"),
            vp::Control::new(vp::ControlKind::ComboBox, "ctlUnits"),
        ];
        let mut gone = vp::Form {
            name: "Gone".into(),
            caption: "Gone".into(),
            width: 200.0,
            height: 100.0,
            enabled: true,
            ..Default::default()
        };
        gone.controls = vec![vp::Control::new(vp::ControlKind::TextBox, "tbPages")];
        let mut p = project(
            "Forms",
            "Sub Go()\r\nCrop.Show\r\nGone.Show\r\nDebug.Print \"done\"\r\nEnd Sub\r\n",
            vec![crop, gone],
        );
        p.modules[1].source = "Private Sub UserForm_Initialize()\r\n\
            Crop.ctlUnits.AddItem \"mm\"\r\n\
            Crop.ctlUnits.ListWidth = 34\r\n\
            Crop.ctlLength = 5\r\n\
            Debug.Print \"length \" & ctlLength.Text & \" \" & ctlUnits.ListCount\r\n\
            End Sub\r\n"
            .into();
        p.modules[2].source = "Private Sub UserForm_Activate()\r\n\
            Unload Me\r\n\
            tbPages.Value = 1\r\n\
            Debug.Print \"after \" & tbPages.Value\r\n\
            End Sub\r\n"
            .into();
        let mut app = app_with(p);
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |app| {
            let Some(run) = &app.macros.run else { return };
            let open: Vec<u32> = run
                .forms
                .lock()
                .unwrap()
                .list
                .iter()
                .filter(|f| f.visible)
                .map(|f| f.id)
                .collect();
            for id in open {
                app.macro_form_event(id, "", "QueryClose", vec![serde_json::json!(0)]);
            }
        });
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let out = &app.scripts_output;
        for want in ["length 5 1", "after 1", "done"] {
            assert!(out.iter().any(|l| l == want), "{want}: {out:?}");
        }
    }

    /// A form that downloads its picture with URLDownloadToCacheFile and
    /// shows it with LoadPicture (QRCodeInserter of #2165 stopped with
    /// error 445 on the Windows function): the file comes into a file of
    /// the app's own, its path back into the macro's buffer, and the
    /// image control shows it.
    #[test]
    fn a_form_downloads_its_picture_and_shows_it() {
        let dir = std::env::temp_dir().join(format!("td-download-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("qr code.png");
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            3,
            3,
            image::Rgba([0, 0, 0, 255]),
        ))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        std::fs::write(&src, &png).unwrap();
        let url = format!(
            "file://{}",
            src.to_string_lossy().replace('\\', "/").replace(' ', "%20")
        );
        let mut qr = vp::Form {
            name: "QR".into(),
            caption: "QR".into(),
            width: 200.0,
            height: 100.0,
            enabled: true,
            ..Default::default()
        };
        qr.controls = vec![vp::Control::new(vp::ControlKind::Image, "imgQR")];
        let code = "Private Declare PtrSafe Function URLDownloadToCacheFile Lib \"urlmon\" Alias \"URLDownloadToCacheFileA\" (ByVal lpUnkcaller As Long, ByVal szURL As String, ByVal szFileName As String, ByVal dwBufLength As Long, ByVal dwReserved As Long, ByVal IBindStatusCallback As Long) As Long\r\n\
             Public Function DownloadFile(URL As String) As String\r\n\
             Dim szFileName As String\r\n\
             szFileName = Space$(300)\r\n\
             If URLDownloadToCacheFile(0, URL, szFileName, Len(szFileName), 0, 0) = 0 Then\r\n\
             DownloadFile = Trim(szFileName)\r\n\
             End If\r\n\
             End Function\r\n\
             Sub Go()\r\n\
             Debug.Print \"missing [\" & DownloadFile(\"file:///no/such/file.png\") & \"]\"\r\n\
             QR.Show\r\n\
             End Sub\r\n";
        let mut p = project("Forms", code, vec![qr]);
        p.modules[1].source = format!(
            "Private Sub UserForm_Initialize()\r\n\
             Dim f As String\r\n\
             f = Main.DownloadFile(\"{url}\")\r\n\
             Debug.Print \"got \" & (Dir(f) <> \"\")\r\n\
             imgQR.Picture = LoadPicture(f)\r\n\
             End Sub\r\n"
        );
        let mut app = app_with(p);
        app.run_macro(0, "Main", "Go");
        let mut shown = None;
        drive(&mut app, |app| {
            let Some(run) = &app.macros.run else { return };
            let open: Vec<(u32, Option<Vec<u8>>)> = run
                .forms
                .lock()
                .unwrap()
                .list
                .iter()
                .filter(|f| f.visible)
                .map(|f| (f.id, f.control("imgQR").and_then(|c| c.picture.clone())))
                .collect();
            for (id, pic) in open {
                shown = pic;
                app.macro_form_event(id, "", "QueryClose", vec![serde_json::json!(0)]);
            }
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let out = &app.scripts_output;
        for want in ["missing []", "got True"] {
            assert!(out.iter().any(|l| l == want), "{want}: {out:?}");
        }
        assert_eq!(shown.as_deref(), Some(&png[..]));
    }

    #[test]
    fn errors_name_the_line_and_stop_ends_a_loop() {
        let code = "Sub Bad()\r\nDim a As Long\r\na = 1 / 0\r\nEnd Sub\r\nSub Forever()\r\nDim i As Long\r\nDo\r\ni = i + 1\r\nLoop\r\nEnd Sub\r\n";
        let mut app = app_with(project("Err", code, Vec::new()));
        app.run_macro(0, "Main", "Bad");
        drive(&mut app, |_| {});
        let (e, p) = app.macros.error.take().expect("an error");
        assert_eq!((e.number, e.line, p), (11, 3, 0), "{e:?}");
        assert_eq!(e.module, "Main");
        app.run_macro(0, "Main", "Forever");
        std::thread::sleep(std::time::Duration::from_millis(200));
        app.stop_macro();
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none());
        assert!(app.status.contains("Err.Main.Forever"), "{}", app.status);
    }

    #[test]
    fn a_project_written_as_gms_runs_after_loading() {
        let code = "Sub Count()\r\nDebug.Print ActivePage.Shapes.Count + 41\r\nEnd Sub\r\n";
        let bytes = vp::write_gms(&project("Saved", code, Vec::new())).unwrap();
        let dir = temp("gms-file");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Saved.gms");
        std::fs::write(&file, &bytes).unwrap();
        let mut app = App::headless();
        app.macros.dir = Some(dir.join("kept"));
        app.macros.loaded = true;
        app.install_macro_files(&[file]);
        assert_eq!(app.macros.projects.len(), 1);
        app.run_macro(0, "Main", "Count");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let n = app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.shapes.len())
            .sum::<usize>()
            + 41;
        assert!(
            app.scripts_output.contains(&n.to_string()),
            "{:?}",
            app.scripts_output
        );
    }

    /// Local check of projects on disk (not run by default): every
    /// `.gms` in `TRACEDRAW_MACRO_FILES` (separated by ':') is compiled
    /// and listed; the macros named in `TRACEDRAW_MACRO_RUN`
    /// ("Module.Macro,...") run on a sample drawing, answering each box
    /// with its first button or default text and closing each form
    /// (after a click on the button `TRACEDRAW_MACRO_CLICK` names, once).
    #[test]
    #[ignore]
    fn projects_from_the_environment() {
        let Ok(files) = std::env::var("TRACEDRAW_MACRO_FILES") else {
            return;
        };
        let wanted: Vec<String> = std::env::var("TRACEDRAW_MACRO_RUN")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        for f in files.split(':') {
            let bytes = std::fs::read(f).unwrap();
            let project = vp::read_gms(&bytes).unwrap();
            let mut app = App::headless();
            app.macros.dir = Some(temp("env"));
            app.macros.loaded = true;
            crate::scripting::run(
                &mut app,
                "for (let i = 0; i < 3; i++) { const r = ActiveLayer.CreateRectangle(10 + i * 30, 10, 30 + i * 30, 30); r.Fill.ApplyUniformFill(Color.CMYK(0, 100, 100, 0)); r.CreateSelection ? 0 : 0; } ActiveLayer.CreateArtisticText(10, 80, 'Sample text');",
            )
            .unwrap();
            app.select_all();
            let i = app.macros.install(project, Some(&bytes)).unwrap();
            let p = &app.macros.projects[i];
            println!(
                "== {} ({} modules, {} forms)",
                p.name(),
                p.project.modules.len(),
                p.project.forms.len()
            );
            for w in &p.warnings {
                println!("   warning: {w}");
            }
            match p.compile() {
                Ok(_) => println!(
                    "   compiles; macros: {:?}",
                    p.macros()
                        .iter()
                        .map(|(m, n)| format!("{m}.{n}"))
                        .collect::<Vec<_>>()
                ),
                Err(e) => println!("   COMPILE ERROR {e}"),
            }
            // The Macro Editor's compatibility report.
            for issue in crate::macro_convert::check(&p.project).issues {
                println!("   check {:?} line {}: {}", issue.level, issue.line, issue.text);
            }
            let macros = p.macros();
            for (m, n) in macros {
                if !wanted.iter().any(|w| w == &format!("{m}.{n}")) {
                    continue;
                }
                app.select_all();
                app.run_macro(i, &m, &n);
                let mut shown = Vec::new();
                let mut click = std::env::var("TRACEDRAW_MACRO_CLICK").ok();
                drive(&mut app, |app| {
                    match app.macros.prompt.take() {
                        Some(Prompt::MsgBox {
                            prompt, buttons, ..
                        }) => {
                            shown.push(format!(
                                "MsgBox({buttons}) {}",
                                prompt.lines().next().unwrap_or("")
                            ));
                            let first = match buttons & 0xF {
                                2 => 3,
                                3 | 4 => 6,
                                5 => 4,
                                _ => 1,
                            };
                            app.answer_macro(Input::Answer(first));
                        }
                        Some(Prompt::InputBox { text, .. }) => {
                            shown.push("InputBox".into());
                            app.answer_macro(Input::Text(Some(text)));
                        }
                        None => {}
                    }
                    let Some(run) = &app.macros.run else { return };
                    let visible: Vec<(u32, String)> = run
                        .forms
                        .lock()
                        .unwrap()
                        .list
                        .iter()
                        .filter(|f| f.visible)
                        .map(|f| (f.id, f.caption.clone()))
                        .collect();
                    for (id, caption) in visible {
                        shown.push(format!("form {caption}"));
                        // The button `TRACEDRAW_MACRO_CLICK` names is
                        // clicked once before the form closes.
                        if let Some(b) = click.take() {
                            shown.push(format!("click {b}"));
                            app.macro_form_event(id, &b, "Click", Vec::new());
                            std::thread::sleep(std::time::Duration::from_millis(300));
                            continue;
                        }
                        app.macro_form_event(id, "", "QueryClose", vec![serde_json::json!(0)]);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                });
                let err = app.macros.error.take();
                if let Some((_, steps, _)) = app.macros.skipped.take() {
                    for (e, n) in steps {
                        println!(
                            "   skipped x{n}: {} {} ({}.{} line {})",
                            e.number, e.description, e.module, e.procedure, e.line
                        );
                    }
                }
                let objects: usize = app.doc().all_layers().map(|l| l.shapes.len()).sum();
                println!(
                    "   run {m}.{n}: {} {:?} objects={objects} out={:?}",
                    match &err {
                        None => "ok".to_string(),
                        Some((e, _)) => format!(
                            "ERROR {} {} ({}.{} line {})",
                            e.number, e.description, e.module, e.procedure, e.line
                        ),
                    },
                    shown,
                    app.scripts_output.iter().take(5).collect::<Vec<_>>()
                );
                app.scripts_output.clear();
            }
        }
    }

    /// The crop-marks form of the same collection (#2165): it reads the
    /// object's box through the variables it hands `GetBoundingBox` (with
    /// the outline), draws each mark with `CreateLineSegment`, gathers
    /// the marks from `ActiveSelectionRange` (CorelDRAW selects what it
    /// makes) and groups them. The marks were drawn around the page's
    /// corner and the object itself was grouped with them.
    #[test]
    fn a_crop_marks_macro_marks_the_corners_of_the_object() {
        let code = "Sub Go()\r\n\
            Dim xPos As Double, yPos As Double, xWidth As Double, yHeight As Double\r\n\
            Dim s As Shape, sr As ShapeRange, crop As Shape, cropRange As New ShapeRange\r\n\
            ActiveDocument.Unit = cdrMillimeter\r\n\
            Set s = ActiveLayer.CreateRectangle(50#, 80#, 90#, 60#)\r\n\
            s.Outline.SetProperties 1#\r\n\
            Set sr = ActiveSelectionRange\r\n\
            Debug.Print \"selected \" & sr.Count\r\n\
            For Each s In sr\r\n\
            s.GetBoundingBox xPos, yPos, xWidth, yHeight, True\r\n\
            Debug.Print \"box \" & xPos & \" \" & yPos & \" \" & xWidth & \" \" & yHeight\r\n\
            Set crop = ActiveLayer.CreateLineSegment(xPos - 8, yPos, xPos - 3, yPos)\r\n\
            crop.Outline.SetProperties 0.2\r\n\
            cropRange.AddRange ActiveSelectionRange\r\n\
            Set crop = ActiveLayer.CreateLineSegment(xPos + xWidth, yPos + yHeight + 3, xPos + xWidth, yPos + yHeight + 8)\r\n\
            crop.Outline.SetProperties 0.2\r\n\
            cropRange.AddRange ActiveSelectionRange\r\n\
            Next s\r\n\
            Debug.Print \"marks \" & cropRange.Count\r\n\
            cropRange.Group\r\n\
            End Sub\r\n";
        let mut app = app_with(project("Crop", code, Vec::new()));
        app.run_macro(0, "Main", "Go");
        drive(&mut app, |_| {});
        assert!(app.macros.error.is_none(), "{:?}", app.macros.error);
        let out = &app.scripts_output;
        for want in ["selected 1", "box 49.5 59.5 41 21", "marks 2"] {
            assert!(out.iter().any(|l| l == want), "{want}: {out:?}");
        }
        let top: Vec<&tracedraw_core::document::Shape> = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .collect();
        assert_eq!(top.len(), 2, "the rectangle and the group of marks");
        let group = top
            .iter()
            .find(|s| matches!(s.kind, tracedraw_core::document::ShapeKind::Group { .. }))
            .expect("the marks are grouped");
        let b = group.bounds();
        assert!(
            (b.x0 - 41.5).abs() < 1e-6 && (b.x1 - 90.5).abs() < 1e-6,
            "{b:?}"
        );
        assert!(
            (b.y0 - 59.5).abs() < 1e-6 && (b.y1 - 88.5).abs() < 1e-6,
            "{b:?}"
        );
    }
}
