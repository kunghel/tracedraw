//! The property bar's Blur slider (#4091): with one object selected that
//! has a Gaussian Blur among its effects, its radius is on the bar, so the
//! blur is changed by dragging, the drawing in view, without opening the
//! effect's window. The change is made when the slider is let go (or a
//! value typed), one undo step; the effect is not rendered again on every
//! step of the drag. See docs/behavior/bitmap-effects.md, Blur on the
//! property bar.

use crate::app::App;
use crate::i18n::tr;
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::ShapeId;

/// The effect's id and the setting the slider changes.
const EFFECT: &str = "gaussian_blur";
const RADIUS: &str = "radius";

/// The selected object and the place of its first Gaussian Blur in its
/// effects, with the radius.
pub fn blur_of_selection(app: &App) -> Option<(ShapeId, usize, f64)> {
    let [id] = app.selection[..] else {
        return None;
    };
    let effects = app.bitmap_effects(id);
    let i = effects.iter().position(|e| e.id == EFFECT)?;
    let r = effects[i].params.get(RADIUS).copied().unwrap_or(1.0);
    Some((id, i, r))
}

impl App {
    /// Set the radius of the `index`-th effect of `id` (a Gaussian Blur),
    /// one undo step; nothing when it is that already.
    pub fn set_blur_radius(&mut self, id: ShapeId, index: usize, radius: f64) {
        let Some(spec) = crate::fx::spec(EFFECT) else {
            return;
        };
        let (lo, hi) = spec
            .params
            .iter()
            .find(|p| p.name == RADIUS)
            .and_then(|p| match p.kind {
                crate::fx::ParamKind::Range { min, max, .. } => Some((min, max)),
                _ => None,
            })
            .unwrap_or((0.1, 250.0));
        let radius = if radius.is_finite() { radius.clamp(lo, hi) } else { return };
        let same = self
            .bitmap_effects(id)
            .get(index)
            .is_some_and(|e| e.id == EFFECT && e.params.get(RADIUS).copied() == Some(radius));
        if same {
            return;
        }
        self.edit_bitmap_effects(id, "Edit Effect", move |list| {
            if let Some(e) = list.get_mut(index).filter(|e| e.id == EFFECT) {
                e.params.insert(RADIUS.into(), radius);
            }
        });
    }
}

/// The Blur slider on the property bar, when the selection has a blur.
pub fn blur_part(app: &mut App, ui: &mut Ui) {
    let Some((id, index, radius)) = blur_of_selection(app) else {
        return;
    };
    ui.separator();
    // The value shown while the slider is held, kept between frames.
    let key = egui::Id::new(("blur_bar", id.raw(), index));
    let mut v: f64 = ui.data(|d| d.get_temp(key)).unwrap_or(radius);
    ui.label(tr("toolbar.blur"));
    let r = ui
        .add(
            egui::Slider::new(&mut v, 0.1..=250.0)
                .logarithmic(true)
                .max_decimals(1)
                .suffix(" px"),
        )
        .rich_tip(crate::ui::tips::pair("toolbar.blur", "toolbar.blur_tip"));
    let held = r.dragged() || r.has_focus();
    if held {
        ui.data_mut(|d| d.insert_temp(key, v));
    } else {
        ui.data_mut(|d| d.remove::<f64>(key));
    }
    if (r.changed() || r.drag_stopped() || r.lost_focus()) && !held {
        app.set_blur_radius(id, index, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::ShapeKind;

    /// #4091: an object with a Gaussian Blur shows its radius on the bar;
    /// a new radius is one undo step on that effect alone.
    #[test]
    fn the_bar_changes_the_blur_radius() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: kurbo::Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        assert!(blur_of_selection(&app).is_none(), "no blur, no slider");
        let mut blur = crate::fx::new_effect(EFFECT);
        blur.params.insert(RADIUS.into(), 3.0);
        app.add_bitmap_effect(blur);
        let (sid, index, r) = blur_of_selection(&app).expect("the blur");
        assert_eq!((sid, index, r), (id, 0, 3.0));
        app.set_blur_radius(id, index, 12.0);
        assert_eq!(blur_of_selection(&app).map(|b| b.2), Some(12.0));
        // Out of range: kept to the effect's range.
        app.set_blur_radius(id, index, 1e6);
        assert_eq!(blur_of_selection(&app).map(|b| b.2), Some(250.0));
        app.undo();
        assert_eq!(blur_of_selection(&app).map(|b| b.2), Some(12.0));
        app.undo();
        assert_eq!(blur_of_selection(&app).map(|b| b.2), Some(3.0));
        // Two objects selected: no slider.
        let other = app
            .new_shape(ShapeKind::Rect {
                rect: kurbo::Rect::new(30.0, 0.0, 40.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id, other]);
        assert!(blur_of_selection(&app).is_none());
        assert!(crate::ui::effect_dialog::compact_effect(EFFECT));
    }
}
