//! File access for both builds.
//!
//! Natively these are the file system and the system file dialogs, used
//! asynchronously on desktop. In a browser there is no file system: a file
//! the user picks is kept in memory under a synthetic path
//! (`/upload/<name>`), so the path-based open and import code reads it with
//! [`read`] like any file, and [`write`] offers the bytes as a download
//! named after the path's file name.
//!
//! Dialogs come in three shapes:
//! - [`Dialog::pick_file`] runs a closure with the picked path:
//!   on a later frame on desktop and in a browser (see [`run_finished`]).
//! - [`Dialog::pick_into`] stores the picked path under a key that a
//!   dialog's own code collects with [`take_picked`] on a later frame.
//! - [`Dialog::save_file`] runs a closure with the chosen path on a later
//!   native frame, or at once with `/download/<file name>` in a browser.

use crate::app::App;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type Job = Box<dyn FnOnce(&mut App, PathBuf)>;

thread_local! {
    /// Dialogs finished in the background (browser), waiting for a frame.
    static FINISHED: RefCell<Vec<(Job, PathBuf)>> = const { RefCell::new(Vec::new()) };
    /// Picked paths waiting for the dialog that asked for them.
    static PICKED: RefCell<HashMap<String, PathBuf>> = RefCell::new(HashMap::new());
    /// Uploaded files by synthetic path (browser only).
    static STORE: RefCell<HashMap<PathBuf, Vec<u8>>> = RefCell::new(HashMap::new());
    /// Uploads whose job ran but that the app still reads on a later
    /// frame (an import waiting for its place, a PDF question), freed
    /// once nothing does (#3231).
    static LINGERING: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
type NativeJob = Box<dyn FnOnce(&mut App, Vec<PathBuf>) + 'static>;

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
struct NativeDialogs {
    next_id: u64,
    pending: HashMap<u64, NativeJob>,
    sender: std::sync::mpsc::Sender<(u64, Vec<PathBuf>)>,
    receiver: std::sync::mpsc::Receiver<(u64, Vec<PathBuf>)>,
    context: Option<egui::Context>,
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
impl NativeDialogs {
    fn new() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        Self {
            next_id: 0,
            pending: HashMap::new(),
            sender,
            receiver,
            context: None,
        }
    }

    fn register(&mut self, job: NativeJob) -> Option<u64> {
        let Some(next_id) = self.next_id.checked_add(1) else {
            log::error!("native dialog request IDs exhausted");
            return None;
        };
        let id = self.next_id;
        self.next_id = next_id;
        self.pending.insert(id, job);
        Some(id)
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
thread_local! {
    // Callbacks stay on the UI thread; only IDs and owned paths cross threads.
    static NATIVE_DIALOGS: RefCell<NativeDialogs> = RefCell::new(NativeDialogs::new());
}

/// Resolve a native picker on a worker and defer its callback to the UI frame.
/// An empty result cancels the job without invoking its callback.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn schedule_native_dialog(
    future: impl std::future::Future<Output = Vec<PathBuf>> + Send + 'static,
    job: impl FnOnce(&mut App, Vec<PathBuf>) + 'static,
) {
    let registration = NATIVE_DIALOGS.with(|state| {
        let mut state = state.borrow_mut();
        let id = state.register(Box::new(job))?;
        Some((id, state.sender.clone(), state.context.clone()))
    });
    let Some((id, sender, context)) = registration else {
        return;
    };
    if let Err(error) = std::thread::Builder::new()
        .name("native-file-dialog".into())
        .spawn(move || {
            let paths = pollster::block_on(future);
            if sender.send((id, paths)).is_ok() {
                if let Some(context) = context {
                    context.request_repaint();
                }
            }
        })
    {
        NATIVE_DIALOGS.with(|state| {
            state.borrow_mut().pending.remove(&id);
        });
        log::error!("cannot start native file dialog worker: {error}");
    }
}

/// True in the browser build: no file system, downloads instead of saves.
pub const WEB: bool = cfg!(target_arch = "wasm32");

/// The browser's auto-backup folder: the session markers live in the
/// page's local storage and the backups in its IndexedDB database
/// (`web::backups`), and [`read`], [`write_creating`],
/// [`write_creating_with`], [`exists`], [`remove`] and [`list_dir`] reach
/// them under this path as if they were files.
pub const WEB_BACKUP_DIR: &str = "/backup";

/// True for a path in [`WEB_BACKUP_DIR`] in the browser build.
fn in_web_backups(path: &Path) -> bool {
    WEB && path.starts_with(WEB_BACKUP_DIR)
}

/// True in the Android app: files come and go through the system's
/// document picker, named by [`document_path`] (`android.rs`,
/// docs/behavior/android.md).
pub const ANDROID: bool = cfg!(target_os = "android");

/// The folder under which a document the Android picker returned is
/// named: `/content/<its URI, escaped>/<its display name>`. The name
/// gives the title and the type (its extension), the URI the bytes.
pub const DOCUMENT_ROOT: &str = "/content";

/// The path naming the document at the content URI `uri` shown as
/// `name`: `/content/<uri escaped>/<name>`, so that `file_name` and
/// `extension` work as for any file and [`read`] and [`write`] find the
/// URI again with [`document_uri`].
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn document_path(uri: &str, name: &str) -> PathBuf {
    let mut escaped = String::with_capacity(uri.len());
    for c in uri.chars() {
        match c {
            '%' => escaped.push_str("%25"),
            '/' => escaped.push_str("%2F"),
            c => escaped.push(c),
        }
    }
    let name = name.replace(['/', '\\'], "_");
    let name = if name.trim().is_empty() {
        "document".to_string()
    } else {
        name
    };
    PathBuf::from(DOCUMENT_ROOT).join(escaped).join(name)
}

/// The content URI of a path made by [`document_path`]; `None` for any
/// other path.
pub fn document_uri(path: &Path) -> Option<String> {
    let rest = path
        .to_str()?
        .strip_prefix(DOCUMENT_ROOT)?
        .strip_prefix('/')?;
    let (escaped, _name) = rest.split_once('/')?;
    let mut uri = String::with_capacity(escaped.len());
    let mut chars = escaped.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let code: String = chars.by_ref().take(2).collect();
            match code.as_str() {
                "25" => uri.push('%'),
                "2F" => uri.push('/'),
                other => {
                    uri.push('%');
                    uri.push_str(other);
                }
            }
        } else {
            uri.push(c);
        }
    }
    Some(uri)
}

/// True for a path made by [`document_path`].
pub fn is_document_path(path: &Path) -> bool {
    document_uri(path).is_some()
}

/// A picked document's name with an extension: its own, else one for the
/// MIME type its provider gave (a photo called "IMG_1" of type image/jpeg
/// becomes "IMG_1.jpg"), so the editor can tell the type.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn name_with_extension(name: &str, mime: &str) -> String {
    if Path::new(name).extension().is_some() {
        return name.to_string();
    }
    let ext = match mime.trim().to_ascii_lowercase().as_str() {
        "application/pdf" => "pdf",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/webp" => "webp",
        "image/jxl" => "jxl",
        "image/tiff" => "tiff",
        "image/svg+xml" => "svg",
        "text/plain" => "txt",
        "text/csv" => "csv",
        "application/json" => "json",
        "application/postscript" => "eps",
        _ => return name.to_string(),
    };
    format!("{name}.{ext}")
}

/// `name` as one file name of its own inside a folder: no folder
/// separators or control characters, no leading dots (nothing hidden, no
/// `..`), never empty.
pub fn safe_file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|&c| !is_direction_mark(c))
        .map(|c| {
            if matches!(c, '/' | '\\') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_start_matches('.').trim();
    if clean.is_empty() {
        "file".into()
    } else {
        clean.to_string()
    }
}

/// True for the invisible marks that steer the direction of text
/// (left-to-right and right-to-left marks, embeddings, overrides and
/// isolates). Right-to-left keyboards and text copied from right-to-left
/// writing bring them along; in a file name they only hide the extension
/// and make two names that look the same differ.
pub fn is_direction_mark(c: char) -> bool {
    // By code point: they are never drawn, so they are not symbols the
    // interface's fonts must have (`every_interface_symbol_is_in_the_bundled_fonts`).
    matches!(
        c as u32,
        0x061C | 0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069
    )
}

/// A file name typed or made from a drawing's name, ready to be written
/// in any language: direction marks and control characters go, folder
/// separators become `_`, and so do the characters Windows refuses in a
/// name (`: * ? " < > |`) on Windows, where it also drops the dots and
/// spaces at the end that it would drop itself. Letters of every script
/// and their accents and vowel marks stay as they are. Empty when
/// nothing is left.
pub fn clean_file_name(name: &str) -> String {
    clean_file_name_for(name, cfg!(windows))
}

/// [`clean_file_name`] for Windows (`windows`) or another system.
pub fn clean_file_name_for(name: &str, windows: bool) -> String {
    let clean: String = name
        .chars()
        .filter(|&c| !is_direction_mark(c) && !c.is_control())
        .map(|c| {
            let reserved = windows && matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|');
            if matches!(c, '/' | '\\') || reserved {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim();
    if windows {
        clean.trim_end_matches(['.', ' ']).to_string()
    } else {
        clean.to_string()
    }
}

/// The MIME type the system's save picker is told for a file with the
/// extension `ext` (any case); a plain byte stream for the formats it
/// does not know, which keeps the name as typed.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn mime_for_extension(ext: &str) -> &'static str {
    match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "jxl" => "image/jxl",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "zip" => "application/zip",
        "eps" | "ps" => "application/postscript",
        _ => "application/octet-stream",
    }
}

/// The folder the Save Drawing dialog starts in when the platform fixes
/// it: the Android app's documents folder; `None` elsewhere (the last
/// folder used, Documents or home).
pub fn save_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        let dir = android_documents_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("cannot create {}: {e}", dir.display());
        }
        return Some(dir);
    }
    #[allow(unreachable_code)]
    None
}

/// Where the Android app saves drawings and exports: its own folder under
/// the shared storage (`Android/data/<package>/files`, which file managers
/// show), set by `android_main`; the settings folder when it is missing.
#[cfg(target_os = "android")]
fn android_documents_dir() -> PathBuf {
    std::env::var_os("TRACEDRAW_DOCUMENTS_DIR")
        .map(PathBuf::from)
        .or_else(crate::settings::config_dir)
        .unwrap_or_else(|| PathBuf::from("/sdcard/Download"))
}

/// The path naming the same file however it was reached (links, `.`
/// and `..`); the absolute path when that fails (the file is gone);
/// `path` as it is in a browser.
pub fn canonical(path: &Path) -> PathBuf {
    if WEB || is_document_path(path) {
        return path.to_path_buf();
    }
    std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Two canonical paths name one file: on Windows without regard to the
/// letter case and to the `\\?\` prefix.
pub fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        let key = |p: &Path| {
            let s = p.to_string_lossy().to_lowercase();
            s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
        };
        key(a) == key(b)
    } else {
        a == b
    }
}

/// A canonical path as people type it: on Windows without the `\\?\`
/// prefix (`\\?\C:\a` is `C:\a`, `\\?\UNC\server\share` is
/// `\\server\share`); other paths as they are.
pub fn plain_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Read a whole file (or an uploaded one in a browser).
pub fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    if in_web_backups(path) {
        return crate::web::backups::read(path)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound));
    }
    if WEB {
        return STORE
            .with(|s| s.borrow().get(path).cloned())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{} is not available in the browser", path.display()),
                )
            });
    }
    #[cfg(target_os = "android")]
    if let Some(uri) = document_uri(path) {
        return crate::android::documents::read(&uri);
    }
    std::fs::read(path)
}

/// Read a whole file as UTF-8 text.
pub fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
    let bytes = read(path)?;
    String::from_utf8(bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

/// Write a whole file; in a browser, download it under the path's name
/// (or keep it in memory, under [`MEMORY_ROOT`]).
pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = path.as_ref();
    if in_memory(path) {
        store(path, bytes.as_ref().to_vec());
        return Ok(());
    }
    if WEB {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "download".into());
        return download(&name, bytes.as_ref());
    }
    #[cfg(target_os = "android")]
    if let Some(uri) = document_uri(path) {
        return crate::android::documents::write(&uri, bytes.as_ref());
    }
    replace_with(path, |w| std::io::Write::write_all(w, bytes.as_ref()))
}

/// Write a file as `fill` makes it, without holding the whole file in
/// memory; in a browser, the bytes are gathered and downloaded.
pub fn write_with(
    path: impl AsRef<Path>,
    fill: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let path = path.as_ref();
    // A download, or a document the Android picker returned: gathered,
    // then handed over whole.
    if WEB || is_document_path(path) {
        let mut bytes = Vec::new();
        fill(&mut bytes)?;
        return write(path, bytes);
    }
    replace_with(path, fill)
}

