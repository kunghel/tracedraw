//! The spreadsheet program a table's cells are edited in (#4144): the one
//! the system opens `.xlsx` workbooks with, found once and named in the
//! menus ("Edit in ..."), or the program chosen under Options > File
//! Locations. See docs/behavior/table.md, "Editing a table in a
//! spreadsheet program".

use std::path::{Path, PathBuf};

/// A spreadsheet program: the name to show and its program file (or
/// application bundle, or desktop entry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadsheetAppInfo {
    pub name: String,
    pub path: PathBuf,
}

/// The media type of an `.xlsx` workbook.
pub const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// The program the system opens `.xlsx` workbooks with, found on first
/// use and kept (asking the system can start a process).
pub fn system_default() -> Option<SpreadsheetAppInfo> {
    static FOUND: std::sync::OnceLock<Option<SpreadsheetAppInfo>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| if cfg!(test) { None } else { query() })
        .clone()
}

/// The name to show for the program `chosen` (Options; empty for the
/// system's): the chosen program's file name without its extension, else
/// the system's program's name; `None` when none is known.
pub fn display_name(chosen: &str) -> Option<String> {
    let chosen = chosen.trim().trim_matches('"');
    if !chosen.is_empty() {
        let p = Path::new(chosen);
        let stem = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())?;
        return Some(stem);
    }
    system_default().map(|a| a.name).filter(|n| !n.trim().is_empty())
}

#[cfg(windows)]
fn query() -> Option<SpreadsheetAppInfo> {
    windows::query()
}

#[cfg(target_os = "macos")]
fn query() -> Option<SpreadsheetAppInfo> {
    macos::query()
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
fn query() -> Option<SpreadsheetAppInfo> {
    let out = std::process::Command::new("xdg-mime")
        .args(["query", "default", XLSX_MIME])
        .output()
        .ok()?;
    let entry = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if entry.is_empty() || !entry.ends_with(".desktop") {
        return None;
    }
    let file = desktop_dirs()
        .into_iter()
        .map(|d| d.join("applications").join(&entry))
        .find(|p| p.is_file())?;
    let text = std::fs::read_to_string(&file).ok()?;
    let (name, exec) = desktop_entry(&text);
    Some(SpreadsheetAppInfo {
        name: name.unwrap_or_else(|| entry.trim_end_matches(".desktop").to_string()),
        path: exec.map(PathBuf::from).unwrap_or(file),
    })
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
fn query() -> Option<SpreadsheetAppInfo> {
    None
}

/// The folders desktop entries are looked for in, the user's first.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(d) => dirs.push(PathBuf::from(d)),
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(PathBuf::from(home).join(".local/share"));
            }
        }
    }
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs
}

/// A desktop entry's `Name` and the program of its `Exec` line (the
/// `[Desktop Entry]` group only; field codes such as `%U` left out).
#[cfg_attr(
    not(all(unix, not(any(target_os = "macos", target_os = "android")))),
    allow(dead_code)
)]
fn desktop_entry(text: &str) -> (Option<String>, Option<String>) {
    let mut in_entry = false;
    let (mut name, mut exec) = (None, None);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(v) = line.strip_prefix("Name=") {
            name.get_or_insert_with(|| v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Exec=") {
            let v = v.trim();
            let program = if let Some(rest) = v.strip_prefix('"') {
                rest.split('"').next().unwrap_or("").to_string()
            } else {
                v.split_whitespace().next().unwrap_or("").to_string()
            };
            if !program.is_empty() {
                exec.get_or_insert(program);
            }
        }
    }
    (name, exec)
}

#[cfg(windows)]
mod windows {
    use super::SpreadsheetAppInfo;
    use std::path::PathBuf;

    const ASSOCSTR_EXECUTABLE: u32 = 2;
    const ASSOCSTR_FRIENDLYAPPNAME: u32 = 4;

