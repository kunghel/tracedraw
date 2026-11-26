//! The file system for VBA macros: `__fs(op, a, b, c)`, which the VBA
//! runtime's file statements, file functions and FileSystemObject call
//! (`__host_fs`). Every answer is a string; a failure is "\u{1}" and the
//! VBA error number (53 file not found, 76 path not found, 75 access
//! error, 58 already exists). Text is read as UTF-8 when it is, else in
//! the Windows Western code page, as macros write it; it is written in
//! that code page when every character fits, else as UTF-8.

use boa_engine::{Context, JsResult, JsString, JsValue};

/// `__fs(op, a, b, c)` for the script engine.
pub fn native_fs(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let mut s = [String::new(), String::new(), String::new(), String::new()];
    for (i, slot) in s.iter_mut().enumerate() {
        if let Some(v) = args.get(i).filter(|v| !v.is_undefined() && !v.is_null()) {
            *slot = v.to_string(ctx)?.to_std_string_escaped();
        }
    }
    let r = match fs_op(&s[0], &s[1], &s[2], &s[3]) {
        Ok(v) => v,
        Err(n) => format!("\u{1}{n}"),
    };
    Ok(JsValue::from(JsString::from(r.as_str())))
}

/// A VBA error number.
type Fail = u32;

fn io_fail(e: &std::io::Error) -> Fail {
    match e.kind() {
        std::io::ErrorKind::NotFound => 53,
        std::io::ErrorKind::AlreadyExists => 58,
        std::io::ErrorKind::PermissionDenied => 70,
        _ => 75,
    }
}

/// A file's text: UTF-8 (without its byte order mark) when it is, UTF-16
/// with a byte order mark, else the Western code page.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(bytes).0.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(bytes).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// Text as a macro writes it: the Western code page when every character
/// has a place in it, else UTF-8.
pub fn encode_text(s: &str) -> Vec<u8> {
    let (bytes, _, unmappable) = encoding_rs::WINDOWS_1252.encode(s);
    if unmappable {
        s.as_bytes().to_vec()
    } else {
        bytes.into_owned()
    }
}

/// A VBA date serial (days since 30 December 1899) of a time.
fn serial(t: std::time::SystemTime) -> f64 {
    let secs = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs_f64(),
        Err(e) => -e.duration().as_secs_f64(),
    };
    secs / 86400.0 + 25569.0
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn home() -> String {
    let h = env(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if h.is_empty() {
        env("HOME")
    } else {
        h
    }
}

fn join(a: &str, b: &str) -> String {
    std::path::Path::new(a).join(b).to_string_lossy().into_owned()
}

/// The user's documents folder (where macros start, as CurDir).
fn documents() -> String {
    let h = home();
    let d = join(&h, "Documents");
    if std::path::Path::new(&d).is_dir() {
        d
    } else {
        h
    }
}

/// A special folder by the name WScript.Shell gives it, or by the number
/// FileSystemObject.GetSpecialFolder takes.
fn special(name: &str) -> String {
    let h = home();
    match name.to_ascii_lowercase().as_str() {
        "0" => env("WINDIR"),
        "1" => join(&env("WINDIR"), "System32"),
        "2" => std::env::temp_dir().to_string_lossy().into_owned(),
        "desktop" | "alluserdesktop" => join(&h, "Desktop"),
        "mydocuments" | "personal" => documents(),
        "templates" => join(&h, "Templates"),
        "appdata" => {
            let a = env("APPDATA");
            if a.is_empty() {
                join(&h, ".config")
            } else {
                a
            }
        }
        "localappdata" => {
            let a = env("LOCALAPPDATA");
            if a.is_empty() {
                join(&h, ".local/share")
            } else {
                a
            }
        }
        "fonts" => {
            if cfg!(windows) {
                join(&env("WINDIR"), "Fonts")
            } else {
                join(&h, ".fonts")
            }
        }
        "favorites" => join(&h, "Favorites"),
        "recent" => join(&h, "Recent"),
        _ => String::new(),
    }
}

/// Files of a folder matching a name with `*` and `?`, without regard to
/// case.
fn matches(p: &[char], n: &[char]) -> bool {
    match (p.first(), n.first()) {
        (None, None) => true,
        (Some('*'), _) => matches(&p[1..], n) || (!n.is_empty() && matches(p, &n[1..])),
        (Some('?'), Some(_)) => matches(&p[1..], &n[1..]),
        (Some(a), Some(b)) => a.to_lowercase().eq(b.to_lowercase()) && matches(&p[1..], &n[1..]),
        _ => false,
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

#[cfg(not(target_arch = "wasm32"))]
fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let p = e.path();
        let t = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &t)?;
        } else {
            std::fs::copy(&p, &t)?;
        }
    }
    Ok(())
}