/// Write the file `path` as `fill` makes it, never losing the file that
/// was there when the write fails half way (a full disk, an error while
/// the drawing is written, the program stopped): the new file is made
/// whole under a `.part` name beside it first, and only then takes the
/// old one's place. A file that was there is written over in place from
/// the `.part` file, so it keeps its permissions and links and an open
/// drawing's hold on it (`file_hold`) stays; should that copy fail, the
/// `.part` file stays with the whole new file and the error names it. A
/// new file is the `.part` file renamed. Where no `.part` file can be
/// made (a folder that takes no new files, a device), the file is
/// written directly, as before.
fn replace_with(
    path: &Path,
    fill: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) -> std::io::Result<()> {
    use std::io::Write as _;
    let direct = |fill: &mut dyn FnMut(&mut dyn std::io::Write) -> std::io::Result<()>| {
        let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
        fill(&mut w)?;
        w.flush()
    };
    let mut fill = Some(fill);
    let mut once = |w: &mut dyn std::io::Write| match fill.take() {
        Some(f) => f(w),
        None => Ok(()),
    };
    let existing = std::fs::metadata(path).ok();
    if existing.as_ref().is_some_and(|m| !m.is_file()) {
        return direct(&mut once);
    }
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let Ok(file) = std::fs::File::create(&part) else {
        return direct(&mut once);
    };
    let written = (|| {
        let mut w = std::io::BufWriter::new(file);
        once(&mut w)?;
        w.flush()?;
        w.get_ref().sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    if existing.is_none() {
        return match std::fs::rename(&part, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                Err(e)
            }
        };
    }
    let copied = std::fs::File::open(&part).and_then(|mut from| {
        let mut to = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)?;
        std::io::copy(&mut from, &mut to)?;
        to.flush()?;
        to.sync_all()
    });
    match copied {
        Ok(()) => {
            let _ = std::fs::remove_file(&part);
            Ok(())
        }
        Err(e) => Err(std::io::Error::new(
            e.kind(),
            format!("{e}; the whole file is in {}", part.display()),
        )),
    }
}

/// Write a file as `fill` makes it, creating its folder first
/// (auto-backups). It is written under a `.part` name and renamed when
/// complete, so a write cut short never leaves half a file under the
/// real name. In a browser only the auto-backup folder
/// ([`WEB_BACKUP_DIR`]) is written.
pub fn write_creating_with(
    path: impl AsRef<Path>,
    fill: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let path = path.as_ref();
    if in_web_backups(path) {
        let mut bytes = Vec::new();
        fill(&mut bytes)?;
        return write_creating(path, bytes);
    }
    if WEB {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let written = std::fs::File::create(&part).and_then(|f| {
        let mut w = std::io::BufWriter::new(f);
        fill(&mut w)?;
        std::io::Write::flush(&mut w)
    });
    match written.and_then(|()| std::fs::rename(&part, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

/// Copy a file, creating the target folder (backups). Nothing happens in
/// a browser, which has no files to copy.
pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
    if WEB {
        return Ok(());
    }
    if let Some(dir) = to.as_ref().parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(from, to).map(|_| ())
}

/// True when a file exists on disk (never in a browser). A document the
/// Android picker returned counts as existing: the system holds it and
/// asked about replacing it already.
pub fn exists(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    if in_web_backups(path) {
        return crate::web::backups::exists(path);
    }
    if WEB {
        return false;
    }
    (ANDROID && is_document_path(path)) || path.is_file()
}

/// Write a file, creating its folder first (auto-backups). In a browser
/// only the auto-backup folder ([`WEB_BACKUP_DIR`]) is written.
pub fn write_creating(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    if in_web_backups(path.as_ref()) {
        return crate::web::backups::write(path.as_ref(), bytes.as_ref());
    }
    if WEB {
        return Ok(());
    }
    if let Some(dir) = path.as_ref().parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes)
}

/// Write `bytes` to a device such as a serial port (`/dev/ttyUSB0`,
/// `COM3`) without creating or truncating anything: the device must
/// exist. A bare Windows port name (`COM10`) is opened through the
/// device namespace (`\\.\COM10`), which every port number needs past
/// 9. Not in a browser or on Android, which have no such devices.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn write_device(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let device = device_path(path);
    let mut f = std::fs::OpenOptions::new().write(true).open(device)?;
    f.write_all(bytes)?;
    f.flush()
}

/// The path a port name opens: a bare Windows port name (`COM10`)
/// through the device namespace (`\\.\COM10`).
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn device_path(path: &str) -> String {
    let path = path.trim();
    let upper = path.to_ascii_uppercase();
    if cfg!(windows)
        && upper.starts_with("COM")
        && upper.len() > 3
        && upper[3..].bytes().all(|b| b.is_ascii_digit())
    {
        format!("\\\\.\\{path}")
    } else {
        path.to_string()
    }
}

/// Open a device (a serial or USB-serial port) to write and read, the
/// way [`write_device`] finds it; nothing is created.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn open_device(path: &str) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(device_path(path))
}

/// Delete a file (in a browser, only in the auto-backup folder and the
/// folders kept in memory).
pub fn remove(path: impl AsRef<Path>) -> std::io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    if in_web_backups(path.as_ref()) {
        crate::web::backups::remove(path.as_ref());
        return Ok(());
    }
    if in_memory(path.as_ref()) {
        forget(path.as_ref());
    }
    if WEB {
        return Ok(());
    }
    std::fs::remove_file(path)
}

/// The kind of system the app runs on, where what a dialog can do
/// depends on it (#1399).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Web,
    Android,
}

impl Platform {
    /// The platform of this build.
    pub const fn current() -> Platform {
        if WEB {
            Platform::Web
        } else if ANDROID {
            Platform::Android
        } else {
            Platform::Desktop
        }
    }
}

/// The places where the desktop asks for a folder (#1399).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderUse {
    /// The Export docker's folder: the assets are written into it.
    ExportAssets,
    /// File > Package: the drawing, its pictures and fonts.
    Package,
    /// Font Manager > Add Folder: its fonts are read.
    FontFolder,
    /// Print Merge's pictures folder: the pictures cells name are read.
    MergePictures,
    /// Picture Mosaic's library: its pictures are read.
    MosaicLibrary,
    /// Options > Text > Spelling, More dictionaries: Hunspell files read.
    DictionaryFolder,
    /// Options > Save, the backup made before saving.
    BackupFolder,
    /// Options > Save, the auto-backups.
    AutoBackupFolder,
    /// The Save Drawing dialog's Browse, the system's folder window
    /// with the places only it knows (#3416).
    SaveDrawing,
    /// The bitmap pattern picker's folder of pictures (#3323).
    PatternFolder,
}

impl FolderUse {
    #[cfg(test)]
    pub const ALL: [FolderUse; 10] = [
        FolderUse::ExportAssets,
        FolderUse::Package,
        FolderUse::FontFolder,
        FolderUse::MergePictures,
        FolderUse::MosaicLibrary,
        FolderUse::DictionaryFolder,
        FolderUse::BackupFolder,
        FolderUse::AutoBackupFolder,
        FolderUse::SaveDrawing,
        FolderUse::PatternFolder,
    ];

    /// What the folder button does in this build.
    pub fn action(self) -> FolderAction {
        folder_action(self, Platform::current())
    }

    /// The tip that says why the button does what it does in this build
    /// (an i18n key), `None` on the desktop.
    pub fn tip(self) -> Option<&'static str> {
        folder_tip(self, Platform::current())
    }
}

/// What a folder button does (#1399).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderAction {
    /// The system's folder dialog ([`pick_folder_into`]), on the desktop.
    PickFolder(FolderToken),
    /// The files are written into one zip file, downloaded in a browser
    /// and saved where the system's save picker says on Android.
    SaveZip,
    /// The files a folder would hold are chosen with the open picker,
    /// several at once.
    OpenFiles,
    /// No folder can be chosen: the button is greyed, with this tip (an
    /// i18n key) saying why.
    Unavailable(&'static str),
}

/// Leave to show the system's folder dialog: only [`folder_action`] makes
/// one, and only for the desktop, so no dialog asks for a folder in a
/// browser or on Android, where none can be picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderToken(());

/// What the button of `place` does on `platform`. The desktop picks a
/// folder; elsewhere a folder that is only written into becomes one zip
/// file, a folder that is read becomes several picked files, and the
/// backup folders, which the app writes on its own later, cannot be
/// chosen.
pub fn folder_action(place: FolderUse, platform: Platform) -> FolderAction {
    use FolderAction::*;
    use FolderUse::*;
    match (platform, place) {
        (Platform::Desktop, _) => PickFolder(FolderToken(())),
        // File > Package is greyed in a browser (#1416).
        (Platform::Web, Package) => Unavailable("package.desktop_only"),
        (_, ExportAssets | Package) => SaveZip,
        (_, FontFolder | MergePictures | MosaicLibrary | DictionaryFolder) => OpenFiles,
        (Platform::Web, BackupFolder | AutoBackupFolder) => Unavailable("folder.backup_web"),
        (Platform::Android, BackupFolder | AutoBackupFolder) => {
            Unavailable("folder.backup_android")
        }
        (_, SaveDrawing) => Unavailable("folder.save_desktop"),
        // Import picture... reads one file there (#3323).
        (_, PatternFolder) => Unavailable("folder.pattern_desktop"),
    }
}

/// The tip of the button of `place` on `platform` (an i18n key): why it
/// writes a zip, opens files or is greyed; `None` where it picks a folder.
pub fn folder_tip(place: FolderUse, platform: Platform) -> Option<&'static str> {
    let web = platform == Platform::Web;
    match folder_action(place, platform) {
        FolderAction::PickFolder(_) => None,
        FolderAction::SaveZip if web => Some("folder.zip_web"),
        FolderAction::SaveZip => Some("folder.zip_android"),
        FolderAction::OpenFiles if web => Some("folder.files_web"),
        // The pictures of Print Merge: the system's folder picker (#1926).
        FolderAction::OpenFiles if place == FolderUse::MergePictures => Some("folder.tree_android"),
        FolderAction::OpenFiles => Some("folder.files_android"),
        FolderAction::Unavailable(key) => Some(key),
    }
}

/// Choose a folder with the system's dialog. The token comes from
/// [`folder_action`], which gives one on the desktop only; a picked folder is
/// kept under `key` for the next UI frame, and Cancel leaves it untouched.
pub fn pick_folder_into(token: FolderToken, start: Option<&Path>, key: &str) {
    let key = key.to_string();
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    schedule_native_dialog(native_folder_pick(token, start), move |_, paths| {
        if let Some(path) = paths.into_iter().next() {
            PICKED.with(|p| p.borrow_mut().insert(key, path));
        }
    });
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    let _ = (token, start, key);
}

/// Choose a folder and resume an action on the UI thread after a selection.
pub fn pick_folder_with(
    token: FolderToken,
    start: Option<&Path>,
    then: impl FnOnce(&mut App, PathBuf) + 'static,
) {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    schedule_native_dialog(native_folder_pick(token, start), move |app, paths| {
        if let Some(path) = paths.into_iter().next() {
            then(app, path);
        }
    });
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    let _ = (token, start, then);
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn native_folder_dialog(token: FolderToken, start: Option<&Path>) -> rfd::AsyncFileDialog {
    let _ = token;
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(start) = start.filter(|s| s.is_dir()) {
        dialog = dialog.set_directory(start);
    }
    dialog
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn native_folder_pick(
    token: FolderToken,
    start: Option<&Path>,
) -> impl std::future::Future<Output = Vec<PathBuf>> + Send + 'static {
    #[cfg(test)]
    {
        let _ = native_folder_dialog(token, start);
        std::future::ready(test_pick::ask().into_iter().take(1).collect())
    }
    #[cfg(not(test))]
    {
        let dialog = native_folder_dialog(token, start);
        async move {
            dialog
                .pick_folder()
                .await
                .into_iter()
                .map(|handle| handle.path().to_path_buf())
                .collect()
        }
    }
}

/// Where the browser keeps the files of [`gather`] and [`zip_written`]
/// in memory, as if they were in folders.
const MEMORY_ROOT: &str = "/memory";

/// True for a path the browser keeps in memory ([`MEMORY_ROOT`]).
fn in_memory(path: &Path) -> bool {
    WEB && path.starts_with(MEMORY_ROOT)
}

/// The folder for the app's own work called `name`: under
/// [`MEMORY_ROOT`] in a browser, in the temporary folder elsewhere.
fn work_dir(name: &str) -> PathBuf {
    if WEB {
        return PathBuf::from(MEMORY_ROOT).join(name);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::temp_dir()
            .join(format!("tracedraw-work-{}", std::process::id()))
            .join(name)
    }
    #[cfg(target_arch = "wasm32")]
    {
        PathBuf::from(MEMORY_ROOT).join(name)
    }
}

/// Empty `dir` (made when missing; in a browser, its files forgotten).
fn clear_dir(dir: &Path) -> std::io::Result<()> {
    if WEB {
        STORE.with(|s| s.borrow_mut().retain(|p, _| !p.starts_with(dir)));
        return Ok(());
    }
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)
}

