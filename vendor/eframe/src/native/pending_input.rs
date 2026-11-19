//! TraceDraw: the input kept while the window is minimized, put ahead of
//! the next (see vendor/README.md, #3058).

/// Put `newer` after the input kept from frames no pass consumed. Events
/// and dropped files pile up, as `RawInput::append` does them, but the
/// files hovered are `newer`'s list whole: egui-winit hands the full list
/// with every input, so appended, the list kept from the last frame was
/// doubled at every frame the window was minimized. On Windows, where the
/// files of a drop that never reached the program stay listed, the copies
/// counted as a new drag and the file was imported again at every restore
/// (#3058).
pub fn append_pending(raw: &mut egui::RawInput, newer: egui::RawInput) {
    let hovered = newer.hovered_files.clone();
    raw.append(newer);
    raw.hovered_files = hovered;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hovered(path: &str) -> egui::HoveredFile {
        egui::HoveredFile {
            path: Some(path.into()),
            ..Default::default()
        }
    }

    /// Kept over many minimized frames, the files hovered stay the list
    /// of the latest input; events still pile up.
    #[test]
    fn the_hovered_files_kept_while_minimized_are_not_doubled() {
        let mut kept = egui::RawInput::default();
        for k in 0..5 {
            let newer = egui::RawInput {
                hovered_files: vec![hovered("a.svg")],
                events: vec![egui::Event::Text(format!("{k}"))],
                ..Default::default()
            };
            append_pending(&mut kept, newer);
        }
        assert_eq!(kept.hovered_files.len(), 1);
        assert_eq!(kept.events.len(), 5);
        // The drag ended: nothing hovered.
        append_pending(&mut kept, egui::RawInput::default());
        assert!(kept.hovered_files.is_empty());
    }
}