/// One file system operation; see the module notes.
#[cfg(not(target_arch = "wasm32"))]
pub fn fs_op(op: &str, a: &str, b: &str, c: &str) -> Result<String, Fail> {
    use std::path::Path;
    let pa = Path::new(a);
    match op {
        "cwd" => Ok(documents()),
        "env" => Ok(env(a)),
        "special" => Ok(special(a)),
        "exists" => Ok(if pa.is_dir() {
            "d"
        } else if pa.is_file() {
            "f"
        } else {
            ""
        }
        .into()),
        "read" => {
            let bytes = std::fs::read(pa).map_err(|e| io_fail(&e))?;
            Ok(decode_text(&bytes))
        }
        "write" => {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(c == "1")
                .truncate(c != "1")
                .open(pa)
                .map_err(|e| match e.kind() {
                    std::io::ErrorKind::NotFound => 76,
                    _ => io_fail(&e),
                })?;
            f.write_all(&encode_text(b)).map_err(|e| io_fail(&e))?;
            Ok(String::new())
        }
        "size" => Ok(std::fs::metadata(pa)
            .map_err(|e| io_fail(&e))?
            .len()
            .to_string()),
        "mtime" => {
            let m = std::fs::metadata(pa).map_err(|e| io_fail(&e))?;
            let t = m.modified().map_err(|e| io_fail(&e))?;
            Ok(serial(t).to_string())
        }
        "attr" => {
            let m = std::fs::metadata(pa).map_err(|e| io_fail(&e))?;
            let mut v = if m.is_dir() { 16 } else { 32 };
            if m.permissions().readonly() {
                v |= 1;
            }
            Ok(v.to_string())
        }
        "list" => {
            let rd = std::fs::read_dir(pa).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => 76,
                _ => io_fail(&e),
            })?;
            let mut items: Vec<(String, bool, u64)> = rd
                .flatten()
                .map(|e| {
                    let m = e.metadata().ok();
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        m.as_ref().is_some_and(|m| m.is_dir()),
                        m.map(|m| m.len()).unwrap_or(0),
                    )
                })
                .collect();
            items.sort_by_key(|i| i.0.to_lowercase());
            let parts: Vec<String> = items
                .iter()
                .map(|(n, d, s)| format!("{{\"n\":{},\"d\":{d},\"s\":{s}}}", json_str(n)))
                .collect();
            Ok(format!("[{}]", parts.join(",")))
        }
        // Kill: the files matching the name; how many went.
        "kill" => {
            let name = pa
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !name.contains(['*', '?']) {
                if !pa.is_file() {
                    return Err(53);
                }
                std::fs::remove_file(pa).map_err(|e| io_fail(&e))?;
                return Ok("1".into());
            }
            let folder = pa
                .parent()
                .filter(|d| !d.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let pat: Vec<char> = name.chars().collect();
            let mut n = 0;
            for e in std::fs::read_dir(folder).map_err(|_| 76u32)?.flatten() {
                let fname: Vec<char> = e.file_name().to_string_lossy().chars().collect();
                if e.path().is_file() && matches(&pat, &fname) {
                    std::fs::remove_file(e.path()).map_err(|e| io_fail(&e))?;
                    n += 1;
                }
            }
            Ok(n.to_string())
        }
        "mkdir" => {
            if pa.exists() {
                return Err(75);
            }
            std::fs::create_dir(pa).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => 76,
                _ => io_fail(&e),
            })?;
            Ok(String::new())
        }
        // RmDir: an empty folder; with `b` "1" (FileSystemObject) its
        // contents too.
        "rmdir" => {
            if !pa.is_dir() {
                return Err(76);
            }
            if b == "1" {
                std::fs::remove_dir_all(pa)
            } else {
                std::fs::remove_dir(pa)
            }
            .map_err(|e| io_fail(&e))?;
            Ok(String::new())
        }
        // Copy a file (or a folder and what it holds) to `b`; `c` "1"
        // overwrites.
        "copy" => {
            let to = Path::new(b);
            if to.exists() && c != "1" && !pa.is_dir() {
                return Err(58);
            }
            if pa.is_dir() {
                copy_dir(pa, to).map_err(|e| io_fail(&e))?;
            } else {
                if !pa.is_file() {
                    return Err(53);
                }
                std::fs::copy(pa, to).map_err(|e| io_fail(&e))?;
            }
            Ok(String::new())
        }
        "move" => {
            if !pa.exists() {
                return Err(53);
            }
            if Path::new(b).exists() {
                return Err(58);
            }
            if std::fs::rename(pa, b).is_err() {
                // Another drive: copy, then remove.
                if pa.is_dir() {
                    copy_dir(pa, Path::new(b)).map_err(|e| io_fail(&e))?;
                    std::fs::remove_dir_all(pa).map_err(|e| io_fail(&e))?;
                } else {
                    std::fs::copy(pa, b).map_err(|e| io_fail(&e))?;
                    std::fs::remove_file(pa).map_err(|e| io_fail(&e))?;
                }
            }
            Ok(String::new())
        }
        _ => Err(5),
    }
}

