//! The conversion to readable JavaScript and the facts the Macro Editor's
//! compatibility check reads (`tracedraw_vba::convert`).

use tracedraw_vba::convert::{module_facts, to_javascript, JsHints};
use tracedraw_vba::{Module, ModuleKind};

fn js(src: &str) -> String {
    let m = [Module::new("Main", ModuleKind::Standard, src)];
    to_javascript(&m, "P", &JsHints::default()).unwrap().js
}

#[test]
fn select_with_and_constants_read_as_javascript() {
    let out = js("Sub Go()\r\n\
        Dim s As String, n As Long\r\n\
        Select Case n\r\n\
        Case 1 To 3: s = \"low\"\r\n\
        Case Is > 10, 7: s = \"high\"\r\n\
        Case Else: s = Left(\"abc\", 2) & n\r\n\
        End Select\r\n\
        With ActiveLayer\r\n\
        .Name = \"L\" & n\r\n\
        End With\r\n\
        If MsgBox(\"Go?\", vbYesNo) = vbYes Then n = n \\ 2\r\n\
        End Sub\r\n");
    for want in [
        "const vbYesNo = 4;",
        "const vbYes = 6;",
        "const sel = n;",
        "if (sel >= 1 && sel <= 3) {",
        "} else if (sel > 10 || sel == 7) {",
        "s = Left(\"abc\", 2) + n;",
        "const it = ActiveLayer;",
        "it.Name = \"L\" + n;",
        "n = Math.trunc(n / 2);",
    ] {
        assert!(out.contains(want), "{want}\n{out}");
    }
}

#[test]
fn functions_return_their_result_and_notes_mark_what_differs() {
    let out = js("Function Twice(v As Double) As Double\r\n\
        v = v * 2\r\n\
        Twice = v\r\n\
        End Function\r\n\
        Sub Go()\r\n\
        On Error Resume Next\r\n\
        GoTo Done\r\n\
        Done:\r\n\
        End Sub\r\n");
    assert!(out.contains("function Twice(v) {"), "{out}");
    assert!(out.contains("let result = 0;"), "{out}");
    assert!(out.contains("result = v;"), "{out}");
    assert!(out.contains("return result;"), "{out}");
    // ByRef parameter changed, error handler and GoTo: three notes.
    assert_eq!(out.matches("// TODO:").count(), 3, "{out}");
}

#[test]
fn facts_list_members_with_their_lines() {
    let m = Module::new(
        "Main",
        ModuleKind::Standard,
        "Sub Go()\r\nActiveDocument.Unit = 3\r\nKill \"a\"\r\nEnd Sub\r\n",
    );
    let f = module_facts(&m).unwrap();
    assert_eq!(f.members[0].line, 2);
    assert_eq!(f.members[0].name, "Unit");
    assert_eq!(f.unsupported.len(), 1);
    assert_eq!(f.unsupported[0].0, 3);
    assert_eq!(f.procs, vec![("Go".to_string(), 1)]);
}
