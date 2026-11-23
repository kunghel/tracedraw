//! Workspace files (`.tdws`): the user's own setup of the window, kept in
//! a file to carry to another computer or to go back to later. A file
//! holds the parts chosen when it was written (keyboard shortcuts; where
//! the bars sit and which show, with the toolbox; the colour palette
//! options; the interface size, desktop colour and page border; every
//! other preference, such as the colour mode and the document defaults),
//! and an import applies the parts chosen then that the file has. Files are
//! JSON; anything odd in one is dropped or set back to its default, never
//! trusted.

use crate::app::{App, Workspace};
use crate::i18n::{tr, trf};
use crate::settings::{PaletteBarPrefs, PalettePrefs, ShownBars};
use crate::ui::bars::BarLayout;
use crate::ui::status::StatusInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// What every workspace file says it is.
pub const KIND: &str = "tracedraw-workspace";
/// The version written. Files of later versions are read as far as their
/// parts are understood.
pub const VERSION: u32 = 1;
/// The file name extension.
pub const EXTENSION: &str = "tdws";
/// Larger files are not workspace files.
const MAX_FILE: usize = 4 << 20;
/// At most this many shortcuts are taken from a file.
const MAX_SHORTCUTS: usize = 4096;
/// Ids, keys and tool names are short.
const MAX_TEXT: usize = 64;
/// Toolbox lists are short.
const MAX_LIST: usize = 256;
/// Floating bars lie within this distance of the origin (points).
const MAX_POSITION: f32 = 100_000.0;

/// The parts a workspace file can hold; kept in the preferences so the
/// choice is remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceParts {
    pub shortcuts: bool,
    pub bars: bool,
    pub palette: bool,
    pub appearance: bool,
    /// Every other preference (#3721).
    pub preferences: bool,
}

impl Default for WorkspaceParts {
    fn default() -> Self {
        WorkspaceParts::ALL
    }
}

impl WorkspaceParts {
    pub const ALL: WorkspaceParts = WorkspaceParts {
        shortcuts: true,
        bars: true,
        palette: true,
        appearance: true,
        preferences: true,
    };

    /// Each part with the i18n key of its name, in the order they are
    /// listed.
    pub fn flags_mut(&mut self) -> [(&mut bool, &'static str); 5] {
        [
            (&mut self.shortcuts, PART_SHORTCUTS),
            (&mut self.bars, PART_BARS),
            (&mut self.palette, PART_PALETTE),
            (&mut self.appearance, PART_APPEARANCE),
            (&mut self.preferences, PART_PREFERENCES),
        ]
    }

    pub fn any(&self) -> bool {
        self.shortcuts || self.bars || self.palette || self.appearance || self.preferences
    }
}

const PART_SHORTCUTS: &str = "workspace.part_shortcuts";
const PART_BARS: &str = "workspace.part_bars";
const PART_PALETTE: &str = "workspace.part_palette";
const PART_APPEARANCE: &str = "workspace.part_appearance";
const PART_PREFERENCES: &str = "workspace.part_preferences";

/// Preferences the preferences part never carries: those of this
/// computer or of one session (files and folders opened, the window, the
/// graphics card, measurements, updates, keys and accounts, programs and
/// scripts installed here), and those another part carries.
const NOT_CARRIED: &[&str] = &[
    // This computer and this session.
    "recent_files",
    "last_dir",
    "last_export_dir",
    "folder_bookmarks",
    "thesaurus_file",
    "bitmap_editor",
    "window_maximized",
    "window_size",
    "hardware_acceleration",
    "gpu_adapter",
    "gpu_effects",
    "working_renderer",
    "screen_ppi",
    "benchmarks",
    "benchmark_resume",
    "upgrades",
    "check_updates",
    "update_check_asked",
    "last_update_check",
    "skipped_update",
    "pending_update",
    "recent_fonts",
    "recent_glyphs",
    "font_manager",
    "welcome_tab_closed",
    "github",
    "ai",
    "ai_enabled",
    "ai_vectorize",
    "ai_image",
    "mcp_window",
    "script_filters",
    "plugins",
    "object_apps",
    "workspace_parts",
    // The other parts.
    "workspace",
    "shortcuts",
    "node_shortcuts",
    "bars",
    "shown_bars",
    "dockers",
    "toolbox",
    "toolbox_hidden",
    "show_outline_flyout",
    "status_info",
    "bar_items",
    "palette",
    "palette_bars",
    "main_palette",
    "palette_names",
    "ui_size",
    "desktop_rgb",
    "desktop_custom",
    "theme",
    "center_dialogs",
    "ui_shadows",
    "colored_icons",
    "docker_tabs_icons_only",
    "follow_text_size",
    "follow_high_contrast",
];
/// At most this many preferences are taken from a file.
const MAX_PREFERENCES: usize = 1024;

fn carried(key: &str) -> bool {
    short_text(key) && !NOT_CARRIED.contains(&key)
}

/// The preferences the preferences part carries, as the preferences file
/// writes them.
fn carried_preferences(
    s: &crate::settings::Settings,
) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::to_value(s) {
        Ok(serde_json::Value::Object(mut all)) => {
            all.retain(|k, _| carried(k));
            all
        }
        _ => serde_json::Map::new(),
    }
}