/// The browser has no files.
#[cfg(target_arch = "wasm32")]
pub fn fs_op(op: &str, a: &str, _b: &str, _c: &str) -> Result<String, Fail> {
    match op {
        "cwd" | "special" => Ok(String::new()),
        "env" => Ok(env(a)),
        "exists" => Ok(String::new()),
        _ => Err(75),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_reads_in_the_encoding_it_was_written_in() {
        assert_eq!(decode_text("Café".as_bytes()), "Café");
        assert_eq!(decode_text(b"Caf\xe9"), "Café");
        assert_eq!(decode_text(b"\xEF\xBB\xBFok"), "ok");
        assert_eq!(encode_text("Café"), b"Caf\xe9");
        assert_eq!(encode_text("Привет"), "Привет".as_bytes());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn files_are_written_read_listed_and_removed() {
        let dir = std::env::temp_dir().join(format!("td-vba-fs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let d = dir.to_string_lossy().to_string();
        fs_op("mkdir", &d, "", "").unwrap();
        assert_eq!(fs_op("mkdir", &d, "", ""), Err(75));
        let f = join(&d, "a.txt");
        fs_op("write", &f, "one\r\n", "").unwrap();
        fs_op("write", &f, "two\r\n", "1").unwrap();
        assert_eq!(fs_op("read", &f, "", "").unwrap(), "one\r\ntwo\r\n");
        assert_eq!(fs_op("exists", &f, "", "").unwrap(), "f");
        assert_eq!(fs_op("exists", &d, "", "").unwrap(), "d");
        assert_eq!(fs_op("size", &f, "", "").unwrap(), "10");
        fs_op("copy", &f, &join(&d, "b.txt"), "").unwrap();
        assert_eq!(fs_op("copy", &f, &join(&d, "b.txt"), ""), Err(58));
        assert!(fs_op("list", &d, "", "").unwrap().contains("\"n\":\"b.txt\""));
        assert_eq!(fs_op("kill", &join(&d, "*.txt"), "", "").unwrap(), "2");
        assert_eq!(fs_op("read", &f, "", ""), Err(53));
        fs_op("rmdir", &d, "", "").unwrap();
        assert_eq!(fs_op("exists", &d, "", "").unwrap(), "");
    }
}
