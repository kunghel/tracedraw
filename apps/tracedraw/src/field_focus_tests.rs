//! Ctrl+K and the other command shortcuts after a property bar field
//! was typed into: the video of #2893 typed in the bar, then pressed
//! Ctrl+K, and nothing broke apart.
use super::*;

/// The rectangle of `setup` with a drop shadow, the bar's width field
/// clicked and "60" typed in it, not yet entered.
fn typed_in_the_width_field() -> (egui::Context, App, ShapeId) {
    let (ctx, mut app, id) = setup();
    app.set_shadow(Some(tracedraw_core::Shadow::default()), false);
    let texts = texts_drawn(&ctx, &mut app, Vec::new());
    let at = text_at(&texts, "50.0 mm").expect("the width field");
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert!(
        ctx.egui_wants_keyboard_input(),
        "the field has the keyboard"
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::A, Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    frame(
        &ctx,
        &mut app,
        vec![key_up(egui::Key::A, Modifiers::COMMAND)],
        Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("60".into())],
        Modifiers::NONE,
    );
    assert!(ctx.egui_wants_keyboard_input(), "still typing");
    (ctx, app, id)
}

fn tap(ctx: &egui::Context, app: &mut App, k: egui::Key, m: Modifiers) {
    frame(ctx, app, vec![key(k, m)], m);
    frame(ctx, app, vec![key_up(k, m)], Modifiers::NONE);
    frame(ctx, app, Vec::new(), Modifiers::NONE);
}

fn broken_apart(app: &App, id: ShapeId) -> bool {
    app.doc().find_shape(id).expect("rect").shadow.is_none() && shape_count(app) == 2
}

/// Enter takes the value and gives the keyboard back to the drawing:
/// Ctrl+K then breaks the shadow off.
#[test]
fn ctrl_k_after_enter_in_a_bar_field() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    tap(&ctx, &mut app, egui::Key::Enter, Modifiers::NONE);
    assert!(
        close(bounds(&app, id).width(), 60.0),
        "{:?}",
        bounds(&app, id)
    );
    assert!(!ctx.egui_wants_keyboard_input(), "back to the drawing");
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    assert!(broken_apart(&app, id));
}

/// A click on the object takes the value and gives the keyboard back
/// to the drawing, the object still selected.
#[test]
fn ctrl_k_after_a_click_on_the_object() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    let on = app.view.to_screen(Point::new(30.0, 25.0));
    click_at(&ctx, &mut app, on, Modifiers::NONE);
    assert!(
        close(bounds(&app, id).width(), 60.0),
        "{:?}",
        bounds(&app, id)
    );
    assert!(!ctx.egui_wants_keyboard_input(), "back to the drawing");
    assert_eq!(app.selection, vec![id]);
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    assert!(broken_apart(&app, id));
}

/// Ctrl+K pressed while the field still has the keyboard: the field
/// takes its value and the command runs all the same.
#[test]
fn ctrl_k_while_a_bar_field_has_the_keyboard() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    assert!(broken_apart(&app, id), "{}", shape_count(&app));
    assert!(
        close(bounds(&app, id).width(), 60.0),
        "{:?}",
        bounds(&app, id)
    );
    assert!(!ctx.egui_wants_keyboard_input(), "back to the drawing");
}

/// The keys that edit the text of a field stay the field's: Ctrl+Z,
/// Ctrl+C and Ctrl+A do not undo, copy or select in the drawing.
#[test]
fn editing_keys_stay_in_a_bar_field() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    let steps = app.engine.history_labels().0.len();
    let selected = app.selection.clone();
    for k in [
        egui::Key::Z,
        egui::Key::C,
        egui::Key::A,
        egui::Key::X,
        egui::Key::V,
    ] {
        frame(
            &ctx,
            &mut app,
            vec![key(k, Modifiers::COMMAND)],
            Modifiers::COMMAND,
        );
        frame(
            &ctx,
            &mut app,
            vec![key_up(k, Modifiers::COMMAND)],
            Modifiers::NONE,
        );
        assert!(ctx.egui_wants_keyboard_input(), "{k:?}: the field keeps it");
    }
    assert_eq!(app.engine.history_labels().0.len(), steps, "nothing undone");
    assert_eq!(app.selection, selected);
    assert_eq!(shape_count(&app), 1, "nothing pasted or cut");
    assert!(app.doc().find_shape(id).expect("rect").shadow.is_some());
}

