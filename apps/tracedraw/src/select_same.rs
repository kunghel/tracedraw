//! Select Same panel state and the command bridge to selection-only history.

use crate::app::App;
use tracedraw_core::{select_same::SelectSameOptions, Command};

#[derive(Debug, Clone, Default)]
pub struct SelectSameState {
    pub options: SelectSameOptions,
}

impl App {
    /// Match the selected reference objects using the panel's enabled criteria.
    /// Selection changes are undoable without marking the document modified.
    pub fn select_same_matches(&mut self) -> usize {
        if !self.has_document()
            || self.selection.is_empty()
            || !self.select_same.options.has_criteria()
        {
            return 0;
        }
        let command = Command::SelectSame {
            page: self.page,
            references: self.selection.clone(),
            active_layer: self.active_layer(),
            options: self.select_same.options.clone(),
        };
        if let Err(error) = self.engine.run(&command) {
            log::warn!("Select Same failed: {error}");
            return 0;
        }
        let mut ids = self.engine.take_selection_result().unwrap_or_default();
        if self.clip_frame_edit.is_some() {
            ids.retain(|id| {
                !self.outside_clip_edit(*id)
                    && self
                        .doc()
                        .ancestors(*id)
                        .iter()
                        .all(|parent| !self.outside_clip_edit(*parent))
            });
        }
        self.selection_command("docker.select_same", ids);
        self.selection.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{select_same::SizeComparison, Color, Fill, Rect, ShapeId, ShapeKind};

    /// Exercise a real native drawing without saving or changing its objects.
    #[test]
    #[ignore]
    fn select_same_on_native_fixture() {
        let path = std::env::var("SELECT_SAME_FIXTURE").expect("native drawing path");
        let document = tracedraw_io::load_native(path).expect("native drawing");
        let page = document.pages.first().expect("first page").id;
        let reference = document
            .layers_for_page(page)
            .expect("page layers")
            .into_iter()
            .filter(|layer| layer.visible && !layer.locked)
            .flat_map(|layer| &layer.shapes)
            .find(|shape| shape.visible && !shape.locked)
            .expect("selectable reference")
            .id;
        let mut app = App::headless();
        app.new_document();
        app.engine.replace(document);
        app.page = page;
        app.select(vec![reference]);
        let revision = app.engine.revision();
        for dimensions in [false, true] {
            app.select_same.options.size = dimensions;
            let count = app.select_same_matches();
            assert!(app.selection.contains(&reference));
            assert_eq!(app.engine.revision(), revision);
            assert!(!app.engine.is_dirty());
            eprintln!("native fixture: dimensions={dimensions}, matches={count}");
            app.select(vec![reference]);
        }
    }

    fn rect(app: &mut App, x: f64, red: bool) -> ShapeId {
        app.new_shape_styled(
            ShapeKind::Rect {
                rect: Rect::new(x, 10.0, x + 10.0, 20.0),
                radius: 0.0,
                corners: None,
            },
            |shape| {
                shape.fill = Fill::Solid(if red {
                    Color::rgb8(230, 20, 20)
                } else {
                    Color::rgb8(20, 20, 230)
                })
            },
        )
        .expect("create rectangle")
    }

    #[test]
    fn select_same_is_undoable_without_modifying_the_drawing() {
        let mut app = App::headless();
        app.new_document();
        let a = rect(&mut app, 10.0, true);
        let b = rect(&mut app, 30.0, true);
        let _blue = rect(&mut app, 50.0, false);
        app.select(vec![a]);
        app.engine.mark_saved();
        let revision = app.engine.revision();
        let history = app.engine.history_labels();
        assert_eq!(app.select_same_matches(), 2);
        assert_eq!(app.selection, vec![a, b]);
        assert!(!app.engine.is_dirty());
        assert_eq!(app.engine.revision(), revision);
        assert_eq!(app.engine.history_labels(), history);
        assert!(app.undo_selection_step());
        assert_eq!(app.selection, vec![a]);
        assert!(app.redo_selection_step());
        assert_eq!(app.selection, vec![a, b]);
        assert!(!app.engine.is_dirty());
    }

    #[test]
    fn groups_holding_locked_objects_keep_the_count_and_selection_history_consistent() {
        let mut app = App::headless();
        app.new_document();
        let group = |app: &mut App, x| {
            let first = rect(app, x, true);
            let second = rect(app, x + 20.0, true);
            app.select(vec![first, second]);
            app.group_selection();
            (*app.selection.first().expect("group"), first)
        };
        let (reference, _) = group(&mut app, 10.0);
        let (_, locked_child) = group(&mut app, 50.0);
        app.engine
            .run(&Command::SetLocked {
                shapes: vec![locked_child],
                locked: true,
            })
            .expect("lock grouped object");
        let (other, _) = group(&mut app, 90.0);
        app.select(vec![reference]);
        app.engine.mark_saved();
        let revision = app.engine.revision();
        let history = app.engine.history_labels();
        assert_eq!(app.select_same_matches(), 2);
        assert_eq!(app.selection, vec![reference, other]);
        assert!(app.undo_selection_step());
        assert_eq!(app.selection, vec![reference]);
        assert!(app.redo_selection_step());
        assert_eq!(app.selection, vec![reference, other]);
        assert_eq!(app.engine.revision(), revision);
        assert_eq!(app.engine.history_labels(), history);
        assert!(!app.engine.is_dirty());
    }

    #[test]
    fn empty_criteria_is_noop_but_no_matches_clears_selection_undoably() {
        let mut app = App::headless();
        app.new_document();
        let a = rect(&mut app, 10.0, true);
        app.select(vec![a]);
        app.select_same.options.fill = false;
        app.select_same.options.kind = false;
        assert_eq!(app.select_same_matches(), 0);
        assert_eq!(app.selection, vec![a]);
        assert!(app.selection_steps.undo.is_empty());
        app.select_same.options.size = true;
        app.select_same.options.size_comparison = SizeComparison::Larger;
        assert_eq!(app.select_same_matches(), 0);
        assert!(app.selection.is_empty());
        assert!(app.undo_selection_step());
        assert_eq!(app.selection, vec![a]);
        app.select_same.options.tolerance_mm = f64::NAN;
        assert_eq!(app.select_same_matches(), 0);
        assert_eq!(app.selection, vec![a]);
    }

    #[test]
    fn current_layer_uses_the_active_objects_layer() {
        let mut app = App::headless();
        app.new_document();
        let a = rect(&mut app, 10.0, true);
        let original = app.active_layer().expect("layer");
        app.engine
            .run(&Command::AddLayer {
                page: app.page,
                name: "Second".into(),
            })
            .expect("add layer");
        let second = app
            .doc()
            .page(app.page)
            .expect("page")
            .layers
            .last()
            .expect("layer")
            .id;
        app.picked_layer = Some(second);
        let b = rect(&mut app, 30.0, true);
        app.select(vec![a]);
        app.select_same.options.current_layer_only = true;
        assert_eq!(app.select_same_matches(), 1);
        assert_eq!(app.selection, vec![b]);
        app.select(vec![a]);
        app.picked_layer = Some(original);
        assert_eq!(app.select_same_matches(), 1);
        assert_eq!(app.selection, vec![a]);
    }

    #[test]
    fn clip_edit_excludes_outside_objects_and_their_descendants() {
        let mut app = App::headless();
        app.new_document();
        let inside = rect(&mut app, 10.0, true);
        let other_inside = rect(&mut app, 30.0, true);
        let outside = rect(&mut app, 50.0, true);
        let other_outside = rect(&mut app, 60.0, true);
        let frame = rect(&mut app, 70.0, false);
        app.select(vec![outside, other_outside]);
        app.group_selection();
        let outside_group = *app.selection.first().expect("group");
        app.clip_frame_edit = Some((frame, [outside_group].into_iter().collect()));
        app.select(vec![inside]);
        assert_eq!(app.select_same_matches(), 2);
        assert_eq!(app.selection, vec![inside, other_inside]);
    }
}
