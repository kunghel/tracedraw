//! Small commands that take a shortcut of one's own: Wireframe and Smooth
//! back and forth (#4077), the fill and outline windows (#4089) and the
//! lines of a text put in order (#4078).

use crate::app::{App, ViewMode};
use std::cmp::Ordering;
use tracedraw_core::document::{merge_equal_spans, ShapeKind, TextSpan};
use tracedraw_core::Command;

/// How Text > Sort Lines puts the lines in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortLines {
    /// A to Z, capitals and small letters alike, numbers inside a line by
    /// their value ("Item 2" before "Item 10").
    AToZ,
    /// Z to A.
    ZToA,
    /// By the first number in each line, smallest first; lines without a
    /// number after the others, in the order they had.
    NumbersUp,
    /// By the first number in each line, largest first.
    NumbersDown,
}

impl App {
    /// View > Toggle Wireframe / Smooth (#4077): Wireframe when the
    /// drawing shows any other way, Smooth when it shows as Wireframe.
    pub fn toggle_wireframe_view(&mut self) {
        let mode = if self.view_mode == ViewMode::Wireframe {
            ViewMode::Enhanced
        } else {
            ViewMode::Wireframe
        };
        self.view_mode = mode;
        self.wireframe = mode == ViewMode::Wireframe;
    }

    /// Text > Sort Lines (#4078): the lines (paragraphs) of every selected
    /// text, or of the text being edited, put in order; each line keeps
    /// its own formatting. Blank lines go to the end. All the texts in one
    /// undo step, and none when no line moves.
    pub fn sort_text_lines(&mut self, how: SortLines) {
        let mut shapes = self.text_shapes();
        if let Some(te) = &self.text_edit {
            if let Some(s) = self.doc().find_shape(te.shape).cloned() {
                shapes = vec![s];
            }
        }
        let mut cmds = Vec::new();
        for s in shapes {
            let ShapeKind::Text {
                spans,
                origin,
                frame,
                align,
                para,
                on_path,
            } = s.kind.clone()
            else {
                continue;
            };
            let Some(spans) = sort_lines(&spans, how) else {
                continue;
            };
            cmds.push(Command::SetShapeKind {
                shape: s.id,
                kind: ShapeKind::Text {
                    spans,
                    origin,
                    frame,
                    align,
                    para,
                    on_path,
                },
            });
        }
        if cmds.is_empty() {
            return;
        }
        if let Some(te) = self.text_edit.as_mut() {
            te.caret = 0;
            te.anchor = 0;
        }
        if let Err(e) = self.run_batch_keeping_lines("Sort Lines", &cmds) {
            log::warn!("sort lines: {e}");
        }
    }
}

/// The spans of a text with its lines in order, or `None` when the order
/// does not change. A line is what lies between two line breaks; its
/// characters keep their formatting (paragraph formatting rides on the
/// characters, so it follows its line).
pub fn sort_lines(spans: &[TextSpan], how: SortLines) -> Option<Vec<TextSpan>> {
    let template = spans.first()?.clone();
    let mut lines: Vec<Vec<TextSpan>> = vec![Vec::new()];
    for sp in spans {
        for (i, piece) in sp.text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if !piece.is_empty() {
                if let Some(line) = lines.last_mut() {
                    let mut part = sp.clone();
                    part.text = piece.to_string();
                    line.push(part);
                }
            }
        }
    }
    let text = |line: &[TextSpan]| line.iter().map(|s| s.text.as_str()).collect::<String>();
    let keys: Vec<String> = lines.iter().map(|l| text(l)).collect();
    let mut order: Vec<usize> = (0..lines.len()).collect();
    let blank = |i: usize| keys[i].trim().is_empty();
    order.sort_by(|&a, &b| match (blank(a), blank(b)) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => compare_lines(&keys[a], &keys[b], how),
    });
    if order.iter().copied().eq(0..lines.len()) {
        return None;
    }
    let mut out: Vec<TextSpan> = Vec::new();
    for (n, &i) in order.iter().enumerate() {
        if n > 0 {
            match out.last_mut() {
                Some(last) => last.text.push('\n'),
                None => {
                    let mut sp = template.clone();
                    sp.text = "\n".into();
                    out.push(sp);
                }
            }
        }
        out.extend(lines[i].iter().cloned());
    }
    if out.is_empty() {
        return None;
    }
    Some(merge_equal_spans(out))
}

