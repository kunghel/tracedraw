//! Plugins (#1802): scripts that add commands to Tools > Plugins. A
//! plugin is a JavaScript file that defines `TraceDrawPlugin` (see
//! `docs/behavior/plugins.md`), written against the same object model as
//! the Scripts panel. Plugins are installed from a file or from the
//! Scripts panel, kept in the preferences, and turned on or off, edited
//! or removed in the Plugin Manager.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::scripting::{self, Plugin};

/// What Tools > Plugins > New Plugin puts in the Scripts panel.
pub const TEMPLATE: &str = r#"// A plugin adds its commands to Tools > Plugins.
// Edit it, then click "Install as Plugin".
const TraceDrawPlugin = {
  name: 'My Plugin',
  description: 'What the plugin does.',
  version: '1.0',
  commands: [
    {
      name: 'Box with a lid',
      run() {
        // Lengths in millimetres, y upwards (see Help > Scripts).
        const box = ActiveLayer.CreateRectangle(40, 40, 80, 60);
        const lid = ActiveLayer.CreateRectangle(40, 100, 80, 20, 4);
        lid.Fill.ApplyUniformFill(Color.RGB(230, 120, 40));
        print('Made a box and its lid');
      },
    },
    {
      name: 'Count objects',
      run() {
        print('Objects on the page: ' + ActivePage.Shapes.Count);
      },
    },
  ],
};
"#;

/// The command key of a plugin's command, for its menu row and the
/// keyboard shortcuts of Options > Customization (#3519). Dots would end
/// the key's last part early (see `i18n::tr`).
pub fn command_key(plugin: &str, command: &str) -> String {
    format!(
        "menu.plugin_command.{}|{}",
        plugin.replace('.', " "),
        command.replace('.', " ")
    )
}

/// A plugin command's name and plugin, from its key.
pub fn command_name(key: &str) -> Option<(String, String)> {
    let rest = key.strip_prefix("menu.plugin_command.")?;
    let (plugin, command) = rest.split_once('|')?;
    Some((command.to_string(), plugin.to_string()))
}

impl App {
    /// Install `src` as a plugin, replacing one of the same name (which
    /// keeps whether it was on). Says so on the status line.
    pub fn install_plugin(&mut self, src: &str) -> bool {
        match scripting::plugin_info(src) {
            Ok(mut p) => {
                let name = p.name.clone();
                let plugins = &mut self.settings.plugins;
                match plugins.iter_mut().find(|q| q.name == p.name) {
                    Some(q) => {
                        p.enabled = q.enabled;
                        *q = p;
                    }
                    None => plugins.push(p),
                }
                self.settings.save();
                self.status = trf("status.plugin_installed", &[("name", &name)]);
                true
            }
            Err(e) => {
                self.status = trf("status.plugin_failed", &[("e", &e)]);
                false
            }
        }
    }

    /// Tools > Plugins > Install Plugin: a `.js` file chosen.
    pub fn install_plugin_file(&mut self) {
        crate::files::Dialog::new()
            .add_filter("JavaScript (*.js)", &["js"])
            .pick_file(self, |app, path| {
                match crate::files::read_to_string(&path) {
                    Ok(src) => {
                        app.install_plugin(&src);
                    }
                    Err(e) => app.status = format!("{}: {e}", path.display()),
                }
            });
    }

    pub fn remove_plugin(&mut self, name: &str) {
        self.settings.plugins.retain(|p| p.name != name);
        self.settings.save();
    }

    pub fn set_plugin_enabled(&mut self, name: &str, on: bool) {
        if let Some(p) = self.settings.plugins.iter_mut().find(|p| p.name == name) {
            p.enabled = on;
            self.settings.save();
        }
    }

    /// A plugin's source in the Scripts panel, to change and install
    /// again (New Plugin: the template).
    pub fn edit_plugin(&mut self, src: &str) {
        self.macros.scripts_vba = false;
        self.script_source = src.to_string();
        self.show_dockers = true;
        self.docker_tab = crate::app::DockerTab::Scripts;
    }