/// `current` with the preferences of `from` over it. A value of the wrong
/// shape keeps the current one; the others still come across.
fn merged_preferences(
    current: &crate::settings::Settings,
    from: &serde_json::Map<String, serde_json::Value>,
) -> crate::settings::Settings {
    use serde_json::Value;
    let Ok(Value::Object(mut base)) = serde_json::to_value(current) else {
        return current.clone();
    };
    let keys: Vec<&String> = from.keys().filter(|k| carried(k)).collect();
    let mut all = base.clone();
    for k in &keys {
        all.insert((*k).clone(), from[*k].clone());
    }
    if let Ok(s) = serde_json::from_value(Value::Object(all)) {
        return s;
    }
    for k in keys {
        let old = base.insert(k.clone(), from[k].clone());
        if serde_json::from_value::<crate::settings::Settings>(Value::Object(base.clone())).is_err()
        {
            match old {
                Some(v) => base.insert(k.clone(), v),
                None => base.remove(k),
            };
        }
    }
    serde_json::from_value(Value::Object(base)).unwrap_or_else(|_| current.clone())
}

/// Where the bars sit and which show, and the toolbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BarsPart {
    pub layout: BarLayout,
    pub shown: ShownBars,
    /// The tool each toolbox group shows (tool ids, in group order).
    pub toolbox: Vec<String>,
    /// Toolbox flyouts hidden with the toolbox's "+" button.
    pub toolbox_hidden: Vec<String>,
    pub show_outline_flyout: bool,
    pub status_info: StatusInfo,
    /// The buttons of the Standard and Zoom toolbars, when changed.
    pub bar_items: std::collections::BTreeMap<String, Vec<String>>,
    /// The dockers open, their columns, the one in front of each and the
    /// floating ones with their windows: a workspace brings its panels
    /// back (#3350). Absent in files written before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dockers: Option<crate::settings::DockerSetup>,
}

/// The interface's look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearancePart {
    /// The interface size relative to the system's scaling (1 = 100 %);
    /// 0 is 100 % too.
    pub ui_size: f32,
    pub desktop_rgb: [u8; 3],
    /// The desktop colour was picked (white is then white, #530).
    pub desktop_custom: bool,
    pub page_border: bool,
    /// The page's shadow (#1906).
    pub page_shadow: bool,
    /// The rest of Customization > Appearance: the theme, dialogs in the
    /// middle, shadows, coloured tool icons, panel tabs as pictures and
    /// following the desktop's text size and high contrast. An exported
    /// workspace left them out, so importing it kept the theme and the
    /// rest as they were. `None` in a file written before: kept as is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center_dialogs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_shadows: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colored_icons: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_tabs_icons_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_text_size: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_high_contrast: Option<bool>,
}

impl Default for AppearancePart {
    fn default() -> Self {
        AppearancePart {
            ui_size: 0.0,
            desktop_rgb: [255, 255, 255],
            desktop_custom: false,
            page_border: true,
            page_shadow: true,
            theme: None,
            center_dialogs: None,
            ui_shadows: None,
            colored_icons: None,
            docker_tabs_icons_only: None,
            follow_text_size: None,
            follow_high_contrast: None,
        }
    }
}

/// A workspace file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WorkspaceFile {
    /// Always [`KIND`].
    pub kind: String,
    pub version: u32,
    /// The built-in workspace the bars start from (Window > Workspace).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Menu command keys and tool ids with their shortcuts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcuts: Option<Vec<(String, String)>>,
    /// The Node editing table's shortcuts (with the shortcuts part,
    /// #3243).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_shortcuts: Option<Vec<(String, String)>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bars: Option<BarsPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette: Option<PalettePrefs>,
    /// The palette bars open besides the default one (with the palette
    /// part).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_bars: Option<Vec<PaletteBarPrefs>>,
    /// The palette the default bar shows (with the palette part).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_palette: Option<String>,
    /// The names given to palettes (with the palette part).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_names: Option<std::collections::BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appearance: Option<AppearancePart>,
    /// Every other preference, as the preferences file writes them: the
    /// colour mode and colour management, the document defaults, units,
    /// tools, view, saving, printing and the rest (#3721).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferences: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Why a file was not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// Too large, not JSON, or a part not shaped as it should be.
    Unreadable(String),
    /// JSON, but not a workspace file.
    NotAWorkspace,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Unreadable(why) => write!(f, "unreadable workspace file: {why}"),
            ReadError::NotAWorkspace => write!(f, "not a workspace file"),
        }
    }
}