fn compare_lines(a: &str, b: &str, how: SortLines) -> Ordering {
    match how {
        SortLines::AToZ => natural(a, b),
        SortLines::ZToA => natural(b, a),
        SortLines::NumbersUp | SortLines::NumbersDown => {
            match (first_number(a), first_number(b)) {
                (Some(x), Some(y)) => {
                    let o = x.total_cmp(&y);
                    if how == SortLines::NumbersUp {
                        o
                    } else {
                        o.reverse()
                    }
                }
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        }
    }
}

/// Compares two lines as a reader would: capitals and small letters
/// alike, runs of digits by their value.
fn natural(a: &str, b: &str) -> Ordering {
    let a: Vec<char> = a.trim().chars().flat_map(char::to_lowercase).collect();
    let b: Vec<char> = b.trim().chars().flat_map(char::to_lowercase).collect();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let si = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = a[si..i].iter().collect();
            let nb: String = b[sj..j].iter().collect();
            let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
            let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
            if o != Ordering::Equal {
                return o;
            }
            continue;
        }
        let o = a[i].cmp(&b[j]);
        if o != Ordering::Equal {
            return o;
        }
        i += 1;
        j += 1;
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// The first number in a line: digits with an optional minus sign right
/// before them and an optional decimal part after a point or a comma.
fn first_number(s: &str) -> Option<f64> {
    let c: Vec<char> = s.chars().collect();
    let start = c.iter().position(char::is_ascii_digit)?;
    let mut end = start;
    while end < c.len() && c[end].is_ascii_digit() {
        end += 1;
    }
    if end + 1 < c.len() && (c[end] == '.' || c[end] == ',') && c[end + 1].is_ascii_digit() {
        end += 1;
        while end < c.len() && c[end].is_ascii_digit() {
            end += 1;
        }
    }
    let mut digits: String = c[start..end].iter().collect::<String>().replace(',', ".");
    if start > 0 && c[start - 1] == '-' {
        digits.insert(0, '-');
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, bold: bool) -> TextSpan {
        let mut s = TextSpan::new(text, "Sans", 12.0);
        s.bold = bold;
        s
    }

    fn lines(spans: &[TextSpan]) -> Vec<String> {
        tracedraw_core::spans_text(spans)
            .split('\n')
            .map(str::to_string)
            .collect()
    }

    /// #4078: names A to Z and Z to A, capitals and small letters alike,
    /// "Item 2" before "Item 10"; a line keeps its own formatting.
    #[test]
    fn lines_sort_by_letters_and_keep_their_formatting() {
        let spans = vec![
            span("pear\nApple\n", false),
            span("Item 10", true),
            span("\nItem 2\nbanana", false),
        ];
        let up = sort_lines(&spans, SortLines::AToZ).expect("moved");
        assert_eq!(
            lines(&up),
            ["Apple", "banana", "Item 2", "Item 10", "pear"]
        );
        // "Item 10" stays bold, the others not.
        let bold: String = up
            .iter()
            .filter(|s| s.bold)
            .map(|s| s.text.trim_end_matches('\n'))
            .collect();
        assert_eq!(bold, "Item 10");
        let down = sort_lines(&spans, SortLines::ZToA).expect("moved");
        assert_eq!(
            lines(&down),
            ["pear", "Item 10", "Item 2", "banana", "Apple"]
        );
        // Already in order: nothing to change.
        assert!(sort_lines(&up, SortLines::AToZ).is_none());
    }

    /// #4078: by the first number in each line, up or down, decimals and
    /// negative numbers too; lines without a number and blank lines last.
    #[test]
    fn lines_sort_by_their_numbers() {
        let spans = vec![span("12 eggs\n\nno number\n-3 below\n2,5 kg\n100", false)];
        let up = sort_lines(&spans, SortLines::NumbersUp).expect("moved");
        assert_eq!(
            lines(&up),
            ["-3 below", "2,5 kg", "12 eggs", "100", "no number", ""]
        );
        let down = sort_lines(&spans, SortLines::NumbersDown).expect("moved");
        assert_eq!(
            lines(&down),
            ["100", "12 eggs", "2,5 kg", "-3 below", "no number", ""]
        );
        assert_eq!(first_number("abc"), None);
        assert_eq!(first_number("x 7.25 y"), Some(7.25));
    }

    /// The command: the selected text sorted in one undo step.
    #[test]
    fn sort_lines_command_is_one_undo_step() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Text {
                spans: vec![span("c\na\nb", false)],
                origin: Default::default(),
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            })
            .expect("text");
        app.select(vec![id]);
        let text = |app: &App| match &app.doc().find_shape(id).expect("text").kind {
            ShapeKind::Text { spans, .. } => tracedraw_core::spans_text(spans),
            _ => String::new(),
        };
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(crate::ui::menus::run_command(
                &mut app,
                ui.ctx(),
                "menu.text.sort_a_z"
            ));
        });
        out.textures_delta.clear();
        assert_eq!(text(&app), "a\nb\nc");
        app.undo();
        assert_eq!(text(&app), "c\na\nb");
    }

    /// #4077: one command between Wireframe and Smooth; #4089: the fill
    /// and outline windows as commands.
    #[test]
    fn view_toggle_and_paint_windows_run_as_commands() {
        use crate::ui::dialogs::Dialog;
        let ctx = crate::theme::ui_context();
        let run = |app: &mut App, key: &str| {
            let mut ran = false;
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                ran = crate::ui::menus::run_command(app, ui.ctx(), key);
            });
            out.textures_delta.clear();
            ran
        };
        let mut app = App::headless();
        app.view_mode = ViewMode::Normal;
        assert!(run(&mut app, "menu.view.toggle_wireframe"));
        assert_eq!(app.view_mode, ViewMode::Wireframe);
        assert!(app.wireframe);
        assert!(run(&mut app, "menu.view.toggle_wireframe"));
        assert_eq!(app.view_mode, ViewMode::Enhanced);
        assert!(!app.wireframe);
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        assert!(run(&mut app, "menu.object.edit_fill"));
        assert!(matches!(app.dialog, Dialog::EditFill(_)));
        app.dialog = Dialog::None;
        assert!(run(&mut app, "menu.object.outline_pen"));
        assert!(matches!(app.dialog, Dialog::OutlinePen(_)));
    }
}
