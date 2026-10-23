//! Paragraph text wrapped around objects and formatting kept per
//! paragraph, through the application's commands.

use crate::app::{App, TextEdit};
use tracedraw_core::{
    geometry::Rect, paragraphs::paragraph_styles, Command, Document, ParagraphStyle, Point,
    ShapeId, ShapeKind, Size, TextAlign, TextSpan, TextWrap, WrapMode, WrapSide,
};

fn fonts_available() -> bool {
    tracedraw_text::install();
    !tracedraw_text::fonts().families().is_empty()
}

fn frame(app: &mut App, text: &str) -> ShapeId {
    app.new_shape(ShapeKind::Text {
        spans: vec![TextSpan::new(text, "DejaVu Sans", 12.0)],
        origin: Point::new(10.0, 100.0),
        frame: Some(Size::new(100.0, 80.0)),
        align: TextAlign::Left,
        para: ParagraphStyle::default(),
        on_path: None,
    })
    .expect("frame")
}

fn styles(app: &App, id: ShapeId) -> Vec<(TextAlign, ParagraphStyle)> {
    match &app.doc().find_shape(id).expect("text").kind {
        ShapeKind::Text {
            spans, align, para, ..
        } => paragraph_styles(spans, *align, para),
        _ => Vec::new(),
    }
}

#[test]
fn paragraph_text_wraps_around_an_object_above_it_and_undo_takes_it_back() {
    if !fonts_available() {
        return;
    }
    let mut app = App::headless();
    let text = vec!["word"; 150].join(" ");
    let id = frame(&mut app, &text);
    let before = app.doc().find_shape(id).expect("frame").page_path();
    // A square over the middle of the frame (page 50..70 x 130..150).
    let sq = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(50.0, 130.0, 70.0, 150.0),
            radius: 0.0,
            corners: None,
        })
        .expect("square");
    // Not wrapping yet: the text is as it was.
    assert_eq!(app.doc().find_shape(id).expect("frame").page_path(), before);
    app.run(Command::SetWrapStyle {
        shapes: vec![sq],
        style: TextWrap {
            mode: WrapMode::Square,
            side: WrapSide::Straddle,
            offset: 1.0,
        },
    });
    let s = app.doc().find_shape(id).cloned().expect("frame");
    assert!(s.wrap_text || app.doc().find_shape(sq).is_some_and(|q| q.wrap_text));
    let after = s.page_path();
    assert_ne!(after, before);
    // No letter inside the square and its offset.
    let (layout, to_page) =
        crate::text_editing::layout_of_text_shape(&s, Default::default()).expect("layout");
    for line in &layout.lines {
        let a = to_page * Point::new(line.edges[0], line.baseline);
        let b = to_page * Point::new(line.edges.last().copied().unwrap_or(0.0), line.baseline);
        let (top, bottom) = (a.y + line.ascent, a.y - line.descent);
        if top > 129.0 && bottom < 151.0 {
            assert!(b.x <= 49.05 || a.x >= 70.95, "{a:?}..{b:?}");
        }
    }
    // Saved and opened again, the text still flows around the square.
    let json = app.doc().to_json().expect("json");
    let back = Document::from_json(&json).expect("opens");
    assert_eq!(back.find_shape(id).expect("frame").page_path(), after);
    // Moving the square moves the hole; undo brings the text back.
    app.run(Command::SetWrapText {
        shapes: vec![sq],
        wrap: false,
    });
    assert_eq!(app.doc().find_shape(id).expect("frame").page_path(), before);
    let _ = app.engine.undo();
    assert_eq!(app.doc().find_shape(id).expect("frame").page_path(), after);
}

#[test]
fn objects_below_the_frame_do_not_push_its_text() {
    if !fonts_available() {
        return;
    }
    let mut app = App::headless();
    let sq = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(50.0, 130.0, 70.0, 150.0),
            radius: 0.0,
            corners: None,
        })
        .expect("square");
    app.run(Command::SetWrapText {
        shapes: vec![sq],
        wrap: true,
    });
    let id = frame(&mut app, &vec!["word"; 150].join(" "));
    match &app.doc().find_shape(id).expect("frame").kind {
        ShapeKind::Text { para, .. } => assert!(para.flow.is_empty()),
        _ => unreachable!(),
    }
}