/// Ctrl+S in the code of the macro editor (Alt+F11) saves the macro, as
/// its Save button says, and the code keeps the keyboard. Since #2893
/// the code gave the keyboard up and Ctrl+S went to the drawing: the
/// Save Drawing dialog opened and the macro stayed unsaved.
#[test]
fn ctrl_s_in_the_macro_code_saves_the_macro() {
    let (ctx, mut app, _) = setup();
    let dir = std::env::temp_dir().join(format!("tracedraw-ctrl-s-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    app.macros.dir = Some(dir.clone());
    app.open_macro_editor();
    assert!(app.macros.editor.is_some(), "{}", app.status);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let code = egui::Id::new("macro_code_edit");
    ctx.memory_mut(|m| m.request_focus(code));
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("'".into())],
        Modifiers::NONE,
    );
    assert!(app.macros.editor.as_ref().expect("editor").dirty, "typed");
    tap(&ctx, &mut app, egui::Key::S, Modifiers::COMMAND);
    assert!(
        matches!(app.dialog, crate::ui::dialogs::Dialog::None),
        "{:?}",
        app.dialog
    );
    assert!(!app.macros.editor.as_ref().expect("editor").dirty, "saved");
    assert_eq!(ctx.memory(|m| m.focused()), Some(code));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Ctrl itself arrives as a key press (on Windows with Ctrl already
/// held): the field keeps the keyboard, and the Ctrl+V after it pastes
/// into the field, not on the drawing (#3198).
#[test]
fn ctrl_alone_keeps_the_keyboard_in_a_bar_field() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    for k in [egui::Key::ControlLeft, egui::Key::ControlRight] {
        frame(
            &ctx,
            &mut app,
            vec![key(k, Modifiers::COMMAND)],
            Modifiers::COMMAND,
        );
        frame(&ctx, &mut app, Vec::new(), Modifiers::COMMAND);
        assert!(ctx.egui_wants_keyboard_input(), "{k:?}: the field keeps it");
    }
    frame(
        &ctx,
        &mut app,
        vec![Event::Paste("5".into())],
        Modifiers::COMMAND,
    );
    frame(
        &ctx,
        &mut app,
        vec![key_up(egui::Key::ControlLeft, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(ctx.egui_wants_keyboard_input(), "still typing");
    assert_eq!(shape_count(&app), 1, "nothing pasted on the drawing");
    tap(&ctx, &mut app, egui::Key::Enter, Modifiers::NONE);
    assert!(
        close(bounds(&app, id).width(), 605.0),
        "the paste went into the field: {:?}",
        bounds(&app, id)
    );
}

/// A text with "Hello" typed and left, then edited again with " world"
/// typed at its end: two undo steps.
fn editing_hello_world() -> (egui::Context, App, ShapeId) {
    let (ctx, mut app, _) = setup();
    app.set_tool(Tool::Text);
    app.start_text(Point::new(20.0, 80.0), None);
    app.text_insert("Hello");
    let id = app.text_edit.as_ref().expect("editing").shape;
    app.finish_text();
    app.begin_text_edit(id);
    app.text_insert(" world");
    assert_eq!(app.edit_text(), "Hello world");
    (ctx, app, id)
}

/// Ctrl+Z while a text is being typed undoes the typing and the text is
/// edited on; Ctrl+Shift+Z brings it back (#3108: Ctrl+Z did nothing
/// until the editing was left).
#[test]
fn ctrl_z_while_typing_undoes_and_the_text_is_edited_on() {
    let (ctx, mut app, id) = editing_hello_world();
    tap(&ctx, &mut app, egui::Key::Z, Modifiers::COMMAND);
    assert_eq!(
        app.text_edit.as_ref().map(|t| t.shape),
        Some(id),
        "still editing"
    );
    assert_eq!(app.edit_text(), "Hello");
    tap(
        &ctx,
        &mut app,
        egui::Key::Z,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert_eq!(app.text_edit.as_ref().map(|t| t.shape), Some(id));
    assert_eq!(app.edit_text(), "Hello world");
    // Typing goes on at the caret, into a step of its own.
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("!".into())],
        Modifiers::NONE,
    );
    assert_eq!(app.edit_text(), "Hello world!");
    tap(&ctx, &mut app, egui::Key::Z, Modifiers::COMMAND);
    assert_eq!(app.edit_text(), "Hello world");
    // Alt+Backspace undoes too; undoing the text's creation ends the
    // editing.
    tap(&ctx, &mut app, egui::Key::Backspace, Modifiers::ALT);
    assert_eq!(app.edit_text(), "Hello");
    tap(&ctx, &mut app, egui::Key::Z, Modifiers::COMMAND);
    assert!(app.text_edit.is_none());
    assert!(app.doc().find_shape(id).is_none());
}

/// Ctrl+Shift+T while a text is being typed with the Text tool opens
/// Edit Text for it (#3266).
#[test]
fn ctrl_shift_t_while_typing_opens_edit_text() {
    let (ctx, mut app, id) = editing_hello_world();
    tap(
        &ctx,
        &mut app,
        egui::Key::T,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    match &app.dialog {
        crate::ui::dialogs::Dialog::EditText { shape, text, .. } => {
            assert_eq!(*shape, id);
            assert_eq!(text, "Hello world");
        }
        d => panic!("{d:?}"),
    }
    assert_eq!(app.doc().find_shape(id).map(|_| ()), Some(()));
}

/// A tool or a command given F13 to F24 (Options > Customization >
/// Commands) is picked or runs while a text is being typed; the
/// typing's own Ctrl keys stay its own (#3251).
#[test]
fn keys_f13_to_f24_given_a_command_work_while_typing() {
    let (ctx, mut app, id) = editing_hello_world();
    app.settings
        .shortcuts
        .push((Tool::Rectangle.id().to_string(), "F13".into()));
    app.settings
        .shortcuts
        .push(("menu.view.rulers".to_string(), "Shift+F21".into()));
    // Ctrl+B is the typing's bold, Ctrl+K (no use in typing) nothing.
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    assert_eq!(app.edit_text(), "Hello world");
    assert!(app.text_edit.is_some());
    tap(&ctx, &mut app, egui::Key::F13, Modifiers::NONE);
    assert_eq!(app.tool, Tool::Rectangle);
    assert!(app.text_edit.is_none(), "the editing ended for the tool");
    assert!(app.doc().find_shape(id).is_some());
    app.set_tool(Tool::Text);
    app.begin_text_edit(id);
    let rulers = app.show_rulers;
    tap(&ctx, &mut app, egui::Key::F21, Modifiers::SHIFT);
    assert_eq!(app.show_rulers, !rulers, "the command ran");
    assert_eq!(app.edit_text(), "Hello world");
}

/// View > Zoom 1:1 is a command a key can be given (#3103): pressed,
/// the drawing shows at its real size, the zoom box's 100%.
#[test]
fn zoom_1to1_takes_a_key_and_shows_the_real_size() {
    let (ctx, mut app, _) = setup();
    app.settings
        .shortcuts
        .push(("menu.view.zoom_1to1".to_string(), "F22".into()));
    app.set_zoom_percent(37.0);
    assert!((app.zoom_percent() - 37.0).abs() < 0.01);
    tap(&ctx, &mut app, egui::Key::F22, Modifiers::NONE);
    assert!(
        (app.zoom_percent() - 100.0).abs() < 0.01,
        "{}",
        app.zoom_percent()
    );
}

/// Edit > Find and Replace: a word copied elsewhere and pasted with
/// Ctrl+V goes into the Find field (and the Replace field), not on the
/// drawing (#3331).
#[test]
fn paste_goes_into_the_find_and_replace_fields() {
    let (ctx, mut app, _) = setup();
    app.show_dockers = true;
    app.docker_tab = crate::app::DockerTab::FindReplace;
    app.find_state.find = "abc".into();
    app.find_state.replace = "xyz".into();
    for _ in 0..3 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    for (field, kind) in [("abc", 0), ("xyz", 0), ("abc", 1)] {
        app.find_state.kind = kind;
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
        let texts = texts_drawn(&ctx, &mut app, Vec::new());
        let at = text_at(&texts, field).unwrap_or_else(|| panic!("the {field} field"));
        click_at(&ctx, &mut app, at, Modifiers::NONE);
        assert!(ctx.egui_wants_keyboard_input(), "{field}: the field has the keyboard");
        frame(&ctx, &mut app, vec![key(egui::Key::End, Modifiers::NONE)], Modifiers::NONE);
        frame(
            &ctx,
            &mut app,
            vec![key(egui::Key::ControlLeft, Modifiers::COMMAND)],
            Modifiers::COMMAND,
        );
        frame(
            &ctx,
            &mut app,
            vec![key(egui::Key::V, Modifiers::COMMAND), Event::Paste("Q".into())],
            Modifiers::COMMAND,
        );
        frame(
            &ctx,
            &mut app,
            vec![key_up(egui::Key::ControlLeft, Modifiers::NONE)],
            Modifiers::NONE,
        );
        assert_eq!(shape_count(&app), 1, "{field}: nothing pasted on the drawing");
        let got = if field == "xyz" { &app.find_state.replace } else { &app.find_state.find };
        assert_eq!(got, &format!("{field}Q"), "the paste went into the field");
        app.find_state.find = "abc".into();
        app.find_state.replace = "xyz".into();
        tap(&ctx, &mut app, egui::Key::Escape, Modifiers::NONE);
    }
}

/// Text > Edit Text: Enter starts a new line and Tab types a tab, and
/// the box stays open; Alt+O is OK, with no "o" typed and no Object
/// menu opened behind the box (#3370, #3372).
#[test]
fn edit_text_box_takes_enter_and_tab_and_alt_o_is_ok() {
    let (ctx, mut app, id) = editing_hello_world();
    app.finish_text();
    app.select(vec![id]);
    app.open_edit_text_dialog();
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("edit_text_dialog_box")));
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    frame(&ctx, &mut app, vec![key(egui::Key::End, Modifiers::COMMAND)], Modifiers::COMMAND);
    tap(&ctx, &mut app, egui::Key::Enter, Modifiers::NONE);
    frame(&ctx, &mut app, vec![Event::Text("x".into())], Modifiers::NONE);
    tap(&ctx, &mut app, egui::Key::Tab, Modifiers::NONE);
    frame(&ctx, &mut app, vec![Event::Text("y".into())], Modifiers::NONE);
    match &app.dialog {
        crate::ui::dialogs::Dialog::EditText { text, .. } => {
            assert_eq!(text, "Hello world\nx\ty")
        }
        d => panic!("the box closed: {d:?}"),
    }
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::O, Modifiers::ALT), Event::Text("o".into())],
        Modifiers::ALT,
    );
    frame(&ctx, &mut app, vec![key_up(egui::Key::O, Modifiers::NONE)], Modifiers::NONE);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::None), "OK closed it");
    let shape = app.doc().find_shape(id).expect("text");
    let tracedraw_core::ShapeKind::Text { spans, .. } = &shape.kind else {
        panic!("not a text")
    };
    assert_eq!(tracedraw_core::spans_text(spans), "Hello world\nx\ty");
    assert!(!egui::Popup::is_any_open(&ctx), "no menu opened");
}