/// Picked files put together in a folder of their own called `key`, for
/// code that reads a folder (Print Merge's pictures, the Picture Mosaic
/// library) where no folder can be picked (#1399): copies in the
/// temporary folder (Android), kept in memory in a browser. The files
/// the folder held before are gone. The folder.
pub fn gather(key: &str, paths: &[PathBuf]) -> std::io::Result<PathBuf> {
    let dir = work_dir(&format!("picked-{}", safe_file_name(key)));
    clear_dir(&dir)?;
    for path in paths {
        let name = path
            .file_name()
            .map(|n| safe_file_name(&n.to_string_lossy()))
            .unwrap_or_else(|| "file".into());
        let bytes = read(path)?;
        write(dir.join(name), bytes)?;
    }
    Ok(dir)
}

/// True for a folder [`gather`] made.
pub fn is_gathered(dir: &Path) -> bool {
    dir.parent() == Some(work_dir("").as_path())
        && dir
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("picked-"))
}

/// The files `fill` writes (with [`write`]) into the folder it is given,
/// as one zip file with each file under `top/`, in name order (#1399):
/// where no folder can be picked, the files go where the user says as
/// one file. The folder is gone afterwards.
pub fn zip_written(top: &str, fill: impl FnOnce(&Path)) -> Result<Vec<u8>, String> {
    static WORK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = WORK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let work = work_dir(&format!("zip-{n}"));
    let top = safe_file_name(top);
    let dir = work.join(&top);
    clear_dir(&dir).map_err(|e| e.to_string())?;
    fill(&dir);
    let mut files = list_dir(&dir);
    files.sort();
    let entries: Result<Vec<(String, Vec<u8>)>, String> = files
        .iter()
        .map(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            read(p)
                .map(|b| (format!("{top}/{name}"), b))
                .map_err(|e| e.to_string())
        })
        .collect();
    let _ = clear_dir(&work);
    if !WEB {
        let _ = std::fs::remove_dir_all(&work);
    }
    zip_entries(&entries?)
}

/// Named files as one zip file (deflated).
pub fn zip_entries(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use std::io::Write as _;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        zip.start_file(name.as_str(), options)
            .map_err(|e| e.to_string())?;
        zip.write_all(data).map_err(|e| e.to_string())?;
    }
    zip.finish()
        .map(|c| c.into_inner())
        .map_err(|e| e.to_string())
}

/// The files directly inside a folder (not its subfolders), in no
/// particular order; empty when it cannot be read, and in a browser
/// outside the auto-backup folder and the folders kept in memory.
pub fn list_dir(path: impl AsRef<Path>) -> Vec<PathBuf> {
    #[cfg(target_arch = "wasm32")]
    if in_web_backups(path.as_ref()) {
        return crate::web::backups::list(path.as_ref());
    }
    if in_memory(path.as_ref()) {
        let dir = path.as_ref();
        return STORE.with(|s| {
            s.borrow()
                .keys()
                .filter(|p| p.parent() == Some(dir))
                .cloned()
                .collect()
        });
    }
    if WEB {
        return Vec::new();
    }
    match std::fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// One entry of a folder listing (the Save Drawing dialog).
#[derive(Debug, Clone, PartialEq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Bytes (files).
    pub size: u64,
    /// Seconds since 1970, when known.
    pub modified: Option<i64>,
}

/// The folders and files directly inside `dir`: folders first, then by
/// name ignoring case; hidden ones (a leading dot) left out. Empty in a
/// browser or when the folder cannot be read.
pub fn list_entries(dir: impl AsRef<Path>) -> Vec<DirEntry> {
    if WEB {
        return Vec::new();
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<DirEntry> = read
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let path = e.path();
            // Follow links: a link to a folder lists as a folder.
            let meta = std::fs::metadata(&path).ok();
            let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
            let size = meta.as_ref().map_or(0, |m| m.len());
            let modified = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            Some(DirEntry {
                name,
                path,
                is_dir,
                size,
                modified,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

/// True for a folder on disk (never in a browser).
pub fn is_dir(path: impl AsRef<Path>) -> bool {
    !WEB && path.as_ref().is_dir()
}

/// Make a folder (nothing in a browser).
pub fn create_dir(path: impl AsRef<Path>) -> std::io::Result<()> {
    if WEB {
        return Ok(());
    }
    std::fs::create_dir(path)
}

/// Make a folder and the folders it is in (nothing in a browser).
pub fn create_dir_all(path: impl AsRef<Path>) -> std::io::Result<()> {
    if WEB {
        return Ok(());
    }
    std::fs::create_dir_all(path)
}

/// Rename a file or folder; one already called `to` is never replaced
/// (nothing in a browser).
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
    if WEB {
        return Ok(());
    }
    if to.as_ref().exists() {
        return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to)
}

/// A folder worth a shortcut in the Save Drawing dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Home,
    Desktop,
    Documents,
    Downloads,
    Pictures,
    /// A drive or the file system's root.
    Root,
    /// A mounted drive, memory card, network share or other volume
    /// (#1391): named after its folder.
    Drive,
    /// A cloud storage folder (OneDrive, Dropbox, Google Drive, iCloud
    /// Drive), named after its folder: the system's own file windows
    /// list them, the Save Drawing dialog did not (#3416).
    Cloud,
}

/// The user's home folder.
pub fn home_dir() -> Option<PathBuf> {
    if WEB {
        return None;
    }
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// The shortcuts that exist on this computer: home, desktop, documents,
/// downloads and pictures (the names the system gives them, read from
/// the user's folder settings on Linux), then the drives (Windows) or
/// the root, then the mounted drives, memory cards and network shares
/// ([`mounted_drives`]). Empty in a browser.
pub fn places() -> Vec<(Place, PathBuf)> {
    let mut out = Vec::new();
    let Some(home) = home_dir() else {
        return out;
    };
    out.push((Place::Home, home.clone()));
    let xdg = xdg_user_dirs(&home);
    for (place, key, default) in [
        (Place::Desktop, "XDG_DESKTOP_DIR", "Desktop"),
        (Place::Documents, "XDG_DOCUMENTS_DIR", "Documents"),
        (Place::Downloads, "XDG_DOWNLOAD_DIR", "Downloads"),
        (Place::Pictures, "XDG_PICTURES_DIR", "Pictures"),
    ] {
        let dir = xdg
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| home.join(default));
        if dir.is_dir() && dir != home {
            out.push((place, dir));
        }
    }
    for dir in cloud_dirs(&home, &|k| std::env::var_os(k)) {
        if dir.is_dir() && !out.iter().any(|(_, p)| *p == dir) {
            out.push((Place::Cloud, dir));
        }
    }
    if cfg!(windows) {
        for letter in b'C'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", letter as char));
            if root.is_dir() {
                out.push((Place::Root, root));
            }
        }
    } else {
        out.push((Place::Root, PathBuf::from("/")));
    }
    for drive in mounted_drives() {
        if !out.iter().any(|(_, p)| *p == drive) {
            out.push((Place::Drive, drive));
        }
    }
    out
}

/// The cloud storage folders the desktop programs of OneDrive, Dropbox,
/// Google Drive and iCloud make in the home folder `home` (or name in
/// the variables `var` reads), whether they exist or not (#3416).
pub fn cloud_dirs(
    home: &Path,
    var: &dyn Fn(&str) -> Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for key in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(v) = var(key).filter(|v| !v.is_empty()) {
            out.push(PathBuf::from(v));
        }
    }
    for name in ["OneDrive", "Dropbox", "Google Drive", "iCloudDrive"] {
        out.push(home.join(name));
    }
    let mut seen = Vec::new();
    out.retain(|p| {
        let fresh = !seen.contains(p);
        seen.push(p.clone());
        fresh
    });
    out
}

/// Where this computer puts mounted drives (#1391).
#[derive(Debug, Clone, Default)]
pub struct MountRoots {
    /// The system's table of mounts (`/proc/self/mounts` on Linux).
    pub mounts: Option<String>,
    /// Folders whose subfolders are mounted drives: `/media/$USER`,
    /// `/run/media/$USER`, `/media` and `/mnt` on Linux, `/Volumes` on
    /// macOS.
    pub parents: Vec<PathBuf>,
    /// The folder of the desktop's network shares and phones
    /// (`$XDG_RUNTIME_DIR/gvfs`).
    pub gvfs: Option<PathBuf>,
}

impl MountRoots {
    /// This computer's.
    pub fn system() -> Self {
        let mut roots = MountRoots::default();
        if WEB || ANDROID || cfg!(windows) {
            return roots;
        }
        if cfg!(target_os = "macos") {
            roots.parents.push(PathBuf::from("/Volumes"));
            return roots;
        }
        roots.mounts = std::fs::read_to_string("/proc/self/mounts").ok();
        let user = std::env::var("USER")
            .ok()
            .filter(|u| !u.is_empty())
            .or_else(|| {
                home_dir().and_then(|h| h.file_name().map(|n| n.to_string_lossy().to_string()))
            });
        if let Some(user) = user {
            roots.parents.push(Path::new("/media").join(&user));
            roots.parents.push(Path::new("/run/media").join(&user));
        }
        roots.parents.push(PathBuf::from("/media"));
        roots.parents.push(PathBuf::from("/mnt"));
        roots.gvfs = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join("gvfs"));
        roots
    }
}

/// The mounted drives, memory cards and network shares of this computer,
/// for the Save Drawing dialog (#1391). Empty on Windows (its drive
/// letters are listed as roots), on Android and in a browser.
pub fn mounted_drives() -> Vec<PathBuf> {
    drives_in(&MountRoots::system())
}

/// The drives `roots` holds: the folders mounted under one of its
/// parents (from the mount table, and the parents' subfolders, so a
/// drive shows when the table cannot be read), then the network shares
/// under `gvfs`. Each once, in that order, sorted by name within each;
/// the parents themselves, the boot volume (a link to `/` in
/// `/Volumes`), hidden folders and folders that are not there are left
/// out.
pub fn drives_in(roots: &MountRoots) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mounted = roots
        .mounts
        .as_deref()
        .map(parse_mount_points)
        .unwrap_or_default();
    // Folders that are only parents of drives (/media holding $USER).
    let is_parent = |p: &Path| roots.parents.iter().any(|q| q == p);
    for parent in &roots.parents {
        let mut here: Vec<PathBuf> = mounted
            .iter()
            .filter(|m| m.parent() == Some(parent.as_path()))
            .cloned()
            .collect();
        for e in list_entries(parent) {
            if e.is_dir {
                here.push(e.path);
            }
        }
        here.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        for p in here {
            let hidden = p
                .file_name()
                .is_none_or(|n| n.to_string_lossy().starts_with('.'));
            let boot = std::fs::read_link(&p).is_ok_and(|t| t == Path::new("/"));
            if hidden || boot || is_parent(&p) || !p.is_dir() || found.contains(&p) {
                continue;
            }
            found.push(p);
        }
    }
    if let Some(gvfs) = &roots.gvfs {
        for e in list_entries(gvfs) {
            if e.is_dir && !found.contains(&e.path) {
                found.push(e.path);
            }
        }
    }
    found
}

/// The mount points of a mount table (`/proc/self/mounts`: device,
/// mount point, type, ... per line; a space in a path is written `\040`).
pub fn parse_mount_points(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(|p| PathBuf::from(unescape_mount(p)))
        .collect()
}

/// `\040` (space), `\011` (tab), `\012` (new line) and `\134`
/// (backslash) of a mount table.
fn unescape_mount(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b))
        {
            let n = bytes[i + 1..i + 4]
                .iter()
                .fold(0u32, |n, b| n * 8 + u32::from(b - b'0'));
            out.push(n as u8);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A drive's name in the Save Drawing dialog: its folder's name; for a
/// network share of the desktop (`smb-share:server=nas,share=photos`)
/// "photos (nas)", or the server alone.
pub fn drive_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let Some((_, fields)) = name.split_once(':') else {
        return name;
    };
    let field = |key: &str| {
        fields
            .split(',')
            .find_map(|f| f.strip_prefix(key)?.strip_prefix('='))
            .map(str::to_string)
    };
    match (field("share"), field("server").or_else(|| field("host"))) {
        (Some(share), Some(server)) => format!("{share} ({server})"),
        (None, Some(server)) => server,
        _ => name,
    }
}

/// The folders bookmarked in the desktop's file manager
/// (`~/.config/gtk-3.0/bookmarks`, else `~/.gtk-bookmarks`), with their
/// names, that exist. Empty on Windows, macOS, Android and in a browser.
pub fn desktop_bookmarks() -> Vec<(String, PathBuf)> {
    if WEB || ANDROID || cfg!(windows) || cfg!(target_os = "macos") {
        return Vec::new();
    }
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    [
        config.join("gtk-3.0/bookmarks"),
        home.join(".gtk-bookmarks"),
    ]
    .iter()
    .find_map(|f| std::fs::read_to_string(f).ok())
    .map(|text| parse_bookmarks(&text))
    .unwrap_or_default()
    .into_iter()
    .filter(|(_, p)| p.is_dir())
    .collect()
}

