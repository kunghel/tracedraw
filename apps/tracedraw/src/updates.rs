//! Checking for a newer release (Help > Updates, and once a day at start
//! when Options > Global > General allows it). The check asks GitHub for
//! the latest release of the project, compares its version with this one
//! and, when it is newer, offers the download for this system and the
//! release notes. Nothing is installed without the user: Download opens
//! the installer's link, and the system's installer does the rest. Three
//! kinds of copy can also update themselves from the dialog's Install
//! button, without uninstalling anything ([`InstallTarget`]): a Linux
//! AppImage is replaced by the release's AppImage, a Windows copy runs the
//! release's setup program over itself, and a macOS app bundle is replaced
//! by the one on the release's disk image. Each file's SHA-256 digest is
//! checked before anything is touched ([`Installer`]).
//!
//! Desktop only: the browser build is always the latest.

// The browser build keeps the checks and leaves the installing out.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// The project's latest release, as GitHub describes it.
pub const LATEST_URL: &str = "https://api.github.com/repos/tracedraw/tracedraw/releases/latest";
/// The release list, for when no download fits this system.
pub const RELEASES_PAGE: &str = "https://github.com/tracedraw/tracedraw/releases";
/// TraceDraw's page in the Microsoft Store (the browser version offers it
/// on Windows).
pub const MICROSOFT_STORE_PAGE: &str = "https://apps.microsoft.com/detail/9P50L12VHF33";
/// TraceDraw's page in the Microsoft Store app, which Help > Updates opens
/// in a copy installed from the Store.
#[cfg(not(target_os = "android"))]
pub const STORE_PAGE: &str = "ms-windows-store://pdp/?productid=9P50L12VHF33";
/// TraceDraw's page on Google Play, which Help > Updates opens in the
/// Android app.
#[cfg(target_os = "android")]
pub const STORE_PAGE: &str = "https://play.google.com/store/apps/details?id=dev.tracedraw.app";
/// How often the check at start runs, seconds.
pub const DAILY: u64 = 24 * 60 * 60;

/// A release newer than this version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    /// The release's page (its notes).
    pub page: String,
    /// The download for this system, when the release has one.
    pub download: Option<String>,
    /// The release file that updates this copy in place (the AppImage,
    /// the Windows setup program or the macOS disk image), with its
    /// digest.
    pub installer: Option<Package>,
}

/// A release file that can be installed in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub url: String,
    /// Lower-case hex.
    pub sha256: String,
    pub size: u64,
}

/// Where a check stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    /// This version is the latest.
    Latest,
    Available(Release),
    Failed(String),
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// "sha256:<hex>" (GitHub computes it for every upload).
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// `1.2.3` from `v1.2.3` (pre-release parts after `-` are dropped).
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>());
    let major = it.next()?.ok()?;
    let minor = it.next().unwrap_or(Ok(0)).ok()?;
    let patch = it.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

/// Whether `tag` names a version later than `current`.
pub fn is_newer(tag: &str, current: &str) -> bool {
    match (parse_version(tag), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// How this copy of TraceDraw was installed, which decides the release
/// file that updates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Linux, run as an AppImage (`APPIMAGE` names its file).
    AppImage,
    /// Linux, installed from the release's `.deb` (dpkg lists the program).
    Deb,
    /// Linux, the program unpacked from the release's tar.gz.
    Tarball,
    /// Windows, installed with the release's setup program.
    Setup,
    /// Windows, the portable zip (the settings folder next to the program).
    Portable,
    /// macOS, the disk image's app.
    Dmg,
    /// Anything the release has no file for (an rpm or other distribution
    /// package, a sandboxed copy, the browser): no download is offered.
    Other,
}

/// The release's architecture names for `arch` (as
/// `std::env::consts::ARCH`): the one in the archive, AppImage, disk
/// image and Windows names, and the Debian one. `None` for an
/// architecture the release has no build of.
fn arch_names(arch: &str) -> Option<(&'static str, &'static str)> {
    match arch {
        "x86_64" => Some(("x86_64", "amd64")),
        "aarch64" => Some(("arm64", "arm64")),
        // 32-bit Windows (#1975); no other system has a 32-bit build.
        "x86" => Some(("x86", "i386")),
        _ => None,
    }
}

/// The exact names of the release file of `version` that updates a copy
/// installed as `kind` on `os` (as `std::env::consts::OS`) and `arch`,
/// the preferred one first. Empty when the release builds no such file.
/// The names are the ones the release workflow gives its files. The
/// x86_64 AppImage is `linux-amd64` from 0.19.9 on, so that it sorts
/// before the arm64 one for updaters of 0.19.8 and earlier, which take
/// the first AppImage listed (#1038); older releases name it `linux-x86_64`.
/// 32-bit Windows (`x86`) has its own `windows-x86` files (#1975).
pub fn asset_names(version: &str, os: &str, arch: &str, kind: Kind) -> Vec<String> {
    let Some((a, deb)) = arch_names(arch) else {
        return Vec::new();
    };
    if arch == "x86" && os != "windows" {
        return Vec::new();
    }
    let one = match (os, kind) {
        ("linux", Kind::AppImage) if arch == "x86_64" => {
            return vec![
                format!("tracedraw-{version}-linux-amd64.AppImage"),
                format!("tracedraw-{version}-linux-x86_64.AppImage"),
            ];
        }
        ("linux", Kind::AppImage) => format!("tracedraw-{version}-linux-{a}.AppImage"),
        ("linux", Kind::Tarball) => format!("tracedraw-{version}-linux-{a}.tar.gz"),
        ("linux", Kind::Deb) => format!("tracedraw_{version}_{deb}.deb"),
        ("windows", Kind::Setup) => format!("tracedraw-{version}-windows-{a}-setup.exe"),
        ("windows", Kind::Portable) => format!("tracedraw-{version}-windows-{a}-portable.zip"),
        ("macos", Kind::Dmg) => format!("tracedraw-{version}-macos-{a}.dmg"),
        _ => return Vec::new(),
    };
    vec![one]
}

/// How a Linux copy was installed: run as an AppImage, inside a sandbox
/// (Flatpak, Snap), from the `.deb` (dpkg's file list for the package
/// names `exe`), from another system package (a program under `/usr`
/// that dpkg does not list, such as an rpm), else unpacked from the
/// tar.gz.
pub fn linux_kind(
    appimage: bool,
    sandboxed: bool,
    exe: &std::path::Path,
    dpkg_list: Option<&str>,
) -> Kind {
    if appimage {
        return Kind::AppImage;
    }
    if sandboxed {
        return Kind::Other;
    }
    let listed = dpkg_list.is_some_and(|l| l.lines().any(|line| std::path::Path::new(line) == exe));
    if listed {
        Kind::Deb
    } else if exe.starts_with("/usr") {
        Kind::Other
    } else {
        Kind::Tarball
    }
}

/// Whether this copy runs from the Microsoft Store's package (MSIX): it
/// then has a package identity. The Store updates such a copy itself and
/// its policy (10.2.10.1) refuses an app that offers downloads of code, so
/// it never asks GitHub for a release.
#[cfg(windows)]
pub fn store_install() -> bool {
    static STORE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STORE.get_or_init(|| {
        // 32-bit Windows: the DLL exports the plain name, not the stdcall
        // decorated one raw-dylib imports by default (#1975).
        #[cfg_attr(
            target_arch = "x86",
            link(
                name = "kernel32",
                kind = "raw-dylib",
                import_name_type = "undecorated"
            )
        )]
        #[cfg_attr(not(target_arch = "x86"), link(name = "kernel32", kind = "raw-dylib"))]
        unsafe extern "system" {
            fn GetCurrentPackageFullName(length: *mut u32, name: *mut u16) -> i32;
        }
        // A packaged process answers that the empty buffer is too small;
        // one without a package answers APPMODEL_ERROR_NO_PACKAGE (15700).
        const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
        let mut length = 0u32;
        // SAFETY: with a zero length and no buffer the call only reports
        // the length the name needs, or that the process has no package; it
        // writes nothing but `length`.
        let status = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
        status == ERROR_INSUFFICIENT_BUFFER
    })
}

/// The Android app is updated by Google Play, whose policy refuses an app
/// that downloads code of its own: it never asks GitHub for a release and
/// never asks at its first start whether to.
#[cfg(target_os = "android")]
pub fn store_install() -> bool {
    true
}

#[cfg(not(any(windows, target_os = "android")))]
pub fn store_install() -> bool {
    false
}

/// How the running copy was installed.
#[cfg(not(target_arch = "wasm32"))]
pub fn current_kind() -> Kind {
    let exe = std::env::current_exe().ok();
    match std::env::consts::OS {
        "linux" => {
            let Some(exe) = exe else {
                return Kind::Other;
            };
            let dpkg = std::fs::read_to_string("/var/lib/dpkg/info/tracedraw.list").ok();
            linux_kind(
                std::env::var_os("APPIMAGE").is_some(),
                std::env::var_os("FLATPAK_ID").is_some() || std::env::var_os("SNAP").is_some(),
                &exe,
                dpkg.as_deref(),
            )
        }
        "windows" => match exe.as_deref().and_then(crate::settings::portable_dir) {
            Some(_) => Kind::Portable,
            None => Kind::Setup,
        },
        "macos" => Kind::Dmg,
        _ => Kind::Other,
    }
}

/// Read GitHub's answer: the release when it is newer than `current`
/// (drafts and pre-releases never count), `None` when this version is the
/// latest. The download is the release file named exactly for this
/// system, its processor and how this copy was installed (`kind`); when
/// the release has no such file there is none, never a file built for
/// another processor or package.
pub fn parse_latest(
    json: &str,
    current: &str,
    os: &str,
    arch: &str,
    kind: Kind,
) -> Result<Option<Release>, String> {
    let r: ApiRelease = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if r.draft || r.prerelease || !is_newer(&r.tag_name, current) {
        return Ok(None);
    }
    let version = r.tag_name.trim_start_matches(['v', 'V']).to_string();
    let find = |kind: Kind| {
        asset_names(&version, os, arch, kind)
            .into_iter()
            .find_map(|name| r.assets.iter().find(|a| a.name == name))
    };
    let download = find(kind).map(|a| a.browser_download_url.clone());
    let installer = if kind != Kind::Other {
        find(kind).and_then(|a| {
            let hex = a.digest.as_deref()?.strip_prefix("sha256:")?;
            (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) && a.size > 0).then(
                || Package {
                    url: a.browser_download_url.clone(),
                    sha256: hex.to_ascii_lowercase(),
                    size: a.size,
                },
            )
        })
    } else {
        None
    };
    Ok(Some(Release {
        version,
        page: r.html_url,
        download,
        installer,
    }))
}

/// A check running in the background; its result lands in `state`.
#[derive(Debug, Clone, Default)]
pub struct Checker {
    pub state: Arc<Mutex<UpdateState>>,
}

impl Checker {
    pub fn get(&self) -> UpdateState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Start a check unless one is running; `ctx` gets a frame when it
    /// ends. In a browser the check is a fetch of the same address.
    pub fn start(&self, ctx: Option<egui::Context>) {
        if let Ok(mut s) = self.state.lock() {
            if *s == UpdateState::Checking {
                return;
            }
            *s = UpdateState::Checking;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let state = self.state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = crate::net::HttpRequest {
                    method: "GET",
                    url: LATEST_URL.into(),
                    headers: vec![("Accept".into(), "application/vnd.github+json".into())],
                    body: String::new(),
                    bytes: Vec::new(),
                };
                let next = match crate::net::send(req).await {
                    Ok(r) if r.status == 200 => {
                        match parse_latest(
                            &r.body,
                            env!("CARGO_PKG_VERSION"),
                            "web",
                            "wasm32",
                            Kind::Other,
                        ) {
                            Ok(Some(r)) => UpdateState::Available(r),
                            Ok(None) => UpdateState::Latest,
                            Err(e) => UpdateState::Failed(e),
                        }
                    }
                    Ok(r) => UpdateState::Failed(format!("GitHub answered {}", r.status)),
                    Err(e) => UpdateState::Failed(e),
                };
                if let Ok(mut s) = state.lock() {
                    *s = next;
                }
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
                crate::web::request_repaint();
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let state = self.state.clone();
            std::thread::spawn(move || {
                let result = fetch().and_then(|json| {
                    parse_latest(
                        &json,
                        env!("CARGO_PKG_VERSION"),
                        std::env::consts::OS,
                        std::env::consts::ARCH,
                        current_kind(),
                    )
                });
                let next = match result {
                    Ok(Some(r)) => UpdateState::Available(r),
                    Ok(None) => UpdateState::Latest,
                    Err(e) => UpdateState::Failed(e),
                };
                if let Ok(mut s) = state.lock() {
                    *s = next;
                }
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            });
        }
    }
}

/// Where an in-place install goes: the running AppImage's file (Linux,
/// `APPIMAGE` set by the AppImage runtime).
pub fn appimage_target() -> Option<std::path::PathBuf> {
    if std::env::consts::OS != "linux" || crate::files::WEB {
        return None;
    }
    let p = std::path::PathBuf::from(std::env::var_os("APPIMAGE")?);
    p.is_file().then_some(p)
}

