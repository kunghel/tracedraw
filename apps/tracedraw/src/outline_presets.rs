//! Outline presets (#4090): an outline (colour, width, style, caps,
//! corners, arrowheads, calligraphy, position) saved under a name in the
//! settings and given to other objects from the Outline Pen dialog's
//! Presets row. See docs/behavior/outline-pen.md, Presets.

use crate::app::App;
use serde::{Deserialize, Serialize};
use tracedraw_core::patch::StrokeField;
use tracedraw_core::Stroke;

/// At most this many outline presets are kept.
pub const MAX_OUTLINE_PRESETS: usize = 100;

/// A saved outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutlinePreset {
    pub name: String,
    pub stroke: Stroke,
}

/// A name for a new preset: `base` with the first number not taken.
pub fn free_outline_name(presets: &[OutlinePreset], base: &str) -> String {
    (1..)
        .map(|n| format!("{base} {n}"))
        .find(|n| !presets.iter().any(|p| &p.name == n))
        .unwrap_or_else(|| base.to_string())
}

impl App {
    /// Save `stroke` as an outline preset named `name` (a preset of that
    /// name is replaced); the list keeps its order and at most
    /// [`MAX_OUTLINE_PRESETS`]. Returns the preset's place in the list.
    pub fn save_outline_preset(&mut self, name: &str, stroke: &Stroke) -> Option<usize> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let list = &mut self.settings.outline_presets;
        let at = match list.iter().position(|p| p.name == name) {
            Some(i) => {
                list[i].stroke = stroke.clone();
                i
            }
            None => {
                if list.len() >= MAX_OUTLINE_PRESETS {
                    return None;
                }
                list.push(OutlinePreset {
                    name: name.to_string(),
                    stroke: stroke.clone(),
                });
                list.len() - 1
            }
        };
        self.settings.save();
        Some(at)
    }

    /// Remove the outline preset at `index`.
    pub fn delete_outline_preset(&mut self, index: usize) {
        if index < self.settings.outline_presets.len() {
            self.settings.outline_presets.remove(index);
            self.settings.save();
        }
    }

    /// Give the selected objects the outline preset at `index`, every
    /// part of it, as one undo step.
    pub fn apply_outline_preset(&mut self, index: usize) {
        let Some(p) = self.settings.outline_presets.get(index).cloned() else {
            return;
        };
        let old = self.paint_targets().first().and_then(|s| s.stroke.clone());
        self.patch_selection_stroke(&old, &Some(p.stroke), &StrokeField::ALL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{Color, ShapeKind};

    /// #4090: an outline saved under a name, given to another object in
    /// one undo step; the same name replaces it; deleted, it is gone.
    #[test]
    fn an_outline_preset_is_saved_and_given_to_other_objects() {
        let mut app = App::headless();
        app.settings.outline_presets.clear();
        let rect = |app: &mut App, x: f64| {
            app.new_shape(ShapeKind::Rect {
                rect: kurbo::Rect::new(x, 0.0, x + 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect")
        };
        let a = rect(&mut app, 0.0);
        let b = rect(&mut app, 20.0);
        let thick = Stroke {
            width: 2.5,
            color: Color::rgb8(200, 0, 0),
            dash: vec![4.0, 2.0],
            ..Stroke::default()
        };
        assert_eq!(free_outline_name(&app.settings.outline_presets, "Outline"), "Outline 1");
        assert_eq!(app.save_outline_preset("Red dashes", &thick), Some(0));
        assert_eq!(free_outline_name(&app.settings.outline_presets, "Outline"), "Outline 1");
        assert_eq!(app.save_outline_preset("  ", &thick), None, "no name, no preset");
        app.select(vec![b]);
        let before = app.doc().find_shape(b).and_then(|s| s.stroke.clone());
        app.apply_outline_preset(0);
        let got = app.doc().find_shape(b).and_then(|s| s.stroke.clone()).expect("outline");
        assert_eq!(got.width, 2.5);
        assert_eq!(got.color, Color::rgb8(200, 0, 0));
        assert_eq!(got.dash, vec![4.0, 2.0]);
        assert_ne!(app.doc().find_shape(a).and_then(|s| s.stroke.clone()), Some(got));
        app.undo();
        assert_eq!(app.doc().find_shape(b).and_then(|s| s.stroke.clone()), before);
        // The same name replaces the outline kept.
        let thin = Stroke {
            width: 0.2,
            ..thick.clone()
        };
        assert_eq!(app.save_outline_preset("Red dashes", &thin), Some(0));
        assert_eq!(app.settings.outline_presets.len(), 1);
        assert_eq!(app.settings.outline_presets[0].stroke.width, 0.2);
        app.delete_outline_preset(0);
        assert!(app.settings.outline_presets.is_empty());
    }
}