/// The `file://` lines of a bookmarks file (`file:///home/me/My%20Work
/// Work`: the address, then an optional name), with the name given or
/// the folder's own; other addresses (`sftp://`, `smb://`) are left out,
/// as they are no folders here.
pub fn parse_bookmarks(text: &str) -> Vec<(String, PathBuf)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (uri, label) = match line.split_once(' ') {
                Some((u, l)) => (u, l.trim()),
                None => (line, ""),
            };
            let path = PathBuf::from(percent_decode(uri.strip_prefix("file://")?));
            if !path.is_absolute() {
                return None;
            }
            let name = if label.is_empty() {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            } else {
                label.to_string()
            };
            Some((name, path))
        })
        .collect()
}

/// `%20` and the other `%XX` escapes of an address, as bytes (UTF-8).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The folders used lately, newest first, for the Save Drawing dialog
/// (#1391): the last folder a dialog used, then the folders of the
/// recent drawings; each once, at most `max`, only those that exist.
pub fn recent_dirs(last_dir: Option<&Path>, recent_files: &[PathBuf], max: usize) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let dirs = last_dir.map(Path::to_path_buf).into_iter().chain(
        recent_files
            .iter()
            .filter_map(|f| f.parent().map(Path::to_path_buf)),
    );
    for d in dirs {
        if out.len() >= max {
            break;
        }
        if !d.as_os_str().is_empty() && !out.contains(&d) && is_dir(&d) {
            out.push(d);
        }
    }
    out
}

/// The `XDG_*_DIR` folders of `~/.config/user-dirs.dirs` (Linux), with
/// `$HOME` expanded.
fn xdg_user_dirs(home: &Path) -> Vec<(String, PathBuf)> {
    let Ok(text) = std::fs::read_to_string(home.join(".config/user-dirs.dirs")) else {
        return Vec::new();
    };
    parse_user_dirs(&text, home)
}

/// Read `KEY="$HOME/Folder"` lines.
pub(crate) fn parse_user_dirs(text: &str, home: &Path) -> Vec<(String, PathBuf)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let value = value.trim().trim_matches('"');
            let path = match value.strip_prefix("$HOME") {
                Some(rest) => home.join(rest.trim_start_matches('/')),
                None => PathBuf::from(value),
            };
            Some((key.trim().to_string(), path))
        })
        .collect()
}

/// Decode an image file (any format the image crate was built with).
pub fn open_image(path: impl AsRef<Path>) -> Result<image::DynamicImage, String> {
    let bytes = read(path).map_err(|e| e.to_string())?;
    decode_picture(&bytes).map_err(|e| e.to_string())
}

/// The image library's own guard on the memory a picture may take to
/// decode: 512 MiB.
const LIBRARY_DECODE_LIMIT: u64 = 512 * 1024 * 1024;

/// Most bytes decoding a picture file may take. The image library's own
/// guard (512 MiB) turned print-size pictures away: a 1 m square at
/// 300 dpi in CMYK needs 557 MB, so files of a metre and more did not
/// import (#2000). Pictures may take what a PDF's pictures read at their
/// own size take ([`FULL_PICTURE_PIXELS`] pixels, with room for 16-bit
/// samples and a CMYK file's buffer beside the result); where pictures
/// are always reduced (the browser, Android) and on 32-bit systems the
/// library's guard stays.
///
/// [`FULL_PICTURE_PIXELS`]: tracedraw_io::pdf_import::FULL_PICTURE_PIXELS
pub fn picture_decode_limit() -> u64 {
    if crate::ui::pdf_import_dialog::always_reduce_pictures() || cfg!(target_pointer_width = "32") {
        LIBRARY_DECODE_LIMIT
    } else {
        tracedraw_io::pdf_import::FULL_PICTURE_PIXELS.saturating_mul(8)
    }
}

/// Decode a picture file (any format the image crate was built with)
/// within [`picture_decode_limit`].
pub fn decode_picture(bytes: &[u8]) -> image::ImageResult<image::DynamicImage> {
    decode_picture_within(bytes, picture_decode_limit())
}

/// Decode a picture file taking at most `limit` bytes, turned the way
/// its Exif orientation says (a photo taken with the camera on its side
/// stands upright, #2167).
pub fn decode_picture_within(bytes: &[u8], limit: u64) -> image::ImageResult<image::DynamicImage> {
    use image::ImageDecoder;
    if is_jxl(bytes) {
        return decode_jxl(bytes, limit);
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(limit);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder()?;
    limits.reserve(decoder.total_bytes())?;
    // A file whose orientation cannot be read stands as stored.
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// A JPEG XL file: the bare codestream or the ISO container (#3210).
pub fn is_jxl(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0x0a]) || bytes.starts_with(b"\0\0\0\x0cJXL \r\n\x87\n")
}

/// Decode a JPEG XL file within `limit` bytes (#3210). The codestream's
/// own orientation is applied by the decoder; an Exif orientation in the
/// file is not applied again, as the format says.
fn decode_jxl(bytes: &[u8], limit: u64) -> image::ImageResult<image::DynamicImage> {
    use image::ImageDecoder;
    let mut decoder = jxl_oxide::integration::JxlDecoder::new(std::io::Cursor::new(bytes))?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(limit);
    decoder.set_limits(limits.clone())?;
    limits.reserve(decoder.total_bytes())?;
    image::DynamicImage::from_decoder(decoder)
}

/// Whether the Exif orientation of picture file `bytes` turns it a
/// quarter turn, so that its width and height (and its resolution
/// across and down) change places.
pub fn picture_turns_sideways(bytes: &[u8]) -> bool {
    use image::metadata::Orientation;
    matches!(
        picture_orientation(bytes),
        Some(
            Orientation::Rotate90
                | Orientation::Rotate270
                | Orientation::Rotate90FlipH
                | Orientation::Rotate270FlipH
        )
    )
}

/// The Exif orientation of picture file `bytes`, and its width and
/// height as stored, from its header; `None` when it cannot be read.
pub fn picture_header(bytes: &[u8]) -> Option<(image::metadata::Orientation, u32, u32)> {
    use image::ImageDecoder;
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    let (w, h) = decoder.dimensions();
    Some((decoder.orientation().ok()?, w, h))
}

fn picture_orientation(bytes: &[u8]) -> Option<image::metadata::Orientation> {
    picture_header(bytes).map(|(o, _, _)| o)
}

/// Keep bytes under a synthetic path so [`read`] finds them (browser).
pub fn store(path: &Path, bytes: Vec<u8>) {
    STORE.with(|s| {
        s.borrow_mut().insert(path.to_path_buf(), bytes);
    });
}

/// Keep an uploaded file past the dialog that picked it (a profile that
/// is read again later): returns the path to read it from. Natively the
/// file stays where it is.
pub fn keep(path: &Path) -> PathBuf {
    if !WEB {
        return path.to_path_buf();
    }
    let kept = PathBuf::from("/kept").join(path.file_name().unwrap_or_default());
    if let Ok(bytes) = read(path) {
        store(&kept, bytes);
    }
    kept
}

/// Forget an uploaded file once nothing needs it any more.
pub fn forget(path: &Path) {
    STORE.with(|s| {
        s.borrow_mut().remove(path);
    });
}

/// Run the dialogs that finished since the last frame.
/// Called once per frame before the interface is drawn.
pub fn run_finished(app: &mut App, _ctx: &egui::Context) {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        let jobs = NATIVE_DIALOGS.with(|state| {
            let mut state = state.borrow_mut();
            state.context = Some(_ctx.clone());
            let mut jobs = Vec::new();
            while let Ok((id, paths)) = state.receiver.try_recv() {
                if let Some(job) = state.pending.remove(&id) {
                    if !paths.is_empty() {
                        jobs.push((job, paths));
                    }
                }
            }
            jobs
        });
        // Drop the state borrow before invoking jobs so they can queue dialogs.
        for (job, paths) in jobs {
            job(app, paths);
        }
    }
    #[cfg(target_os = "android")]
    android_pickers::run_answers(app);
    let jobs: Vec<(Job, PathBuf)> = FINISHED.with(|f| std::mem::take(&mut *f.borrow_mut()));
    for (job, path) in jobs {
        job(app, path.clone());
        linger(path);
    }
    // An upload is freed once the app no longer reads it: File > Import
    // reads it again when the placement cursor is clicked or a PDF's
    // question is answered, frames after its job ran (#3231).
    let lingering: Vec<PathBuf> = LINGERING.with(|l| std::mem::take(&mut *l.borrow_mut()));
    for path in lingering {
        if app.reads_later(&path) {
            linger(path);
        } else {
            forget(&path);
        }
    }
}

/// Free `path` on a frame when the app no longer reads it.
fn linger(path: PathBuf) {
    LINGERING.with(|l| {
        let mut l = l.borrow_mut();
        if !l.contains(&path) {
            l.push(path);
        }
    });
}

/// The paths whose extension is one of `exts` (lower case; any case in
/// the name), all of them when `exts` is empty: a folder's files kept to
/// the types a dialog asks for (#1926).
#[cfg(any(target_os = "android", test))]
pub fn with_extensions(paths: Vec<PathBuf>, exts: &[String]) -> Vec<PathBuf> {
    if exts.is_empty() {
        return paths;
    }
    paths
        .into_iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
        })
        .collect()
}

/// The Android document picker (Storage Access Framework): a request
/// shows the system's picker and remembers what to do with the answer,
/// which the activity hands back on a later frame (`android.rs`).
#[cfg(target_os = "android")]
mod android_pickers {
    use super::*;
    use crate::android::documents;

    /// What a request waits to do with the picked documents.
    enum Waiting {
        /// Run with the first document.
        One(Job),
        /// Run with every document.
        Many(Box<dyn FnOnce(&mut App, Vec<PathBuf>)>),
        /// Keep the first document for [`take_picked`] under this key.
        Into(String),
    }

    thread_local! {
        static WAITING: RefCell<HashMap<i32, Waiting>> = RefCell::new(HashMap::new());
        /// Documents other apps opened with TraceDraw, waiting for no
        /// dialog or question to be up (#646).
        static OPENED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    }

    fn wait(request: i32, waiting: Waiting) {
        WAITING.with(|w| {
            w.borrow_mut().insert(request, waiting);
        });
    }

    fn forget(request: i32) {
        WAITING.with(|w| {
            w.borrow_mut().remove(&request);
        });
    }

    /// Show the picker for one document (or several) and run `then`.
    fn pick(multiple: bool, then: Waiting) {
        let request = documents::next_request();
        wait(request, then);
        if !documents::open(request, multiple) {
            forget(request);
        }
    }

    pub fn pick_one(then: Job) {
        pick(false, Waiting::One(then));
    }

    pub fn pick_many(then: Box<dyn FnOnce(&mut App, Vec<PathBuf>)>) {
        pick(true, Waiting::Many(then));
    }

    pub fn pick_into(key: String) {
        pick(false, Waiting::Into(key));
    }

    /// Show the system's folder picker and run `then` with the documents
    /// in the folder chosen (#1926).
    pub fn pick_tree(then: Box<dyn FnOnce(&mut App, Vec<PathBuf>)>) {
        let request = documents::next_request();
        wait(request, Waiting::Many(then));
        if !documents::open_tree(request) {
            forget(request);
        }
    }

    /// Show the picker for a place to save `name` and run `then` with the
    /// document made there.
    pub fn save(name: &str, then: Job) {
        let ext = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let request = documents::next_request();
        wait(request, Waiting::One(then));
        if !documents::create(request, mime_for_extension(ext), name) {
            forget(request);
        }
    }

    /// The paths of an answer's documents.
    fn paths(answer: &documents::Answer) -> Vec<PathBuf> {
        answer
            .picked
            .iter()
            .map(|p| document_path(&p.uri, &name_with_extension(&p.name, &p.mime)))
            .collect()
    }