/// What an update replaces, which decides how the release file goes in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallTarget {
    /// The running AppImage's file: the new one is moved over it.
    AppImage(std::path::PathBuf),
    /// A Windows copy installed with setup: the release's setup program
    /// runs over it (same install folder, settings and file associations
    /// kept) once TraceDraw has closed.
    Setup,
    /// The running macOS app bundle: replaced by the one on the release's
    /// disk image.
    Bundle(std::path::PathBuf),
    /// A Linux copy unpacked from the tar.gz: the program (path of the
    /// running file) and the command line tool beside it are replaced by
    /// the archive's.
    Tarball(std::path::PathBuf),
    /// The Windows portable copy (path of the running file): the zip's
    /// top-level files replace the ones beside it; the settings folder is
    /// never touched.
    Portable(std::path::PathBuf),
    /// A Linux copy installed from the `.deb` (path of the program): the
    /// package is installed with `pkexec dpkg -i`, which asks for the
    /// administrator's password, once TraceDraw has closed.
    Deb(std::path::PathBuf),
}

/// What the running copy can update itself with, `None` for a copy that
/// only gets Download (a system package other than the `.deb`, a
/// sandbox, a `.deb` without `pkexec` to ask for the password, the
/// browser). Worked out once, before any update has replaced the
/// program's file: afterwards the system no longer names it.
#[cfg(not(target_arch = "wasm32"))]
pub fn install_target() -> Option<InstallTarget> {
    static TARGET: std::sync::OnceLock<Option<InstallTarget>> = std::sync::OnceLock::new();
    TARGET
        .get_or_init(|| {
            let exe = std::env::current_exe().ok();
            match current_kind() {
                Kind::AppImage => appimage_target().map(InstallTarget::AppImage),
                Kind::Setup => Some(InstallTarget::Setup),
                Kind::Portable => exe.map(InstallTarget::Portable),
                Kind::Dmg => exe
                    .and_then(|exe| app_bundle_of(&exe))
                    .map(InstallTarget::Bundle),
                Kind::Tarball => exe.map(InstallTarget::Tarball),
                Kind::Deb => exe.filter(|_| on_path("pkexec")).map(InstallTarget::Deb),
                Kind::Other => None,
            }
        })
        .clone()
}

#[cfg(target_arch = "wasm32")]
pub fn install_target() -> Option<InstallTarget> {
    None
}

/// Whether the program `name` is in a folder of `PATH`.
#[cfg(not(target_arch = "wasm32"))]
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// The `.app` folder that holds the program `exe`
/// (`X.app/Contents/MacOS/<program>`), `None` for a program outside a
/// bundle (a build run from the source tree).
pub fn app_bundle_of(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let named = |p: &std::path::Path, name: &str| p.file_name().is_some_and(|n| n == name);
    (named(macos, "MacOS")
        && named(contents, "Contents")
        && bundle.extension().is_some_and(|e| e == "app"))
    .then(|| bundle.to_path_buf())
}

/// The first `.app` folder inside `dir` (the mounted disk image).
fn find_app(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut apps: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.extension().is_some_and(|e| e == "app"))
        .collect();
    apps.sort();
    apps.into_iter().next()
}

/// `.<name><suffix>` next to `path`: a hidden neighbour on the same
/// volume, so that moving it over `path` is one rename.
fn neighbour(path: &std::path::Path, suffix: &str) -> Option<std::path::PathBuf> {
    let name = path.file_name()?.to_string_lossy();
    Some(path.with_file_name(format!(".{name}{suffix}")))
}

/// Put the folder `fresh` in place of `bundle`. The old bundle moves
/// aside first and comes back if the new one cannot be moved in, so the
/// installed copy is never left without an app.
fn swap_bundle(fresh: &std::path::Path, bundle: &std::path::Path) -> Result<(), String> {
    let old = neighbour(bundle, ".old").ok_or("no bundle name")?;
    let _ = std::fs::remove_dir_all(&old);
    if let Err(e) = std::fs::rename(bundle, &old) {
        let _ = std::fs::remove_dir_all(fresh);
        return Err(e.to_string());
    }
    if let Err(e) = std::fs::rename(fresh, bundle) {
        let _ = std::fs::rename(&old, bundle);
        let _ = std::fs::remove_dir_all(fresh);
        return Err(e.to_string());
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// A program to start when TraceDraw has exited after Restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
}

/// Starts the new AppImage.
fn appimage_launch(appimage: &std::path::Path) -> Launch {
    Launch {
        program: appimage.display().to_string(),
        args: Vec::new(),
    }
}

/// Starts the release's setup program through the shell (it asks for
/// the administrator's approval when the copy was installed for all
/// users, which a program started directly cannot do). The setup runs
/// with a progress window and no questions, closes nothing that is open
/// (TraceDraw has exited), never restarts the computer and, through
/// `/relaunch=1`, starts the new TraceDraw when it is done (the
/// `[Run]` entry of `packaging/windows/tracedraw.iss`).
fn setup_launch(setup: &std::path::Path) -> Launch {
    Launch {
        program: "cmd".into(),
        args: [
            "/C",
            "start",
            "",
            &setup.display().to_string(),
            "/SILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/relaunch=1",
        ]
        .map(String::from)
        .to_vec(),
    }
}

/// Starts the program `exe` again.
fn exe_launch(exe: &std::path::Path) -> Launch {
    Launch {
        program: exe.display().to_string(),
        args: Vec::new(),
    }
}

/// Installs the package `deb` as the administrator (`pkexec` asks for the
/// password), removes it and starts the program `exe`, whether the
/// install worked or not: a refused password leaves the old version,
/// which then starts.
fn deb_launch(deb: &std::path::Path, exe: &std::path::Path) -> Launch {
    Launch {
        program: "sh".into(),
        args: vec![
            "-c".into(),
            "pkexec dpkg -i \"$1\"; rm -f \"$1\"; exec \"$2\"".into(),
            "sh".into(),
            deb.display().to_string(),
            exe.display().to_string(),
        ],
    }
}

/// Opens a new instance of the app bundle `bundle`.
fn bundle_launch(bundle: &std::path::Path) -> Launch {
    Launch {
        program: "open".into(),
        args: vec!["-n".into(), bundle.display().to_string()],
    }
}

/// Where an in-place install stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InstallState {
    #[default]
    Idle,
    Downloading {
        done: u64,
        total: u64,
    },
    /// The whole file is in: its size and SHA-256 digest are checked.
    Checking,
    /// The checked file is being put in place (unpacked, copied or
    /// moved over the old one). It cannot be cancelled any more.
    Installing,
    /// The new version is in place; Restart starts it, otherwise it runs
    /// from the next start.
    Installed(String),
    /// The setup program of the new version is downloaded and checked;
    /// Restart closes TraceDraw and runs it.
    Ready(String),
    Failed(String),
}

impl InstallState {
    /// An install is running: downloading, checking or installing.
    pub fn busy(&self) -> bool {
        matches!(
            self,
            InstallState::Downloading { .. } | InstallState::Checking | InstallState::Installing
        )
    }
}

/// How far an install got, as it tells the one who started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// This many bytes of the file are written.
    Bytes(u64),
    /// The whole file is in; its digest is compared.
    Checking,
    /// The file is put in place.
    Installing,
}

/// What an install reports its steps to. An error stops the install
/// there (Cancel), and the partial files are removed.
pub type Report<'a> = &'a mut dyn FnMut(Step) -> Result<(), String>;

/// The error of a download stopped by Cancel.
pub const CANCELLED: &str = "cancelled";

/// Download `from` into `part`, checking its size and SHA-256 digest
/// against `package`; `progress` gets the bytes written so far, then
/// [`Step::Checking`]. The partial file is left for the caller to move
/// or remove.
fn write_verified(
    from: &mut dyn std::io::Read,
    package: &Package,
    part: &std::path::Path,
    progress: Report,
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    let mut out = std::fs::File::create(part).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = from.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > package.size {
            return Err("the download is larger than the release says".to_string());
        }
        hash.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        progress(Step::Bytes(done))?;
    }
    progress(Step::Checking)?;
    if done != package.size {
        return Err(format!(
            "the download stopped at {done} of {} bytes",
            package.size
        ));
    }
    let got: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != package.sha256 {
        return Err("the download does not match the release's digest".to_string());
    }
    out.sync_all().map_err(|e| e.to_string())
}