impl WorkspaceFile {
    /// The file as pretty JSON.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).unwrap_or_default()
    }

    /// Read a workspace file, dropping or resetting whatever is out of
    /// bounds in it.
    pub fn from_bytes(bytes: &[u8]) -> Result<WorkspaceFile, ReadError> {
        if bytes.len() > MAX_FILE {
            return Err(ReadError::Unreadable(format!(
                "{} bytes, more than a workspace holds",
                bytes.len()
            )));
        }
        let mut file: WorkspaceFile =
            serde_json::from_slice(bytes).map_err(|e| ReadError::Unreadable(e.to_string()))?;
        if file.kind != KIND || file.version == 0 {
            return Err(ReadError::NotAWorkspace);
        }
        file.sanitize();
        Ok(file)
    }

    /// The parts this file has.
    #[cfg(test)]
    pub fn parts(&self) -> WorkspaceParts {
        WorkspaceParts {
            shortcuts: self.shortcuts.is_some(),
            bars: self.bars.is_some(),
            palette: self.palette.is_some(),
            appearance: self.appearance.is_some(),
            preferences: self.preferences.is_some(),
        }
    }

    fn sanitize(&mut self) {
        if self.base.as_deref().is_some_and(|b| !short_text(b)) {
            self.base = None;
        }
        if let Some(list) = self.shortcuts.take() {
            self.shortcuts = Some(clean_shortcuts(list));
        }
        if let Some(list) = self.node_shortcuts.take() {
            self.node_shortcuts = Some(clean_shortcuts(list));
        }
        if let Some(prefs) = &mut self.preferences {
            prefs.retain(|k, _| carried(k));
            if prefs.len() > MAX_PREFERENCES {
                self.preferences = None;
            }
        }
        if let Some(bars) = &mut self.bars {
            let d = BarLayout::default();
            let l = &mut bars.layout;
            for (pos, default) in [
                (&mut l.toolbox_pos, d.toolbox_pos),
                (&mut l.palette_pos, d.palette_pos),
                (&mut l.standard_pos, d.standard_pos),
                (&mut l.property_pos, d.property_pos),
            ] {
                if !pos.iter().all(|v| v.is_finite() && v.abs() <= MAX_POSITION) {
                    *pos = default;
                }
            }
            // The floating toolbox's size: a usable one, else as at first.
            if l.toolbox_size.is_some_and(|size| {
                !size
                    .iter()
                    .all(|v| v.is_finite() && *v > 0.0 && *v <= MAX_POSITION)
            }) {
                l.toolbox_size = None;
            }
            // A pick out of bounds keeps its group's place, as nothing.
            bars.toolbox.truncate(MAX_LIST);
            for id in &mut bars.toolbox {
                if !short_text(id) {
                    id.clear();
                }
            }
            bars.toolbox_hidden.truncate(MAX_LIST);
            bars.toolbox_hidden.retain(|id| short_text(id));
            // The toolbars the user made (#3264): each slot once, a
            // short name.
            l.custom
                .retain(|c| (c.slot as usize) < crate::ui::bars::MAX_CUSTOM);
            let mut seen = std::collections::BTreeSet::new();
            l.custom.retain(|c| seen.insert(c.slot));
            for c in &mut l.custom {
                if !short_text(&c.name) {
                    c.name.clear();
                }
            }
            let custom: Vec<&str> = l
                .custom
                .iter()
                .map(|c| crate::ui::bars::Bar::Custom(c.slot).id())
                .collect();
            bars.bar_items.retain(|bar, _| {
                crate::ui::bar_items::ItemBar::ALL
                    .iter()
                    .any(|b| b.id() == bar)
                    || custom.contains(&bar.as_str())
            });
            for list in bars.bar_items.values_mut() {
                list.truncate(MAX_LIST);
                list.retain(|id| short_text(id));
            }
            if let Some(d) = &mut bars.dockers {
                for list in [
                    &mut d.open,
                    &mut d.left,
                    &mut d.inner,
                    &mut d.left_inner,
                ] {
                    list.truncate(MAX_LIST);
                    list.retain(|k| short_text(k));
                }
                d.stacks.truncate(MAX_LIST);
                d.stacks
                    .retain(|st| st.split.is_finite() && short_text(&st.tab));
                d.floating.truncate(MAX_LIST);
                d.floating.retain(|f| {
                    short_text(&f.tab)
                        && f.pos.iter().chain(&f.size).all(|v| v.is_finite() && v.abs() <= MAX_POSITION)
                });
            }
        }
        if let Some(bars) = &mut self.palette_bars {
            bars.truncate(MAX_LIST);
            // A palette's id may be a file's whole path.
            bars.retain(|b| !b.palette.trim().is_empty() && b.palette.len() <= 4096);
            bars.iter_mut().for_each(PaletteBarPrefs::sanitize);
        }
        if let Some(names) = &mut self.palette_names {
            names.retain(|id, name| id.len() <= 4096 && !id.is_empty() && short_text(name));
        }
        if let Some(a) = &mut self.appearance {
            if !(a.ui_size == 0.0 || (0.5..=4.0).contains(&a.ui_size)) {
                a.ui_size = 0.0;
            }
            if a.theme.as_deref().is_some_and(|t| !short_text(t)) {
                a.theme = None;
            }
        }
    }
}

/// Not empty and not too long.
fn short_text(s: &str) -> bool {
    !s.trim().is_empty() && s.chars().count() <= MAX_TEXT
}