/// Bitmaps > Resample: a width typed and left by clicking the percent
/// box stays, and the percent box shows the percent it gives (#3332).
#[test]
fn resample_width_typed_stays_when_another_box_is_clicked() {
    let (ctx, mut app, _) = setup();
    app.dialog = crate::ui::dialogs::Dialog::Resample(crate::resample::ResampleState::new(
        (400, 200),
        (101.6, 50.8),
    ));
    for _ in 0..3 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    let texts = texts_drawn(&ctx, &mut app, Vec::new());
    let width = text_at(&texts, "400 px").unwrap_or_else(|| panic!("width box: {texts:?}"));
    click_at(&ctx, &mut app, width, Modifiers::NONE);
    assert!(ctx.egui_wants_keyboard_input());
    frame(&ctx, &mut app, vec![Event::Text("200".into())], Modifiers::NONE);
    let percent = text_at(&texts, "100.0 %").expect("percent box");
    click_at(&ctx, &mut app, percent, Modifiers::NONE);
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    let texts = texts_drawn(&ctx, &mut app, Vec::new());
    assert!(
        texts.iter().any(|(t, _)| t == "50.0 %"),
        "the percent box follows: {texts:?}"
    );
    // Leaving the percent box, untouched, keeps the width.
    tap(&ctx, &mut app, egui::Key::Tab, Modifiers::NONE);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let crate::ui::dialogs::Dialog::Resample(st) = &app.dialog else {
        panic!("the dialog closed")
    };
    assert_eq!((st.w, st.h), (200, 100));
}

