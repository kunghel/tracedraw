//! Text files and the file system (#4315): the file statements, the file
//! functions and the FileSystemObject over a host that keeps its files in
//! memory, and keeping going past what the host cannot do.

#[allow(dead_code)]
mod common;
use boa_engine::{Context, Source};
use common::FAKE_MODEL;
use tracedraw_vba::{compile_with, entry_call, Module, ModuleKind, Options, RUNTIME_JS};

/// A host file system in memory: `__files` by path, folders in `__dirs`.
const FAKE_FS: &str = r##"
var __files = { "/docs/in.csv": "\"Ann\",30,#TRUE#\r\n\"Bob, Jr\",41.5,#FALSE#\r\n",
                "/docs/names.txt": "one\r\ntwo\nthree" };
var __dirs = { "/docs": true, "/": true };
function __parent(p) { const i = p.lastIndexOf("/"); return i <= 0 ? "/" : p.slice(0, i); }
function __host_fs(op, a, b, c) {
  const fail = (n) => "\u0001" + n;
  switch (op) {
    case "cwd": return "/docs";
    case "env": return a === "USERNAME" ? "ann" : "";
    case "special": return a === "2" ? "/tmp" : "/docs";
    case "exists": return __dirs[a] ? "d" : (a in __files ? "f" : "");
    case "read": return a in __files ? __files[a] : fail(53);
    case "write":
      if (!__dirs[__parent(a)]) return fail(76);
      __files[a] = (c === "1" && a in __files ? __files[a] : "") + b; return "";
    case "size": return a in __files ? String(__files[a].length) : fail(53);
    case "mtime": return "45000.5";
    case "attr": return __dirs[a] ? "16" : "32";
    case "list": {
      const pre = a.replace(/\/$/, "") + "/";
      const out = [];
      for (const f of Object.keys(__files)) if (f.startsWith(pre) && f.slice(pre.length).indexOf("/") < 0) out.push({ n: f.slice(pre.length), d: false, s: __files[f].length });
      for (const d of Object.keys(__dirs)) if (d.startsWith(pre) && d.length > pre.length && d.slice(pre.length).indexOf("/") < 0) out.push({ n: d.slice(pre.length), d: true, s: 0 });
      out.sort((x, y) => x.n.localeCompare(y.n));
      return JSON.stringify(out);
    }
    case "kill": {
      if (a.indexOf("*") < 0) { if (!(a in __files)) return fail(53); delete __files[a]; return "1"; }
      const re = new RegExp("^" + a.replace(/\./g, "\\.").replace(/\*/g, "[^/]*") + "$");
      let n = 0;
      for (const f of Object.keys(__files)) if (re.test(f)) { delete __files[f]; n++; }
      return String(n);
    }
    case "mkdir": if (__dirs[a] || a in __files) return fail(75); __dirs[a] = true; return "";
    case "rmdir": if (!__dirs[a]) return fail(76); delete __dirs[a]; return "";
    case "copy": if (!(a in __files)) return fail(53); if (b in __files && c !== "1") return fail(58); __files[b] = __files[a]; return "";
    case "move": if (!(a in __files)) return fail(53); __files[b] = __files[a]; delete __files[a]; return "";
  }
  return fail(5);
}
"##;

fn run_opts(src: &str, keep_going: bool) -> (Vec<String>, Result<(), String>, String) {
    let modules = [Module::new("Module1", ModuleKind::Standard, src)];
    let opts = Options {
        keep_going,
        ..Default::default()
    };
    let program = compile_with(&modules, &opts).unwrap_or_else(|e| panic!("compile error: {e}"));
    let mut ctx = Context::default();
    ctx.eval(Source::from_bytes(FAKE_MODEL)).expect("model");
    ctx.eval(Source::from_bytes(FAKE_FS)).expect("fs");
    ctx.eval(Source::from_bytes(RUNTIME_JS)).expect("runtime");
    if let Err(e) = ctx.eval(Source::from_bytes(&program.js)) {
        panic!("program: {e}\n{}", program.js);
    }
    let call = entry_call(&program, "Module1", "Main");
    let result = ctx
        .eval(Source::from_bytes(&call))
        .map(|_| ())
        .map_err(|e| e.to_string());
    let get = |ctx: &mut Context, js: &str| {
        ctx.eval(Source::from_bytes(js))
            .ok()
            .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
            .unwrap_or_default()
    };
    let text = get(&mut ctx, "__lines.join('\\u0001')");
    let lines = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\u{1}').map(str::to_string).collect()
    };
    let skipped = get(&mut ctx, "JSON.stringify(__v.skipped)");
    (lines, result, skipped)
}

fn run(src: &str) -> Vec<String> {
    let (lines, result, _) = run_opts(src, false);
    if let Err(e) = result {
        panic!("run-time error: {e}");
    }
    lines
}