    #[cfg_attr(
        target_arch = "x86",
        link(name = "shlwapi", kind = "raw-dylib", import_name_type = "undecorated")
    )]
    #[cfg_attr(not(target_arch = "x86"), link(name = "shlwapi", kind = "raw-dylib"))]
    unsafe extern "system" {
        fn AssocQueryStringW(
            flags: u32,
            what: u32,
            assoc: *const u16,
            extra: *const u16,
            out: *mut u16,
            out_len: *mut u32,
        ) -> i32;
    }

    fn assoc(what: u32) -> Option<String> {
        let ext: Vec<u16> = ".xlsx".encode_utf16().chain(Some(0)).collect();
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `ext` ends with a zero, `buf` holds `len` characters and
        // the call writes at most that many (with the zero), setting `len`.
        let hr = unsafe {
            AssocQueryStringW(
                0,
                what,
                ext.as_ptr(),
                std::ptr::null(),
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        if hr != 0 {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..end]);
        (!s.trim().is_empty()).then_some(s)
    }

    pub fn query() -> Option<SpreadsheetAppInfo> {
        let path = assoc(ASSOCSTR_EXECUTABLE)?;
        let name = assoc(ASSOCSTR_FRIENDLYAPPNAME).unwrap_or_else(|| {
            PathBuf::from(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });
        Some(SpreadsheetAppInfo {
            name,
            path: PathBuf::from(path),
        })
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::SpreadsheetAppInfo;
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::path::PathBuf;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            text: *const c_char,
            encoding: u32,
        ) -> *const c_void;
        fn CFStringGetCString(
            string: *const c_void,
            buffer: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> u8;
        fn CFURLCopyFileSystemPath(url: *const c_void, style: isize) -> *const c_void;
        fn CFRelease(object: *const c_void);
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultApplicationURLForContentType(
            content_type: *const c_void,
            roles: u32,
            error: *mut *const c_void,
        ) -> *const c_void;
    }

    const UTF8: u32 = 0x0800_0100;
    /// kLSRolesAll.
    const ROLES_ALL: u32 = 0xFFFF_FFFF;
    /// kCFURLPOSIXPathStyle.
    const POSIX_PATH: isize = 0;

    pub fn query() -> Option<SpreadsheetAppInfo> {
        let uti = CString::new("org.openxmlformats.spreadsheetml.sheet").ok()?;
        // SAFETY: each object made or copied here is checked for null and
        // released once; the buffer is as long as the size given.
        unsafe {
            let ct = CFStringCreateWithCString(std::ptr::null(), uti.as_ptr(), UTF8);
            if ct.is_null() {
                return None;
            }
            let url =
                LSCopyDefaultApplicationURLForContentType(ct, ROLES_ALL, std::ptr::null_mut());
            CFRelease(ct);
            if url.is_null() {
                return None;
            }
            let path = CFURLCopyFileSystemPath(url, POSIX_PATH);
            CFRelease(url);
            if path.is_null() {
                return None;
            }
            let mut buf = vec![0 as c_char; 4096];
            let ok = CFStringGetCString(path, buf.as_mut_ptr(), buf.len() as isize, UTF8);
            CFRelease(path);
            if ok == 0 {
                return None;
            }
            let text = CStr::from_ptr(buf.as_ptr()).to_string_lossy().to_string();
            let path = PathBuf::from(text);
            let name = path.file_stem()?.to_string_lossy().to_string();
            Some(SpreadsheetAppInfo { name, path })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_entry_gives_its_name_and_program() {
        let text = "[Desktop Entry]\nName=Sheets\nName[pt]=Planilhas\nExec=/opt/sheets/bin/sheets --calc %U\n\n[Desktop Action New]\nName=New\nExec=other\n";
        assert_eq!(
            desktop_entry(text),
            (Some("Sheets".into()), Some("/opt/sheets/bin/sheets".into()))
        );
        let quoted = "[Desktop Entry]\nExec=\"/opt/My Sheets/run\" %F\n";
        assert_eq!(desktop_entry(quoted).1.as_deref(), Some("/opt/My Sheets/run"));
    }

    #[test]
    fn the_chosen_program_is_named_by_its_file() {
        assert_eq!(
            display_name("/usr/bin/sheets").as_deref(),
            Some("sheets")
        );
        assert_eq!(
            display_name("\"C:\\Programs\\Calc\\calc.exe\"").as_deref(),
            if cfg!(windows) { Some("calc") } else { Some("C:\\Programs\\Calc\\calc") }
        );
        // Tests ask no system: nothing to name.
        assert_eq!(display_name(""), None);
    }
}