    /// Run a command of an installed plugin: one undo step; what it
    /// prints goes to the Scripts panel's output.
    pub fn run_plugin_command(&mut self, name: &str, index: usize) {
        let Some(plugin) = self
            .settings
            .plugins
            .iter()
            .find(|p| p.name == name)
            .cloned()
        else {
            return;
        };
        self.finish_text();
        match scripting::run_plugin(self, &plugin, index) {
            Ok(out) => {
                self.scripts_output.extend(out);
                if self.status.is_empty() {
                    self.status = trf(
                        "status.plugin_done",
                        &[(
                            "name",
                            plugin.commands.get(index).map_or("", |s| s.as_str()),
                        )],
                    );
                }
            }
            Err(e) => {
                self.scripts_output.push(format!("Error: {e}"));
                self.status = trf("status.plugin_error", &[("name", &plugin.name), ("e", &e)]);
            }
        }
    }
}

/// Tools > Plugins: each plugin that is on (its command, or a submenu of
/// its commands), then the Plugin Manager, New Plugin and Install Plugin.
pub fn menu_items(app: &mut App, ui: &mut egui::Ui) {
    use crate::ui::menus::{item, item_text, sep, sub};
    let plugins: Vec<Plugin> = app
        .settings
        .plugins
        .iter()
        .filter(|p| p.enabled)
        .cloned()
        .collect();
    for p in &plugins {
        if let [only] = p.commands.as_slice() {
            if item_text(ui, only, "", true, None, &command_key(&p.name, only)) {
                app.run_plugin_command(&p.name, 0);
            }
        } else {
            // Shown as its name: the part of the key after the last dot.
            let key = format!("menu.plugin.{}", p.name.replace('.', "\u{b7}"));
            sub(ui, &key, |ui| {
                for (i, c) in p.commands.iter().enumerate() {
                    if item_text(ui, c, "", true, None, &command_key(&p.name, c)) {
                        app.run_plugin_command(&p.name, i);
                    }
                }
            });
        }
    }
    if !plugins.is_empty() {
        sep(ui);
    }
    if item(ui, "menu.tools.plugin_manager", "", true) {
        app.dialog = crate::ui::dialogs::Dialog::Plugins;
    }
    if item(ui, "menu.tools.plugin_new", "", true) {
        app.edit_plugin(TEMPLATE);
    }
    if item(ui, "menu.tools.plugin_install", "", true) {
        app.install_plugin_file();
    }
}