/// File > Print and Enter prints, as the Print button is the dialog's
/// default (#3337, #3342).
#[test]
fn enter_prints_from_the_print_dialog() {
    let (ctx, mut app, _) = setup();
    app.print();
    if let crate::ui::dialogs::Dialog::Print(st) = &mut app.dialog {
        st.destination = 1;
    }
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    let _ = crate::files::test_save::asked();
    tap(&ctx, &mut app, egui::Key::Enter, Modifiers::NONE);
    let asked = crate::files::test_save::asked();
    assert_eq!(asked.len(), 1, "Enter printed to a PDF file");
    assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::None));
}

/// One frame, giving back the texts it put on the system clipboard.
fn frame_copied(
    ctx: &egui::Context,
    app: &mut App,
    events: Vec<Event>,
    modifiers: Modifiers,
) -> Vec<String> {
    let mut all = vec![Event::ModifiersChanged(modifiers)];
    all.extend(events);
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(1600.0, 1000.0),
        )),
        events: all,
        ..Default::default()
    };
    crate::pen::filter_raw_input(ctx, &mut input);
    crate::interaction::release_chorded_left(ctx, &mut input);
    let out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
    out.platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// Ctrl+C in the width field of the property bar copies its number, not
/// the object, also when Ctrl itself arrives as a key press first (on
/// Windows) (#3460).
#[test]
fn ctrl_c_in_a_size_field_copies_the_number() {
    let (ctx, mut app, id) = setup();
    let texts = texts_drawn(&ctx, &mut app, Vec::new());
    let at = text_at(&texts, "50.0 mm").expect("the width field");
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert!(ctx.egui_wants_keyboard_input(), "the field has the keyboard");
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::A, Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    let copied = frame_copied(
        &ctx,
        &mut app,
        vec![
            key(egui::Key::ControlLeft, Modifiers::COMMAND),
            key(egui::Key::C, Modifiers::COMMAND),
            Event::Copy,
        ],
        Modifiers::COMMAND,
    );
    assert_eq!(copied, vec!["50.0 mm".to_string()]);
    assert!(app.clipboard.is_none(), "the object was not copied");
    assert!(ctx.egui_wants_keyboard_input(), "the field keeps it");
    frame(
        &ctx,
        &mut app,
        vec![key_up(egui::Key::ControlLeft, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(shape_count(&app), 1);
    assert_eq!(app.selection, vec![id]);
}