/// Write `from` next to `target`, check its size and SHA-256 digest, give
/// it the target's permissions and move it over the target. On any
/// failure the target is left as it was and the partial file removed.
/// `progress` gets the bytes written so far and the steps after them.
pub fn install_file(
    from: &mut dyn std::io::Read,
    package: &Package,
    target: &std::path::Path,
    progress: Report,
) -> Result<(), String> {
    let part = neighbour(target, ".update").ok_or("no file name")?;
    let result = (|| {
        write_verified(from, package, &part, progress)?;
        progress(Step::Installing)?;
        let perms = std::fs::metadata(target)
            .map_err(|e| e.to_string())?
            .permissions();
        std::fs::set_permissions(&part, perms).map_err(|e| e.to_string())?;
        std::fs::rename(&part, target).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// Write `from` to the new file `dest`, checked as [`install_file`]
/// checks; on any failure nothing is left at `dest`.
pub fn save_verified(
    from: &mut dyn std::io::Read,
    package: &Package,
    dest: &std::path::Path,
    progress: Report,
) -> Result<(), String> {
    let part = neighbour(dest, ".part").ok_or("no file name")?;
    let result = write_verified(from, package, &part, progress)
        .and_then(|()| std::fs::rename(&part, dest).map_err(|e| e.to_string()));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// A file name made of the version's letters, digits, dots and dashes.
fn safe_version(version: &str) -> String {
    version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Run `program` and fail with what it printed when it does not succeed.
#[cfg(not(target_arch = "wasm32"))]
fn run_tool(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    Err(format!("{program}: {}", said.trim()))
}

/// Put the app on the disk image `dmg` in place of `bundle`: mount the
/// image read-only without showing it, copy its app next to the bundle
/// (keeping its signature and attributes), swap the two and unmount.
#[cfg(not(target_arch = "wasm32"))]
fn install_bundle(
    dmg: &std::path::Path,
    mount: &std::path::Path,
    bundle: &std::path::Path,
) -> Result<(), String> {
    use std::ffi::OsStr;
    let _ = std::fs::remove_dir_all(mount);
    std::fs::create_dir_all(mount).map_err(|e| e.to_string())?;
    run_tool(
        "hdiutil",
        &[
            OsStr::new("attach"),
            OsStr::new("-nobrowse"),
            OsStr::new("-readonly"),
            OsStr::new("-noautoopen"),
            OsStr::new("-mountpoint"),
            mount.as_os_str(),
            dmg.as_os_str(),
        ],
    )?;
    let result = (|| {
        let app = find_app(mount).ok_or("the disk image holds no app")?;
        let fresh = neighbour(bundle, ".update").ok_or("no bundle name")?;
        let _ = std::fs::remove_dir_all(&fresh);
        run_tool("ditto", &[app.as_os_str(), fresh.as_os_str()]).inspect_err(|_| {
            let _ = std::fs::remove_dir_all(&fresh);
        })?;
        swap_bundle(&fresh, bundle)
    })();
    let _ = run_tool(
        "hdiutil",
        &[
            OsStr::new("detach"),
            mount.as_os_str(),
            OsStr::new("-force"),
        ],
    );
    result
}

/// The folder the release files of an update are downloaded to, new and
/// empty. On Linux it is the user's cache folder, closed to everyone
/// else (mode 0700): a `.deb` waits there to be installed as the
/// administrator, and a shared `/tmp` would let another user swap the
/// file after its digest was checked.
#[cfg(not(target_arch = "wasm32"))]
fn staging_dir() -> Result<std::path::PathBuf, String> {
    let base = if cfg!(target_os = "linux") {
        let var = |name: &str| {
            std::env::var_os(name)
                .map(std::path::PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|home| home.join(".cache")))
            .ok_or("no cache folder")?
    } else {
        std::env::temp_dir()
    };
    let dir = base.join("tracedraw-update");
    let _ = std::fs::remove_dir_all(&dir);
    let mut made = std::fs::DirBuilder::new();
    made.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        made.mode(0o700);
    }
    made.create(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

const DAMAGED: &str = "the archive is damaged";
/// The largest file taken out of a release archive.
const MAX_UNPACKED: u64 = 1 << 30;

/// `.<name><suffix>` in `dir`: where the new copy of `name` is written.
fn hidden(dir: &std::path::Path, name: &str, suffix: &str) -> std::path::PathBuf {
    dir.join(format!(".{name}{suffix}"))
}

fn read_full(from: &mut dyn std::io::Read, buf: &mut [u8]) -> Result<usize, String> {
    let mut n = 0;
    while n < buf.len() {
        let k = from.read(&mut buf[n..]).map_err(|e| e.to_string())?;
        if k == 0 {
            break;
        }
        n += k;
    }
    Ok(n)
}

/// Read and drop `n` bytes.
fn skip_bytes(from: &mut dyn std::io::Read, n: u64) -> Result<(), String> {
    let done = std::io::copy(&mut std::io::Read::take(from, n), &mut std::io::sink())
        .map_err(|e| e.to_string())?;
    if done == n {
        Ok(())
    } else {
        Err(DAMAGED.into())
    }
}

/// A tar header's number: octal digits up to the first NUL or space.
fn octal(field: &[u8]) -> Result<u64, String> {
    let text: String = field
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| DAMAGED.to_string())
}

fn c_string(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// Remove the hidden new copies of `names` from `dir`.
fn discard(dir: &std::path::Path, names: &[String]) {
    for name in names {
        let _ = std::fs::remove_file(hidden(dir, name, ".update"));
    }
}

/// Make the new copy of an archive entry: `.<name>.update` in `dir`,
/// filled from `from` (`size` bytes).
fn write_entry(
    dir: &std::path::Path,
    name: &str,
    from: &mut dyn std::io::Read,
    size: u64,
    mode: u32,
) -> Result<(), String> {
    if size > MAX_UNPACKED {
        return Err(DAMAGED.into());
    }
    let part = hidden(dir, name, ".update");
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let copied =
        std::io::copy(&mut std::io::Read::take(from, size), &mut out).map_err(|e| e.to_string())?;
    if copied != size {
        return Err(DAMAGED.into());
    }
    out.sync_all().map_err(|e| e.to_string())?;
    drop(out);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode((mode & 0o777) | 0o600);
        std::fs::set_permissions(&part, perms).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

/// Write the regular files of the tar stream `tar` that `want` names
/// (by the entry's path; it answers the name to install it under) as
/// hidden new copies in `dir`; the names are returned. Directories,
/// links and the other entries are skipped. Nothing is left in `dir`
/// when it fails.
fn unpack_tar(
    tar: &mut dyn std::io::Read,
    dir: &std::path::Path,
    want: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<String>, String> {
    let mut made: Vec<String> = Vec::new();
    let result = (|| {
        let mut header = [0u8; 512];
        loop {
            if read_full(tar, &mut header)? < 512 || header.iter().all(|&b| b == 0) {
                return Ok(());
            }
            let sum: u64 = header
                .iter()
                .enumerate()
                .map(|(i, &b)| {
                    if (148..156).contains(&i) {
                        32
                    } else {
                        u64::from(b)
                    }
                })
                .sum();
            if octal(&header[148..156])? != sum {
                return Err(DAMAGED.to_string());
            }
            let size = octal(&header[124..136])?;
            let mode = octal(&header[100..108])? as u32;
            let mut path = c_string(&header[0..100]);
            if &header[257..262] == b"ustar" {
                let prefix = c_string(&header[345..500]);
                if !prefix.is_empty() {
                    path = format!("{prefix}/{path}");
                }
            }
            let padding = (512 - size % 512) % 512;
            let regular = matches!(header[156], b'0' | 0);
            let name = path.trim_start_matches("./");
            match want(name).filter(|_| regular) {
                Some(install_as) => {
                    made.push(install_as.clone());
                    write_entry(dir, &install_as, tar, size, mode)?;
                    skip_bytes(tar, padding)?;
                }
                None => skip_bytes(tar, size + padding)?,
            }
        }
    })();
    match result {
        Ok(()) => Ok(made),
        Err(e) => {
            discard(dir, &made);
            Err(e)
        }
    }
}

/// As [`unpack_tar`] for the files of the zip `archive`.
fn unpack_zip(
    archive: std::fs::File,
    dir: &std::path::Path,
    want: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<String>, String> {
    let mut zip = zip::ZipArchive::new(archive).map_err(|e| e.to_string())?;
    let mut made: Vec<String> = Vec::new();
    let result = (|| {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            if entry.is_dir() {
                continue;
            }
            // A name that does not decode is not one this update installs.
            let install_as = match entry.name() {
                Ok(name) => want(&name),
                Err(_) => None,
            };
            let Some(install_as) = install_as else {
                continue;
            };
            made.push(install_as.clone());
            let size = entry.size();
            write_entry(dir, &install_as, &mut entry, size, 0o755)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(made),
        Err(e) => {
            discard(dir, &made);
            Err(e)
        }
    }
}

/// Move the new copy of `target` (`part`) over it. With `aside` the
/// old file is renamed away first and renamed back if the new one cannot
/// be moved in: Windows refuses to overwrite a running program but lets
/// it be renamed. Returns the old file that was set aside.
fn replace_file(
    part: &std::path::Path,
    target: &std::path::Path,
    aside: bool,
) -> Result<Option<std::path::PathBuf>, String> {
    if !(aside && target.exists()) {
        std::fs::rename(part, target).map_err(|e| e.to_string())?;
        return Ok(None);
    }
    let old = neighbour(target, ".old").ok_or("no file name")?;
    let _ = std::fs::remove_file(&old);
    std::fs::rename(target, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(part, target) {
        let _ = std::fs::rename(&old, target);
        return Err(e.to_string());
    }
    Ok(Some(old))
}

/// Put the hidden new copies of `names` in place in `dir`. The old
/// files that were set aside are removed when they can be (a running
/// program's cannot on Windows; [`clean_leftovers`] gets it next time).
fn swap_in(dir: &std::path::Path, names: &[String], aside: bool) -> Result<(), String> {
    let mut olds = Vec::new();
    for name in names {
        let step = replace_file(&hidden(dir, name, ".update"), &dir.join(name), aside);
        match step {
            Ok(old) => olds.extend(old),
            Err(e) => {
                discard(dir, names);
                return Err(e);
            }
        }
    }
    for old in olds {
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}

/// Remove what an earlier update left beside the running program: the
/// old program a Windows update set aside while it ran (only Windows
/// sets one aside, so elsewhere the folder is not even read).
pub fn clean_leftovers() {
    if !cfg!(windows) {
        return;
    }
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
    else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(".tracedraw") && name.ends_with(".old") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The name a Linux tar.gz entry is installed under beside the running
/// program `main` (which may have been renamed): the program, and the
/// command line tool when it is installed there; nothing else.
fn tarball_name(entry: &str, main: &str, dir: &std::path::Path) -> Option<String> {
    match entry {
        "tracedraw" => Some(main.to_string()),
        "tracedraw-cli" if dir.join(entry).is_file() => Some(entry.to_string()),
        _ => None,
    }
}

/// The name a portable zip's entry is installed under: files at the top
/// level of the zip only, so never the settings folder.
fn portable_name(entry: &str, main: &str) -> Option<String> {
    let plain = !entry.is_empty() && !entry.starts_with('.') && !entry.contains(['/', '\\', ':']);
    plain.then(|| {
        if entry == "tracedraw.exe" {
            main.to_string()
        } else {
            entry.to_string()
        }
    })
}

/// Replace the program `exe` and its tool by the ones in the tar.gz
/// `archive`.
fn install_tarball(archive: &std::path::Path, exe: &std::path::Path) -> Result<(), String> {
    let dir = exe.parent().ok_or("no program folder")?;
    let main = exe
        .file_name()
        .ok_or("no program name")?
        .to_string_lossy()
        .to_string();
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    let names = unpack_tar(&mut tar, dir, &|e| tarball_name(e, &main, dir))?;
    if !names.contains(&main) {
        discard(dir, &names);
        return Err("the archive holds no program".into());
    }
    swap_in(dir, &names, false)
}

/// Replace the portable program `exe` and the files beside it by the
/// ones in the zip `archive`.
fn install_zip(archive: &std::path::Path, exe: &std::path::Path) -> Result<(), String> {
    let dir = exe.parent().ok_or("no program folder")?;
    let main = exe
        .file_name()
        .ok_or("no program name")?
        .to_string_lossy()
        .to_string();
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let names = unpack_zip(file, dir, &|e| portable_name(e, &main))?;
    if !names.contains(&main) {
        discard(dir, &names);
        return Err("the archive holds no program".into());
    }
    swap_in(dir, &names, cfg!(windows))
}

/// Download `package` and bring it to the state `target` needs, and say
/// what to start after TraceDraw exits and whether the update is already
/// in place (`false`: it is only downloaded, the system's installer
/// applies it).
#[cfg(not(target_arch = "wasm32"))]
fn stage(
    version: &str,
    package: &Package,
    target: &InstallTarget,
    body: &mut dyn std::io::Read,
    progress: Report,
) -> Result<(Launch, bool), String> {
    match target {
        InstallTarget::AppImage(path) => {
            install_file(body, package, path, progress)?;
            Ok((appimage_launch(path), true))
        }
        InstallTarget::Setup => {
            let dir = staging_dir()?;
            let file = dir.join(format!("TraceDraw-{}-setup.exe", safe_version(version)));
            save_verified(body, package, &file, progress)?;
            Ok((setup_launch(&file), false))
        }
        InstallTarget::Deb(exe) => {
            let dir = staging_dir()?;
            let file = dir.join(format!("tracedraw_{}.deb", safe_version(version)));
            save_verified(body, package, &file, progress)?;
            Ok((deb_launch(&file, exe), false))
        }
        InstallTarget::Bundle(bundle) => {
            let dir = staging_dir()?;
            let dmg = dir.join("TraceDraw.dmg");
            save_verified(body, package, &dmg, progress)?;
            if let Err(e) = progress(Step::Installing) {
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
            let done = install_bundle(&dmg, &dir.join("mount"), bundle);
            let _ = std::fs::remove_dir_all(&dir);
            done?;
            Ok((bundle_launch(bundle), true))
        }
        InstallTarget::Tarball(exe) | InstallTarget::Portable(exe) => {
            let dir = staging_dir()?;
            let archive = dir.join("update.archive");
            save_verified(body, package, &archive, progress)?;
            if let Err(e) = progress(Step::Installing) {
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
            let done = if matches!(target, InstallTarget::Tarball(_)) {
                install_tarball(&archive, exe)
            } else {
                install_zip(&archive, exe)
            };
            let _ = std::fs::remove_dir_all(&dir);
            done?;
            Ok((exe_launch(exe), true))
        }
    }
}

/// How long the speed of a download takes to follow a change, seconds
/// (the time constant of its exponential smoothing).
const SPEED_SMOOTHING: f64 = 2.5;
/// Samples of a download closer together than this are merged, seconds.
const SAMPLE_GAP: f64 = 0.25;
/// The time left is shown once the download has run this long, seconds
/// (before that the speed says little).
const LEFT_AFTER: f64 = 3.0;
/// A pause longer than this counts as nothing arriving, seconds.
const STALL: f64 = 1.0;

/// The speed of a download and the time it has left: the rate between
/// samples, smoothed exponentially so that the numbers shown do not
/// jump. Times are seconds since any fixed moment.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Meter {
    /// The first sample: time and bytes.
    first: Option<(f64, u64)>,
    /// The last sample taken into the speed.
    last: Option<(f64, u64)>,
    /// Bytes per second, smoothed.
    speed: Option<f64>,
    /// Samples taken into the speed.
    samples: u32,
}

/// The weight of a new rate measured over `dt` seconds.
fn smoothing(dt: f64) -> f64 {
    1.0 - (-dt / SPEED_SMOOTHING).exp()
}

impl Meter {
    /// `done` bytes are in at time `t`.
    pub fn record(&mut self, t: f64, done: u64) {
        let Some((t0, d0)) = self.last else {
            self.first = Some((t, done));
            self.last = Some((t, done));
            return;
        };
        let dt = t - t0;
        if dt < SAMPLE_GAP {
            return;
        }
        let rate = done.saturating_sub(d0) as f64 / dt;
        self.speed = Some(match self.speed {
            None => rate,
            Some(s) => s + smoothing(dt) * (rate - s),
        });
        self.last = Some((t, done));
        self.samples += 1;
    }

    /// The smoothed speed at time `t`, bytes per second: a download that
    /// has stalled slows down on screen too.
    pub fn speed_at(&self, t: f64) -> Option<f64> {
        let s = self.speed?;
        let (t0, _) = self.last?;
        let quiet = t - t0 - STALL;
        Some(if quiet > 0.0 {
            s * (1.0 - smoothing(quiet))
        } else {
            s
        })
    }

    /// The seconds left at time `t` with `done` of `total` bytes in, once
    /// that means something: after [`LEFT_AFTER`] seconds and a few
    /// samples, at a speed above nothing.
    pub fn left_at(&self, t: f64, done: u64, total: u64) -> Option<f64> {
        let (t_first, _) = self.first?;
        if t - t_first < LEFT_AFTER || self.samples < 3 {
            return None;
        }
        let s = self.speed_at(t)?;
        if s < 1.0 {
            return None;
        }
        let left = total.saturating_sub(done) as f64 / s;
        // Days left is no estimate at all.
        (left < 24.0 * 3600.0).then_some(left)
    }
}

/// The share of a download done, 0 to 1.
pub fn fraction(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done.min(total) as f64 / total as f64) as f32
    }
}

/// The whole percent of a download done, rounded down: 100 only when
/// every byte is in.
pub fn percent(done: u64, total: u64) -> u32 {
    if total == 0 {
        0
    } else {
        (u128::from(done.min(total)) * 100 / u128::from(total)) as u32
    }
}

/// A download as the dialog shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
    /// Bytes per second, smoothed; `None` until it is measured.
    pub speed: Option<f64>,
    /// Seconds left, once that means something.
    pub left: Option<f64>,
}

/// Why an install failed, in the words the dialog uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trouble {
    /// The server could not be reached or the connection broke.
    Network,
    /// The file came in short, long or with another digest.
    Damaged,
    /// The folder of the program cannot be written.
    Permission,
    /// The disk is full.
    Space,
    Other,
}

/// What kind of trouble the install error `error` is.
pub fn trouble(error: &str) -> Trouble {
    let e = error.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| e.contains(w));
    if any(&["digest", "larger than", "damaged"]) {
        Trouble::Damaged
    } else if any(&[
        "no space",
        "not enough space",
        "disk full",
        "os error 28)",
        "os error 112)",
    ]) {
        Trouble::Space
    } else if any(&[
        "permission denied",
        "access is denied",
        "read-only",
        "os error 13)",
    ]) {
        Trouble::Permission
    } else if any(&[
        "answered",
        "redirect",
        "connect",
        "dns",
        "resolve",
        "timed out",
        "timeout",
        "network",
        "certificate",
        "tls",
        "the download stopped",
    ]) {
        Trouble::Network
    } else {
        Trouble::Other
    }
}

/// A download that Cancel stops at once: the body is read on a thread of
/// its own, so that a connection that hangs does not hold the install.
/// Every read checks the flag, and waits for data a tenth of a second at
/// a time.
#[cfg(not(target_arch = "wasm32"))]
struct Cancellable {
    chunks: std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>,
    chunk: Vec<u8>,
    at: usize,
    ended: bool,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Cancellable {
    fn new(
        mut body: Box<dyn std::io::Read + Send>,
        stop: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let (send, chunks) = std::sync::mpsc::sync_channel(4);
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match body.read(&mut buf) {
                    Ok(0) => {
                        let _ = send.send(Ok(Vec::new()));
                        return;
                    }
                    Ok(n) => {
                        // Nobody reads any more: the download was cancelled.
                        if send.send(Ok(buf[..n].to_vec())).is_err() {
                            return;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        let _ = send.send(Err(e));
                        return;
                    }
                }
            }
        });
        Cancellable {
            chunks,
            chunk: Vec::new(),
            at: 0,
            ended: false,
            stop,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::io::Read for Cancellable {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        use std::sync::mpsc::RecvTimeoutError;
        loop {
            if self.stop.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(std::io::Error::other(CANCELLED));
            }
            if self.at < self.chunk.len() {
                let n = out.len().min(self.chunk.len() - self.at);
                out[..n].copy_from_slice(&self.chunk[self.at..self.at + n]);
                self.at += n;
                return Ok(n);
            }
            if self.ended {
                return Ok(0);
            }
            match self
                .chunks
                .recv_timeout(std::time::Duration::from_millis(100))
            {
                Ok(Ok(chunk)) => {
                    self.ended = chunk.is_empty();
                    self.chunk = chunk;
                    self.at = 0;
                }
                Ok(Err(e)) => return Err(e),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(std::io::Error::other("the download stopped"));
                }
            }
        }
    }
}

/// How often a running download tells the window how far it is.
#[cfg(not(target_arch = "wasm32"))]
const REPORT_EVERY: std::time::Duration = std::time::Duration::from_millis(150);

/// The speed of the running download, with the moment it started.
#[derive(Debug, Clone, Copy)]
struct Transfer {
    started: web_time::Instant,
    meter: Meter,
}

impl Transfer {
    fn new() -> Self {
        Transfer {
            started: web_time::Instant::now(),
            meter: Meter::default(),
        }
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}

/// Opens the download of a release file.
#[cfg(not(target_arch = "wasm32"))]
type Opener = Box<dyn FnOnce(&str) -> Result<Box<dyn std::io::Read + Send>, String> + Send>;

/// An in-place install running in the background.
#[derive(Debug, Clone, Default)]
pub struct Installer {
    pub state: Arc<Mutex<InstallState>>,
    /// What to start after the update, set when it is in place or ready.
    launch: Arc<Mutex<Option<Launch>>>,
    /// Restart was chosen: [`Installer::run_pending`] starts `launch`.
    armed: Arc<std::sync::atomic::AtomicBool>,
    /// The Cancel flag of the running install (each start has its own).
    stop: Arc<Mutex<Arc<std::sync::atomic::AtomicBool>>>,
    /// The thread of the last install, which a new one waits for: a
    /// cancelled download may still be removing its files.
    worker: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    /// The speed of the running download.
    transfer: Arc<Mutex<Option<Transfer>>>,
}

impl Installer {
    pub fn get(&self) -> InstallState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// The running download with its speed and time left; `None` when
    /// nothing downloads.
    pub fn progress(&self) -> Option<Progress> {
        let InstallState::Downloading { done, total } = self.get() else {
            return None;
        };
        let transfer = self.transfer.lock().ok().and_then(|t| *t);
        let (speed, left) = transfer
            .map(|t| {
                let now = t.now();
                (t.meter.speed_at(now), t.meter.left_at(now, done, total))
            })
            .unwrap_or((None, None));
        Some(Progress {
            done,
            total,
            speed,
            left,
        })
    }

    /// Cancel: stop the running download, remove what it wrote and go
    /// back to [`InstallState::Idle`] at once. `false` when nothing
    /// downloads (a file being checked or put in place is not stopped
    /// half way).
    pub fn cancel(&self) -> bool {
        let Ok(mut s) = self.state.lock() else {
            return false;
        };
        if !matches!(*s, InstallState::Downloading { .. }) {
            return false;
        }
        if let Ok(stop) = self.stop.lock() {
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        *s = InstallState::Idle;
        true
    }

    /// Wait for the install thread to end (tests).
    #[cfg(test)]
    pub fn wait(&self) {
        let worker = self.worker.lock().ok().and_then(|mut w| w.take());
        if let Some(w) = worker {
            let _ = w.join();
        }
    }

    /// Choose Restart: when TraceDraw exits, the new version starts (or,
    /// for the Windows setup, installs and starts). The caller then
    /// exits the usual way, asking about unsaved drawings. `false` when
    /// there is nothing to start.
    pub fn arm(&self) -> bool {
        let ready = self.launch.lock().is_ok_and(|l| l.is_some());
        self.armed.store(ready, std::sync::atomic::Ordering::SeqCst);
        ready
    }

    /// Restart was cancelled (a drawing's question was answered Cancel):
    /// nothing starts at exit.
    pub fn disarm(&self) {
        self.armed.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// An update in place with `launch` to start after it (for tests of
    /// what Restart does).
    #[cfg(test)]
    pub fn staged_for_test(&self, launch: Launch) {
        if let Ok(mut l) = self.launch.lock() {
            *l = Some(launch);
        }
    }

    /// Whether Restart was chosen and the program has not exited yet.
    #[cfg(test)]
    pub fn is_armed(&self) -> bool {
        self.armed.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// At exit: start what Restart chose, if anything.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_pending(&self) {
        if !self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let Some(launch) = self.launch.lock().ok().and_then(|mut l| l.take()) else {
            return;
        };
        let mut command = std::process::Command::new(&launch.program);
        command.args(&launch.args);
        // A new AppImage must not take the old one's mount for its own.
        for var in ["APPDIR", "ARGV0", "OWD"] {
            command.env_remove(var);
        }
        #[cfg(windows)]
        {
            // No console window flashes for the shell that starts setup.
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        if let Err(e) = command.spawn() {
            log::warn!("could not start {}: {e}", launch.program);
        }
    }

    /// Download `package` and put it in place of `target`, unless an
    /// install is running.
    pub fn start(
        &self,
        version: String,
        package: Package,
        target: InstallTarget,
        ctx: Option<egui::Context>,
    ) {
        #[cfg(target_arch = "wasm32")]
        let _ = (version, package, target, ctx);
        #[cfg(not(target_arch = "wasm32"))]
        self.start_from(version, package, target, ctx, Box::new(download));
    }

    /// [`Installer::start`] with the download opened by `open`.
    #[cfg(not(target_arch = "wasm32"))]
    fn start_from(
        &self,
        version: String,
        package: Package,
        target: InstallTarget,
        ctx: Option<egui::Context>,
        open: Opener,
    ) {
        use std::sync::atomic::{AtomicBool, Ordering};
        let stop = Arc::new(AtomicBool::new(false));
        {
            let Ok(mut s) = self.state.lock() else {
                return;
            };
            if s.busy() {
                return;
            }
            *s = InstallState::Downloading {
                done: 0,
                total: package.size,
            };
            if let Ok(mut flag) = self.stop.lock() {
                *flag = stop.clone();
            }
        }
        self.disarm();
        if let Ok(mut l) = self.launch.lock() {
            *l = None;
        }
        if let Ok(mut t) = self.transfer.lock() {
            *t = Some(Transfer::new());
        }
        let previous = self.worker.lock().ok().and_then(|mut w| w.take());
        let state = self.state.clone();
        let launch = self.launch.clone();
        let transfer = self.transfer.clone();
        let handle = std::thread::spawn(move || {
            if let Some(previous) = previous {
                let _ = previous.join();
            }
            // Every change goes in under the state's lock, unless Cancel
            // came first: then the state is the dialog's again.
            let set = |next: InstallState| -> Result<(), String> {
                {
                    let Ok(mut s) = state.lock() else {
                        return Err(CANCELLED.into());
                    };
                    if stop.load(Ordering::SeqCst) {
                        return Err(CANCELLED.into());
                    }
                    *s = next;
                }
                if let Some(ctx) = &ctx {
                    ctx.request_repaint();
                }
                Ok(())
            };
            // When the window last heard; the first block it hears of
            // at once.
            let mut told: Option<web_time::Instant> = None;
            let total = package.size;
            let mut report = |step: Step| -> Result<(), String> {
                match step {
                    Step::Bytes(done) => {
                        if let Ok(mut t) = transfer.lock() {
                            if let Some(t) = t.as_mut() {
                                let now = t.now();
                                t.meter.record(now, done);
                            }
                        }
                        // The window hears of it now and then, not for
                        // every block.
                        if told.is_none_or(|t| t.elapsed() >= REPORT_EVERY) || done == total {
                            told = Some(web_time::Instant::now());
                            set(InstallState::Downloading { done, total })
                        } else if stop.load(Ordering::SeqCst) {
                            Err(CANCELLED.into())
                        } else {
                            Ok(())
                        }
                    }
                    Step::Checking => set(InstallState::Checking),
                    Step::Installing => set(InstallState::Installing),
                }
            };
            let result = open(&package.url).and_then(|body| {
                let mut body = Cancellable::new(body, stop.clone());
                stage(&version, &package, &target, &mut body, &mut report)
            });
            let done = match result {
                Ok((next, in_place)) => {
                    if !stop.load(Ordering::SeqCst) {
                        if let Ok(mut l) = launch.lock() {
                            *l = Some(next);
                        }
                    }
                    if in_place {
                        InstallState::Installed(version)
                    } else {
                        InstallState::Ready(version)
                    }
                }
                Err(e) => InstallState::Failed(e),
            };
            // Cancelled: the dialog is back to Idle already.
            let _ = set(done);
        });
        if let Ok(mut w) = self.worker.lock() {
            *w = Some(handle);
        }
    }
}

/// A file of zeros of `left` bytes read at about `rate` bytes a second.
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
struct Trickle {
    left: u64,
    rate: u64,
    sent: u64,
    started: std::time::Instant,
}

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
impl std::io::Read for Trickle {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let due = (self.started.elapsed().as_secs_f64() * self.rate as f64) as u64;
        if self.sent >= due {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let n = (out.len() as u64).min(self.left).min(16 * 1024) as usize;
        out[..n].fill(0);
        self.left -= n as u64;
        self.sent += n as u64;
        Ok(n)
    }
}

/// A pretend update for looking at the progress dialog (debug builds
/// only, for the visual checks): with `TRACEDRAW_UPDATE_DEMO` set to a
/// speed in bytes a second, Help > Updates finds version 0.43.0 and
/// installs a 45 MB file of zeros, read at that speed, over a scratch
/// file in the temporary folder. Nothing else is touched. `false` when
/// the variable is not set.
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
pub fn demo(checker: &Checker, installer: &Installer, ctx: Option<egui::Context>) -> bool {
    use sha2::{Digest, Sha256};
    let Some(rate) = std::env::var("TRACEDRAW_UPDATE_DEMO")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|r| *r > 0)
    else {
        return false;
    };
    let size: u64 = 45 * 1024 * 1024 + 314_573;
    let mut hash = Sha256::new();
    let zeros = vec![0u8; 64 * 1024];
    let mut left = size;
    while left > 0 {
        let n = left.min(zeros.len() as u64) as usize;
        hash.update(&zeros[..n]);
        left -= n as u64;
    }
    let sha256: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let package = Package {
        url: "demo".into(),
        sha256,
        size,
    };
    let version = "0.43.0".to_string();
    if let Ok(mut s) = checker.state.lock() {
        *s = UpdateState::Available(Release {
            version: version.clone(),
            page: RELEASES_PAGE.into(),
            download: None,
            installer: Some(package.clone()),
        });
    }
    let dir = std::env::temp_dir().join("tracedraw-update-demo");
    let target = dir.join("TraceDraw.AppImage");
    if std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&target, b"old"))
        .is_err()
    {
        return false;
    }
    installer.start_from(
        version,
        package,
        InstallTarget::AppImage(target),
        ctx,
        Box::new(move |_| {
            Ok(Box::new(Trickle {
                left: size,
                rate,
                sent: 0,
                started: std::time::Instant::now(),
            }) as Box<dyn std::io::Read + Send>)
        }),
    );
    true
}

#[cfg(not(target_arch = "wasm32"))]
fn download(url: &str) -> Result<Box<dyn std::io::Read + Send>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(20)))
        .tls_config(crate::net::tls_config())
        .build()
        .into();
    // Redirects (GitHub sends downloads to its file host) are followed
    // here, each on a request of its own; the redirect's body is skipped.
    let mut url = url.to_string();
    for _ in 0..5 {
        let resp = agent
            .get(&url)
            .config()
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .header(
                "User-Agent",
                concat!("TraceDraw/", env!("CARGO_PKG_VERSION")),
            )
            .call()
            .map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        if (300..400).contains(&status) {
            let next = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or("a redirect without a location")?;
            url = next.to_string();
            continue;
        }
        if status != 200 {
            return Err(format!("the download answered {status}"));
        }
        return Ok(Box::new(resp.into_body().into_reader()));
    }
    Err("too many redirects".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .tls_config(crate::net::tls_config())
        .build()
        .into();
    let mut resp = agent
        .get(LATEST_URL)
        .header(
            "User-Agent",
            concat!("TraceDraw/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(1 << 20)
        .read_to_string()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The files of the v0.19.8 release, named and listed in the order
    /// GitHub gave them (the arm64 Linux files come first), with the
    /// version moved to 0.5.0. Each URL ends in the file's name.
    const SAMPLE: &str = r#"{
        "tag_name": "v0.5.0",
        "html_url": "https://github.com/tracedraw/tracedraw/releases/tag/v0.5.0",
        "prerelease": false,
        "draft": false,
        "assets": [
            {"name": "tracedraw-0.5.0-linux-arm64.AppImage", "browser_download_url": "https://x/tracedraw-0.5.0-linux-arm64.AppImage", "size": 7, "digest": "sha256:68e481c13fc5dc04ed6b106a9f801e1fa2689889f070a02b4fb89e8cb29c91ef"},
            {"name": "tracedraw-0.5.0-linux-arm64.tar.gz", "browser_download_url": "https://x/tracedraw-0.5.0-linux-arm64.tar.gz"},
            {"name": "tracedraw-0.5.0-linux-x86_64.AppImage", "browser_download_url": "https://x/tracedraw-0.5.0-linux-x86_64.AppImage", "size": 5, "digest": "sha256:2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824"},
            {"name": "tracedraw-0.5.0-linux-x86_64.tar.gz", "browser_download_url": "https://x/tracedraw-0.5.0-linux-x86_64.tar.gz"},
            {"name": "tracedraw-0.5.0-macos-arm64.dmg", "browser_download_url": "https://x/tracedraw-0.5.0-macos-arm64.dmg"},
            {"name": "tracedraw-0.5.0-macos-x86_64.dmg", "browser_download_url": "https://x/tracedraw-0.5.0-macos-x86_64.dmg"},
            {"name": "tracedraw-0.5.0-windows-x86_64-portable.zip", "browser_download_url": "https://x/tracedraw-0.5.0-windows-x86_64-portable.zip"},
            {"name": "tracedraw-0.5.0-windows-x86_64-setup.exe", "browser_download_url": "https://x/tracedraw-0.5.0-windows-x86_64-setup.exe"},
            {"name": "tracedraw_0.5.0_amd64.deb", "browser_download_url": "https://x/tracedraw_0.5.0_amd64.deb"},
            {"name": "tracedraw_0.5.0_arm64.deb", "browser_download_url": "https://x/tracedraw_0.5.0_arm64.deb"}
        ]
    }"#;

    fn pick_in(json: &str, os: &str, arch: &str, kind: Kind) -> Option<String> {
        parse_latest(json, "0.4.0", os, arch, kind)
            .expect("parses")
            .expect("newer")
            .download
            .map(|u| u.trim_start_matches("https://x/").to_string())
    }

    fn pick(os: &str, arch: &str, kind: Kind) -> Option<String> {
        pick_in(SAMPLE, os, arch, kind)
    }

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("v0.5.0", "0.4.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.99.0"));
        assert!(!is_newer("v0.4.0", "0.4.0"));
        assert!(!is_newer("v0.3.9", "0.4.0"));
        assert!(!is_newer("nightly", "0.4.0"));
        assert!(!is_newer("v0.5.0-rc1", "0.5.0"));
    }

    /// Every system, processor and kind of install gets exactly its own
    /// file of the release (#1038: an x86_64 AppImage took the arm64 one,
    /// listed first).
    #[test]
    fn the_download_fits_the_system() {
        use Kind::*;
        let cases = [
            (
                "linux",
                "x86_64",
                AppImage,
                "tracedraw-0.5.0-linux-x86_64.AppImage",
            ),
            (
                "linux",
                "aarch64",
                AppImage,
                "tracedraw-0.5.0-linux-arm64.AppImage",
            ),
            (
                "linux",
                "x86_64",
                Tarball,
                "tracedraw-0.5.0-linux-x86_64.tar.gz",
            ),
            (
                "linux",
                "aarch64",
                Tarball,
                "tracedraw-0.5.0-linux-arm64.tar.gz",
            ),
            ("linux", "x86_64", Deb, "tracedraw_0.5.0_amd64.deb"),
            ("linux", "aarch64", Deb, "tracedraw_0.5.0_arm64.deb"),
            (
                "windows",
                "x86_64",
                Setup,
                "tracedraw-0.5.0-windows-x86_64-setup.exe",
            ),
            (
                "windows",
                "x86_64",
                Portable,
                "tracedraw-0.5.0-windows-x86_64-portable.zip",
            ),
            ("macos", "aarch64", Dmg, "tracedraw-0.5.0-macos-arm64.dmg"),
            ("macos", "x86_64", Dmg, "tracedraw-0.5.0-macos-x86_64.dmg"),
        ];
        for (os, arch, kind, want) in cases {
            assert_eq!(
                pick(os, arch, kind).as_deref(),
                Some(want),
                "{os} {arch} {kind:?}"
            );
            // The file list's order does not matter.
            let reversed = {
                let mut v: serde_json::Value = serde_json::from_str(SAMPLE).expect("json");
                v["assets"].as_array_mut().expect("assets").reverse();
                v.to_string()
            };
            assert_eq!(pick_in(&reversed, os, arch, kind).as_deref(), Some(want));
        }
        let r = parse_latest(SAMPLE, "0.4.0", "linux", "x86_64", AppImage)
            .expect("parses")
            .expect("newer");
        assert_eq!(r.version, "0.5.0");
        assert!(r.page.ends_with("v0.5.0"));
    }

    /// #1975: 32-bit Windows (`x86`) has a setup and a portable zip of
    /// its own, `windows-x86`, which the 64-bit copies never take, nor it
    /// theirs; no other system has a 32-bit build.
    #[test]
    fn thirty_two_bit_windows_gets_its_own_files() {
        use Kind::*;
        let json = r#"{
            "tag_name": "v0.5.0",
            "html_url": "https://github.com/tracedraw/tracedraw/releases/tag/v0.5.0",
            "prerelease": false,
            "draft": false,
            "assets": [
                {"name": "tracedraw-0.5.0-windows-x86-portable.zip", "browser_download_url": "https://x/tracedraw-0.5.0-windows-x86-portable.zip"},
                {"name": "tracedraw-0.5.0-windows-x86-setup.exe", "browser_download_url": "https://x/tracedraw-0.5.0-windows-x86-setup.exe"},
                {"name": "tracedraw-0.5.0-windows-x86_64-portable.zip", "browser_download_url": "https://x/tracedraw-0.5.0-windows-x86_64-portable.zip"},
                {"name": "tracedraw-0.5.0-windows-x86_64-setup.exe", "browser_download_url": "https://x/tracedraw-0.5.0-windows-x86_64-setup.exe"}
            ]
        }"#;
        let cases = [
            ("x86", Setup, "tracedraw-0.5.0-windows-x86-setup.exe"),
            ("x86", Portable, "tracedraw-0.5.0-windows-x86-portable.zip"),
            ("x86_64", Setup, "tracedraw-0.5.0-windows-x86_64-setup.exe"),
            (
                "x86_64",
                Portable,
                "tracedraw-0.5.0-windows-x86_64-portable.zip",
            ),
        ];
        for (arch, kind, want) in cases {
            assert_eq!(
                pick_in(json, "windows", arch, kind).as_deref(),
                Some(want),
                "{arch} {kind:?}"
            );
        }
        // A release without the 32-bit files offers the 32-bit copy none.
        assert_eq!(pick("windows", "x86", Setup), None);
        assert_eq!(pick("windows", "x86", Portable), None);
        // No 32-bit build of the other systems.
        for (os, kind) in [("linux", AppImage), ("linux", Deb), ("macos", Dmg)] {
            assert!(asset_names("0.5.0", os, "x86", kind).is_empty(), "{os}");
        }
    }

    /// #1975: the release builds the 32-bit Windows programs, with the C
    /// runtime inside as the 64-bit ones, and names their setup and zip
    /// as the updater of a 32-bit copy looks for them; the setup installs
    /// on any Windows, recommending the x86_64 setup on 64-bit Windows.
    #[test]
    fn the_release_builds_the_32_bit_windows_files() {
        let ci = include_str!("../../../.github/workflows/build.yml");
        let iss = include_str!("../../../packaging/windows/tracedraw.iss");
        let cargo = include_str!("../../../.cargo/config.toml");
        assert!(ci.contains("target: i686-pc-windows-msvc"));
        assert!(ci.contains(r#"$arch = if ("${{ matrix.target }}" -like "i686-*") { "x86" }"#));
        assert!(ci.contains("/DArch=$arch"));
        assert!(ci.contains("-windows-$arch-portable.zip"));
        assert!(iss.contains("OutputBaseFilename=tracedraw-{#Version}-windows-{#Arch}-setup"));
        assert!(iss.contains("#if Arch == \"x86\"\n"));
        assert!(iss.contains("ArchitecturesAllowed=x86compatible\n"));
        assert!(iss.contains("if IsWin64 then"));
        assert!(cargo.contains(
            "[target.i686-pc-windows-msvc]\nrustflags = [\"-C\", \"target-feature=+crt-static\"]"
        ));
        // What those make for a version is what the updater asks for.
        assert_eq!(
            asset_names("0.5.0", "windows", "x86", Kind::Setup),
            vec!["tracedraw-0.5.0-windows-x86-setup.exe".to_string()]
        );
        assert_eq!(
            asset_names("0.5.0", "windows", "x86", Kind::Portable),
            vec!["tracedraw-0.5.0-windows-x86-portable.zip".to_string()]
        );
    }

    /// #1975: on 32-bit Windows a function a program imports by
    /// `raw-dylib` is looked up by its stdcall decorated name
    /// (`_GetUserDefaultUILanguage@0`) unless told otherwise, which the
    /// system's DLLs do not export: the program would not start. Every
    /// such import names its DLL through `cfg_attr`, undecorated on x86.
    #[test]
    fn raw_dylib_imports_are_undecorated_on_32_bit_windows() {
        fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("source folder").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        sources(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let mut imports = 0;
        for file in files {
            let text = std::fs::read_to_string(&file).expect("source");
            // This test's own text names the attribute too.
            if file.ends_with("updates.rs") {
                let code = text.split("mod tests").next().unwrap_or("");
                imports += check_raw_dylib(code, &file);
            } else {
                imports += check_raw_dylib(&text, &file);
            }
        }
        assert!(imports >= 5, "the Windows imports were found ({imports})");
    }

    /// The `raw-dylib` imports of `code`, each checked as
    /// [`raw_dylib_imports_are_undecorated_on_32_bit_windows`] asks.
    fn check_raw_dylib(code: &str, file: &std::path::Path) -> usize {
        let lines: Vec<&str> = code.lines().map(str::trim).collect();
        let mut imports = 0;
        for (n, line) in lines.iter().enumerate() {
            if !line.contains("kind = \"raw-dylib\"") {
                continue;
            }
            assert!(
                !line.starts_with("#[link("),
                "{}:{}: a raw-dylib import without its x86 form",
                file.display(),
                n + 1
            );
            if line.starts_with("#[cfg_attr(not(target_arch = \"x86\")") {
                imports += 1;
                let before = lines[n.saturating_sub(6)..n].join(" ");
                assert!(
                    before.contains("import_name_type = \"undecorated\""),
                    "{}:{}: no undecorated x86 form before this import",
                    file.display(),
                    n + 1
                );
            }
        }
        imports
    }

    /// With no file for this system, processor or install there is no
    /// download (the release page is offered instead), never another
    /// processor's or package's file.
    #[test]
    fn no_file_of_another_system_is_offered() {
        use Kind::*;
        // No builds for these processors and systems.
        assert_eq!(pick("windows", "aarch64", Setup), None);
        assert_eq!(pick("windows", "aarch64", Portable), None);
        assert_eq!(pick("linux", "riscv64", AppImage), None);
        assert_eq!(pick("linux", "x86", Tarball), None);
        assert_eq!(pick("freebsd", "x86_64", Tarball), None);
        assert_eq!(pick("web", "wasm32", Other), None);
        // No rpm or other system package is released.
        assert_eq!(pick("linux", "x86_64", Other), None);
        // A kind that belongs to another system.
        assert_eq!(pick("linux", "x86_64", Setup), None);
        assert_eq!(pick("macos", "aarch64", AppImage), None);
        assert_eq!(pick("windows", "x86_64", Dmg), None);
        // A release missing this processor's file offers none, not the
        // other processor's.
        let no_x86: String = SAMPLE
            .lines()
            .filter(|l| !l.contains("x86_64") && !l.contains("amd64"))
            .collect::<Vec<_>>()
            .join("\n");
        for kind in [AppImage, Tarball, Deb] {
            assert_eq!(pick_in(&no_x86, "linux", "x86_64", kind), None, "{kind:?}");
        }
        assert_eq!(pick_in(&no_x86, "macos", "x86_64", Dmg), None);
        let r = parse_latest(&no_x86, "0.4.0", "linux", "x86_64", AppImage)
            .expect("parses")
            .expect("newer");
        assert!(r.installer.is_none(), "no install in place either");
        // Names that only end like this system's file do not count.
        let lookalike = SAMPLE.replace(
            "tracedraw-0.5.0-linux-x86_64.AppImage\", \"browser",
            "tracedraw-0.5.0-linux-x86_64.AppImage.zsync\", \"browser",
        );
        assert_eq!(pick_in(&lookalike, "linux", "x86_64", AppImage), None);
    }

    /// From 0.19.9 on the x86_64 AppImage is named `linux-amd64`, which
    /// sorts before `linux-arm64`: updaters of 0.19.8 and earlier take the
    /// first AppImage of GitHub's list (sorted by name) and so get the
    /// x86_64 file. This version looks for `linux-amd64` first and still
    /// finds `linux-x86_64` in older releases (#1038).
    #[test]
    fn the_x86_64_appimage_is_named_amd64_from_0_19_9() {
        use Kind::*;
        let renamed = SAMPLE.replace("linux-x86_64.AppImage", "linux-amd64.AppImage");
        assert_eq!(
            pick_in(&renamed, "linux", "x86_64", AppImage).as_deref(),
            Some("tracedraw-0.5.0-linux-amd64.AppImage")
        );
        let p = parse_latest(&renamed, "0.4.0", "linux", "x86_64", AppImage)
            .expect("parses")
            .expect("newer")
            .installer
            .expect("appimage");
        assert_eq!(p.url, "https://x/tracedraw-0.5.0-linux-amd64.AppImage");
        assert_eq!(p.size, 5);
        // The tar.gz keeps its name; arm64 is unchanged.
        assert_eq!(
            pick_in(&renamed, "linux", "x86_64", Tarball).as_deref(),
            Some("tracedraw-0.5.0-linux-x86_64.tar.gz")
        );
        assert_eq!(
            pick_in(&renamed, "linux", "aarch64", AppImage).as_deref(),
            Some("tracedraw-0.5.0-linux-arm64.AppImage")
        );
        // With both names in a release the amd64 one wins, in any order.
        let both = SAMPLE.replace(
            r#"{"name": "tracedraw-0.5.0-linux-x86_64.tar.gz""#,
            r#"{"name": "tracedraw-0.5.0-linux-amd64.AppImage", "browser_download_url": "https://x/tracedraw-0.5.0-linux-amd64.AppImage"},
            {"name": "tracedraw-0.5.0-linux-x86_64.tar.gz""#,
        );
        assert_eq!(
            pick_in(&both, "linux", "x86_64", AppImage).as_deref(),
            Some("tracedraw-0.5.0-linux-amd64.AppImage")
        );
        let reversed = {
            let mut v: serde_json::Value = serde_json::from_str(&both).expect("json");
            v["assets"].as_array_mut().expect("assets").reverse();
            v.to_string()
        };
        assert_eq!(
            pick_in(&reversed, "linux", "x86_64", AppImage).as_deref(),
            Some("tracedraw-0.5.0-linux-amd64.AppImage")
        );
        // Names that only end like the file do not count.
        let lookalike = renamed.replace(
            "tracedraw-0.5.0-linux-amd64.AppImage\", \"browser",
            "tracedraw-0.5.0-linux-amd64.AppImage.zsync\", \"browser",
        );
        assert_eq!(pick_in(&lookalike, "linux", "x86_64", AppImage), None);
        // What an updater of 0.19.8 does: the first AppImage of the list
        // sorted by name is the x86_64 one now.
        let v: serde_json::Value = serde_json::from_str(&renamed).expect("json");
        let mut names: Vec<&str> = v["assets"]
            .as_array()
            .expect("assets")
            .iter()
            .filter_map(|a| a["name"].as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names.iter().find(|n| n.ends_with(".AppImage")).copied(),
            Some("tracedraw-0.5.0-linux-amd64.AppImage")
        );
        // The release script gives the x86_64 AppImage that name.
        let script = include_str!("../../../packaging/linux/build-appimage.sh");
        assert!(script.contains("NAME=amd64"), "build-appimage.sh");
        assert!(script.contains("linux-$NAME.AppImage"), "build-appimage.sh");
    }

    #[test]
    fn how_a_linux_copy_was_installed() {
        use std::path::Path;
        let list = "/.\n/usr\n/usr/bin\n/usr/bin/tracedraw\n/usr/bin/tracedraw-cli\n";
        let bin = Path::new("/usr/bin/tracedraw");
        let home = Path::new("/home/me/tracedraw/tracedraw");
        assert_eq!(linux_kind(true, false, bin, Some(list)), Kind::AppImage);
        assert_eq!(linux_kind(false, false, bin, Some(list)), Kind::Deb);
        assert_eq!(linux_kind(false, false, bin, None), Kind::Other, "rpm");
        assert_eq!(linux_kind(false, true, bin, Some(list)), Kind::Other);
        assert_eq!(linux_kind(false, false, home, Some(list)), Kind::Tarball);
        assert_eq!(linux_kind(false, false, home, None), Kind::Tarball);
        assert_eq!(
            linux_kind(
                false,
                false,
                Path::new("/usr/local/bin/tracedraw"),
                Some(list)
            ),
            Kind::Other
        );
    }

    #[test]
    fn the_same_or_older_drafts_and_pre_releases_are_not_offered() {
        assert_eq!(
            parse_latest(SAMPLE, "0.5.0", "linux", "x86_64", Kind::AppImage),
            Ok(None)
        );
        let pre = SAMPLE.replace("\"prerelease\": false", "\"prerelease\": true");
        assert_eq!(
            parse_latest(&pre, "0.4.0", "linux", "x86_64", Kind::AppImage),
            Ok(None)
        );
        let draft = SAMPLE.replace("\"draft\": false", "\"draft\": true");
        assert_eq!(
            parse_latest(&draft, "0.4.0", "linux", "x86_64", Kind::AppImage),
            Ok(None)
        );
        assert!(parse_latest("not json", "0.4.0", "linux", "x86_64", Kind::AppImage).is_err());
        assert!(parse_latest("{}", "0.4.0", "linux", "x86_64", Kind::AppImage).is_err());
    }

    #[test]
    fn the_appimage_comes_with_its_digest() {
        let r = parse_latest(SAMPLE, "0.4.0", "linux", "x86_64", Kind::AppImage)
            .expect("parses")
            .expect("newer");
        let p = r.installer.expect("appimage");
        assert_eq!(p.url, "https://x/tracedraw-0.5.0-linux-x86_64.AppImage");
        assert_eq!(p.size, 5);
        assert_eq!(
            p.sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        // Not on other systems, nor without a usable digest.
        let w = parse_latest(SAMPLE, "0.4.0", "windows", "x86_64", Kind::Setup).expect("parses");
        assert!(w.expect("newer").installer.is_none());
        // The files of the other copies have no digest in this release: they
        // only get Download, and a copy with no file gets nothing.
        for kind in [Kind::Tarball, Kind::Deb, Kind::Other] {
            let t = parse_latest(SAMPLE, "0.4.0", "linux", "x86_64", kind).expect("parses");
            assert!(t.expect("newer").installer.is_none(), "{kind:?}");
        }
        // An arm64 AppImage installs the arm64 file, with its digest.
        let arm = parse_latest(SAMPLE, "0.4.0", "linux", "aarch64", Kind::AppImage)
            .expect("parses")
            .expect("newer")
            .installer
            .expect("appimage");
        assert_eq!(arm.url, "https://x/tracedraw-0.5.0-linux-arm64.AppImage");
        assert_eq!(arm.size, 7);
        assert!(arm.sha256.starts_with("68e481c1"));
        let bad = SAMPLE.replace("sha256:2CF2", "md5:2CF2");
        let b = parse_latest(&bad, "0.4.0", "linux", "x86_64", Kind::AppImage).expect("parses");
        assert!(b.expect("newer").installer.is_none());
    }

    #[test]
    fn installing_replaces_the_file_only_when_it_checks_out() {
        let dir = std::env::temp_dir().join(format!("td-install-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("TraceDraw.AppImage");
        std::fs::write(&target, b"old").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        let good = Package {
            url: String::new(),
            // sha256("hello")
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
            size: 5,
        };
        let part = dir.join(".TraceDraw.AppImage.update");
        // A wrong digest, a short and a long download leave it alone.
        for (body, pkg) in [
            (&b"hellp"[..], good.clone()),
            (&b"hell"[..], good.clone()),
            (&b"hello!"[..], good.clone()),
        ] {
            let mut r = body;
            assert!(install_file(&mut r, &pkg, &target, &mut |_| Ok(())).is_err());
            assert_eq!(std::fs::read(&target).expect("read"), b"old");
            assert!(!part.exists());
        }
        let mut seen = 0;
        let mut r = &b"hello"[..];
        let mut steps = Vec::new();
        install_file(&mut r, &good, &target, &mut |step| {
            if let Step::Bytes(d) = step {
                seen = d;
            } else {
                steps.push(step);
            }
            Ok(())
        })
        .expect("installs");
        assert_eq!(steps, [Step::Checking, Step::Installing]);
        assert_eq!(seen, 5);
        assert_eq!(std::fs::read(&target).expect("read"), b"hello");
        assert!(!part.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target)
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Windows setup program and the macOS disk image install in place
    /// as the AppImage does: they come with their digest, and only for the
    /// copy installed that way.
    #[test]
    fn the_setup_and_the_disk_image_come_with_their_digest() {
        let hex = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let with_digest = |name: &str| {
            let from =
                format!("{{\"name\": \"{name}\", \"browser_download_url\": \"https://x/{name}\"");
            SAMPLE.replace(
                &from,
                &format!("{from}, \"size\": 9, \"digest\": \"sha256:{hex}\""),
            )
        };
        let setup = with_digest("tracedraw-0.5.0-windows-x86_64-setup.exe");
        let p = parse_latest(&setup, "0.4.0", "windows", "x86_64", Kind::Setup)
            .expect("parses")
            .expect("newer")
            .installer
            .expect("setup with digest");
        assert_eq!(p.url, "https://x/tracedraw-0.5.0-windows-x86_64-setup.exe");
        assert_eq!((p.size, p.sha256.as_str()), (9, hex));
        // The files of other kinds of copy have no digest here, and a
        // release without the digest only gets Download.
        for (os, kind) in [("windows", Kind::Portable), ("linux", Kind::Deb)] {
            let r = parse_latest(&setup, "0.4.0", os, "x86_64", kind).expect("parses");
            assert!(r.expect("newer").installer.is_none(), "{kind:?}");
        }
        let plain = parse_latest(SAMPLE, "0.4.0", "windows", "x86_64", Kind::Setup);
        assert!(plain.expect("parses").expect("newer").installer.is_none());

        let dmg = with_digest("tracedraw-0.5.0-macos-arm64.dmg");
        let p = parse_latest(&dmg, "0.4.0", "macos", "aarch64", Kind::Dmg)
            .expect("parses")
            .expect("newer")
            .installer
            .expect("disk image with digest");
        assert_eq!(p.url, "https://x/tracedraw-0.5.0-macos-arm64.dmg");
        // Another processor's image is never the one.
        let r = parse_latest(&dmg, "0.4.0", "macos", "x86_64", Kind::Dmg).expect("parses");
        assert!(r.expect("newer").installer.is_none());
    }

    #[test]
    fn the_app_bundle_is_found_from_the_program() {
        use std::path::Path;
        let exe = Path::new("/Applications/TraceDraw.app/Contents/MacOS/tracedraw");
        assert_eq!(
            app_bundle_of(exe).as_deref(),
            Some(Path::new("/Applications/TraceDraw.app"))
        );
        // A build run from the source tree is not in a bundle.
        assert_eq!(
            app_bundle_of(Path::new("/src/target/release/tracedraw")),
            None
        );
        assert_eq!(
            app_bundle_of(Path::new("/x/TraceDraw/Contents/MacOS/tracedraw")),
            None
        );
        assert_eq!(app_bundle_of(Path::new("tracedraw")), None);
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("td-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    /// The new bundle takes the old one's place; when it cannot, the old
    /// one is back and no helper folder is left.
    #[test]
    fn swapping_the_bundle_never_leaves_the_app_missing() {
        let dir = scratch("bundle");
        let bundle = dir.join("TraceDraw.app");
        let fresh = dir.join(".TraceDraw.app.update");
        let old = dir.join(".TraceDraw.app.old");
        for (d, text) in [(&bundle, "old"), (&fresh, "new")] {
            std::fs::create_dir_all(d.join("Contents")).expect("dir");
            std::fs::write(d.join("Contents/version"), text).expect("write");
        }
        swap_bundle(&fresh, &bundle).expect("swaps");
        assert_eq!(
            std::fs::read_to_string(bundle.join("Contents/version")).expect("read"),
            "new"
        );
        assert!(!fresh.exists() && !old.exists());
        // The new folder is gone: the installed one stays.
        assert!(swap_bundle(&fresh, &bundle).is_err());
        assert_eq!(
            std::fs::read_to_string(bundle.join("Contents/version")).expect("read"),
            "new"
        );
        assert!(!old.exists());
        // No bundle to replace: nothing is moved in.
        let missing = dir.join("Missing.app");
        std::fs::create_dir_all(&fresh).expect("dir");
        assert!(swap_bundle(&fresh, &missing).is_err());
        assert!(!missing.exists() && !fresh.exists());
        // The first app of a mounted image is the one installed.
        let mount = dir.join("mount");
        std::fs::create_dir_all(mount.join("TraceDraw.app")).expect("dir");
        std::fs::create_dir_all(mount.join("Applications")).expect("dir");
        assert_eq!(find_app(&mount), Some(mount.join("TraceDraw.app")));
        assert_eq!(find_app(&dir.join("nothing")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_downloaded_file_is_kept_only_when_it_checks_out() {
        let dir = scratch("save");
        let dest = dir.join("setup.exe");
        let good = Package {
            url: String::new(),
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
            size: 5,
        };
        for body in [&b"hellp"[..], &b"hell"[..], &b"hello!"[..]] {
            let mut r = body;
            assert!(save_verified(&mut r, &good, &dest, &mut |_| Ok(())).is_err());
            assert!(!dest.exists());
            assert_eq!(
                std::fs::read_dir(&dir).expect("dir").count(),
                0,
                "no leftover"
            );
        }
        let mut r = &b"hello"[..];
        save_verified(&mut r, &good, &dest, &mut |_| Ok(())).expect("saves");
        assert_eq!(std::fs::read(&dest).expect("read"), b"hello");
        assert_eq!(std::fs::read_dir(&dir).expect("dir").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What starts after Restart: setup through the shell with its quiet
    /// switches, the new app bundle, the new AppImage.
    #[test]
    fn restart_starts_the_new_version() {
        use std::path::Path;
        let setup = setup_launch(Path::new(
            r"C:\Users\Ana Maria\Temp\TraceDraw-0.29.0-setup.exe",
        ));
        assert_eq!(setup.program, "cmd");
        assert_eq!(
            setup.args,
            [
                "/C",
                "start",
                "",
                r"C:\Users\Ana Maria\Temp\TraceDraw-0.29.0-setup.exe",
                "/SILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/relaunch=1"
            ]
        );
        let iss = include_str!("../../../packaging/windows/tracedraw.iss");
        assert!(
            iss.contains("{param:relaunch|0}"),
            "tracedraw.iss reads /relaunch"
        );
        assert_eq!(
            bundle_launch(Path::new("/Applications/TraceDraw.app")),
            Launch {
                program: "open".into(),
                args: vec!["-n".into(), "/Applications/TraceDraw.app".into()]
            }
        );
        assert_eq!(
            appimage_launch(Path::new("/home/me/TraceDraw.AppImage")).program,
            "/home/me/TraceDraw.AppImage"
        );
        assert_eq!(safe_version("0.29.0-rc1"), "0.29.0-rc1");
        assert_eq!(safe_version("1/../x y"), "1_.._x_y");
    }

    /// Restart arms the launch for the exit only when the update is in
    /// place; Cancel (or an install that starts again) disarms it; it
    /// runs once, at exit.
    #[cfg(unix)]
    #[test]
    fn the_new_version_starts_only_after_restart() {
        let dir = scratch("restart");
        let mark = dir.join("started");
        let installer = Installer::default();
        assert!(!installer.arm(), "nothing is ready");
        installer.run_pending();
        *installer.launch.lock().expect("lock") = Some(Launch {
            program: "sh".into(),
            args: vec!["-c".into(), format!("touch '{}'", mark.display())],
        });
        // Not armed: the exit starts nothing.
        installer.run_pending();
        assert!(!mark.exists());
        assert!(installer.arm() && installer.is_armed());
        installer.disarm();
        installer.run_pending();
        assert!(!mark.exists(), "cancelled");
        assert!(installer.arm());
        installer.run_pending();
        for _ in 0..100 {
            if mark.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(mark.exists(), "started at exit");
        assert!(!installer.is_armed());
        // Once: a second exit has nothing left to start.
        assert!(!installer.arm());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every kind of copy that has a release file installs it in place
    /// when the file comes with its digest (a different file for each).
    #[test]
    fn every_kind_of_copy_with_a_file_updates_in_place() {
        let hex = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let cases = [
            (
                "linux",
                "x86_64",
                Kind::Tarball,
                "tracedraw-0.5.0-linux-x86_64.tar.gz",
            ),
            (
                "linux",
                "aarch64",
                Kind::Tarball,
                "tracedraw-0.5.0-linux-arm64.tar.gz",
            ),
            ("linux", "x86_64", Kind::Deb, "tracedraw_0.5.0_amd64.deb"),
            ("linux", "aarch64", Kind::Deb, "tracedraw_0.5.0_arm64.deb"),
            (
                "windows",
                "x86_64",
                Kind::Portable,
                "tracedraw-0.5.0-windows-x86_64-portable.zip",
            ),
        ];
        for (os, arch, kind, file) in cases {
            let from =
                format!("{{\"name\": \"{file}\", \"browser_download_url\": \"https://x/{file}\"");
            let json = SAMPLE.replace(
                &from,
                &format!("{from}, \"size\": 9, \"digest\": \"sha256:{hex}\""),
            );
            let p = parse_latest(&json, "0.4.0", os, arch, kind)
                .expect("parses")
                .expect("newer")
                .installer
                .unwrap_or_else(|| panic!("{kind:?} {arch}"));
            assert_eq!(p.url, format!("https://x/{file}"));
            // A copy of another kind, or processor, is not given this file.
            let other = if kind == Kind::Deb {
                Kind::Tarball
            } else {
                Kind::Deb
            };
            let r = parse_latest(&json, "0.4.0", os, arch, other).expect("parses");
            assert!(r.expect("newer").installer.is_none(), "{kind:?}");
        }
    }

    /// One entry of a tar archive (ustar header, checksum included).
    fn tar_entry(name: &str, data: &[u8], mode: u32, kind: u8) -> Vec<u8> {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..108].copy_from_slice(format!("{mode:07o}\0").as_bytes());
        h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        h[148..156].fill(b' ');
        h[156] = kind;
        h[257..263].copy_from_slice(b"ustar\0");
        let sum: u64 = h.iter().map(|&b| u64::from(b)).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        let mut out = h.to_vec();
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(512) * 512, 0);
        out
    }

    /// `tar` (as it is, with no end marker added) compressed.
    fn gzip(tar: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(tar).expect("gzip");
        gz.finish().expect("gzip")
    }

    fn tar_gz(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut tar: Vec<u8> = entries.concat();
        tar.extend_from_slice(&[0u8; 1024]);
        gzip(&tar)
    }

    fn leftovers(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with('.'))
            .collect();
        names.sort();
        names
    }

    /// The tar.gz update replaces the program (whatever the copy is
    /// called) and the command line tool beside it, keeps the program
    /// executable, and ignores everything else in the archive.
    #[test]
    fn the_tar_gz_replaces_the_program_and_its_tool() {
        let dir = scratch("tarball");
        let exe = dir.join("my-tracedraw");
        std::fs::write(&exe, b"old program").expect("write");
        std::fs::write(dir.join("tracedraw-cli"), b"old tool").expect("write");
        std::fs::write(dir.join("notes.txt"), b"mine").expect("write");
        let archive = dir.join("update.tar.gz");
        std::fs::write(
            &archive,
            tar_gz(&[
                tar_entry("./", b"", 0o755, b'5'),
                tar_entry("tracedraw", b"new program", 0o755, b'0'),
                tar_entry("tracedraw-cli", b"new tool", 0o755, b'0'),
                tar_entry("notes.txt", b"theirs", 0o644, b'0'),
                tar_entry("../escape", b"no", 0o644, b'0'),
                tar_entry("link", b"", 0o777, b'2'),
            ]),
        )
        .expect("write");
        install_tarball(&archive, &exe).expect("installs");
        assert_eq!(std::fs::read(&exe).expect("read"), b"new program");
        assert_eq!(
            std::fs::read(dir.join("tracedraw-cli")).expect("read"),
            b"new tool"
        );
        assert_eq!(std::fs::read(dir.join("notes.txt")).expect("read"), b"mine");
        assert!(
            !dir.join("tracedraw").exists(),
            "installed under the copy's name"
        );
        assert!(!dir.parent().expect("parent").join("escape").exists());
        assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&exe).expect("meta").permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        // The tool is only updated where it is installed.
        std::fs::remove_file(dir.join("tracedraw-cli")).expect("remove");
        install_tarball(&archive, &exe).expect("installs");
        assert!(!dir.join("tracedraw-cli").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A damaged archive, one without the program and one that stops early
    /// leave the installed files and the folder as they were.
    #[test]
    fn a_bad_tar_gz_changes_nothing() {
        let dir = scratch("badtar");
        let exe = dir.join("tracedraw");
        std::fs::write(&exe, b"old program").expect("write");
        std::fs::write(dir.join("tracedraw-cli"), b"old tool").expect("write");
        let archive = dir.join("update.tar.gz");
        let mut bad_sum = tar_entry("tracedraw-cli", b"new tool", 0o755, b'0');
        bad_sum[0] = b'X';
        let mut short = tar_entry("tracedraw", b"new program", 0o755, b'0');
        short.truncate(512 + 3);
        let no_program = tar_gz(&[tar_entry("tracedraw-cli", b"new tool", 0o755, b'0')]);
        let cases = [
            tar_gz(&[
                tar_entry("tracedraw-cli", b"new tool", 0o755, b'0'),
                bad_sum,
            ]),
            gzip(&short),
            no_program,
            b"not a gzip file".to_vec(),
        ];
        for body in cases {
            std::fs::write(&archive, body).expect("write");
            assert!(install_tarball(&archive, &exe).is_err());
            assert_eq!(std::fs::read(&exe).expect("read"), b"old program");
            assert_eq!(
                std::fs::read(dir.join("tracedraw-cli")).expect("read"),
                b"old tool"
            );
            assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The portable zip's top-level files replace the ones beside the
    /// program; the settings folder, in the zip and beside the program,
    /// is never touched.
    #[test]
    fn the_portable_zip_keeps_the_settings_folder() {
        use std::io::Write;
        let dir = scratch("portable");
        let exe = dir.join("tracedraw.exe");
        std::fs::write(&exe, b"old exe").expect("write");
        std::fs::create_dir_all(dir.join("tracedraw-settings")).expect("dir");
        std::fs::write(dir.join("tracedraw-settings/prefs.json"), b"mine").expect("write");
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in [
            ("tracedraw.exe", &b"new exe"[..]),
            ("tracedraw-cli.exe", b"new tool"),
            ("README.md", b"new readme"),
            ("tracedraw-settings/prefs.json", b"theirs"),
            ("tracedraw-settings\\README.txt", b"theirs"),
            ("..\\escape.txt", b"no"),
            ("sub/dir.txt", b"no"),
        ] {
            zip.start_file(name, options).expect("entry");
            zip.write_all(data).expect("data");
        }
        let archive = dir.join("update.archive");
        std::fs::write(&archive, zip.finish().expect("zip").into_inner()).expect("write");
        install_zip(&archive, &exe).expect("installs");
        assert_eq!(std::fs::read(&exe).expect("read"), b"new exe");
        assert_eq!(
            std::fs::read(dir.join("tracedraw-cli.exe")).expect("read"),
            b"new tool"
        );
        assert_eq!(
            std::fs::read(dir.join("README.md")).expect("read"),
            b"new readme"
        );
        assert_eq!(
            std::fs::read(dir.join("tracedraw-settings/prefs.json")).expect("read"),
            b"mine"
        );
        assert!(!dir.join("tracedraw-settings/README.txt").exists());
        assert!(!dir.join("tracedraw-settings\\README.txt").exists());
        assert!(!dir.join("sub").exists());
        assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
        // A zip without the program changes nothing.
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("README.md", options).expect("entry");
        zip.write_all(b"other").expect("data");
        std::fs::write(&archive, zip.finish().expect("zip").into_inner()).expect("write");
        assert!(install_zip(&archive, &exe).is_err());
        assert_eq!(
            std::fs::read(dir.join("README.md")).expect("read"),
            b"new readme"
        );
        assert!(leftovers(&dir).is_empty());
        assert!(install_zip(&dir.join("missing.zip"), &exe).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows cannot overwrite a running program but can rename it: the
    /// old file is set aside, the new one moved in, and the old one
    /// removed when it can be; a failure puts the old file back.
    #[test]
    fn a_running_program_is_set_aside_not_overwritten() {
        let dir = scratch("aside");
        let exe = dir.join("tracedraw.exe");
        std::fs::write(&exe, b"old").expect("write");
        std::fs::write(hidden(&dir, "tracedraw.exe", ".update"), b"new").expect("write");
        let old = replace_file(&hidden(&dir, "tracedraw.exe", ".update"), &exe, true)
            .expect("replaces")
            .expect("set aside");
        assert_eq!(std::fs::read(&exe).expect("read"), b"new");
        assert_eq!(std::fs::read(&old).expect("read"), b"old");
        // The new copy is missing: the program stays.
        assert!(replace_file(&hidden(&dir, "tracedraw.exe", ".update"), &exe, true).is_err());
        assert_eq!(std::fs::read(&exe).expect("read"), b"new");
        let _ = std::fs::remove_file(&old);
        // Through swap_in, the set-aside file is gone afterwards.
        std::fs::write(hidden(&dir, "tracedraw.exe", ".update"), b"newer").expect("write");
        swap_in(&dir, &["tracedraw.exe".to_string()], true).expect("swaps");
        assert_eq!(std::fs::read(&exe).expect("read"), b"newer");
        assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_entries_are_installed_under_safe_names_only() {
        let dir = scratch("names");
        std::fs::write(dir.join("tracedraw-cli"), b"").expect("write");
        assert_eq!(tarball_name("tracedraw", "td", &dir).as_deref(), Some("td"));
        assert_eq!(
            tarball_name("tracedraw-cli", "td", &dir).as_deref(),
            Some("tracedraw-cli")
        );
        assert_eq!(tarball_name("bin/tracedraw", "td", &dir), None);
        assert_eq!(tarball_name("README", "td", &dir), None);
        assert_eq!(
            portable_name("tracedraw.exe", "td.exe").as_deref(),
            Some("td.exe")
        );
        assert_eq!(
            portable_name("LICENSE-MIT", "td.exe").as_deref(),
            Some("LICENSE-MIT")
        );
        for bad in [
            "",
            ".hidden",
            "a/b",
            "a\\b",
            "C:evil",
            "..",
            "tracedraw-settings/x",
        ] {
            assert_eq!(portable_name(bad, "td.exe"), None, "{bad:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The `.deb` is installed by `pkexec dpkg -i`, then removed, and the
    /// program started whatever happened to the install.
    #[test]
    fn the_deb_installs_as_the_administrator_then_starts_the_program() {
        use std::path::Path;
        let l = deb_launch(
            Path::new("/home/me/.cache/tracedraw-update/tracedraw_0.29.0.deb"),
            Path::new("/usr/bin/tracedraw"),
        );
        assert_eq!(l.program, "sh");
        assert_eq!(l.args[0], "-c");
        // The file names are arguments of the script, never part of it.
        assert_eq!(
            l.args[1],
            "pkexec dpkg -i \"$1\"; rm -f \"$1\"; exec \"$2\""
        );
        assert_eq!(
            l.args[3],
            "/home/me/.cache/tracedraw-update/tracedraw_0.29.0.deb"
        );
        assert_eq!(l.args[4], "/usr/bin/tracedraw");
        assert_eq!(
            exe_launch(Path::new("/opt/td/tracedraw")),
            Launch {
                program: "/opt/td/tracedraw".into(),
                args: Vec::new()
            }
        );
    }

    /// Against GitHub: the latest release's x86_64 AppImage downloads and
    /// checks out. Run by hand (`cargo test -- --ignored`), it needs the network.
    #[test]
    #[ignore]
    fn live_install_from_the_latest_release() {
        let json = fetch().expect("latest release");
        let r = parse_latest(&json, "0.0.1", "linux", "x86_64", Kind::AppImage)
            .expect("parses")
            .expect("newer");
        let p = r.installer.expect("appimage with digest");
        // v0.19.8 and earlier name it linux-x86_64, later ones linux-amd64.
        assert!(
            p.url.ends_with("-linux-x86_64.AppImage") || p.url.ends_with("-linux-amd64.AppImage"),
            "{}",
            p.url
        );
        let dir = std::env::temp_dir().join(format!("td-live-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("TraceDraw.AppImage");
        std::fs::write(&target, b"old").expect("write");
        let installer = Installer::default();
        installer.start(
            r.version.clone(),
            p.clone(),
            InstallTarget::AppImage(target.clone()),
            None,
        );
        for _ in 0..1200 {
            if !matches!(installer.get(), InstallState::Downloading { .. }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert_eq!(installer.get(), InstallState::Installed(r.version));
        assert_eq!(std::fs::metadata(&target).expect("meta").len(), p.size);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The percentage is rounded down and reaches 100 only with the last
    /// byte; the bar's share stays between 0 and 1.
    #[test]
    fn the_percentage_counts_whole_percent_done() {
        assert_eq!(percent(0, 0), 0, "no size yet");
        assert_eq!(percent(0, 1000), 0);
        assert_eq!(percent(123, 1000), 12);
        assert_eq!(percent(999, 1000), 99, "never 100 before the end");
        assert_eq!(percent(1000, 1000), 100);
        assert_eq!(percent(2000, 1000), 100, "more than the size");
        assert_eq!(percent(u64::MAX - 1, u64::MAX), 99, "no overflow");
        assert_eq!(fraction(0, 0), 0.0);
        assert!((fraction(1, 4) - 0.25).abs() < 1e-6);
        assert_eq!(fraction(5, 4), 1.0);
    }

    /// The speed follows the download smoothly: a steady rate reads as
    /// that rate, one fast burst moves it only part of the way, and a
    /// stall brings it down. The time left waits until the speed means
    /// something, then is the bytes left over the speed.
    #[test]
    fn the_speed_is_smoothed_and_the_time_left_waits_for_it() {
        const MB: u64 = 1 << 20;
        let mut m = Meter::default();
        assert_eq!(m.speed_at(0.0), None);
        assert_eq!(m.left_at(0.0, 0, 10 * MB), None);
        // One megabyte a second, sampled every 0.1 s (merged to 0.25 s).
        for i in 0..=20u64 {
            let t = i as f64 * 0.1;
            m.record(t, i * MB / 10);
        }
        let s = m.speed_at(2.0).expect("measured");
        assert!((s - MB as f64).abs() < 0.01 * MB as f64, "{s}");
        assert_eq!(m.left_at(2.0, 2 * MB, 10 * MB), None, "too early");
        for i in 21..=40u64 {
            m.record(i as f64 * 0.1, i * MB / 10);
        }
        let left = m.left_at(4.0, 4 * MB, 10 * MB).expect("meaningful now");
        assert!((left - 6.0).abs() < 0.1, "{left}");
        // A burst of 5 MB in half a second: the speed rises, but far less
        // than the burst's 10 MB/s.
        let mut burst = m;
        burst.record(4.5, 9 * MB);
        let s = burst.speed_at(4.5).expect("speed");
        assert!(s > 1.5 * MB as f64 && s < 4.0 * MB as f64, "{s}");
        // Nothing arrives for ten seconds: the speed shown falls, and so
        // the time left grows.
        let stalled = m.speed_at(14.0).expect("speed");
        assert!(stalled < 0.1 * MB as f64, "{stalled}");
        assert!(m.left_at(14.0, 4 * MB, 10 * MB).is_none_or(|l| l > 60.0));
        // A finished download has no time left.
        assert_eq!(m.left_at(4.0, 10 * MB, 10 * MB), Some(0.0));
    }

    /// Install errors read as the trouble the dialog explains.
    #[test]
    fn install_errors_are_told_in_plain_words() {
        for (e, t) in [
            (
                "the download does not match the release's digest",
                Trouble::Damaged,
            ),
            (
                "the download is larger than the release says",
                Trouble::Damaged,
            ),
            ("the archive is damaged", Trouble::Damaged),
            ("the download answered 404", Trouble::Network),
            ("the download stopped at 5 of 9 bytes", Trouble::Network),
            ("io: Connection refused (os error 111)", Trouble::Network),
            ("timeout: connect", Trouble::Network),
            ("Permission denied (os error 13)", Trouble::Permission),
            ("Access is denied. (os error 5)", Trouble::Permission),
            ("No space left on device (os error 28)", Trouble::Space),
            (
                "There is not enough space on the disk. (os error 112)",
                Trouble::Space,
            ),
            ("no file name", Trouble::Other),
        ] {
            assert_eq!(trouble(e), t, "{e}");
        }
    }

    /// A reader that hands out `first` and then waits until the test lets
    /// it go on (or for ever, as a hanging connection does).
    struct Gate {
        first: Vec<u8>,
        rest: Vec<u8>,
        go: std::sync::mpsc::Receiver<()>,
        given: bool,
    }

    impl std::io::Read for Gate {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let from = if !self.given {
                self.given = true;
                std::mem::take(&mut self.first)
            } else {
                if self.rest.is_empty() {
                    return Ok(0);
                }
                // Hangs until the test says go, or the sender is gone.
                if self.go.recv().is_err() {
                    return Err(std::io::Error::other("gone"));
                }
                std::mem::take(&mut self.rest)
            };
            out[..from.len()].copy_from_slice(&from);
            Ok(from.len())
        }
    }

    /// An installer whose download is `body`.
    fn start_with(
        installer: &Installer,
        target: &std::path::Path,
        package: &Package,
        body: impl std::io::Read + Send + 'static,
    ) {
        let body: Box<dyn std::io::Read + Send> = Box::new(body);
        installer.start_from(
            "9.0.0".into(),
            package.clone(),
            InstallTarget::AppImage(target.to_path_buf()),
            None,
            Box::new(move |_| Ok(body)),
        );
    }

    fn wait_for(installer: &Installer, what: impl Fn(&InstallState) -> bool) -> InstallState {
        for _ in 0..200 {
            let s = installer.get();
            if what(&s) {
                return s;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        installer.get()
    }

    fn hello() -> Package {
        Package {
            url: String::new(),
            // sha256("hello")
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
            size: 5,
        }
    }

    /// Cancel stops a download that hangs half way at once: the state is
    /// Idle straight away, the thread ends, the partial file is removed,
    /// the old file is untouched and nothing is ready to restart into.
    /// A new Install after it runs to the end.
    #[test]
    fn cancel_stops_the_download_and_removes_the_partial_file() {
        let dir = scratch("cancel");
        let target = dir.join("TraceDraw.AppImage");
        std::fs::write(&target, b"old").expect("write");
        let part = dir.join(".TraceDraw.AppImage.update");
        let installer = Installer::default();
        assert!(!installer.cancel(), "nothing to cancel");
        let (go, wait) = std::sync::mpsc::channel();
        start_with(
            &installer,
            &target,
            &hello(),
            Gate {
                first: b"he".to_vec(),
                rest: b"llo".to_vec(),
                go: wait,
                given: false,
            },
        );
        let s = wait_for(&installer, |s| {
            matches!(s, InstallState::Downloading { done: 2, .. })
        });
        assert_eq!(s, InstallState::Downloading { done: 2, total: 5 });
        assert!(part.exists(), "the partial file is written");
        let p = installer.progress().expect("downloading");
        assert_eq!((p.done, p.total), (2, 5));
        assert!(installer.cancel());
        assert_eq!(installer.get(), InstallState::Idle, "at once");
        assert_eq!(installer.progress(), None);
        // The connection still hangs, yet the install thread ends.
        installer.wait();
        assert!(!part.exists(), "the partial file is gone");
        assert_eq!(std::fs::read(&target).expect("read"), b"old");
        assert_eq!(installer.get(), InstallState::Idle, "stays Idle");
        assert!(!installer.arm(), "nothing to restart into");
        assert!(!installer.cancel(), "cancelled once");
        drop(go);
        // Install again: it runs to the end.
        start_with(&installer, &target, &hello(), &b"hello"[..]);
        installer.wait();
        assert_eq!(installer.get(), InstallState::Installed("9.0.0".into()));
        assert_eq!(std::fs::read(&target).expect("read"), b"hello");
        assert!(installer.arm());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An install goes Downloading, Checking, Installing, then Installed;
    /// a damaged download stops at Failed with the old file kept, and a
    /// second Install while one runs is ignored.
    #[test]
    fn an_install_goes_through_its_steps() {
        let dir = scratch("steps");
        let target = dir.join("TraceDraw.AppImage");
        std::fs::write(&target, b"old").expect("write");
        // The steps the install reports, in order.
        let mut steps = Vec::new();
        let (launch, in_place) = stage(
            "9.0.0",
            &hello(),
            &InstallTarget::AppImage(target.clone()),
            &mut &b"hello"[..],
            &mut |step| {
                steps.push(step);
                Ok(())
            },
        )
        .expect("installs");
        assert!(in_place);
        assert_eq!(launch.program, target.display().to_string());
        assert_eq!(
            steps,
            [Step::Bytes(5), Step::Checking, Step::Installing],
            "downloaded, checked, put in place"
        );
        // A report that says stop (Cancel) stops before the file moves.
        std::fs::write(&target, b"old").expect("write");
        let stopped = stage(
            "9.0.0",
            &hello(),
            &InstallTarget::AppImage(target.clone()),
            &mut &b"hello"[..],
            &mut |step| match step {
                Step::Checking => Err(CANCELLED.into()),
                _ => Ok(()),
            },
        );
        assert_eq!(stopped.map(|_| ()), Err(CANCELLED.to_string()));
        assert_eq!(std::fs::read(&target).expect("read"), b"old");
        assert!(!dir.join(".TraceDraw.AppImage.update").exists());
        // Through the installer: the state while the body waits, then
        // the end.
        let installer = Installer::default();
        let (go, wait) = std::sync::mpsc::channel();
        start_with(
            &installer,
            &target,
            &hello(),
            Gate {
                first: b"hel".to_vec(),
                rest: b"lo".to_vec(),
                go: wait,
                given: false,
            },
        );
        let s = wait_for(&installer, |s| {
            matches!(s, InstallState::Downloading { done: 3, .. })
        });
        assert_eq!(s, InstallState::Downloading { done: 3, total: 5 });
        assert!(s.busy());
        // Install again while it runs: ignored.
        start_with(&installer, &target, &hello(), &b"xxxxx"[..]);
        go.send(()).expect("go");
        installer.wait();
        let done = installer.get();
        assert_eq!(done, InstallState::Installed("9.0.0".into()));
        assert!(!done.busy());
        assert_eq!(std::fs::read(&target).expect("read"), b"hello");
        // A damaged download fails and leaves the file.
        std::fs::write(&target, b"old").expect("write");
        start_with(&installer, &target, &hello(), &b"hellp"[..]);
        installer.wait();
        let InstallState::Failed(e) = installer.get() else {
            panic!("{:?}", installer.get());
        };
        assert_eq!(trouble(&e), Trouble::Damaged, "{e}");
        assert_eq!(std::fs::read(&target).expect("read"), b"old");
        assert!(InstallState::Checking.busy() && InstallState::Installing.busy());
        assert!(!InstallState::Idle.busy());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