    /// Run what the answers that arrived since the last frame were
    /// waited for; a cancelled picker runs nothing.
    pub fn run_answers(app: &mut App) {
        for answer in documents::take_answers() {
            // Documents another app opened with TraceDraw: each in a tab
            // of its own, as a file manager's do on the desktop (#646).
            if answer.request == documents::OPENED_ELSEWHERE {
                let paths = paths(&answer);
                OPENED.with(|o| o.borrow_mut().extend(paths));
                continue;
            }
            let Some(waiting) = WAITING.with(|w| w.borrow_mut().remove(&answer.request)) else {
                continue;
            };
            let paths = paths(&answer);
            match waiting {
                Waiting::One(job) => {
                    if let Some(path) = paths.into_iter().next() {
                        job(app, path);
                    }
                }
                Waiting::Many(job) => {
                    if !paths.is_empty() {
                        job(app, paths);
                    }
                }
                Waiting::Into(key) => {
                    if let Some(path) = paths.into_iter().next() {
                        PICKED.with(|p| {
                            p.borrow_mut().insert(key, path);
                        });
                    }
                }
            }
        }
        // Like the files later starts hand over on the desktop, they wait
        // while a dialog or a question is up or the drawings are closing
        // (instance::open_handed_over).
        if !app.closing_all && matches!(app.dialog, crate::ui::dialogs::Dialog::None) {
            for path in OPENED.with(|o| std::mem::take(&mut *o.borrow_mut())) {
                app.open_path(path);
            }
        }
    }
}

/// Files dropped on the window this frame: `then` runs with each one's
/// path, at once natively, on a later frame in a browser (where the bytes
/// are read first and kept under `/upload/<name>` like a picked file).
pub fn take_dropped(
    ctx: &egui::Context,
    app: &mut App,
    then: impl FnOnce(&mut App, PathBuf) + Clone + 'static,
) {
    let dropped = ctx.input(|i| i.raw.dropped_files.clone());
    for file in dropped {
        #[cfg(not(target_arch = "wasm32"))]
        then.clone()(app, file.path().to_path_buf());
        #[cfg(target_arch = "wasm32")]
        {
            let _ = &app;
            let then = then.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match file.bytes_async().await {
                    Ok(bytes) => {
                        let name = file.path().file_name().unwrap_or_default();
                        let path = PathBuf::from("/upload").join(name);
                        store(&path, bytes);
                        let job: Job = Box::new(then);
                        FINISHED.with(|f| f.borrow_mut().push((job, path)));
                        crate::web::request_repaint();
                    }
                    Err(e) => log::warn!("{}: {e}", file.path().display()),
                }
            });
        }
    }
}

/// The path picked for `key` by [`Dialog::pick_into`], once.
pub fn take_picked(key: &str) -> Option<PathBuf> {
    PICKED.with(|p| p.borrow_mut().remove(key))
}

/// Whether the file picker of a browser with this user agent, platform
/// and number of touch points filters by the system's file types rather
/// than by extension: on an iPhone, iPad or iPod and on Android, where
/// files whose extension the system has no type for (`.cdr`, `.cmx`,
/// `.tdraw` ...) can be greyed out once the page names the extensions it
/// takes (#1311). iPadOS browsers call themselves a Mac ("MacIntel",
/// "Macintosh") with a touch screen; Macs have no touch points. Desktop
/// browsers match the extensions themselves and keep the filters.
#[cfg(any(target_arch = "wasm32", test))]
pub fn picker_filters_by_type(user_agent: &str, platform: &str, touch_points: u32) -> bool {
    let named = ["iPhone", "iPad", "iPod"]
        .iter()
        .any(|d| user_agent.contains(d) || platform.starts_with(d));
    let mac = platform == "MacIntel" || user_agent.contains("Macintosh");
    named || (mac && touch_points > 1) || user_agent.contains("Android")
}

/// A file dialog description, the same in both builds.
#[derive(Debug, Clone, Default)]
pub struct Dialog {
    filters: Vec<(String, Vec<String>)>,
    file_name: Option<String>,
    directory: Option<PathBuf>,
    title: Option<String>,
}