#[test]
fn text_files_are_written_and_read_back_line_by_line() {
    let out = run(r##"
Sub Main()
    Dim n As Integer, s As String, i As Long
    n = FreeFile
    Open "out.txt" For Output As #n
    Print #n, "first"
    Print #n, "a"; "b";
    Print #n, "c"
    Print #n, 5
    Close #n
    Open "out.txt" For Append As #n
    Print #n, "last"
    Close n
    Open "out.txt" For Input As #1
    Do While Not EOF(1)
        Line Input #1, s
        i = i + 1
        Debug.Print i & ":" & s & "|"
    Loop
    Close
    Debug.Print FileLen("/docs/out.txt"), Dir("/docs/out.txt"), CurDir
End Sub
"##);
    assert_eq!(
        out,
        vec!["1:first|", "2:abc|", "3: 5 |", "4:last|", "23            out.txt       /docs"]
    );
}

#[test]
fn input_reads_fields_and_write_quotes_them() {
    let out = run(r##"
Sub Main()
    Dim name, age, ok
    Open "/docs/in.csv" For Input As #1
    Do Until EOF(1)
        Input #1, name, age, ok
        Debug.Print name & "|" & age & "|" & ok & "|" & TypeName(age)
    Loop
    Close #1
    Open "/docs/w.csv" For Output As #2
    Write #2, "x, y", 2.5, True
    Close #2
    Open "/docs/w.csv" For Input As #2
    Line Input #2, name
    Close #2
    Debug.Print name
End Sub
"##);
    assert_eq!(
        out,
        vec![
            "Ann|30|True|Integer",
            "Bob, Jr|41.5|False|Double",
            "\"x, y\",2.5,#TRUE#",
        ]
    );
}

#[test]
fn dir_goes_through_a_folder_and_files_are_copied_renamed_and_removed() {
    let out = run(r##"
Sub Main()
    Dim f As String, n As Long
    FileCopy "/docs/names.txt", "/docs/copy.txt"
    Name "/docs/copy.txt" As "/docs/moved.txt"
    f = Dir("/docs/*.txt")
    Do While f <> ""
        Debug.Print f
        f = Dir
    Loop
    MkDir "/docs/sub"
    Debug.Print Dir("/docs/sub", vbDirectory), (GetAttr("/docs/sub") And vbDirectory) <> 0
    Kill "/docs/*.txt"
    Debug.Print Dir("/docs/*.txt") = ""
    On Error Resume Next
    Open "/docs/missing.txt" For Input As #1
    Debug.Print Err.Number
    Err.Clear
    Kill "/docs/missing.txt"
    Debug.Print Err.Number
End Sub
"##);
    assert_eq!(
        out,
        vec!["moved.txt", "names.txt", "sub           True", "True", "53", "53"]
    );
}

#[test]
fn the_file_system_object_reads_writes_and_lists() {
    let out = run(r##"
Sub Main()
    Dim fso, ts, f, line
    Set fso = CreateObject("Scripting.FileSystemObject")
    Set ts = fso.CreateTextFile(fso.BuildPath("/docs", "log.txt"), True)
    ts.WriteLine "alpha"
    ts.Write "beta"
    ts.Close
    Set ts = fso.OpenTextFile("/docs/log.txt", 1)
    Do Until ts.AtEndOfStream
        Debug.Print ts.ReadLine
    Loop
    ts.Close
    Debug.Print fso.FileExists("/docs/log.txt"), fso.FolderExists("/docs"), fso.GetBaseName("/a/b.c.txt"), fso.GetExtensionName("x.CDR")
    For Each f In fso.GetFolder("/docs").Files
        Debug.Print f.Name & "=" & f.Size
    Next
    fso.CopyFile "/docs/log.txt", "/docs/log2.txt"
    fso.DeleteFile "/docs/log.txt"
    Debug.Print fso.FileExists("/docs/log.txt"), fso.GetFile("/docs/log2.txt").Name, fso.GetParentFolderName("/docs/log2.txt")
    Debug.Print fso.GetSpecialFolder(2).Path, Environ("USERNAME"), CreateObject("WScript.Network").UserName
    Set ts = fso.OpenTextFile("/docs/log2.txt", 8)
    ts.WriteLine "!"
    ts.Close
    Debug.Print fso.OpenTextFile("/docs/log2.txt").ReadAll
End Sub
"##);
    assert_eq!(
        out,
        vec![
            "alpha",
            "beta",
            "True          True          b.c           CDR",
            "in.csv=41",
            "log.txt=11",
            "names.txt=14",
            "False         log2.txt      /docs",
            "/tmp          ann           ann",
            "alpha\r\nbeta!\r\n",
        ]
    );
}

#[test]
fn keep_going_skips_what_cannot_be_done_and_lists_it() {
    let src = r##"
Sub Main()
    Dim xl, s As Shape
    ActiveLayer.Frobnicate 3
    Debug.Print "after a missing member"
    Set xl = CreateObject("Excel.Application")
    xl.Visible = True
    Debug.Print "after Excel"
    For Each s In ActiveSelection.Shapes
        s.Twist 10
    Next
    Debug.Print "after the loop"
    Debug.Print 1 / 0
End Sub
"##;
    // Without keeping going, the first one stops the macro.
    let (lines, result, _) = run_opts(src, false);
    assert!(lines.is_empty(), "{lines:?}");
    assert!(result.unwrap_err().contains("'438'"));
    let (lines, result, skipped) = run_opts(src, true);
    assert_eq!(
        lines,
        vec!["after a missing member", "after Excel", "after the loop"]
    );
    // A real error still stops it.
    assert!(result.unwrap_err().contains("'11'"));
    // The missing member, Excel, what followed from it, and the member in
    // the loop (twice, listed once).
    let v: Vec<&str> = skipped.split("},{").collect();
    assert_eq!(v.len(), 4, "{skipped}");
    assert!(skipped.contains("Frobnicate"), "{skipped}");
    assert!(skipped.contains("\"c\":2"), "{skipped}");
}

#[test]
fn keep_going_leaves_the_macros_own_error_handling_alone() {
    let (lines, result, skipped) = run_opts(
        r##"
Sub Main()
    On Error GoTo Handler
    ActiveLayer.Frobnicate 3
    Debug.Print "not here"
    Exit Sub
Handler:
    Debug.Print "handled " & Err.Number
End Sub
"##,
        true,
    );
    result.unwrap();
    assert_eq!(lines, vec!["handled 438"]);
    assert_eq!(skipped, "[]");
}