#[test]
fn alignment_and_spacing_apply_to_the_paragraphs_being_edited() {
    let mut app = App::headless();
    let id = frame(&mut app, "first\nsecond\nthird");
    app.select(vec![id]);
    // The caret in "second".
    app.text_edit = Some(TextEdit {
        shape: id,
        caret: 8,
        anchor: 8,
        goal_x: None,
        step: None,
    });
    app.apply_text_align(TextAlign::Center);
    app.edit_paragraph_style(id, |p| p.space_before = 3.0);
    let st = styles(&app, id);
    assert_eq!(
        st.iter().map(|s| s.0).collect::<Vec<_>>(),
        vec![TextAlign::Left, TextAlign::Center, TextAlign::Left]
    );
    assert_eq!(
        st.iter().map(|s| s.1.space_before).collect::<Vec<_>>(),
        vec![0.0, 3.0, 0.0]
    );
    // The controls show the paragraph at the caret.
    let s = app.doc().find_shape(id).cloned().expect("text");
    assert_eq!(
        app.paragraph_style_of(&s).map(|p| p.0),
        Some(TextAlign::Center)
    );
    // A selection over the first two paragraphs.
    app.text_edit = Some(TextEdit {
        shape: id,
        caret: 2,
        anchor: 9,
        goal_x: None,
        step: None,
    });
    app.apply_text_align(TextAlign::Right);
    let st = styles(&app, id);
    assert_eq!(
        st.iter().map(|s| s.0).collect::<Vec<_>>(),
        vec![TextAlign::Right, TextAlign::Right, TextAlign::Left]
    );
    // The whole object: the left indent reaches every paragraph, each
    // keeps its alignment and spacing.
    app.text_edit = None;
    app.edit_paragraph_style(id, |p| p.left_indent = 5.0);
    let st = styles(&app, id);
    assert!(st.iter().all(|s| s.1.left_indent == 5.0), "{st:?}");
    assert_eq!(
        st.iter().map(|s| s.0).collect::<Vec<_>>(),
        vec![TextAlign::Right, TextAlign::Right, TextAlign::Left]
    );
    assert_eq!(st[1].1.space_before, 3.0);
    // One undo step each.
    let _ = app.engine.undo();
    assert!(styles(&app, id).iter().all(|s| s.1.left_indent == 0.0));
    // Saved and opened again, every paragraph keeps its own.
    let json = app.doc().to_json().expect("json");
    let back = Document::from_json(&json).expect("opens");
    let ShapeKind::Text {
        spans, align, para, ..
    } = &back.find_shape(id).expect("text").kind
    else {
        unreachable!()
    };
    let st = paragraph_styles(spans, *align, para);
    assert_eq!(
        st.iter().map(|s| s.0).collect::<Vec<_>>(),
        vec![TextAlign::Right, TextAlign::Right, TextAlign::Left]
    );
    assert_eq!(st[1].1.space_before, 3.0);
}

#[test]
fn a_file_without_paragraph_formats_gives_every_paragraph_the_object_ones() {
    let mut app = App::headless();
    let id = frame(&mut app, "a\nb");
    let shape = serde_json::to_string(app.doc().find_shape(id).expect("text")).expect("json");
    assert!(
        !shape.contains("\"paragraph\""),
        "nothing per paragraph is written"
    );
    let json = app.doc().to_json().expect("json");
    // An older file: the object's alignment and spacing.
    let old = json
        .replace("\"align\": \"left\"", "\"align\": \"center\"")
        .replace("\"space_after\": 0.0", "\"space_after\": 2.0");
    let doc = Document::from_json(&old).expect("opens");
    let ShapeKind::Text {
        spans, align, para, ..
    } = &doc.find_shape(id).expect("text").kind
    else {
        unreachable!()
    };
    let st = paragraph_styles(spans, *align, para);
    assert_eq!(st.len(), 2);
    assert!(st
        .iter()
        .all(|(a, p)| *a == TextAlign::Center && p.space_after == 2.0));
}

fn square(app: &mut App) -> ShapeId {
    app.new_shape(ShapeKind::Rect {
        rect: Rect::new(50.0, 130.0, 70.0, 150.0),
        radius: 0.0,
        corners: None,
    })
    .expect("square")
}

#[test]
fn wrap_styles_and_offset_set_from_the_menus_turn_wrapping_on() {
    let mut app = App::headless();
    let sq = square(&mut app);
    app.select(vec![sq]);
    assert_eq!(app.wrap_state(), (false, TextWrap::default()));
    app.set_wrap_offset(5.0);
    let (on, style) = app.wrap_state();
    assert!(on);
    assert_eq!(style.offset, 5.0);
    app.set_wrap_style(WrapMode::Square, WrapSide::Left);
    let (_, style) = app.wrap_state();
    assert_eq!(
        (style.mode, style.side, style.offset),
        (WrapMode::Square, WrapSide::Left, 5.0)
    );
    // Saved with the drawing.
    let back = Document::from_json(&app.doc().to_json().expect("json")).expect("opens");
    let s = back.find_shape(sq).expect("square");
    assert!(s.wrap_text);
    assert_eq!(s.wrap_style, style);
}

#[test]
fn the_context_menu_lists_the_wrap_styles_and_picks_one() {
    let ctx = egui::Context::default();
    let frame = |app: &mut App, target: Option<usize>| {
        if let Some(t) = target {
            crate::ui::menus::replay::start(t);
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let mut close = false;
            crate::ui::context::object_menu(app, ui, &mut close);
        });
        out.textures_delta.clear();
        crate::ui::menus::replay::finish()
    };
    let fresh = || {
        let mut app = App::headless();
        let sq = square(&mut app);
        app.select(vec![sq]);
        (app, sq)
    };
    let (mut app, _) = fresh();
    let (_, rows) = frame(&mut app, Some(usize::MAX));
    assert!(rows > 10);
    let mut seen = Vec::new();
    for t in 0..rows {
        let (mut app, sq) = fresh();
        let (hit, _) = frame(&mut app, Some(t));
        let Some(key) = hit else { continue };
        if key.starts_with("context.wrap_") {
            seen.push(key.clone());
        }
        if key == "context.wrap_square_above_below" {
            let s = app.doc().find_shape(sq).expect("square");
            assert!(s.wrap_text);
            assert_eq!(
                (s.wrap_style.mode, s.wrap_style.side),
                (WrapMode::Square, WrapSide::AboveBelow)
            );
        }
        if key == "context.wrap_text_on" {
            assert!(app.doc().find_shape(sq).expect("square").wrap_text);
        }
    }
    for (mode, side) in TextWrap::STYLES {
        let key = crate::ui::propbar::wrap_style_key(mode, side);
        assert!(seen.iter().any(|k| k == key), "{key} missing: {seen:?}");
    }
    assert!(seen.iter().any(|k| k == "context.wrap_text_on"), "{seen:?}");
}