/// The Plugin Manager: every installed plugin with its description and
/// commands, a switch to turn it on or off, Edit and Remove; Install
/// Plugin and New Plugin below.
pub fn manager(app: &mut App, ctx: &egui::Context, close: &mut bool) {
    use crate::ui::tips::RichTip as _;
    let mut toggle: Option<(String, bool)> = None;
    let mut edit: Option<String> = None;
    let mut remove: Option<String> = None;
    let mut install = false;
    let mut new = false;
    crate::ui::dialogs::window(ctx, tr("dialog.plugins")).show(ctx, |ui| {
        ui.set_min_width(440.0);
        ui.label(
            egui::RichText::new(tr("dialog.plugins_hint")).color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(6.0);
        if app.settings.plugins.is_empty() {
            ui.label(tr("dialog.plugins_none"));
        }
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for p in &app.settings.plugins {
                    ui.group(|ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut on = p.enabled;
                            let title = if p.version.is_empty() {
                                p.name.clone()
                            } else {
                                format!("{} {}", p.name, p.version)
                            };
                            if ui
                                .checkbox(&mut on, egui::RichText::new(title).strong())
                                .rich_tip(crate::ui::tips::keyed("dialog.plugins_on"))
                                .changed()
                            {
                                toggle = Some((p.name.clone(), on));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button(tr("docker.remove"))
                                        .rich_tip(crate::ui::tips::keyed("dialog.plugins_remove"))
                                        .clicked()
                                    {
                                        remove = Some(p.name.clone());
                                    }
                                    if ui
                                        .small_button(tr("docker.edit"))
                                        .rich_tip(crate::ui::tips::keyed("dialog.plugins_edit"))
                                        .clicked()
                                    {
                                        edit = Some(p.source.clone());
                                    }
                                },
                            );
                        });
                        if !p.description.is_empty() {
                            ui.label(&p.description);
                        }
                        ui.label(
                            egui::RichText::new(trf(
                                "dialog.plugins_commands",
                                &[("list", &p.commands.join(", "))],
                            ))
                            .color(crate::theme::Tokens::text_dim()),
                        );
                    });
                }
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            install = ui
                .button(tr("menu.tools.plugin_install"))
                .rich_tip(crate::ui::tips::keyed("menu.tools.plugin_install"))
                .clicked();
            new = ui
                .button(tr("menu.tools.plugin_new"))
                .rich_tip(crate::ui::tips::keyed("menu.tools.plugin_new"))
                .clicked();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(tr("dialog.close"))
                    .rich_tip(crate::ui::tips::keyed("dialog.close"))
                    .clicked()
                {
                    *close = true;
                }
            });
        });
    });
    if let Some((name, on)) = toggle {
        app.set_plugin_enabled(&name, on);
    }
    if let Some(name) = remove {
        app.remove_plugin(&name);
    }
    if let Some(src) = edit {
        app.edit_plugin(&src);
        *close = true;
    }
    if new {
        app.edit_plugin(TEMPLATE);
        *close = true;
    }
    if install {
        app.install_plugin_file();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes(app: &App) -> usize {
        app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.shapes.len())
            .sum()
    }

    #[test]
    fn the_template_installs_and_its_commands_run_as_one_step() {
        let mut app = App::headless();
        assert!(app.install_plugin(TEMPLATE), "{}", app.status);
        let p = &app.settings.plugins[0];
        assert_eq!(p.name, "My Plugin");
        assert_eq!(p.version, "1.0");
        assert_eq!(p.commands, ["Box with a lid", "Count objects"]);
        assert!(p.enabled);
        app.run_plugin_command("My Plugin", 0);
        assert_eq!(shapes(&app), 2, "{}", app.status);
        assert!(app.scripts_output.iter().any(|l| l.contains("box")));
        app.run_plugin_command("My Plugin", 1);
        assert!(app
            .scripts_output
            .iter()
            .any(|l| l.contains("Objects on the page: 2")));
        // The box and its lid came in one undo step.
        app.undo();
        assert_eq!(shapes(&app), 0);
    }

    #[test]
    fn a_plugin_of_one_run_is_one_command_and_reinstalling_keeps_it_off() {
        let mut app = App::headless();
        let src = "const TraceDrawPlugin = { name: 'Dot', run() { ActiveLayer.CreateEllipse(10, 10, 5, 5); } };";
        assert!(app.install_plugin(src));
        assert_eq!(app.settings.plugins[0].commands, ["Dot"]);
        app.set_plugin_enabled("Dot", false);
        assert!(app.install_plugin(src));
        assert_eq!(app.settings.plugins.len(), 1);
        assert!(!app.settings.plugins[0].enabled);
        app.remove_plugin("Dot");
        assert!(app.settings.plugins.is_empty());
    }

    #[test]
    fn scripts_that_are_not_plugins_are_refused() {
        for src in [
            "const x = 1;",
            "const TraceDrawPlugin = { run() {} };",
            "const TraceDrawPlugin = { name: 'Empty', commands: [{ name: 'x' }] };",
            "const TraceDrawPlugin = { name: 'Broken', run() {} ",
        ] {
            assert!(scripting::plugin_info(src).is_err(), "{src}");
        }
        // Installing checks the plugin without changing the drawing.
        let mut app = App::headless();
        let src = "ActiveLayer.CreateRectangle(0, 0, 10, 10);\nconst TraceDrawPlugin = { name: 'Side', run() {} };";
        assert!(app.install_plugin(src));
        assert_eq!(shapes(&app), 0);
    }

    #[test]
    fn an_error_keeps_what_ran_before_it_and_says_so() {
        let mut app = App::headless();
        let src = "const TraceDrawPlugin = { name: 'Oops', run() { ActiveLayer.CreateRectangle(0, 0, 10, 10); throw new Error('stop here'); } };";
        assert!(app.install_plugin(src));
        app.run_plugin_command("Oops", 0);
        assert_eq!(shapes(&app), 1);
        assert!(app.status.contains("stop here"), "{}", app.status);
    }
}
