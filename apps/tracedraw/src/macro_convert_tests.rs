use super::*;

fn project(code: &str) -> vp::Project {
    vp::Project {
        name: "Conv".into(),
        modules: vec![vp::Module {
            name: "Main".into(),
            kind: vp::ModuleKind::Standard,
            source: code.into(),
            has_source: true,
            read_only: false,
            private: false,
        }],
        code_page: 1252,
        ..Default::default()
    }
}

fn shape_count(app: &App) -> usize {
    app.doc().pages[0]
        .layers
        .iter()
        .map(|l| l.shapes.len())
        .sum()
}

fn drive(app: &mut App) {
    let t = std::time::Instant::now();
    while app.macro_tick() {
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(t.elapsed().as_secs() < 20, "the run did not end");
    }
}

const MIXED: &str = "Private Declare Function GetTickCount Lib \"kernel32\" () As Long\r\n\
Sub Fine()\r\n\
    ActiveLayer.CreateRectangle 0, 1, 1, 0\r\n\
End Sub\r\n\
Sub Quiet()\r\n\
    ActiveWindow.ActiveView.ToFitShapeRange ActiveSelectionRange\r\n\
    ActiveShape.Frobnicate 3\r\n\
End Sub\r\n\
Sub Broken()\r\n\
    Dim x As Double, y As Double, s As Long\r\n\
    ActiveDocument.GetUserClick x, y, s, 10, False, 0\r\n\
    Kill \"c:\\temp\\a.txt\"\r\n\
    If ActiveDocument.GetUserClick(x, y, s, 10, False, 0) = 0 Then Exit Sub\r\n\
End Sub\r\n";

#[test]
fn the_check_colours_each_macro_by_what_it_uses() {
    let r = check(&project(MIXED));
    let level = |n: &str| r.macros.iter().find(|m| m.name == n).map(|m| m.level);
    assert_eq!(level("Fine"), Some(Level::Ok), "{r:#?}");
    assert_eq!(level("Quiet"), Some(Level::Warn), "{r:#?}");
    assert_eq!(level("Broken"), Some(Level::Error), "{r:#?}");
    // The Windows function is named on its own line.
    assert!(r
        .issues
        .iter()
        .any(|i| i.line == 1 && i.level == Level::Error && i.text.contains("GetTickCount")));
    // A member that is not in the model, and one that does nothing.
    assert!(r
        .issues
        .iter()
        .any(|i| i.line == 7 && i.text.contains("Frobnicate")));
    assert!(r
        .issues
        .iter()
        .any(|i| i.line == 6 && i.text.contains("ToFitShapeRange")));
    assert_eq!(
        (
            r.count(Level::Ok),
            r.count(Level::Warn),
            r.count(Level::Error)
        ),
        (1, 1, 1)
    );
}

#[test]
fn fix_comments_out_what_is_not_available_and_still_compiles() {
    let mut p = project(MIXED);
    let r = check(&p);
    let done = fix_project(&mut p, &r);
    let src = &p.modules[0].source;
    let lines: Vec<&str> = src.lines().collect();
    // The statement on its own line is commented out; the one inside an
    // If gets a note above it; the If itself stays.
    assert!(
        lines.iter().any(|l| l.trim_start().starts_with("' TODO:")
            && l.contains("ActiveDocument.GetUserClick x, y")),
        "{src}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.trim_start().starts_with("' TODO:") && l.contains("Kill")),
        "{src}"
    );
    assert!(lines
        .iter()
        .any(|l| l.trim_start().starts_with("If ActiveDocument.GetUserClick")));
    assert!(
        done.commented >= 2 && done.notes >= 1,
        "{done:?}\n{src}\n{r:#?}"
    );
    let m = vba::Module::new("Main", vba::ModuleKind::Standard, src.clone());
    vba::compile(&[m]).expect("still compiles");
    // Fixing twice adds nothing new.
    let r2 = check(&p);
    let mut again = p.clone();
    let d2 = fix_project(&mut again, &r2);
    assert_eq!((d2.commented, d2.renamed), (0, 0), "{d2:?}");
    assert_eq!(
        again.modules[0].source.matches("' TODO").count(),
        src.matches("' TODO").count()
    );
}