impl Dialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_filter(mut self, name: impl Into<String>, extensions: &[&str]) -> Self {
        self.filters.push((
            name.into(),
            extensions.iter().map(|e| e.to_string()).collect(),
        ));
        self
    }

    pub fn set_file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    /// The window title of the desktop dialog, such as "Export" (#3994).
    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// A menu command's name without its trailing dots, for a dialog's
    /// title: "Import..." titles the Import window "Import".
    pub fn command_title(key: &str) -> String {
        crate::i18n::tr(key)
            .trim_end_matches(['.', '…', ' '])
            .to_string()
    }

    /// The folder the desktop dialog opens in (ignored in a browser and
    /// on Android, whose pickers choose their own).
    pub fn set_directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.directory = Some(dir.into());
        self
    }

    /// Pick a file and run `then` with its path on a later frame.
    pub fn pick_file(self, app: &mut App, then: impl FnOnce(&mut App, PathBuf) + 'static) {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let _ = app;
            schedule_native_dialog(self.native_pick(false), move |app, paths| {
                if let Some(path) = paths.into_iter().next() {
                    then(app, path);
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = app;
            self.web_pick(Box::new(then));
        }
        // The system's document picker; `then` runs when it answers.
        #[cfg(target_os = "android")]
        {
            let _ = app;
            android_pickers::pick_one(Box::new(then));
        }
    }

    /// A file to open (`mode` 0), where to save (1) or a folder (2), and
    /// `then` with it, or with `None` when the user cancels: on a later
    /// frame on the desktop; elsewhere at once with `None` (a macro's file
    /// box, #4315; macros run on the desktop only).
    pub fn choose(
        self,
        app: &mut App,
        mode: u8,
        then: impl FnOnce(&mut App, Option<PathBuf>) + 'static,
    ) {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let _ = app;
            let job = move |app: &mut App, paths: Vec<PathBuf>| then(app, paths.into_iter().next());
            match mode {
                1 => schedule_native_dialog(self.native_save(), job),
                2 => {
                    let start = self.directory.clone();
                    schedule_native_dialog(native_folder_pick(FolderToken(()), start.as_deref()), job)
                }
                _ => schedule_native_dialog(self.native_pick(false), job),
            }
        }
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        {
            let _ = mode;
            then(app, None);
        }
    }

    /// Pick one or more files and run `then` with their paths: at once
    /// natively, on a later frame in a browser and on Android.
    /// Choose a folder where only files can be picked otherwise, and run
    /// `then` with the files in it the filters take (#1926). On Android
    /// the system's folder picker (`ACTION_OPEN_DOCUMENT_TREE`) asks for
    /// the folder and its files come as picked documents, on a later
    /// frame; in a browser, which has no folder picker, the files are
    /// picked several at once ([`Self::pick_files`]).
    pub fn pick_folder_files(
        self,
        app: &mut App,
        then: impl FnOnce(&mut App, Vec<PathBuf>) + 'static,
    ) {
        #[cfg(target_os = "android")]
        {
            let _ = app;
            let exts = self.extensions();
            android_pickers::pick_tree(Box::new(move |app: &mut App, paths: Vec<PathBuf>| {
                let paths = with_extensions(paths, &exts);
                if !paths.is_empty() {
                    then(app, paths);
                }
            }));
        }
        #[cfg(not(target_os = "android"))]
        self.pick_files(app, then);
    }

    /// Every extension the filters name, lower case.
    #[cfg(any(target_os = "android", test))]
    fn extensions(&self) -> Vec<String> {
        self.filters
            .iter()
            .flat_map(|(_, exts)| exts.iter().map(|e| e.to_ascii_lowercase()))
            .collect()
    }

    pub fn pick_files(self, app: &mut App, then: impl FnOnce(&mut App, Vec<PathBuf>) + 'static) {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let _ = app;
            schedule_native_dialog(self.native_pick(true), then);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = app;
            self.web_pick_many(Box::new(then));
        }
        #[cfg(target_os = "android")]
        {
            let _ = app;
            android_pickers::pick_many(Box::new(then));
        }
    }

    /// Pick a file and keep its path for [`take_picked`]`(key)`.
    pub fn pick_into(self, key: &str) {
        let key = key.to_string();
        #[cfg(target_os = "android")]
        android_pickers::pick_into(key);
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        schedule_native_dialog(self.native_pick(false), move |_, paths| {
            if let Some(path) = paths.into_iter().next() {
                PICKED.with(|p| p.borrow_mut().insert(key, path));
            }
        });
        // The upload outlives this job: the dialog reads it on a later frame.
        #[cfg(target_arch = "wasm32")]
        self.web_pick(Box::new(move |_app, path| {
            let kept = keep(&path);
            PICKED.with(|p| {
                p.borrow_mut().insert(key, kept);
            });
        }));
    }

    /// Choose where to save and run `then` with that path; `then` writes
    /// with [`write`]. A save dialog natively, on a later frame; in a browser there
    /// is no dialog: the path is `/download/<file name>` and the write
    /// becomes a download; on Android the system's picker asks where,
    /// and `then` runs on a later frame with the document it made.
    pub fn save_file(self, app: &mut App, then: impl FnOnce(&mut App, PathBuf) + 'static) {
        #[cfg(target_os = "android")]
        {
            let _ = app;
            let name = self.suggested_name();
            android_pickers::save(&name, Box::new(then));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let name = self.file_name.unwrap_or_else(|| "download".into());
            then(app, PathBuf::from("/download").join(name));
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let _ = app;
            schedule_native_dialog(self.native_save(), move |app, paths| {
                if let Some(path) = paths.into_iter().next() {
                    then(app, path);
                }
            });
        }
    }

    /// Save `bytes` where the user says ([`Dialog::save_file`]); a
    /// failed write is told on the status line.
    pub fn save_bytes(self, app: &mut App, bytes: impl Into<Vec<u8>>) {
        let bytes = bytes.into();
        self.save_file(app, move |app, path| {
            if let Err(e) = write(&path, bytes) {
                app.status = e.to_string();
            }
        });
    }

    /// The name offered for a new file: the one set, with the first
    /// filter's extension when it has none.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    fn suggested_name(&self) -> String {
        let name = self
            .file_name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "drawing".into());
        if Path::new(&name).extension().is_some() {
            return name;
        }
        match self.filters.first().and_then(|(_, exts)| exts.first()) {
            Some(ext) => format!("{name}.{ext}"),
            None => name,
        }
    }

    /// Choose a save path and keep it for [`take_picked`] on a later frame.
    /// Browser and Android paths retain their existing local defaults.
    pub fn save_path_into(self, key: &str) {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let key = key.to_string();
            schedule_native_dialog(self.native_save(), move |_, paths| {
                if let Some(path) = paths.into_iter().next() {
                    PICKED.with(|p| p.borrow_mut().insert(key, path));
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let name = self.file_name.unwrap_or_else(|| "download".into());
            PICKED.with(|p| {
                p.borrow_mut()
                    .insert(key.to_string(), PathBuf::from("/download").join(name))
            });
        }
        #[cfg(target_os = "android")]
        {
            let name = self.suggested_name();
            let dir = android_documents_dir();
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log::warn!("cannot create {}: {e}", dir.display());
            }
            log::info!("saved without a picker to {}", dir.display());
            PICKED.with(|p| p.borrow_mut().insert(key.to_string(), dir.join(name)));
        }
    }

    /// The filters offered, in order: a name and its extensions.
    #[cfg(test)]
    pub fn filters(&self) -> &[(String, Vec<String>)] {
        &self.filters
    }

    /// The file name offered.
    #[cfg(test)]
    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }

    /// The dialog window's title.
    #[cfg(test)]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// The folder the dialog opens in.
    #[cfg(test)]
    pub fn directory(&self) -> Option<&Path> {
        self.directory.as_deref()
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn native_dialog(&self) -> rfd::AsyncFileDialog {
        let mut d = rfd::AsyncFileDialog::new();
        for (name, exts) in &self.filters {
            d = d.add_filter(name, exts);
        }
        if let Some(name) = &self.file_name {
            d = d.set_file_name(name);
        }
        if let Some(dir) = self.directory.as_ref().filter(|d| d.is_dir()) {
            d = d.set_directory(dir);
        }
        if let Some(title) = &self.title {
            d = d.set_title(title);
        }
        d
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn native_pick(self, many: bool) -> impl std::future::Future<Output = Vec<PathBuf>> + Send {
        #[cfg(test)]
        {
            let _ = (self.native_dialog(), many);
            std::future::ready(test_pick::ask())
        }
        #[cfg(not(test))]
        {
            let dialog = self.native_dialog();
            let one = (!many).then(|| dialog.clone().pick_file());
            let multiple = many.then(|| dialog.pick_files());
            async move {
                if let Some(future) = one {
                    future
                        .await
                        .into_iter()
                        .map(|f| f.path().to_path_buf())
                        .collect()
                } else if let Some(future) = multiple {
                    future
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .map(|f| f.path().to_path_buf())
                        .collect()
                } else {
                    Vec::new()
                }
            }
        }
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn native_save(self) -> impl std::future::Future<Output = Vec<PathBuf>> + Send {
        #[cfg(test)]
        {
            let _ = self.native_dialog();
            std::future::ready(test_save::ask(self).into_iter().collect())
        }
        #[cfg(not(test))]
        {
            let future = self.native_dialog().save_file();
            async move {
                future
                    .await
                    .into_iter()
                    .map(|f| f.path().to_path_buf())
                    .collect()
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn web_pick_many(self, then: Box<dyn FnOnce(&mut App, Vec<PathBuf>)>) {
        let d = self.web_dialog();
        wasm_bindgen_futures::spawn_local(async move {
            let Some(handles) = d.pick_files().await else {
                return;
            };
            let mut paths = Vec::new();
            for handle in handles {
                let path = PathBuf::from("/upload").join(handle.file_name());
                store(&path, handle.read().await);
                paths.push(path);
            }
            if paths.is_empty() {
                return;
            }
            // One job for them all, under the first path; the others are
            // forgotten once it ran.
            let first = paths[0].clone();
            let job: Job = Box::new(move |app: &mut App, _| {
                let rest = paths.clone();
                then(app, paths);
                for p in rest.into_iter().skip(1) {
                    linger(p);
                }
            });
            FINISHED.with(|f| f.borrow_mut().push((job, first)));
            crate::web::request_repaint();
        });
    }

    /// The browser's picker with the filters, where they help.
    #[cfg(target_arch = "wasm32")]
    fn web_dialog(&self) -> rfd::AsyncFileDialog {
        let mut d = rfd::AsyncFileDialog::new();
        // On an iPhone, iPad or Android device the picker can grey out
        // every file whose extension the system does not know, `.cdr`
        // among them, as soon as the page names the extensions it takes:
        // there it offers every file instead (#1311).
        if !crate::web::picker_filters_by_type() {
            for (name, exts) in &self.filters {
                let exts: Vec<&str> = exts.iter().map(|e| e.as_str()).collect();
                d = d.add_filter(name, &exts);
            }
        }
        d
    }

    #[cfg(target_arch = "wasm32")]
    fn web_pick(self, then: Job) {
        let d = self.web_dialog();
        wasm_bindgen_futures::spawn_local(async move {
            let Some(handle) = d.pick_file().await else {
                return;
            };
            let name = handle.file_name();
            let bytes = handle.read().await;
            let path = PathBuf::from("/upload").join(name);
            store(&path, bytes);
            FINISHED.with(|f| f.borrow_mut().push((then, path)));
            crate::web::request_repaint();
        });
    }
}

/// Offer bytes as a browser download (no-op natively).
#[cfg(target_arch = "wasm32")]
fn download(name: &str, bytes: &[u8]) -> std::io::Result<()> {
    crate::web::download(name, bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

#[cfg(not(target_arch = "wasm32"))]
fn download(_name: &str, _bytes: &[u8]) -> std::io::Result<()> {
    Ok(())
}

#[cfg(all(test, not(any(target_arch = "wasm32", target_os = "android"))))]
pub(crate) mod test_pick {
    use super::*;
    thread_local! {
        static ANSWERS: RefCell<std::collections::VecDeque<Vec<PathBuf>>> = const { RefCell::new(std::collections::VecDeque::new()) };
    }
    pub fn answer(paths: Vec<PathBuf>) {
        ANSWERS.with(|a| a.borrow_mut().push_back(paths));
    }
    pub fn ask() -> Vec<PathBuf> {
        ANSWERS.with(|a| a.borrow_mut().pop_front().unwrap_or_default())
    }
}

/// Unit tests never open the system's save dialog: each dialog asked
/// for is kept for the test to look at, and it gets the next answer the
/// test queued (`None` for Cancel); with nothing queued it is cancelled.
#[cfg(test)]
pub mod test_save {
    use super::{Dialog, PathBuf, RefCell};
    use std::collections::VecDeque;

    thread_local! {
        static ANSWERS: RefCell<VecDeque<Option<PathBuf>>> = const { RefCell::new(VecDeque::new()) };
        static ASKED: RefCell<Vec<Dialog>> = const { RefCell::new(Vec::new()) };
    }

    /// Dispatch queued native test answers as later UI frames would.
    pub fn finish(app: &mut super::App) {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let start = std::time::Instant::now();
            let ctx = egui::Context::default();
            while super::NATIVE_DIALOGS.with(|s| !s.borrow().pending.is_empty()) {
                assert!(start.elapsed() < std::time::Duration::from_secs(5));
                super::run_finished(app, &ctx);
                std::thread::yield_now();
            }
        }
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        let _ = app;
    }

    /// The answer of the next save dialog: the path chosen, or `None`
    /// for Cancel.
    pub fn answer(path: Option<PathBuf>) {
        ANSWERS.with(|a| a.borrow_mut().push_back(path));
    }

    /// The save dialogs asked for since the last call, in order.
    pub fn asked() -> Vec<Dialog> {
        ASKED.with(|a| std::mem::take(&mut *a.borrow_mut()))
    }

    #[cfg_attr(any(target_arch = "wasm32", target_os = "android"), allow(dead_code))]
    pub(super) fn ask(dialog: Dialog) -> Option<PathBuf> {
        ASKED.with(|a| a.borrow_mut().push(dialog));
        ANSWERS.with(|a| a.borrow_mut().pop_front()).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn queue_native_test_job(job: NativeJob, paths: Vec<PathBuf>) {
        let (id, sender) = NATIVE_DIALOGS.with(|state| {
            let mut state = state.borrow_mut();
            (
                state.register(job).expect("request ID"),
                state.sender.clone(),
            )
        });
        std::thread::spawn(move || sender.send((id, paths)).expect("completion"))
            .join()
            .expect("worker");
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn native_dialog_jobs_wait_for_ui_dispatch() {
        let mut app = App::headless();
        let called = std::rc::Rc::new(RefCell::new(Vec::new()));
        let result = called.clone();
        queue_native_test_job(
            Box::new(move |_, paths| {
                result
                    .borrow_mut()
                    .push((std::thread::current().id(), paths));
            }),
            vec![PathBuf::from("drawing.tdraw")],
        );
        assert!(called.borrow().is_empty());
        run_finished(&mut app, &egui::Context::default());
        assert_eq!(
            *called.borrow(),
            vec![(
                std::thread::current().id(),
                vec![PathBuf::from("drawing.tdraw")]
            )]
        );
        run_finished(&mut app, &egui::Context::default());
        assert_eq!(called.borrow().len(), 1);
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn cancelled_native_dialog_releases_job() {
        let mut app = App::headless();
        let retained = std::rc::Rc::new(());
        let captured = retained.clone();
        queue_native_test_job(
            Box::new(move |_, _| {
                drop(captured);
                panic!("cancelled job must not run");
            }),
            Vec::new(),
        );
        assert_eq!(std::rc::Rc::strong_count(&retained), 2);
        run_finished(&mut app, &egui::Context::default());
        assert_eq!(std::rc::Rc::strong_count(&retained), 1);
        assert!(NATIVE_DIALOGS.with(|state| state.borrow().pending.is_empty()));
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn native_dialog_results_match_request_ids() {
        let mut app = App::headless();
        let seen = std::rc::Rc::new(RefCell::new(Vec::new()));
        let register = |label| {
            NATIVE_DIALOGS.with(|state| {
                let seen = seen.clone();
                let mut state = state.borrow_mut();
                let id = state
                    .register(Box::new(move |_, paths| {
                        seen.borrow_mut().push((label, paths))
                    }))
                    .expect("request ID");
                (id, state.sender.clone())
            })
        };
        let (first, sender) = register("first");
        let (second, _) = register("second");
        assert!(second > first);
        std::thread::spawn(move || {
            sender
                .send((second, vec![PathBuf::from("b.svg")]))
                .expect("second");
            sender
                .send((first, vec![PathBuf::from("a.svg")]))
                .expect("first");
        })
        .join()
        .expect("worker");
        run_finished(&mut app, &egui::Context::default());
        assert_eq!(
            *seen.borrow(),
            vec![
                ("second", vec![PathBuf::from("b.svg")]),
                ("first", vec![PathBuf::from("a.svg")])
            ]
        );
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn native_dialog_dispatch_requests_repaint() {
        let mut app = App::headless();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
            output.textures_delta.clear();
        }
        run_finished(&mut app, &ctx);
        let (sent, received) = std::sync::mpsc::channel();
        ctx.set_request_repaint_callback(move |_| {
            let _ = sent.send(std::thread::current().id());
        });
        let called = std::rc::Rc::new(RefCell::new(None));
        let result = called.clone();
        schedule_native_dialog(
            async { vec![PathBuf::from("picked.svg")] },
            move |_, paths| {
                *result.borrow_mut() = Some((std::thread::current().id(), paths));
            },
        );
        let worker = received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("worker repaint");
        assert_ne!(worker, std::thread::current().id());
        assert!(called.borrow().is_none());
        run_finished(&mut app, &ctx);
        assert_eq!(
            *called.borrow(),
            Some((
                std::thread::current().id(),
                vec![PathBuf::from("picked.svg")]
            ))
        );
    }


    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn native_picker_configuration_keeps_filters_name_and_directory() {
        let dir = std::env::temp_dir();
        let dialog = Dialog::new()
            .add_filter("PDF", &["pdf"])
            .add_filter("Images", &["png", "jpg"])
            .set_file_name("drawing.pdf")
            .set_directory(&dir)
            .set_title("Export");
        let config = format!("{:?}", dialog.native_dialog());
        for expected in ["PDF", "pdf", "Images", "png", "jpg", "drawing.pdf"] {
            assert!(config.contains(expected), "{config}");
        }
        assert!(config.contains("Export"), "{config}");
        assert!(config.contains(&format!("{:?}", dir)));
        let invalid = Dialog::new().set_directory(dir.join("missing-tracedraw-dialog-dir"));
        assert!(format!("{:?}", invalid.native_dialog()).contains("starting_directory: None"));
    }

    /// The Import and Export windows are titled after their commands,
    /// not with the system's "Open" and "Save As" (#3994).
    #[test]
    fn import_and_export_dialogs_are_titled_after_their_commands() {
        assert_eq!(Dialog::command_title("menu.file.import"), "Import");
        assert_eq!(Dialog::command_title("menu.file.export"), "Export");
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn native_file_pick_dispatches_one_path() {
        let mut app = App::headless();
        run_finished(&mut app, &egui::Context::default());
        test_pick::answer(vec![PathBuf::from("chosen.svg")]);
        let seen = std::rc::Rc::new(RefCell::new(None));
        let result = seen.clone();
        Dialog::new().pick_file(&mut app, move |_, path| *result.borrow_mut() = Some(path));
        assert!(seen.borrow().is_none());
        test_save::finish(&mut app);
        assert_eq!(*seen.borrow(), Some(PathBuf::from("chosen.svg")));
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn native_multi_pick_dispatches_all_paths_in_order() {
        let mut app = App::headless();
        run_finished(&mut app, &egui::Context::default());
        test_pick::answer(vec![PathBuf::from("z.svg"), PathBuf::from("a.svg")]);
        let seen = std::rc::Rc::new(RefCell::new(Vec::new()));
        let result = seen.clone();
        Dialog::new().pick_files(&mut app, move |_, paths| *result.borrow_mut() = paths);
        assert!(seen.borrow().is_empty());
        test_save::finish(&mut app);
        assert_eq!(
            *seen.borrow(),
            vec![PathBuf::from("z.svg"), PathBuf::from("a.svg")]
        );
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn native_picker_saves_and_keyed_results_are_deferred_and_one_shot() {
        let mut app = App::headless();
        run_finished(&mut app, &egui::Context::default());
        test_save::answer(Some(PathBuf::from("out.pdf")));
        let seen = std::rc::Rc::new(RefCell::new(None));
        let result = seen.clone();
        Dialog::new().save_file(&mut app, move |_, path| *result.borrow_mut() = Some(path));
        assert!(seen.borrow().is_none());
        test_save::finish(&mut app);
        assert_eq!(*seen.borrow(), Some(PathBuf::from("out.pdf")));
        test_save::answer(Some(PathBuf::from("browse.pdf")));
        Dialog::new().save_path_into("test-save-path");
        assert!(take_picked("test-save-path").is_none());
        test_save::finish(&mut app);
        assert_eq!(
            take_picked("test-save-path"),
            Some(PathBuf::from("browse.pdf"))
        );
        assert!(take_picked("test-save-path").is_none());
        test_pick::answer(vec![PathBuf::from("key.svg")]);
        Dialog::new().pick_into("test-pick");
        assert!(take_picked("test-pick").is_none());
        test_save::finish(&mut app);
        assert_eq!(take_picked("test-pick"), Some(PathBuf::from("key.svg")));
        assert!(take_picked("test-pick").is_none());
        test_save::answer(None);
        Dialog::new().save_file(&mut app, |_, _| panic!("cancelled save"));
        test_pick::answer(Vec::new());
        Dialog::new().pick_file(&mut app, |_, _| panic!("cancelled pick"));
        test_pick::answer(Vec::new());
        Dialog::new().pick_files(&mut app, |_, _| panic!("cancelled multi pick"));
        test_save::finish(&mut app);
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn folder_pick_is_deferred_until_ui_dispatch() {
        let mut app = App::headless();
        run_finished(&mut app, &egui::Context::default());
        let token = match FolderUse::ExportAssets.action() {
            FolderAction::PickFolder(token) => token,
            other => panic!("desktop folder action: {other:?}"),
        };
        let key = "test-folder-pick";
        let path = PathBuf::from("picked-folder");
        test_pick::answer(vec![path.clone()]);
        pick_folder_into(token, None, key);
        assert_eq!(take_picked(key), None);
        test_save::finish(&mut app);
        assert_eq!(take_picked(key), Some(path));
        assert_eq!(take_picked(key), None);
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn folder_cancel_keeps_existing_folder() {
        let mut app = App::headless();
        run_finished(&mut app, &egui::Context::default());
        let token = match FolderUse::FontFolder.action() {
            FolderAction::PickFolder(token) => token,
            other => panic!("desktop folder action: {other:?}"),
        };
        let key = "test-folder-cancel";
        let existing = PathBuf::from("existing-folder");
        PICKED.with(|p| p.borrow_mut().insert(key.into(), existing.clone()));
        test_pick::answer(Vec::new());
        pick_folder_into(token, None, key);
        test_save::finish(&mut app);
        assert_eq!(take_picked(key), Some(existing));
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn folder_pick_from_preserves_start_directory() {
        let dir = std::env::temp_dir();
        let token = match FolderUse::SaveDrawing.action() {
            FolderAction::PickFolder(token) => token,
            other => panic!("desktop folder action: {other:?}"),
        };
        let config = format!("{:?}", native_folder_dialog(token, Some(&dir)));
        assert!(config.contains(&format!("starting_directory: Some({dir:?})")), "{config}");
    }

    /// The cloud folders offered in the Save Drawing dialog: OneDrive
    /// where Windows names it, and the folders the cloud programs make
    /// in the home folder, each once (#3416).
    #[test]
    fn cloud_folders_come_from_the_home_folder_and_onedrive() {
        let home = Path::new("/home/me");
        let var = |k: &str| match k {
            "OneDrive" => Some(std::ffi::OsString::from("/home/me/OneDrive")),
            "OneDriveCommercial" => Some(std::ffi::OsString::from("/home/me/OneDrive - Work")),
            _ => None,
        };
        let dirs = cloud_dirs(home, &var);
        assert_eq!(
            dirs,
            vec![
                home.join("OneDrive"),
                home.join("OneDrive - Work"),
                home.join("Dropbox"),
                home.join("Google Drive"),
                home.join("iCloudDrive"),
            ]
        );
    }

    /// A write that fails half way (a full disk, an error while the
    /// drawing is written) leaves the file it was replacing as it was:
    /// it was written over from the start and left cut short. A write
    /// that succeeds replaces it and leaves no `.part` file; a new file
    /// is made the same way.
    #[test]
    fn a_failed_write_keeps_the_file_it_was_replacing() {
        let dir =
            std::env::temp_dir().join(format!("tracedraw-replace-{}", crate::recovery::new_id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("drawing.tdraw");
        std::fs::write(&path, b"the old drawing").expect("old");
        let failed = write_with(&path, |w| {
            w.write_all(b"half of the new")?;
            Err(std::io::Error::other("no space left on device"))
        });
        assert!(failed.is_err());
        assert_eq!(std::fs::read(&path).expect("read"), b"the old drawing");
        let names = |dir: &Path| -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(dir)
                .expect("list")
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            v.sort();
            v
        };
        assert_eq!(names(&dir), vec!["drawing.tdraw"], "no .part left");
        write_with(&path, |w| w.write_all(b"the new drawing")).expect("save");
        assert_eq!(std::fs::read(&path).expect("read"), b"the new drawing");
        write(&path, b"shorter").expect("write");
        assert_eq!(std::fs::read(&path).expect("read"), b"shorter");
        let new = dir.join("new.cdr");
        write(&new, b"made").expect("new");
        assert_eq!(std::fs::read(&new).expect("read"), b"made");
        assert_eq!(names(&dir), vec!["drawing.tdraw", "new.cdr"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Names in every script stay as typed; only direction marks, control
    /// characters and what the system refuses change.
    #[test]
    fn file_names_in_every_script_are_kept() {
        for windows in [false, true] {
            for name in [
                "שלום.tdraw",
                "café.tdraw",
                "cafe\u{301}.tdraw",
                "文件.tdraw",
                "שָׁלוֹם עולם.cdr",
            ] {
                assert_eq!(clean_file_name_for(name, windows), name);
            }
            assert_eq!(
                clean_file_name_for("\u{200F}\u{202B}שלום\u{202C}.tdraw\u{200E}", windows),
                "שלום.tdraw"
            );
            assert_eq!(
                clean_file_name_for("\u{2067}مرحبا\u{2069}", windows),
                "مرحبا"
            );
            assert_eq!(clean_file_name_for("a/b\\c\n", windows), "a_b_c");
            assert_eq!(clean_file_name_for(" \u{200F} ", windows), "");
        }
        assert_eq!(clean_file_name_for("a: b?", false), "a: b?");
        assert_eq!(clean_file_name_for("a: b?", true), "a_ b_");
        assert_eq!(clean_file_name_for("שלום. .", true), "שלום");
        assert_eq!(safe_file_name("\u{200F}שלום.ttf"), "שלום.ttf");
        assert!(is_direction_mark('\u{061C}'));
        assert!(!is_direction_mark('\u{200D}'), "the joiner shapes letters");
    }

    /// #2000: a print-size picture decodes past the image library's own
    /// 512 MiB guard, which turned away files of about a metre and more
    /// at print resolution; decoding is held to the limit given.
    #[test]
    fn print_size_pictures_decode_past_the_libraries_guard() {
        // 2.5 x 1 m at 300 dpi, 8-bit CMYK: the file's samples beside the
        // RGB result.
        let need = 29_528u64 * 11_811 * (4 + 3);
        assert!(
            need > LIBRARY_DECODE_LIMIT,
            "the library's guard refuses it"
        );
        if cfg!(target_pointer_width = "64")
            && !crate::ui::pdf_import_dialog::always_reduce_pictures()
        {
            assert!(picture_decode_limit() >= need);
        }
        assert!(picture_decode_limit() >= LIBRARY_DECODE_LIMIT);
        let img = image::RgbImage::from_pixel(60, 40, image::Rgb([10, 20, 30]));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png)
            .expect("png");
        let png = png.into_inner();
        assert!(decode_picture_within(&png, 60 * 40 * 3 - 1).is_err());
        let back = decode_picture(&png).expect("decodes");
        assert_eq!((back.width(), back.height()), (60, 40));
        // Not a picture: an error, no panic.
        assert!(decode_picture(b"not a picture").is_err());
        assert!(decode_picture(&png[..png.len() / 2]).is_err());
    }

    /// iPhones and iPads (also iPadOS, which calls itself a Mac with a
    /// touch screen) and Android browsers get a picker without extension
    /// filters; Macs, Windows and Linux keep them (#1311).
    #[test]
    fn mobile_browsers_pick_without_extension_filters() {
        let ipad_os = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
        assert!(picker_filters_by_type(ipad_os, "MacIntel", 5));
        assert!(!picker_filters_by_type(ipad_os, "MacIntel", 0), "a Mac");
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15";
        assert!(picker_filters_by_type(iphone, "iPhone", 5));
        let old_ipad = "Mozilla/5.0 (iPad; CPU OS 12_0 like Mac OS X) AppleWebKit/605.1.15";
        assert!(picker_filters_by_type(old_ipad, "iPad", 5));
        let android = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/130.0 Mobile Safari/537.36";
        assert!(picker_filters_by_type(android, "Linux armv8l", 5));
        let firefox_android =
            "Mozilla/5.0 (Android 14; Mobile; rv:131.0) Gecko/131.0 Firefox/131.0";
        assert!(picker_filters_by_type(firefox_android, "Linux aarch64", 5));
        let windows = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0 Safari/537.36";
        assert!(
            !picker_filters_by_type(windows, "Win32", 10),
            "a touch laptop"
        );
        let linux = "Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko/20100101 Firefox/131.0";
        assert!(!picker_filters_by_type(linux, "Linux x86_64", 0));
    }

    #[test]
    fn picked_paths_are_taken_once() {
        PICKED.with(|p| {
            p.borrow_mut().insert("k".into(), PathBuf::from("/x/a.csv"));
        });
        assert_eq!(take_picked("k"), Some(PathBuf::from("/x/a.csv")));
        assert_eq!(take_picked("k"), None);
    }

    #[test]
    fn finished_jobs_run_once_and_free_their_upload() {
        let mut app = App::headless();
        let path = PathBuf::from("/upload/t.txt");
        store(&path, b"hello".to_vec());
        FINISHED.with(|f| {
            f.borrow_mut().push((
                Box::new(|app: &mut App, p: PathBuf| {
                    app.status = format!("got {}", p.display());
                }),
                path.clone(),
            ))
        });
        run_finished(&mut app, &egui::Context::default());
        assert_eq!(app.status, "got /upload/t.txt");
        run_finished(&mut app, &egui::Context::default());
        assert_eq!(app.status, "got /upload/t.txt");
        assert!(STORE.with(|s| s.borrow().get(&path).is_none()));
    }

    /// An upload File > Import still reads on a later frame (the file
    /// waiting for its place, the ones picked with it, a PDF's question)
    /// stays until nothing does; it was freed as soon as the picker's job
    /// ran, so the import found nothing in a browser (#3231).
    #[test]
    fn an_upload_stays_while_an_import_still_reads_it() {
        let mut app = App::headless();
        let first = PathBuf::from("/upload/two-pages.ai");
        let second = PathBuf::from("/upload/second.pdf");
        store(&first, b"%PDF-1.4".to_vec());
        store(&second, b"%PDF-1.4".to_vec());
        let paths = vec![first.clone(), second.clone()];
        FINISHED.with(|f| {
            f.borrow_mut().push((
                Box::new(move |app: &mut App, _| {
                    app.import_placing = Some(crate::import_place::ImportPlacing {
                        path: paths[0].clone(),
                        drag_from: None,
                    });
                    app.import_queue = paths[1..].iter().cloned().collect();
                    linger(paths[1].clone());
                }),
                first.clone(),
            ))
        });
        let kept = |p: &PathBuf| STORE.with(|s| s.borrow().contains_key(p));
        run_finished(&mut app, &egui::Context::default());
        run_finished(&mut app, &egui::Context::default());
        assert!(kept(&first) && kept(&second));
        // Placed: the PDF asks how to read its text, then reads it again.
        app.import_placing = None;
        app.dialog =
            crate::ui::dialogs::Dialog::PdfText(crate::ui::pdf_import_dialog::PdfTextState {
                path: first.clone(),
                import: true,
                curves: false,
                paragraphs: false,
                picture: false,
                dpi: 300.0,
                page_size: None,
                all_pages: true,
                range: "1".into(),
                reduce_pictures: true,
                dont_ask: false,
            });
        run_finished(&mut app, &egui::Context::default());
        assert!(kept(&first) && kept(&second));
        app.dialog = crate::ui::dialogs::Dialog::None;
        run_finished(&mut app, &egui::Context::default());
        assert!(!kept(&first) && kept(&second));
        app.import_queue.clear();
        run_finished(&mut app, &egui::Context::default());
        assert!(!kept(&second));
    }

    /// A document the Android picker returned is named by its URI and its
    /// display name: the name gives the title and the type, the URI comes
    /// back whole from the path, however many slashes and percent signs
    /// it holds (#893).
    #[test]
    fn a_picked_document_keeps_its_uri_and_shows_its_name() {
        let uri = "content://com.android.externalstorage.documents/document/primary%3ADrawings%2Flogo.cdr";
        let path = document_path(uri, "logo.cdr");
        assert_eq!(path.file_name().unwrap(), "logo.cdr");
        assert_eq!(path.extension().unwrap(), "cdr");
        assert!(path.starts_with(DOCUMENT_ROOT));
        assert!(is_document_path(&path));
        assert_eq!(document_uri(&path).as_deref(), Some(uri));
        assert_eq!(canonical(&path), path);
        // A name with a slash cannot become a folder; an empty one is named.
        assert_eq!(
            document_path(uri, "a/b.svg").file_name().unwrap(),
            "a_b.svg"
        );
        assert_eq!(document_path(uri, "  ").file_name().unwrap(), "document");
        // Ordinary paths are not documents.
        assert_eq!(document_uri(Path::new("/home/a/content/x")), None);
        assert_eq!(document_uri(Path::new("/content")), None);
        assert!(!is_document_path(Path::new("/tmp/logo.cdr")));
    }

    /// A picked document without an extension gets one from its MIME
    /// type, and the save picker is told the type of the name it saves.
    #[test]
    fn names_and_mime_types_follow_each_other() {
        assert_eq!(name_with_extension("IMG_1", "image/jpeg"), "IMG_1.jpg");
        assert_eq!(
            name_with_extension("report", "application/pdf"),
            "report.pdf"
        );
        assert_eq!(name_with_extension("logo.cdr", ""), "logo.cdr");
        assert_eq!(
            name_with_extension("notes", "application/x-unknown"),
            "notes"
        );
        assert_eq!(mime_for_extension("pdf"), "application/pdf");
        assert_eq!(mime_for_extension(".SVG"), "image/svg+xml");
        assert_eq!(mime_for_extension("tdraw"), "application/octet-stream");
        assert_eq!(mime_for_extension("cdr"), "application/octet-stream");
        assert_eq!(
            Dialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name("Graphic1")
                .suggested_name(),
            "Graphic1.pdf"
        );
        assert_eq!(
            Dialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name("Graphic1.pdf")
                .suggested_name(),
            "Graphic1.pdf"
        );
        assert_eq!(Dialog::new().suggested_name(), "drawing");
    }

    /// Every folder button has one behaviour per platform (#1399): the
    /// desktop picks a folder; a browser and Android never do (no folder
    /// can be picked there), and get a zip for folders written into, the
    /// open picker for folders read, or a greyed button with its reason.
    #[test]
    fn folder_buttons_pick_a_folder_on_the_desktop_only() {
        use FolderAction::*;
        use FolderUse::*;
        for place in FolderUse::ALL {
            assert!(
                matches!(folder_action(place, Platform::Desktop), PickFolder(_)),
                "{place:?}"
            );
            assert_eq!(folder_tip(place, Platform::Desktop), None, "{place:?}");
            for platform in [Platform::Web, Platform::Android] {
                let action = folder_action(place, platform);
                assert!(!matches!(action, PickFolder(_)), "{place:?} {platform:?}");
                // Never silent: each says why it does what it does.
                let tip = folder_tip(place, platform).expect("a tip");
                assert!(crate::i18n::has_english(tip), "{tip}");
                if let Unavailable(key) = action {
                    assert_eq!(tip, key);
                }
            }
        }
        let table = |platform| {
            FolderUse::ALL
                .iter()
                .map(|&p| folder_action(p, platform))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            table(Platform::Web),
            [
                SaveZip,
                Unavailable("package.desktop_only"),
                OpenFiles,
                OpenFiles,
                OpenFiles,
                OpenFiles,
                Unavailable("folder.backup_web"),
                Unavailable("folder.backup_web"),
                Unavailable("folder.save_desktop"),
                Unavailable("folder.pattern_desktop"),
            ]
        );
        assert_eq!(
            table(Platform::Android),
            [
                SaveZip,
                SaveZip,
                OpenFiles,
                OpenFiles,
                OpenFiles,
                OpenFiles,
                Unavailable("folder.backup_android"),
                Unavailable("folder.backup_android"),
                Unavailable("folder.save_desktop"),
                Unavailable("folder.pattern_desktop"),
            ]
        );
        assert_eq!(
            folder_tip(ExportAssets, Platform::Web),
            Some("folder.zip_web")
        );
        assert_eq!(
            folder_tip(MosaicLibrary, Platform::Android),
            Some("folder.files_android")
        );
        assert_eq!(Platform::current(), Platform::Desktop);
        assert!(matches!(FontFolder.action(), PickFolder(_)));
        // Print Merge's pictures: the folder picker on Android (#1926).
        assert_eq!(
            folder_tip(MergePictures, Platform::Android),
            Some("folder.tree_android")
        );
        assert_eq!(
            folder_tip(MergePictures, Platform::Web),
            Some("folder.files_web")
        );
    }

    /// #1926: a folder's documents are kept to the types the dialog asks
    /// for (any case), all of them with no filter; the dialog's
    /// extensions are its filters', lower case. The Print Merge docker
    /// asks for a folder where only files could be picked, and on Android
    /// the folder picker answers like the open picker.
    #[test]
    fn a_folders_files_keep_to_the_dialogs_types() {
        let paths: Vec<PathBuf> = ["a.png", "b.JPG", "notes.txt", "c.tiff", "noext"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let d = Dialog::new()
            .add_filter("Pictures", &["png", "JPG", "tiff"])
            .add_filter("More", &["webp"]);
        let exts = d.extensions();
        assert_eq!(exts, ["png", "jpg", "tiff", "webp"]);
        assert_eq!(
            with_extensions(paths.clone(), &exts),
            [
                PathBuf::from("a.png"),
                PathBuf::from("b.JPG"),
                PathBuf::from("c.tiff")
            ]
        );
        assert_eq!(with_extensions(paths.clone(), &[]), paths);
        let docker = include_str!("ui/print_merge_docker.rs");
        assert!(docker.contains(".pick_folder_files(app, |app, paths| {"));
        let files = include_str!("files.rs");
        assert!(files.contains("android_pickers::pick_tree("));
        assert!(files.contains("if !documents::open_tree(request) {"));
    }

    /// The app's sources outside this file, as (path, text).
    fn app_sources() -> Vec<(String, String)> {
        fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
            let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
            entries.sort_by_key(|e| e.path());
            for e in entries {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push((
                        p.display().to_string(),
                        std::fs::read_to_string(&p).unwrap(),
                    ));
                }
            }
        }
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        walk(&src, &mut out);
        out.retain(|(p, _)| !p.ends_with("files.rs"));
        out
    }

    /// No dialog shows the folder dialog on its own: each one asks
    /// `FolderUse::action` for the running platform, whose leave to pick
    /// a folder (the token [`pick_folder_into`] takes) exists on the desktop
    /// only. The six dialogs of #1399 and Options all go through it.
    #[test]
    fn no_dialog_picks_a_folder_without_asking_the_platform() {
        let sources = app_sources();
        for (path, text) in &sources {
            assert!(!text.contains("rfd::FileDialog"), "{path}");
            assert!(!text.contains("files::pick_folder("), "{path}");
            assert!(!text.contains("folder_action("), "{path}");
            assert!(!text.contains("FolderToken"), "{path}");
            if text.contains("files::pick_folder_into(") || text.contains("files::pick_folder_with(") {
                assert!(text.contains(".action()"), "{path}");
            }
        }
        for name in [
            "export_docker.rs",
            "package_dialog.rs",
            "font_manager.rs",
            "print_merge_docker.rs",
            "options.rs",
            "picture_mosaic_docker.rs",
            "export_assets.rs",
            "save_as.rs",
        ] {
            let (_, text) = sources
                .iter()
                .find(|(p, _)| p.ends_with(&format!("/{name}")))
                .expect(name);
            assert!(text.contains("FolderUse::"), "{name}");
        }
    }

    /// Presets, masks, scripts, macro exports and palettes are saved
    /// through `Dialog::save_file`, which asks the system's save picker
    /// on Android (#1402): `save_path_into`, which chooses a local path there without asking,
    /// is left to Export as PDF's Browse, which Android does not show.
    #[test]
    fn saves_ask_where_on_android() {
        for (path, text) in app_sources() {
            if text.contains(".save_path_into(") {
                assert!(path.ends_with("publish_pdf.rs"), "{path}");
            }
        }
    }

    /// Picked files gathered into a folder of their own read like any
    /// folder (Print Merge's pictures, the Picture Mosaic library), and a
    /// new pick replaces the files of the last (#1399).
    #[test]
    fn picked_files_gather_into_a_folder() {
        let src = std::env::temp_dir().join(format!("tracedraw-gather-src-{}", std::process::id()));
        std::fs::create_dir_all(&src).unwrap();
        let (a, b) = (src.join("a.png"), src.join("b c.jpg"));
        std::fs::write(&a, [1u8, 2]).unwrap();
        std::fs::write(&b, [3u8]).unwrap();
        let dir = gather("test pictures", &[a.clone(), b.clone()]).unwrap();
        assert!(is_gathered(&dir), "{}", dir.display());
        assert!(!is_gathered(&src));
        let mut names: Vec<String> = list_dir(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, ["a.png", "b c.jpg"]);
        assert_eq!(read(dir.join("a.png")).unwrap(), [1, 2]);
        let again = gather("test pictures", std::slice::from_ref(&b)).unwrap();
        assert_eq!(again, dir);
        assert_eq!(list_dir(&dir).len(), 1);
        assert!(gather("test pictures", &[src.join("missing.png")]).is_err());
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The files written into the folder `zip_written` gives come back as
    /// one zip, each under the top folder, in name order; the folder is
    /// gone afterwards (#1399).
    #[test]
    fn written_files_come_back_as_one_zip() {
        let mut seen = PathBuf::new();
        let zip = zip_written("Flyer", |dir| {
            seen = dir.to_path_buf();
            write(dir.join("b.svg"), "<svg/>").unwrap();
            write(dir.join("a.png"), [9u8; 4]).unwrap();
        })
        .unwrap();
        assert!(!seen.exists(), "{}", seen.display());
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
        let names: Vec<String> = archive
            .file_names()
            .map(|n| n.unwrap().to_string())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(sorted, ["Flyer/a.png", "Flyer/b.svg"]);
        let mut svg = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("Flyer/b.svg").unwrap(), &mut svg)
            .unwrap();
        assert_eq!(svg, "<svg/>");
    }

    #[test]
    fn native_read_and_write_use_the_file_system() {
        let dir = std::env::temp_dir().join(format!("tracedraw-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.bin");
        write(&p, [1u8, 2, 3]).unwrap();
        assert_eq!(read(&p).unwrap(), vec![1, 2, 3]);
        assert!(read(dir.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Save Drawing dialog lists the mounted drives (#1391): those
    /// under /media/$USER, /run/media/$USER and /mnt (from the mount table,
    /// a space written `\040`, and from the folders there), then the
    /// desktop's network shares, each once, without hidden folders or the
    /// parents themselves.
    #[test]
    fn mounted_drives_memory_cards_and_shares_are_listed() {
        let root = std::env::temp_dir().join(format!("tracedraw-drives-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let media = root.join("media");
        let user_media = media.join("ana");
        let run_media = root.join("run/media/ana");
        let mnt = root.join("mnt");
        let gvfs = root.join("gvfs");
        for d in [
            user_media.join("USB STICK"),
            user_media.join(".hidden"),
            run_media.join("CARD"),
            mnt.join("backup"),
            gvfs.join("smb-share:server=nas,share=photos"),
        ] {
            std::fs::create_dir_all(&d).expect("folder");
        }
        let table = format!(
            "/dev/sda1 / ext4 rw 0 0\n/dev/sdb1 {}/USB\\040STICK vfat rw 0 0\nproc /proc proc rw 0 0\n",
            user_media.display()
        );
        assert_eq!(
            parse_mount_points(&table)[1],
            user_media.join("USB STICK"),
            "a space in a mount point"
        );
        let roots = MountRoots {
            mounts: Some(table),
            parents: vec![
                user_media.clone(),
                run_media.clone(),
                media.clone(),
                mnt.clone(),
            ],
            gvfs: Some(gvfs.clone()),
        };
        assert_eq!(
            drives_in(&roots),
            vec![
                user_media.join("USB STICK"),
                run_media.join("CARD"),
                mnt.join("backup"),
                gvfs.join("smb-share:server=nas,share=photos"),
            ]
        );
        assert_eq!(drive_name(&user_media.join("USB STICK")), "USB STICK");
        assert_eq!(
            drive_name(&gvfs.join("smb-share:server=nas,share=photos")),
            "photos (nas)"
        );
        assert_eq!(drive_name(&gvfs.join("mtp:host=Phone_123")), "Phone_123");
        // Nothing mounted, nothing there: no drives.
        assert!(drives_in(&MountRoots::default()).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The desktop's bookmarks: `file://` addresses with `%20` escapes
    /// and an optional name; other addresses are no folders here (#1391).
    #[test]
    fn desktop_bookmarks_are_read() {
        assert_eq!(
            parse_bookmarks(
                "file:///home/ana/My%20Work Work\nfile:///srv/print\nsftp://host/x Remote\n\n"
            ),
            vec![
                ("Work".to_string(), PathBuf::from("/home/ana/My Work")),
                ("print".to_string(), PathBuf::from("/srv/print")),
            ]
        );
    }

    /// Recent folders: the last folder used, then the folders of the
    /// recent drawings, each once, only those that exist, at most `max`.
    #[test]
    fn recent_folders_come_from_the_last_folder_and_recent_drawings() {
        let root = std::env::temp_dir().join(format!("tracedraw-recent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (a, b, c) = (root.join("a"), root.join("b"), root.join("c"));
        for d in [&a, &b, &c] {
            std::fs::create_dir_all(d).expect("folder");
        }
        let recent = vec![
            b.join("one.tdraw"),
            a.join("two.cdr"),
            root.join("gone/three.tdraw"),
            c.join("four.tdraw"),
        ];
        assert_eq!(
            recent_dirs(Some(&a), &recent, 6),
            vec![a.clone(), b.clone(), c.clone()]
        );
        assert_eq!(recent_dirs(None, &recent, 2), vec![b, a]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
