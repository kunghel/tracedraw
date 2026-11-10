//! Help > Search Commands (Ctrl+Shift+P): a small box over the window
//! that finds a menu command by part of its name and runs it, or a
//! setting of the Options or of Document Setup by its label or its tip
//! and opens its page with the setting framed. The commands are those
//! of the menu bar, gathered by the same walk as Options > Customization
//! > Commands (`menus::commands`) when the box opens; a command runs as
//! its menu row would (`menus::run_command`). The settings are the
//! labels each options page shows (`options::settings_index`). See
//! `docs/behavior/command-search.md`.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Key, Modifiers};

/// The menu row that opens the box: not listed in it.
pub const MENU_KEY: &str = "menu.help.search_commands";

/// The width of the box, points.
const WIDTH: f32 = 460.0;
/// The height of its list, points.
const LIST_H: f32 = 340.0;

/// What a row of the box does when chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    /// Run the menu command.
    Command,
    /// Open the options page (and tab of Options > Text) showing the
    /// setting, framed.
    Setting {
        page: crate::ui::options::OptionsPage,
        tab: usize,
    },
}

/// A menu command or a setting as the box lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// Its i18n key, which runs it (a command) or names it (a setting).
    pub key: String,
    pub target: Target,
    /// Its name and its menus ("Object > Align and Distribute"), in the
    /// interface language.
    pub name: String,
    pub place: String,
    /// The shortcut it runs with, if any.
    pub shortcut: String,
    name_lc: String,
    all_lc: String,
}

impl Found {
    pub fn new(key: &str, name: String, place: String, shortcut: String) -> Self {
        let name_lc = name.to_lowercase();
        let all_lc = format!(
            "{name_lc} {} {}",
            place.to_lowercase(),
            shortcut.to_lowercase()
        );
        Found {
            key: key.to_string(),
            target: Target::Command,
            name,
            place,
            shortcut,
            name_lc,
            all_lc,
        }
    }

    /// A setting: its label, the page it is on, and its tip, which the
    /// words typed may match too.
    pub fn setting(key: &str, name: String, place: String, tip: &str, target: Target) -> Self {
        let mut f = Found::new(key, name, place, String::new());
        f.all_lc = format!("{} {}", f.all_lc, tip.to_lowercase());
        f.target = target;
        f
    }

    /// The setting `e` of the options pages, named in the interface
    /// language.
    fn of_setting(e: &crate::ui::options::SettingEntry) -> Self {
        let name = tr(&e.key);
        let name = name.trim().trim_end_matches([':', '\u{ff1a}']).trim();
        let tip = e.tip.as_deref().map(tr).unwrap_or_default();
        Found::setting(
            &e.key,
            name.to_string(),
            crate::ui::options::setting_place(e.page),
            &tip,
            Target::Setting {
                page: e.page,
                tab: e.tab,
            },
        )
    }

    /// The command `e` of the menu walk, named in the interface language.
    fn of(e: &crate::ui::menus::invoke::Entry) -> Self {
        let name = match crate::macros::command_name(&e.key) {
            Some((name, _)) => name,
            None => tr(&e.key),
        };
        let place = e
            .path
            .iter()
            .map(|k| tr(k).trim_end_matches("...").to_string())
            .collect::<Vec<_>>()
            .join(" > ");
        let shortcut = crate::ui::menus::invoke::shown_shortcut(&e.key, &e.shortcut);
        Found::new(
            &e.key,
            name.trim_end_matches("...").to_string(),
            place,
            shortcut,
        )
    }
}

/// The box's state.
#[derive(Debug, Default)]
pub struct CommandSearch {
    pub open: bool,
    pub query: String,
    /// The highlighted row, an index into the matches shown.
    pub selected: usize,
    /// The menu commands, then the settings, gathered when the box
    /// opens.
    pub entries: Vec<Found>,
    /// The text box takes the keyboard on the next frame.
    focus: bool,
}

impl CommandSearch {
    /// Open the box empty; the commands are gathered when it is drawn.
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
        self.entries.clear();
        self.focus = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.entries.clear();
    }
}

/// Whether the letters of `q` (spaces aside) come in `text` in order.
fn in_order(q: &str, text: &str) -> bool {
    let mut letters = text.chars();
    q.chars()
        .filter(|c| !c.is_whitespace())
        .all(|c| letters.any(|t| t == c))
}