/// The usable shortcuts of a file: the last one given for each id, at most
/// [`MAX_SHORTCUTS`], in the file's order.
fn clean_shortcuts(list: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    let mut kept = Vec::new();
    for (id, keys) in list.into_iter().rev() {
        if kept.len() >= MAX_SHORTCUTS {
            break;
        }
        if short_text(&id) && short_text(&keys) && seen.insert(id.clone()) {
            kept.push((id, keys));
        }
    }
    kept.reverse();
    kept
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

impl App {
    /// The chosen `parts` of the workspace as it is now.
    pub fn export_workspace(&mut self, parts: WorkspaceParts) -> WorkspaceFile {
        self.sync_settings();
        let s = &self.settings;
        WorkspaceFile {
            kind: KIND.into(),
            version: VERSION,
            base: parts.bars.then(|| s.workspace.clone()),
            shortcuts: parts.shortcuts.then(|| s.shortcuts.clone()),
            node_shortcuts: (parts.shortcuts && !s.node_shortcuts.is_empty())
                .then(|| s.node_shortcuts.clone()),
            bars: parts.bars.then(|| BarsPart {
                layout: s.bars.clone(),
                shown: self.shown_bars(),
                toolbox: s.toolbox.clone(),
                toolbox_hidden: s.toolbox_hidden.clone(),
                show_outline_flyout: s.show_outline_flyout,
                status_info: s.status_info,
                bar_items: s.bar_items.clone(),
                dockers: Some(self.main_docker_setup()),
            }),
            palette: parts.palette.then_some(s.palette),
            palette_bars: parts.palette.then(|| s.palette_bars.clone()),
            main_palette: parts.palette.then(|| s.main_palette.clone()),
            palette_names: parts.palette.then(|| s.palette_names.clone()),
            appearance: parts.appearance.then_some(AppearancePart {
                ui_size: s.ui_size,
                desktop_rgb: s.desktop_rgb,
                desktop_custom: s.desktop_custom,
                page_border: self.show_page_border,
                page_shadow: self.show_page_shadow,
                theme: Some(s.theme.clone()),
                center_dialogs: Some(s.center_dialogs),
                ui_shadows: Some(s.ui_shadows),
                colored_icons: Some(s.colored_icons),
                docker_tabs_icons_only: Some(s.docker_tabs_icons_only),
                follow_text_size: Some(s.follow_text_size),
                follow_high_contrast: Some(s.follow_high_contrast),
            }),
            preferences: parts.preferences.then(|| carried_preferences(s)),
        }
    }

    /// Apply the chosen `parts` that `file` has (the preferences are not
    /// written; the caller saves them or, in the options, OK does). The
    /// i18n keys of the parts applied.
    pub fn import_workspace(
        &mut self,
        file: &WorkspaceFile,
        parts: WorkspaceParts,
    ) -> Vec<&'static str> {
        let mut applied = Vec::new();
        // First, so that the parts below have the last word on what they
        // carry.
        if let (true, Some(p)) = (parts.preferences, &file.preferences) {
            self.sync_settings();
            let merged = merged_preferences(&self.settings, p).upgrade();
            self.apply_imported_preferences(merged);
            applied.push(PART_PREFERENCES);
        }
        if let (true, Some(b)) = (parts.bars, &file.bars) {
            if let Some(base) = &file.base {
                self.set_workspace(Workspace::from_id(base));
            }
            self.set_shown_bars(b.shown);
            self.settings.bars = b.layout.clone();
            self.apply_toolbox_picks(&b.toolbox);
            self.settings.toolbox_hidden = b.toolbox_hidden.clone();
            self.settings.show_outline_flyout = b.show_outline_flyout;
            self.settings.status_info = b.status_info;
            self.settings.bar_items = b.bar_items.clone();
            if let Some(dockers) = &b.dockers {
                self.set_docker_setup(dockers);
                self.settings.dockers = Some(self.docker_setup());
            }
            applied.push(PART_BARS);
        }
        if let (true, Some(list)) = (parts.shortcuts, &file.shortcuts) {
            self.settings.shortcuts = list.clone();
            // A file written before a command key was renamed.
            crate::settings::migrate_command_keys(&mut self.settings.shortcuts);
            self.settings.node_shortcuts = file.node_shortcuts.clone().unwrap_or_default();
            crate::settings::migrate_command_keys(&mut self.settings.node_shortcuts);
            self.shortcut_editor.note = None;
            self.shortcut_editor.capture = None;
            applied.push(PART_SHORTCUTS);
        }
        if let (true, Some(p)) = (parts.palette, file.palette) {
            self.settings.palette = p;
            // A file written before palette bars existed: the default
            // palette alone.
            self.settings.palette_bars = file.palette_bars.clone().unwrap_or_default();
            self.settings.main_palette = file.main_palette.clone().unwrap_or_default();
            if let Some(names) = &file.palette_names {
                self.settings.palette_names = names.clone();
            }
            self.rebuild_palette();
            applied.push(PART_PALETTE);
        }
        if let (true, Some(a)) = (parts.appearance, &file.appearance) {
            self.settings.ui_size = a.ui_size;
            self.settings.desktop_rgb = a.desktop_rgb;
            self.settings.desktop_custom = a.desktop_custom;
            self.show_page_border = a.page_border;
            self.show_page_shadow = a.page_shadow;
            let s = &mut self.settings;
            if let Some(t) = &a.theme {
                s.theme = t.clone();
            }
            for (from, to) in [
                (a.center_dialogs, &mut s.center_dialogs),
                (a.ui_shadows, &mut s.ui_shadows),
                (a.colored_icons, &mut s.colored_icons),
                (a.docker_tabs_icons_only, &mut s.docker_tabs_icons_only),
                (a.follow_text_size, &mut s.follow_text_size),
                (a.follow_high_contrast, &mut s.follow_high_contrast),
            ] {
                if let Some(v) = from {
                    *to = v;
                }
            }
            applied.push(PART_APPEARANCE);
        }
        applied
    }

    /// Put imported preferences into effect: what the editor keeps
    /// outside the preferences, the colour engine, the user's palettes,
    /// the undo levels, tab step and spelling.
    fn apply_imported_preferences(&mut self, s: crate::settings::Settings) {
        if s.language != self.settings.language {
            crate::i18n::set_language(crate::i18n::startup_language_here(&s.language));
        }
        self.snap.grid = s.snap.grid;
        self.snap.guides = s.snap.guides;
        self.snap.objects = s.snap.objects;
        self.snap.page = s.snap.page;
        self.snap.pixels = s.snap.pixels;
        self.snap.baseline_grid = s.snap.baseline_grid;
        self.snap.alignment_guides = s.snap.alignment_guides;
        self.snap.dynamic_guides = s.snap.dynamic_guides;
        self.snap.off = s.snap.off;
        self.apply_tool_prefs(&s.tools);
        self.apply_view_prefs(&s.view);
        self.apply_panel_prefs(&s.panels);
        self.nudge_mm = s.nudge_mm;
        self.duplicate_offset =
            tracedraw_core::geometry::Vec2::new(s.duplicate_offset_mm[0], s.duplicate_offset_mm[1]);
        self.units = crate::app::Units::from_id(&s.units);
        self.settings = s;
        self.apply_color_settings();
        self.restore_user_palettes();
        self.rebuild_palette();
        self.apply_runtime_settings();
    }

    /// Write the chosen parts of the workspace to `path`.
    pub fn export_workspace_to(&mut self, path: &Path, parts: WorkspaceParts) {
        let bytes = self.export_workspace(parts).to_bytes();
        let name = file_name(path);
        self.status = match crate::files::write(path, bytes) {
            Ok(()) => trf("status.workspace_exported", &[("name", &name)]),
            Err(e) => {
                log::warn!("could not write {}: {e}", path.display());
                trf("status.workspace_not_written", &[("name", &name)])
            }
        };
    }

    /// Read the workspace file at `path` and apply the chosen parts it
    /// has; `save` writes the preferences at once (the menu; the options
    /// leave it to OK).
    pub fn import_workspace_from(&mut self, path: &Path, parts: WorkspaceParts, save: bool) {
        let name = file_name(path);
        let file = match crate::files::read(path) {
            Ok(bytes) => WorkspaceFile::from_bytes(&bytes),
            Err(e) => Err(ReadError::Unreadable(e.to_string())),
        };
        let file = match file {
            Ok(f) => f,
            Err(e) => {
                log::warn!("{}: {e}", path.display());
                self.status = trf("status.workspace_not_read", &[("name", &name)]);
                return;
            }
        };
        let applied = self.import_workspace(&file, parts);
        if applied.is_empty() {
            self.status = trf("status.workspace_nothing", &[("name", &name)]);
            return;
        }
        if save {
            self.save_settings();
        }
        let parts: Vec<String> = applied.iter().map(|k| tr(k)).collect();
        self.status = trf(
            "status.workspace_imported",
            &[("name", &name), ("parts", &parts.join(", "))],
        );
    }

    /// Choose where to export the workspace (all the parts chosen in the
    /// options).
    pub fn export_workspace_dialog(&mut self, parts: WorkspaceParts) {
        crate::files::Dialog::new()
            .add_filter(tr("workspace.file_filter"), &[EXTENSION])
            .set_file_name(format!("workspace.{EXTENSION}"))
            .save_file(self, move |app, path| {
                let path = with_extension(path);
                app.export_workspace_to(&path, parts);
            });
    }

    /// Choose a workspace file and import the chosen parts it has.
    pub fn import_workspace_dialog(&mut self, parts: WorkspaceParts, save: bool) {
        crate::files::Dialog::new()
            .add_filter(tr("workspace.file_filter"), &[EXTENSION])
            .pick_file(self, move |app, path| {
                app.import_workspace_from(&path, parts, save);
            });
    }
}