#[test]
fn javascript_follows_the_object_model() {
    let code = "Sub Marks()\r\n\
    Dim s As Shape, i As Long\r\n\
    ' Corners\r\n\
    For Each s In ActiveSelectionRange\r\n\
        ActiveLayer.CreateRectangle Right:=s.RightX, Left:=s.LeftX, Top:=1, Bottom:=0\r\n\
    Next s\r\n\
    For i = 1 To ActivePage.Shapes.Count\r\n\
        Debug.Print ActivePage.Shapes(i).Name\r\n\
    Next\r\n\
End Sub\r\n";
    let c = to_javascript(&project(code)).expect("converts");
    let js = &c.js;
    assert!(js.starts_with(cv::JS_MARKER), "{js}");
    assert!(js.contains("// Run: Marks"), "{js}");
    assert!(js.contains("// Corners"), "{js}");
    assert!(js.contains("for (s of ActiveSelectionRange) {"), "{js}");
    // Named arguments go to their places.
    assert!(
        js.contains("ActiveLayer.CreateRectangle(s.LeftX, 1, s.RightX, 0);"),
        "{js}"
    );
    // A collection property with an index reads its item.
    assert!(js.contains("ActivePage.Shapes.Item(i).Name"), "{js}");
    assert!(
        js.contains("for (i = 1; i <= ActivePage.Shapes.Count; i++) {"),
        "{js}"
    );
    assert_eq!(c.todos, 0);
    assert_eq!(
        c.entries,
        vec![("Main".into(), "Marks".into(), "Marks".into())]
    );
}

#[test]
fn converted_javascript_runs_as_a_macro() {
    let code = "Sub Draw()\r\n\
    Dim i As Integer, names As String, r As Shape\r\n\
    ActiveDocument.Unit = cdrMillimeter\r\n\
    For i = 1 To 3\r\n\
        Set r = ActiveLayer.CreateRectangle2(i * 20, 10, 10, 10)\r\n\
        r.Name = \"Box\" & i\r\n\
    Next i\r\n\
    For Each r In ActivePage.Shapes\r\n\
        If Left(r.Name, 3) = \"Box\" Then names = names & UCase(Mid(r.Name, 4))\r\n\
    Next r\r\n\
    Debug.Print \"Made \" & Len(names) & \": \" & names\r\n\
End Sub\r\n";
    let c = to_javascript(&project(code)).expect("converts");
    let mut app = App::headless();
    let before = shape_count(&app);
    app.run_macro_js(&c.js, "Draw", "Scripts".into());
    drive(&mut app);
    assert!(
        app.macros.error.is_none(),
        "{:?}\n{}",
        app.macros.error,
        c.js
    );
    assert_eq!(
        shape_count(&app),
        before + 3,
        "{:?}\n{}",
        app.scripts_output,
        c.js
    );
    assert!(
        app.scripts_output.iter().any(|l| l.contains("Made 3: ")),
        "{:?}\n{}",
        app.scripts_output,
        c.js
    );
    // The Scripts panel runs it too, by its "// Run:" line.
    app.run_script(&c.js);
    drive(&mut app);
    assert_eq!(shape_count(&app), before + 6, "{:?}", app.scripts_output);
}

#[test]
fn javascript_notes_members_that_are_not_available() {
    let c = to_javascript(&project(MIXED)).expect("converts");
    let lines: Vec<&str> = c.js.lines().map(str::trim).collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with("ActiveDocument.GetUserClick("))
        .expect("the call");
    assert!(
        lines[at - 1].starts_with("// TODO: GetUserClick is not available in TraceDraw"),
        "{}",
        c.js
    );
    assert!(c.js.contains("// TODO: Declare GetTickCount"), "{}", c.js);
}

#[test]
fn run_lines_name_the_function() {
    let js = format!(
        "{}\n// Run: A\nfunction A() {{}}\nfunction B() {{}}\n",
        cv::JS_MARKER
    );
    assert_eq!(cv::js_run_name(&js).as_deref(), Some("A"));
    let js2 = set_run_line(&js, "B");
    assert_eq!(cv::js_run_name(&js2).as_deref(), Some("B"));
    assert_eq!(cv::js_entries(&js2), vec!["A".to_string(), "B".to_string()]);
}