/// How well `f` matches `q` (lower case, trimmed, not empty): 0 for a
/// name that begins with it, 1 for a name that holds it, 2 for a name
/// that holds each of its words, 3 for a name, menus and shortcut that
/// together hold each word, 4 for a name that holds its letters in
/// order; `None` when it does not match.
fn score(f: &Found, q: &str) -> Option<u8> {
    if f.name_lc.starts_with(q) {
        return Some(0);
    }
    if f.name_lc.contains(q) {
        return Some(1);
    }
    let mut words = q.split_whitespace();
    if words.clone().all(|w| f.name_lc.contains(w)) {
        return Some(2);
    }
    if words.all(|w| f.all_lc.contains(w)) {
        return Some(3);
    }
    in_order(q, &f.name_lc).then_some(4)
}

/// The commands matching `query`, best first (menu order among equals),
/// as indices into `entries`: every command for an empty query.
pub fn ranked(entries: &[Found], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (0..entries.len()).collect();
    }
    let mut hits: Vec<(u8, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, f)| score(f, &q).map(|s| (s, i)))
        .collect();
    hits.sort_by_key(|&(s, i)| (s, i));
    hits.into_iter().map(|(_, i)| i).collect()
}

/// The box, while it is open: the text box, the matching commands and
/// what the keys do. A command chosen runs on the next frame, through
/// the menus like a shortcut of the user's.
pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.command_search.open {
        return;
    }
    if app.command_search.entries.is_empty() {
        let entries = crate::ui::menus::commands(app, ctx);
        let mut found: Vec<Found> = entries
            .iter()
            .filter(|e| e.key != MENU_KEY)
            .map(Found::of)
            .collect();
        let settings = crate::ui::options::settings_index(app, ctx);
        found.extend(settings.iter().map(Found::of_setting));
        app.command_search.entries = found;
    }
    let mut run: Option<Found> = None;
    let area = egui::Modal::default_area(egui::Id::new("command_search"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 72.0));
    let modal = egui::Modal::new(egui::Id::new("command_search"))
        .area(area)
        .backdrop_color(egui::Color32::from_black_alpha(40))
        .show(ctx, |ui| {
            let st = &mut app.command_search;
            ui.set_width(WIDTH);
            // Taken before the text box sees them: Enter would end its
            // editing, the arrows move its caret.
            let (down, up, enter) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::Enter),
                )
            });
            let edit = ui.add(
                egui::TextEdit::singleline(&mut st.query)
                    .hint_text(tr("command_search.hint"))
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut st.focus) {
                edit.request_focus();
            }
            let edit = edit.rich_tip(tr("command_search.keys"));
            if edit.changed() {
                st.selected = 0;
            }
            let found = ranked(&st.entries, &st.query);
            let last = found.len().saturating_sub(1);
            if down {
                st.selected = (st.selected + 1).min(last);
            }
            if up {
                st.selected = st.selected.saturating_sub(1);
            }
            st.selected = st.selected.min(last);
            if enter {
                run = found.get(st.selected).map(|&i| st.entries[i].clone());
            }
            ui.add_space(4.0);
            if found.is_empty() {
                ui.label(
                    egui::RichText::new(tr("command_search.none"))
                        .color(Tokens::text_dim())
                        .size(11.0),
                );
            }
            let row_h = ui.spacing().interact_size.y + 14.0;
            egui::ScrollArea::vertical()
                .max_height(LIST_H)
                .auto_shrink([false, true])
                .show_rows(ui, row_h, found.len(), |ui, rows| {
                    for n in rows {
                        let Some(f) = found.get(n).and_then(|&i| st.entries.get(i)) else {
                            continue;
                        };
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row_h),
                            egui::Sense::click(),
                        );
                        let chosen = n == st.selected;
                        if chosen || resp.hovered() {
                            ui.painter().rect_filled(rect, 3.0, Tokens::tool_hover());
                        }
                        if chosen && (up || down) {
                            resp.scroll_to_me(None);
                        }
                        let text = ui.visuals().text_color();
                        let left = rect.left() + 6.0;
                        ui.painter().text(
                            egui::pos2(left, rect.top() + 3.0),
                            egui::Align2::LEFT_TOP,
                            &f.name,
                            egui::FontId::proportional(13.0),
                            text,
                        );
                        ui.painter().text(
                            egui::pos2(left, rect.bottom() - 3.0),
                            egui::Align2::LEFT_BOTTOM,
                            &f.place,
                            egui::FontId::proportional(10.5),
                            Tokens::text_dim(),
                        );
                        if !f.shortcut.is_empty() {
                            ui.painter().text(
                                egui::pos2(rect.right() - 6.0, rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                &f.shortcut,
                                egui::FontId::proportional(11.0),
                                Tokens::text_dim(),
                            );
                        }
                        if resp.clicked() {
                            run = Some(f.clone());
                        }
                    }
                });
        });
    if modal.should_close() {
        app.command_search.close();
    }
    if let Some(f) = run {
        app.command_search.close();
        match f.target {
            Target::Command => app.pending_command = Some(f.key),
            Target::Setting { page, tab } => {
                crate::ui::options::open_at_setting(app, page, tab, &f.key);
            }
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, name: &str, place: &str, shortcut: &str) -> Found {
        Found::new(key, name.into(), place.into(), shortcut.into())
    }

    fn keys(entries: &[Found], q: &str) -> Vec<String> {
        ranked(entries, q)
            .into_iter()
            .map(|i| entries[i].key.clone())
            .collect()
    }

    /// Names that begin with the text come first, then names holding
    /// it, then names holding each word, then name, menus and shortcut
    /// together, then the letters in order; case does not count.
    #[test]
    fn matches_rank_from_the_start_of_the_name_to_letters_in_order() {
        let entries = vec![
            entry("a", "Align Top", "Object > Align and Distribute", ""),
            entry("b", "Convert to Curves", "Object", "Ctrl+Q"),
            entry("c", "Curves Fit", "Object", ""),
            entry("d", "Export", "File", "Ctrl+E"),
            entry("e", "Top Curves", "View", ""),
        ];
        assert_eq!(keys(&entries, ""), ["a", "b", "c", "d", "e"]);
        assert_eq!(keys(&entries, "  CURVES "), ["c", "b", "e"]);
        assert_eq!(keys(&entries, "curves top"), ["e"]);
        assert_eq!(keys(&entries, "object top"), ["a"]);
        assert_eq!(keys(&entries, "ctrl+e"), ["d"]);
        assert_eq!(keys(&entries, "cnvcrv"), ["b"]);
        assert!(keys(&entries, "zzz").is_empty());
        // Letters out of order do not match.
        assert!(keys(&entries, "tropxe").is_empty());
    }

    /// Live test of 0.41: settings could not be found ("Fill open
    /// curves"). The box lists every options page's settings by label,
    /// found by their tips' words too; choosing one opens Document Setup
    /// or the Options at its page with the setting framed, scrolled into
    /// view, for a moment.
    #[test]
    fn the_box_finds_settings_and_opens_their_page_framed() {
        use crate::ui::options::OptionsPage;
        crate::i18n::set_language("en");
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        let frame = |app: &mut App, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 1000.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
            out.textures_delta.clear();
        };
        let key = |k: Key| {
            vec![
                egui::Event::Key {
                    key: k,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                egui::Event::Key {
                    key: k,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ]
        };
        frame(&mut app, Vec::new());
        app.options_page = OptionsPage::Grid;
        app.command_search.open();
        let started = web_time::Instant::now();
        frame(&mut app, Vec::new());
        eprintln!("commands and settings gathered in {:?}", started.elapsed());
        assert_eq!(
            app.options_page,
            OptionsPage::Grid,
            "the dialog's page is kept"
        );
        let st = &app.command_search;
        let fill = st
            .entries
            .iter()
            .find(|f| f.key == "options.fill_open_curves")
            .expect("Fill open curves is listed");
        assert_eq!(fill.name, "Fill open curves");
        assert_eq!(fill.place, "Document Setup > General");
        assert_eq!(
            fill.target,
            Target::Setting {
                page: OptionsPage::DocumentGeneral,
                tab: 0
            }
        );
        let first = |q: &str| {
            ranked(&st.entries, q)
                .first()
                .map(|&i| st.entries[i].key.clone())
        };
        assert_eq!(first("fill open"), Some("options.fill_open_curves".into()));
        // By the words of its tip.
        assert!(ranked(&st.entries, "curves that are not closed")
            .iter()
            .any(|&i| st.entries[i].key == "options.fill_open_curves"));
        // Settings of the Options too, and the pages by name; the menu
        // commands come first among equals.
        let settings = |page: OptionsPage| {
            st.entries
                .iter()
                .filter(|f| matches!(f.target, Target::Setting { page: p, .. } if p == page))
                .count()
        };
        for page in [
            OptionsPage::General,
            OptionsPage::Display,
            OptionsPage::ZoomPan,
            OptionsPage::Grid,
            OptionsPage::GlobalGeneral,
        ] {
            assert!(settings(page) > 1, "{page:?}");
        }
        assert_eq!(
            settings(OptionsPage::Commands),
            0,
            "commands are listed once"
        );
        assert!(st
            .entries
            .iter()
            .any(|f| f.key == "options.pan_inertia" && f.place == "Options > Tools > Zoom/Pan"));
        assert!(st.entries.iter().any(|f| f.key == "options.spelling"
            && f.target
                == Target::Setting {
                    page: OptionsPage::Text,
                    tab: 3
                }));
        let last_command = st
            .entries
            .iter()
            .rposition(|f| f.target == Target::Command)
            .expect("commands");
        let first_setting = st
            .entries
            .iter()
            .position(|f| f.target != Target::Command)
            .expect("settings");
        assert!(last_command < first_setting);
        // Typed and chosen: Document Setup opens at General, the setting
        // framed.
        frame(&mut app, vec![egui::Event::Text("fill open curves".into())]);
        frame(&mut app, key(Key::Enter));
        assert!(!app.command_search.open);
        assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::Options));
        assert_eq!(app.options_page, OptionsPage::DocumentGeneral);
        frame(&mut app, Vec::new());
        frame(&mut app, Vec::new());
        let h = app.options_highlight.clone().expect("framed");
        assert_eq!(h.label, "Fill open curves");
        assert!(h.since.is_some(), "drawn");
        assert!(
            crate::ui::options::highlighted_rect(&ctx).is_some(),
            "found on the page"
        );
        // Another page: the frame goes.
        app.options_page = OptionsPage::Bleed;
        frame(&mut app, Vec::new());
        assert!(app.options_highlight.is_none());
    }

    /// The box lists the menu bar's commands in the interface language,
    /// not itself; a command chosen runs through the menus on the next
    /// frame, and Esc closes the box.
    #[test]
    fn the_box_lists_menu_commands_and_runs_the_one_chosen() {
        crate::i18n::set_language("en");
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        let frame = |app: &mut App, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 1000.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
            out.textures_delta.clear();
        };
        let key_event = |k: Key, pressed: bool, modifiers: Modifiers| egui::Event::Key {
            key: k,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        // A press and its release: a key pressed again without a release
        // between is a held key's repeat.
        let key = |k: Key, modifiers: Modifiers| {
            vec![
                key_event(k, true, modifiers),
                key_event(k, false, modifiers),
            ]
        };
        frame(&mut app, Vec::new());
        // Ctrl+Shift+P opens it, with the commands of every menu.
        frame(&mut app, key(Key::P, Modifiers::COMMAND | Modifiers::SHIFT));
        frame(&mut app, Vec::new());
        assert!(app.command_search.open, "Ctrl+Shift+P opens the box");
        let st = &app.command_search;
        assert!(st.entries.len() > 100, "{}", st.entries.len());
        assert!(st.entries.iter().all(|f| f.key != MENU_KEY));
        let dup = st
            .entries
            .iter()
            .find(|f| f.key == "menu.edit.duplicate")
            .expect("Edit > Duplicate");
        assert_eq!(dup.name, "Duplicate");
        assert_eq!(dup.place, "Edit");
        assert_eq!(dup.shortcut, "Ctrl+D");
        assert_eq!(
            ranked(&st.entries, "duplicate")
                .first()
                .map(|&i| st.entries[i].key.as_str()),
            Some("menu.edit.duplicate")
        );
        // Typed into the box, then Enter: Select All runs.
        let a = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.select(Vec::new());
        frame(
            &mut app,
            vec![egui::Event::Text("select all objects".into())],
        );
        assert_eq!(app.command_search.query, "select all objects");
        frame(&mut app, key(Key::Enter, Modifiers::NONE));
        assert!(!app.command_search.open, "Enter closes the box");
        frame(&mut app, Vec::new());
        assert_eq!(app.selection, vec![a], "and runs the command");
        // Esc closes it without running anything.
        frame(&mut app, key(Key::P, Modifiers::COMMAND | Modifiers::SHIFT));
        frame(&mut app, Vec::new());
        assert!(app.command_search.open);
        frame(&mut app, key(Key::Escape, Modifiers::NONE));
        assert!(!app.command_search.open, "Esc closes the box");
    }
}