/// `path` with the workspace extension when it has none.
fn with_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension(EXTENSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::bars::Dock;

    /// A workspace changed in every part.
    fn customized() -> App {
        let mut app = App::headless();
        app.settings.shortcuts = vec![
            ("menu.file.export".into(), "Ctrl+Shift+X".into()),
            ("pick".into(), "Q".into()),
        ];
        app.settings.bars.toolbox = Dock::Right;
        app.settings.bars.palette_pos = [400.0, 500.0];
        app.settings.bars.locked = true;
        app.show_status_bar = false;
        app.show_zoom_toolbar = true;
        app.show_dockers = false;
        app.settings.toolbox_hidden = vec!["eraser".into()];
        app.settings.show_outline_flyout = true;
        app.settings.status_info = StatusInfo::CursorCoordinates;
        app.settings.palette.show_document = false;
        app.settings.palette.right_click_outline = false;
        app.settings.ui_size = 1.25;
        app.settings.desktop_rgb = [200, 210, 220];
        app.show_page_border = false;
        app.set_workspace(Workspace::Illustration);
        app.set_shown_bars(ShownBars {
            dockers: false,
            status_bar: false,
            zoom_toolbar: true,
            ..ShownBars::default()
        });
        app
    }

    #[test]
    fn a_workspace_comes_back_part_by_part() {
        let mut from = customized();
        let bytes = from.export_workspace(WorkspaceParts::ALL).to_bytes();
        let file = WorkspaceFile::from_bytes(&bytes).expect("reads back");
        assert_eq!(file.parts(), WorkspaceParts::ALL);
        let mut to = App::headless();
        let applied = to.import_workspace(&file, WorkspaceParts::ALL);
        assert_eq!(applied.len(), 5);
        assert_eq!(to.settings.shortcuts, from.settings.shortcuts);
        assert_eq!(to.settings.bars, from.settings.bars);
        assert_eq!(to.shown_bars(), from.shown_bars());
        assert!(!to.show_status_bar && to.show_zoom_toolbar && !to.show_dockers);
        assert_eq!(to.workspace, Workspace::Illustration);
        assert_eq!(to.settings.toolbox_hidden, vec!["eraser".to_string()]);
        assert!(to.settings.show_outline_flyout);
        assert_eq!(to.settings.status_info, StatusInfo::CursorCoordinates);
        assert_eq!(to.settings.palette, from.settings.palette);
        assert_eq!(to.settings.ui_size, 1.25);
        assert_eq!(to.settings.desktop_rgb, [200, 210, 220]);
        assert!(!to.show_page_border);
        // Through a file on disk, too.
        let dir = std::env::temp_dir().join(format!("tracedraw-ws-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("mine.tdws");
        from.export_workspace_to(&path, WorkspaceParts::ALL);
        let mut again = App::headless();
        again.import_workspace_from(&path, WorkspaceParts::ALL, false);
        assert_eq!(again.settings.shortcuts, from.settings.shortcuts);
        assert!(again.status.contains("mine.tdws"), "{}", again.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A docker open when the workspace was exported, closed since, is
    /// open again when the workspace is imported, in front (#3350).
    #[test]
    fn a_workspace_brings_its_dockers_back() {
        use crate::app::DockerTab;
        let mut app = App::headless();
        app.open_docker(DockerTab::FindReplace);
        app.open_docker(DockerTab::StepAndRepeat);
        assert!(app.show_dockers);
        let file = WorkspaceFile::from_bytes(&app.export_workspace(WorkspaceParts::ALL).to_bytes())
            .expect("reads back");
        app.close_docker(DockerTab::StepAndRepeat);
        app.close_docker(DockerTab::FindReplace);
        assert!(!app.open_dockers.contains(&DockerTab::FindReplace));
        app.import_workspace(&file, WorkspaceParts::ALL);
        assert!(app.show_dockers, "the dockers shown");
        assert!(app.open_dockers.contains(&DockerTab::FindReplace), "{:?}", app.open_dockers);
        assert!(app.open_dockers.contains(&DockerTab::StepAndRepeat));
        assert_eq!(app.docker_tab, DockerTab::StepAndRepeat);
        // A file written before dockers were kept leaves them as they are.
        let mut old = file.clone();
        if let Some(b) = &mut old.bars {
            b.dockers = None;
        }
        app.close_docker(DockerTab::StepAndRepeat);
        app.import_workspace(&old, WorkspaceParts::ALL);
        assert!(!app.open_dockers.contains(&DockerTab::StepAndRepeat));
    }

    /// The appearance part carries the whole of Customization >
    /// Appearance: the theme and the switches beside it came across as
    /// they were on the computer importing, not as exported. A file
    /// written before leaves them as they are.
    #[test]
    fn the_appearance_part_carries_the_theme_and_its_switches() {
        let mut from = App::headless();
        from.settings.theme = "dark".into();
        from.settings.center_dialogs = !from.settings.center_dialogs;
        from.settings.ui_shadows = !from.settings.ui_shadows;
        from.settings.colored_icons = !from.settings.colored_icons;
        from.settings.docker_tabs_icons_only = !from.settings.docker_tabs_icons_only;
        from.settings.follow_text_size = !from.settings.follow_text_size;
        from.settings.follow_high_contrast = !from.settings.follow_high_contrast;
        let parts = WorkspaceParts {
            shortcuts: false,
            bars: false,
            palette: false,
            appearance: true,
            preferences: false,
        };
        let bytes = from.export_workspace(parts).to_bytes();
        let file = WorkspaceFile::from_bytes(&bytes).expect("reads back");
        let mut to = App::headless();
        to.import_workspace(&file, parts);
        let (a, b) = (&from.settings, &to.settings);
        assert_eq!(b.theme, "dark");
        assert_eq!(b.center_dialogs, a.center_dialogs);
        assert_eq!(b.ui_shadows, a.ui_shadows);
        assert_eq!(b.colored_icons, a.colored_icons);
        assert_eq!(b.docker_tabs_icons_only, a.docker_tabs_icons_only);
        assert_eq!(b.follow_text_size, a.follow_text_size);
        assert_eq!(b.follow_high_contrast, a.follow_high_contrast);
        // An older file has the size and colours alone.
        let old = br#"{"kind":"tracedraw-workspace","version":1,"appearance":{"ui_size":1.5}}"#;
        let file = WorkspaceFile::from_bytes(old).expect("old file");
        let mut kept = App::headless();
        kept.settings.theme = "light".into();
        kept.import_workspace(&file, parts);
        assert_eq!(kept.settings.theme, "light");
        assert_eq!(kept.settings.ui_size, 1.5);
    }

    #[test]
    fn only_the_parts_chosen_come_across() {
        let mut from = customized();
        // Exported with the shortcuts and the palette only.
        let parts = WorkspaceParts {
            bars: false,
            appearance: false,
            preferences: false,
            ..WorkspaceParts::ALL
        };
        let file = from.export_workspace(parts);
        assert!(file.bars.is_none() && file.appearance.is_none() && file.base.is_none());
        // Imported with the shortcuts only.
        let mut to = App::headless();
        let palette = to.settings.palette;
        let bars = to.settings.bars.clone();
        let only_shortcuts = WorkspaceParts {
            shortcuts: true,
            bars: false,
            palette: false,
            appearance: false,
            preferences: false,
        };
        let applied = to.import_workspace(&file, only_shortcuts);
        assert_eq!(applied, vec![PART_SHORTCUTS]);
        assert_eq!(to.settings.shortcuts, from.settings.shortcuts);
        assert_eq!(to.settings.palette, palette);
        assert_eq!(to.settings.bars, bars);
        // Parts the file lacks are not applied even when chosen.
        let applied = to.import_workspace(&file, WorkspaceParts::ALL);
        assert_eq!(applied, vec![PART_SHORTCUTS, PART_PALETTE]);
    }

    /// A workspace file written before a command key was renamed: its
    /// shortcut for that command comes in under the key used now.
    #[test]
    fn a_shortcut_under_a_renamed_key_is_imported_under_the_new_one() {
        let json = serde_json::json!({
            "kind": KIND,
            "version": 1,
            "shortcuts": [["menu.text.grammatik", "Ctrl+Shift+G"], ["menu.file.open", "Ctrl+O"]],
        });
        let file = WorkspaceFile::from_bytes(json.to_string().as_bytes()).expect("read");
        let mut to = App::headless();
        let applied = to.import_workspace(&file, WorkspaceParts::ALL);
        assert_eq!(applied, vec![PART_SHORTCUTS]);
        assert_eq!(
            to.settings.shortcuts,
            [
                ("menu.text.grammar".to_string(), "Ctrl+Shift+G".to_string()),
                ("menu.file.open".into(), "Ctrl+O".into()),
            ]
        );
    }

    #[test]
    fn odd_files_are_refused_or_tamed() {
        for bytes in [
            &b""[..],
            b"\x00\xff\xfe garbage",
            b"[1, 2, 3]",
            b"{\"kind\": \"something else\", \"version\": 1}",
            b"{\"version\": 1}",
            b"{\"kind\": \"tracedraw-workspace\", \"version\": 0}",
            b"{\"kind\": \"tracedraw-workspace\", \"version\": 1, \"shortcuts\": 5}",
            b"{\"kind\": \"tracedraw-workspace\", \"version\": 1, \"bars\": {\"layout\": {\"toolbox\": \"diagonal\"}}}",
        ] {
            assert!(WorkspaceFile::from_bytes(bytes).is_err(), "{:?}", String::from_utf8_lossy(bytes));
        }
        assert_eq!(
            WorkspaceFile::from_bytes(&vec![b' '; MAX_FILE + 1])
                .err()
                .map(|e| matches!(e, ReadError::Unreadable(_))),
            Some(true)
        );
        // A later version with parts of its own still gives what it can.
        let long = "x".repeat(MAX_TEXT + 1);
        let mut shortcuts: Vec<(String, String)> = (0..MAX_SHORTCUTS + 50)
            .map(|i| (format!("cmd{i}"), "Ctrl+K".to_string()))
            .collect();
        shortcuts.push(("menu.file.export".into(), String::new()));
        shortcuts.push((long.clone(), "F2".into()));
        shortcuts.push(("cmd7".into(), "F3".into()));
        let json = serde_json::json!({
            "kind": KIND,
            "version": 7,
            "future_part": {"anything": [1, 2, 3]},
            "base": long,
            "shortcuts": shortcuts,
            "bars": {
                "layout": {"palette_pos": [1e30, 5.0], "standard_pos": [-3.0e5, 0.0], "toolbox_size": [0.0, 5.0]},
                "toolbox": ["pick", long, ""],
                "toolbox_hidden": ["", "eraser", "x".repeat(500)]
            },
            "appearance": {"ui_size": 99.0}
        });
        let file = WorkspaceFile::from_bytes(json.to_string().as_bytes()).expect("tamed");
        let list = file.shortcuts.clone().unwrap_or_default();
        assert_eq!(list.len(), MAX_SHORTCUTS);
        // The last keys given for an id win and keep their place; the
        // oldest entries beyond the limit go.
        assert_eq!(list.last(), Some(&("cmd7".to_string(), "F3".to_string())));
        assert_eq!(list.iter().filter(|(id, _)| id == "cmd7").count(), 1);
        assert!(list.iter().any(|(id, _)| id == "cmd51"));
        assert!(!list.iter().any(|(id, _)| id == "cmd50"));
        assert!(list.iter().all(|(id, k)| short_text(id) && short_text(k)));
        assert!(file.base.is_none());
        let bars = file.bars.clone().unwrap_or_default();
        let d = BarLayout::default();
        assert_eq!(
            (bars.layout.palette_pos, bars.layout.standard_pos),
            (d.palette_pos, d.standard_pos)
        );
        assert_eq!(bars.layout.toolbox_size, None);
        assert_eq!(
            bars.toolbox,
            vec!["pick".to_string(), String::new(), String::new()]
        );
        assert_eq!(bars.toolbox_hidden, vec!["eraser".to_string()]);
        assert_eq!(file.appearance.as_ref().map(|a| a.ui_size), Some(0.0));
        // Applying it is harmless.
        let mut app = App::headless();
        app.import_workspace(&file, WorkspaceParts::ALL);
        assert_eq!(app.settings.ui_size, 0.0);
        // A missing file is a message, not a panic.
        app.import_workspace_from(Path::new("/no/such/dir/x.tdws"), WorkspaceParts::ALL, false);
        assert!(app.status.contains("x.tdws"));
    }

    /// A workspace carries every other preference, such as the colour
    /// mode of new drawings and the document defaults, and none of this
    /// computer's own (#3721).
    #[test]
    fn a_workspace_carries_the_other_preferences() {
        let mut from = App::headless();
        from.settings.color.primary_color_mode = "RGB".into();
        from.settings.new_document.values.rgb = true;
        from.settings.document_defaults.page_spacing = Some(12.0);
        from.settings.default_dpi = 150.0;
        from.settings.undo_levels = 42;
        from.units = crate::app::Units::from_id("in");
        from.nudge_mm = 0.5;
        from.settings.recent_files = vec![PathBuf::from("/here/only.tdraw")];
        from.settings.window_size = [123.0, 456.0];
        let file =
            WorkspaceFile::from_bytes(&from.export_workspace(WorkspaceParts::ALL).to_bytes())
                .expect("reads back");
        let prefs = file.preferences.as_ref().expect("preferences part");
        for key in [
            "recent_files",
            "window_size",
            "github",
            "ai",
            "shortcuts",
            "ui_size",
        ] {
            assert!(!prefs.contains_key(key), "{key} carried");
        }
        let mut to = App::headless();
        let applied = to.import_workspace(&file, WorkspaceParts::ALL);
        assert!(applied.contains(&PART_PREFERENCES));
        assert_eq!(to.settings.color.primary_color_mode, "RGB");
        assert!(to.settings.new_document.values.rgb);
        assert_eq!(to.settings.document_defaults.page_spacing, Some(12.0));
        assert_eq!(to.settings.default_dpi, 150.0);
        assert_eq!(to.settings.undo_levels, 42);
        assert_eq!(to.units, from.units);
        assert_eq!(to.nudge_mm, 0.5);
        assert!(to.settings.recent_files.is_empty());
        assert_ne!(to.settings.window_size, [123.0, 456.0]);
        // It stays through the next save of the preferences.
        to.sync_settings();
        assert_eq!(to.settings.units, from.units.id());

        // A hand-written file: a value of the wrong shape keeps the one
        // there is, the others come across; this computer's own are
        // never taken.
        let json = serde_json::json!({
            "kind": KIND,
            "version": 1,
            "preferences": {
                "color": { "primary_color_mode": "RGB" },
                "default_dpi": "lots",
                "undo_levels": 7,
                "recent_files": ["/elsewhere/a.tdraw"],
                "no_such_preference": true
            }
        });
        let file = WorkspaceFile::from_bytes(json.to_string().as_bytes()).expect("reads");
        let mut app = App::headless();
        let dpi = app.settings.default_dpi;
        let applied = app.import_workspace(&file, WorkspaceParts::ALL);
        assert_eq!(applied, vec![PART_PREFERENCES]);
        assert_eq!(app.settings.color.primary_color_mode, "RGB");
        assert_eq!(app.settings.default_dpi, dpi);
        assert_eq!(app.settings.undo_levels, 7);
        assert!(app.settings.recent_files.is_empty());
        // Not chosen: nothing changes.
        let mut other = App::headless();
        let only = WorkspaceParts {
            preferences: false,
            ..WorkspaceParts::ALL
        };
        assert!(other.import_workspace(&file, only).is_empty());
        assert_eq!(
            other.settings.undo_levels,
            App::headless().settings.undo_levels
        );
    }

    #[test]
    fn bars_shown_survive_a_restart() {
        let mut app = App::headless();
        app.show_status_bar = false;
        app.show_transform_toolbar = true;
        app.sync_settings();
        let saved = app.settings.clone();
        assert_eq!(
            saved
                .shown_bars
                .map(|s| (s.status_bar, s.transform_toolbar)),
            Some((false, true))
        );
        let mut next = App::headless();
        next.apply_settings(saved);
        assert!(!next.show_status_bar && next.show_transform_toolbar);
        // Preferences from before bars were kept: the workspace's own.
        let mut old = crate::settings::Settings::default();
        old.shown_bars = None;
        old.workspace = "lite".into();
        next.apply_settings(old);
        assert!(!next.show_dockers && next.show_status_bar && !next.show_rulers);
    }
}
