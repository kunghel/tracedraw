//! Steps of the view that scripts take and the macro recorder keeps
//! (#4020): Zoom To Fit, Zoom To Page, Zoom To Selection and Edit > Copy
//! Visible Area. They change no object, so they are not commands of the
//! drawing: the recorder notes each one at its place among the commands.

use crate::app::App;

/// A step of the view a script or a recorded macro takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewStep {
    /// View > Zoom To Fit (F4): every object in the window.
    FitAll,
    /// View > Zoom To Page (Shift+F4).
    FitPage,
    /// View > Zoom To Selection (Shift+F2).
    FitSelection,
    /// Edit > Copy Visible Area: what the window shows, as a picture on
    /// the system clipboard.
    CopyVisibleArea,
}

impl ViewStep {
    /// The step's name in a script's call to the host.
    pub fn name(self) -> &'static str {
        match self {
            ViewStep::FitAll => "fit_all",
            ViewStep::FitPage => "fit_page",
            ViewStep::FitSelection => "fit_selection",
            ViewStep::CopyVisibleArea => "copy_visible_area",
        }
    }

    pub fn from_name(name: &str) -> Option<ViewStep> {
        [
            ViewStep::FitAll,
            ViewStep::FitPage,
            ViewStep::FitSelection,
            ViewStep::CopyVisibleArea,
        ]
        .into_iter()
        .find(|s| s.name() == name)
    }

    /// The line of a recorded macro that takes the step again.
    pub fn script_line(self) -> &'static str {
        match self {
            ViewStep::FitAll => "Application.ZoomToFit();\n",
            ViewStep::FitPage => "Application.ZoomToPage();\n",
            ViewStep::FitSelection => "Application.ZoomToSelection();\n",
            ViewStep::CopyVisibleArea => "Application.CopyVisibleArea();\n",
        }
    }
}

impl App {
    /// Keep `step` in the macro being recorded, after the commands run so
    /// far.
    pub(crate) fn note_view_step(&mut self, step: ViewStep) {
        self.note_recorded_line(step.script_line());
    }

    /// Keep the script line `line` in the macro being recorded, after the
    /// commands run so far.
    pub(crate) fn note_recorded_line(&mut self, line: &str) {
        if let Some(rec) = &self.recording {
            let at = rec.len();
            self.recorded_lines.push((at, line.to_string()));
        }
    }

    /// Take a step a script asked for, once its objects are in the
    /// drawing. The page is fitted at once when the window's size is
    /// known, so a Copy Visible Area after it copies the fitted view.
    pub fn run_view_step(&mut self, step: ViewStep) {
        match step {
            ViewStep::FitAll => self.zoom_to_fit(),
            ViewStep::FitSelection => self.zoom_to_selection(),
            ViewStep::CopyVisibleArea => self.copy_visible_area(),
            ViewStep::FitPage => {
                self.zoom_to_page();
                if self.fit_pending && self.canvas_rect.is_positive() {
                    self.view.fit(self.fit_rect(), self.canvas_rect);
                    self.fit_pending = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::document::ShapeKind;
    use tracedraw_core::geometry::{Point, Rect};

    fn app_with_square() -> App {
        let mut app = App::headless();
        app.canvas_rect =
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        app.fit_pending = false;
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(150.0, 120.0, 170.0, 140.0),
            radius: 0.0,
            corners: None,
        })
        .expect("square");
        app.view
            .fit(Rect::new(0.0, 0.0, 600.0, 600.0), app.canvas_rect);
        app
    }

    #[test]
    fn names_go_both_ways() {
        for s in [
            ViewStep::FitAll,
            ViewStep::FitPage,
            ViewStep::FitSelection,
            ViewStep::CopyVisibleArea,
        ] {
            assert_eq!(ViewStep::from_name(s.name()), Some(s));
        }
        assert_eq!(ViewStep::from_name("zoom"), None);
    }

    /// The zoom steps taken while a macro is recorded come back as lines
    /// of the macro, in the order they were taken, among its commands.
    #[test]
    fn the_recorder_keeps_the_zoom_steps() {
        let mut app = app_with_square();
        app.toggle_recording();
        app.zoom_to_fit();
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(10.0, 10.0, 30.0, 30.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
        app.zoom_to_selection();
        app.zoom_to_page();
        app.toggle_recording();
        let s = app.script_source.clone();
        let fit = s.find("Application.ZoomToFit();").expect(&s);
        let rect = s.find("CreateRectangle(").expect(&s);
        let sel = s.find("Application.ZoomToSelection();").expect(&s);
        let page = s.find("Application.ZoomToPage();").expect(&s);
        assert!(fit < rect && rect < sel && sel < page, "{s}");
        // A new recording starts with none of them.
        app.toggle_recording();
        app.toggle_recording();
        assert!(!app.script_source.contains("Zoom"), "{}", app.script_source);
    }

    /// The recorded macro plays back: its Zoom To Fit fits the objects.
    #[test]
    fn a_recorded_zoom_to_fit_plays_back() {
        let mut app = app_with_square();
        app.toggle_recording();
        app.zoom_to_fit();
        app.toggle_recording();
        let script = app.script_source.clone();
        let fitted = app.view.zoom;
        app.view
            .fit(Rect::new(0.0, 0.0, 600.0, 600.0), app.canvas_rect);
        crate::scripting::run(&mut app, &script).unwrap();
        assert!((app.view.zoom - fitted).abs() < 1e-3, "{script}");
    }

    /// A script's Zoom To Fit fits the objects in the window, as F4 does;
    /// a VBA macro's ActiveWindow.ActiveView.ToFitAllObjects too.
    #[test]
    fn scripts_and_macros_zoom_to_fit_the_objects() {
        let mut app = app_with_square();
        let before = app.view.zoom;
        crate::scripting::run(&mut app, "Application.ZoomToFit();").unwrap();
        assert!(app.view.zoom > before * 4.0, "{before} {}", app.view.zoom);
        let area = app.view.visible_page_rect(app.canvas_rect);
        assert!(area.contains(Point::new(160.0, 130.0)), "{area:?}");

        app.view
            .fit(Rect::new(0.0, 0.0, 600.0, 600.0), app.canvas_rect);
        crate::scripting::run_vgcore(&mut app, "ActiveWindow.ActiveView.ToFitAllObjects();", "Macro")
            .unwrap();
        assert!(app.view.zoom > before * 4.0, "{before} {}", app.view.zoom);

        // Zoom To Page and Zoom To Selection, from both.
        crate::scripting::run(&mut app, "Application.ZoomToPage();").unwrap();
        let page = app.view.zoom;
        assert!(page < before * 4.0, "{page}");
        crate::scripting::run_vgcore(&mut app, "ActiveDocument.ActiveLayer.Shapes.Item(1).CreateSelection(); ActiveWindow.ActiveView.ToFitSelection();", "Macro")
            .unwrap();
        assert!(app.view.zoom > page * 4.0, "{page} {}", app.view.zoom);
        crate::scripting::run_vgcore(&mut app, "ActiveWindow.ActiveView.ToFitPage();", "Macro")
            .unwrap();
        assert!((app.view.zoom - page).abs() < 1e-3);
    }
}
