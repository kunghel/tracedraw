//! User interface strings. `tr("menu.file.open")` returns the string in
//! the active language, falling back to English, then to the key itself.
//!
//! Language files are flat `key = "value"` tables in `lang/` (a TOML
//! subset) embedded at build time, so the binary stays self-contained.
//! The browser build embeds English only and fetches another language's
//! file from the page's `lang/` folder when it is chosen (English shows
//! until it arrives), which keeps the module smaller.

use std::collections::{HashMap, HashSet};
use std::sync::{OnceLock, RwLock};

/// Languages shipped with the app, in display order.
pub const LANGUAGES: [(&str, &str); 19] = [
    ("en", "English"),
    ("zh-CN", "中文（简体）"),
    ("hi", "हिन्दी"),
    ("es", "Español"),
    ("fr", "Français"),
    ("ar", "العربية"),
    ("bn", "বাংলা"),
    ("ru", "Русский"),
    ("pt-BR", "Português (Brasil)"),
    ("id", "Bahasa Indonesia"),
    ("de", "Deutsch"),
    ("ja", "日本語"),
    ("zh-TW", "中文（繁體）"),
    ("it", "Italiano"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("tr", "Türkçe"),
    ("cs", "Čeština"),
    ("lo", "ພາສາລາວ"),
];

/// The language files built into the program: all of them on the
/// desktop.
#[cfg(not(target_arch = "wasm32"))]
const FILES: [(&str, &str); 19] = [
    ("en", include_str!("../lang/en.toml")),
    ("zh-CN", include_str!("../lang/zh-CN.toml")),
    ("hi", include_str!("../lang/hi.toml")),
    ("es", include_str!("../lang/es.toml")),
    ("fr", include_str!("../lang/fr.toml")),
    ("ar", include_str!("../lang/ar.toml")),
    ("bn", include_str!("../lang/bn.toml")),
    ("ru", include_str!("../lang/ru.toml")),
    ("pt-BR", include_str!("../lang/pt-BR.toml")),
    ("id", include_str!("../lang/id.toml")),
    ("de", include_str!("../lang/de.toml")),
    ("ja", include_str!("../lang/ja.toml")),
    ("zh-TW", include_str!("../lang/zh-TW.toml")),
    ("it", include_str!("../lang/it.toml")),
    ("nl", include_str!("../lang/nl.toml")),
    ("pl", include_str!("../lang/pl.toml")),
    ("tr", include_str!("../lang/tr.toml")),
    ("cs", include_str!("../lang/cs.toml")),
    ("lo", include_str!("../lang/lo.toml")),
];

/// The browser build embeds English only; the other languages are
/// fetched from the page (`lang/<code>.toml`, see [`file_url`]).
#[cfg(target_arch = "wasm32")]
const FILES: [(&str, &str); 1] = [("en", include_str!("../lang/en.toml"))];

/// The built-in file of a language.
fn builtin(code: &str) -> Option<&'static str> {
    FILES.iter().find(|(c, _)| *c == code).map(|(_, s)| *s)
}

/// Where the browser build fetches a language's file from, relative to
/// the page; `None` for English (always built in) and unknown codes.
pub fn file_url(code: &str) -> Option<String> {
    LANGUAGES
        .iter()
        .any(|(c, _)| *c == code && *c != "en")
        .then(|| format!("lang/{code}.toml"))
}

struct State {
    /// The chosen language.
    code: String,
    /// The language `table` holds: `code`, or English while the chosen
    /// language's file is on its way (browser) or the code is unknown.
    shown: String,
    table: HashMap<String, String>,
    english: HashMap<String, String>,
    /// Languages fetched by the browser build, kept to switch back.
    fetched: HashMap<String, HashMap<String, String>>,
    /// Languages whose file is being fetched.
    pending: HashSet<String>,
}

impl State {
    fn new(english: HashMap<String, String>) -> Self {
        State {
            code: "en".into(),
            shown: "en".into(),
            table: english.clone(),
            english,
            fetched: HashMap::new(),
            pending: HashSet::new(),
        }
    }

    /// Make `code` the language, from its built-in file or one fetched
    /// before; otherwise English shows and the file to fetch is returned
    /// (once: not while it is already on its way).
    fn choose(&mut self, code: &str, builtin: Option<&str>) -> Option<String> {
        self.code = code.to_string();
        let table = match builtin {
            Some(src) => Some(parse(src)),
            None => self.fetched.get(code).cloned(),
        };
        if let Some(table) = table {
            self.table = table;
            self.shown = code.to_string();
            return None;
        }
        self.table = self.english.clone();
        self.shown = "en".into();
        let url = file_url(code)?;
        self.pending.insert(code.to_string()).then_some(url)
    }

    /// A fetched language file arrived (`None`: the fetch failed, and
    /// choosing the language again tries again). Returns whether the
    /// interface changed language.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn arrived(&mut self, code: &str, src: Option<&str>) -> Result<bool, String> {
        self.pending.remove(code);
        let src = src.ok_or_else(|| format!("could not fetch {code}"))?;
        let table = parse(src);
        // A server's error page is not a language file.
        if !table.contains_key("menu.file") {
            return Err(format!("{code} is not a language file"));
        }
        let shown = self.code == code;
        if shown {
            self.table = table.clone();
            self.shown = code.to_string();
        }
        self.fetched.insert(code.to_string(), table);
        Ok(shown)
    }
}

fn state() -> &'static RwLock<State> {
    static STATE: OnceLock<RwLock<State>> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new(State::new(parse(FILES[0].1))))
}

/// Parse the flat `key = "value"` format. Lines starting with `#` and
/// blank lines are ignored; values support `\"`, `\\` and `\n`.
pub fn parse(src: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let rest = rest.trim();
        let Some(body) = rest.strip_prefix('"') else {
            continue;
        };
        let mut value = String::new();
        let mut chars = body.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('n') => value.push('\n'),
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some(o) => {
                        value.push('\\');
                        value.push(o);
                    }
                    None => {}
                },
                '"' => break,
                o => value.push(o),
            }
        }
        map.insert(key.trim().to_string(), value);
    }
    map
}

/// Switch the active language; unknown codes fall back to English. In
/// the browser a language not fetched yet shows in English until its
/// file arrives ([`language_arrived`]).
pub fn set_language(code: &str) {
    let fetch = match state().write() {
        Ok(mut st) => st.choose(code, builtin(code)),
        Err(_) => None,
    };
    if let Some(url) = fetch {
        fetch_file(code, url);
    }
}

#[cfg(target_arch = "wasm32")]
fn fetch_file(code: &str, url: String) {
    crate::web::fetch_language(code.to_string(), url);
}

/// The desktop has every language built in.
#[cfg(not(target_arch = "wasm32"))]
fn fetch_file(code: &str, url: String) {
    log::warn!("language {code} is not built in ({url})");
}

/// A language file the browser build fetched (`None`: the fetch failed;
/// English stays). Returns whether the interface changed language, so
/// the caller asks for a frame.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn language_arrived(code: &str, src: Option<&str>) -> bool {
    let Ok(mut st) = state().write() else {
        return false;
    };
    st.arrived(code, src).unwrap_or_else(|e| {
        log::warn!("interface language: {e}; English stays");
        false
    })
}

/// Every key of the English table.
pub fn english_keys() -> Vec<String> {
    state()
        .read()
        .map(|s| s.english.keys().cloned().collect())
        .unwrap_or_default()
}

pub fn language() -> String {
    state().read().map(|s| s.code.clone()).unwrap_or_default()
}

/// Right-to-left languages flip the docker side and text alignment
/// (once the language shows: English while its file is on its way).
pub fn is_rtl() -> bool {
    state().read().map(|s| s.shown == "ar").unwrap_or(false)
}

#[cfg(test)]
thread_local! {
    /// A language for this thread only, so that a test can lay the
    /// interface out in another language without changing it for the
    /// tests running beside it.
    static THREAD_TABLE: std::cell::RefCell<Option<HashMap<String, String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with `code` as the language of this thread only (tests).
#[cfg(test)]
pub fn with_thread_language<R>(code: &str, f: impl FnOnce() -> R) -> R {
    let src = builtin(code).unwrap_or(FILES[0].1);
    let before = THREAD_TABLE.with(|t| t.replace(Some(parse(src))));
    let out = f();
    THREAD_TABLE.with(|t| *t.borrow_mut() = before);
    out
}

thread_local! {
    /// The keys translated on this thread while [`recording`] runs.
    static RECORDED: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` and return, with its result, every key it translated, in
/// order (Help > Search Commands learns the Options pages' settings this
/// way, `options::settings_index`).
pub fn recording<R>(f: impl FnOnce() -> R) -> (R, Vec<String>) {
    let before = RECORDED.with(|r| r.replace(Some(Vec::new())));
    let out = f();
    let keys = RECORDED.with(|r| r.replace(before)).unwrap_or_default();
    (out, keys)
}

/// Translate a key.
pub fn tr(key: &str) -> String {
    RECORDED.with(|r| {
        if let Ok(mut r) = r.try_borrow_mut() {
            if let Some(keys) = r.as_mut() {
                keys.push(key.to_string());
            }
        }
    });
    #[cfg(test)]
    if let Some(v) = THREAD_TABLE.with(|t| t.borrow().as_ref().and_then(|m| m.get(key).cloned())) {
        return v;
    }
    if let Ok(st) = state().read() {
        if let Some(v) = st.table.get(key) {
            return v.clone();
        }
        if let Some(v) = st.english.get(key) {
            return v.clone();
        }
    }
    log::debug!("missing i18n key {key}");
    key.rsplit('.').next().unwrap_or(key).to_string()
}

/// Whether the English table has `key` (generated keys, optional texts).
pub fn has_english(key: &str) -> bool {
    state()
        .read()
        .map(|st| st.english.contains_key(key))
        .unwrap_or(false)
}

/// Translate with `{name}` placeholders replaced.
pub fn trf(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = tr(key);
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// The saved language that means "follow the system's language": the
/// user picked System language in Options. An empty saved language means
/// no language was ever picked: the installer's choice, then the system's.
pub const SYSTEM: &str = "system";

/// The supported language with this code, compared without case.
pub fn supported(code: &str) -> Option<&'static str> {
    LANGUAGES
        .iter()
        .map(|(c, _)| *c)
        .find(|c| c.eq_ignore_ascii_case(code.trim()))
}

/// The interface language for one locale name: a POSIX locale
/// (`pt_BR.UTF-8`, `sr_RS@latin`), a BCP 47 tag (`zh-Hant-HK`, `pt-PT`)
/// or a bare language (`de`). The closest supported language: the same
/// language in another region (Portuguese of Portugal reads Brazilian
/// Portuguese), Chinese by its script or region (Hant, TW, HK, MO:
/// Traditional; otherwise Simplified). `None` for `C`, `POSIX`, an empty
/// name or a language TraceDraw does not have.
pub fn match_locale(locale: &str) -> Option<&'static str> {
    // Drop the encoding and the modifier, then split into subtags.
    let name = locale.trim();
    let name = name.split(['.', '@']).next().unwrap_or("");
    let lower = name.to_ascii_lowercase().replace('_', "-");
    let mut parts = lower.split('-').filter(|p| !p.is_empty());
    let lang = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    match lang {
        "c" | "posix" => None,
        "zh" => {
            let traditional = rest
                .iter()
                .any(|p| matches!(*p, "hant" | "tw" | "hk" | "mo"));
            let simplified = rest.contains(&"hans");
            Some(if traditional && !simplified {
                "zh-TW"
            } else {
                "zh-CN"
            })
        }
        // The older code of Indonesian, still sent by some systems.
        "in" => Some("id"),
        _ => LANGUAGES.iter().map(|(c, _)| *c).find(|c| {
            c.split('-')
                .next()
                .is_some_and(|base| base.eq_ignore_ascii_case(lang))
        }),
    }
}

/// The first of the system's languages, in its order of preference,
/// that TraceDraw has.
pub fn best_language<S: AsRef<str>>(locales: &[S]) -> Option<&'static str> {
    locales.iter().find_map(|l| match_locale(l.as_ref()))
}

/// The interface language for a Windows language identifier (LANGID,
/// as `GetUserDefaultUILanguage` gives it). The installer has the same
/// table in Pascal (`AppLanguageForLangId` in
/// `packaging/windows/tracedraw.iss`); a test keeps the two equal.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn language_for_langid(id: u16) -> Option<&'static str> {
    let primary = id & 0x3ff;
    Some(match primary {
        0x09 => "en",
        0x04 => match id {
            // Taiwan, Hong Kong, Macau and the neutral Traditional.
            0x0404 | 0x0c04 | 0x1404 | 0x7c04 => "zh-TW",
            _ => "zh-CN",
        },
        0x39 => "hi",
        0x0a => "es",
        0x0c => "fr",
        0x01 => "ar",
        0x45 => "bn",
        0x19 => "ru",
        0x16 => "pt-BR",
        0x21 => "id",
        0x07 => "de",
        0x11 => "ja",
        0x10 => "it",
        0x13 => "nl",
        0x15 => "pl",
        0x1f => "tr",
        0x05 => "cs",
        0x54 => "lo",
        _ => return None,
    })
}

/// The language the Windows installer recorded: [`SYSTEM`] when the
/// user left System language, else one of our codes. `None` for nothing
/// recorded or a value this version does not know.
pub fn installer_choice(value: &str) -> Option<&'static str> {
    let v = value.trim();
    if v.eq_ignore_ascii_case(SYSTEM) {
        return Some(SYSTEM);
    }
    supported(v)
}

/// The interface language at start. The language picked in Options
/// (`saved`, a supported code) always wins; [`SYSTEM`] follows the
/// system. Nothing picked (an empty or unknown `saved`): the installer's
/// choice (`installer`, as recorded), then the system's languages
/// (`system`, in order of preference), then English.
pub fn startup_language<S: AsRef<str>>(
    saved: &str,
    installer: Option<&str>,
    system: &[S],
) -> &'static str {
    let from_system = || best_language(system).unwrap_or("en");
    if saved.trim().eq_ignore_ascii_case(SYSTEM) {
        return from_system();
    }
    if let Some(code) = supported(saved) {
        return code;
    }
    match installer.and_then(installer_choice) {
        Some(SYSTEM) | None => from_system(),
        Some(code) => code,
    }
}

/// [`startup_language`] with the installer's choice and the system's
/// languages read from this machine.
pub fn startup_language_here(saved: &str) -> &'static str {
    if supported(saved).is_some() {
        return startup_language::<&str>(saved, None, &[]);
    }
    let installer = crate::system_locale::installer_language();
    let system = crate::system_locale::preferred_languages();
    let lang = startup_language(saved, installer.as_deref(), &system);
    log::info!(
        "interface language {lang} (saved {saved:?}, installer {installer:?}, system {system:?})"
    );
    lang
}

/// The language the system prefers, as one of ours (English if none).
pub fn system_language() -> &'static str {
    best_language(&crate::system_locale::preferred_languages()).unwrap_or("en")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn language_for_locale(locale: &str) -> &'static str {
        match_locale(locale).unwrap_or("en")
    }

    #[test]
    fn locales_choose_the_interface_language() {
        assert_eq!(language_for_locale("zh_TW.UTF-8"), "zh-TW");
        assert_eq!(language_for_locale("zh_HK.UTF-8"), "zh-TW");
        assert_eq!(language_for_locale("zh_CN.UTF-8"), "zh-CN");
        assert_eq!(language_for_locale("zh_SG"), "zh-CN");
        assert_eq!(language_for_locale("cs_CZ.UTF-8"), "cs");
        assert_eq!(language_for_locale("nl_BE"), "nl");
        assert_eq!(language_for_locale("it_IT"), "it");
        assert_eq!(language_for_locale("pl_PL"), "pl");
        assert_eq!(language_for_locale("tr_TR.UTF-8"), "tr");
        assert_eq!(language_for_locale("pt_BR.UTF-8"), "pt-BR");
        assert_eq!(language_for_locale("sv_SE"), "en");
        assert_eq!(language_for_locale(""), "en");
        assert_eq!(language_for_locale("lo_LA.UTF-8"), "lo");
    }

    #[test]
    fn locales_match_the_closest_supported_language() {
        // Another region of a shipped language.
        assert_eq!(match_locale("pt-PT"), Some("pt-BR"));
        assert_eq!(match_locale("pt_PT.UTF-8"), Some("pt-BR"));
        assert_eq!(match_locale("es-MX"), Some("es"));
        assert_eq!(match_locale("fr-CA"), Some("fr"));
        assert_eq!(match_locale("de-CH"), Some("de"));
        assert_eq!(match_locale("en-GB"), Some("en"));
        // Chinese by script, then region.
        assert_eq!(match_locale("zh-Hant"), Some("zh-TW"));
        assert_eq!(match_locale("zh-Hant-HK"), Some("zh-TW"));
        assert_eq!(match_locale("zh-Hans-HK"), Some("zh-CN"));
        assert_eq!(match_locale("zh-Hans"), Some("zh-CN"));
        assert_eq!(match_locale("zh-MO"), Some("zh-TW"));
        assert_eq!(match_locale("zh"), Some("zh-CN"));
        assert_eq!(match_locale("lo"), Some("lo"));
        assert_eq!(match_locale("lo-LA"), Some("lo"));
        // Codes compared whole, not by their first letters.
        assert_eq!(match_locale("arn-CL"), None);
        assert_eq!(match_locale("ido"), None);
        assert_eq!(match_locale("in_ID"), Some("id"));
        assert_eq!(match_locale("sr_RS@latin"), None);
        assert_eq!(match_locale("C"), None);
        assert_eq!(match_locale("POSIX"), None);
        assert_eq!(match_locale("C.UTF-8"), None);
        assert_eq!(match_locale("  "), None);
        // The first shipped language of a list of preferences.
        assert_eq!(best_language(&["sv-SE", "nb", "de-AT"]), Some("de"));
        assert_eq!(best_language(&["sv-SE"]), None);
        assert_eq!(best_language::<&str>(&[]), None);
    }

    #[test]
    fn windows_language_ids_map_to_our_codes() {
        assert_eq!(language_for_langid(0x0409), Some("en"));
        assert_eq!(language_for_langid(0x0809), Some("en"));
        assert_eq!(language_for_langid(0x0416), Some("pt-BR"));
        assert_eq!(language_for_langid(0x0816), Some("pt-BR"));
        assert_eq!(language_for_langid(0x0404), Some("zh-TW"));
        assert_eq!(language_for_langid(0x0c04), Some("zh-TW"));
        assert_eq!(language_for_langid(0x1404), Some("zh-TW"));
        assert_eq!(language_for_langid(0x0804), Some("zh-CN"));
        assert_eq!(language_for_langid(0x1004), Some("zh-CN"));
        assert_eq!(language_for_langid(0x0407), Some("de"));
        assert_eq!(language_for_langid(0x0c0a), Some("es"));
        assert_eq!(language_for_langid(0x0405), Some("cs"));
        assert_eq!(language_for_langid(0x041d), None); // Swedish
        assert_eq!(language_for_langid(0x0454), Some("lo"));
        // Every shipped language has an id.
        for (code, _) in LANGUAGES {
            assert!(
                (0..0x8000u16).any(|id| language_for_langid(id) == Some(code)),
                "{code}"
            );
        }
    }

    /// The installer's Pascal table (`AppLanguageForLangId`) gives the
    /// same code as `language_for_langid` for every language id, and its
    /// list of languages to pick from is ours.
    #[test]
    fn the_installer_maps_languages_as_the_app_does() {
        let iss = include_str!("../../../packaging/windows/tracedraw.iss");
        let mut primaries = Vec::new();
        for line in iss.lines() {
            let line = line.trim();
            // `$09: Result := 'en';`
            let Some((id, rest)) = line.split_once(": Result := '") else {
                continue;
            };
            let Some(id) = id.strip_prefix('$') else {
                continue;
            };
            let id = u16::from_str_radix(id, 16).expect("hex id");
            let code = rest.trim_end_matches("';");
            if id <= 0x3ff {
                // A primary language, in another region than the first.
                assert_eq!(language_for_langid(0x0800 | id), Some(code), "{id:#x}");
                primaries.push(id);
            } else {
                assert_eq!(language_for_langid(id), Some(code), "{id:#x}");
            }
        }
        assert_eq!(
            primaries.len(),
            LANGUAGES.len() - 1,
            "one per language, Chinese apart"
        );
        // The choices of the installer's page: every code, in our order.
        let codes: Vec<&str> = iss
            .lines()
            .filter_map(|l| l.trim().strip_prefix("AddAppLanguage('"))
            .filter_map(|l| l.split('\'').next())
            .collect();
        let ours: Vec<&str> = LANGUAGES.iter().map(|(c, _)| *c).collect();
        assert_eq!(codes, ours);
    }

    #[test]
    fn the_installer_value_is_a_code_or_system() {
        assert_eq!(installer_choice("system"), Some(SYSTEM));
        assert_eq!(installer_choice("System"), Some(SYSTEM));
        assert_eq!(installer_choice("pt-BR"), Some("pt-BR"));
        assert_eq!(installer_choice("pt-br"), Some("pt-BR"));
        assert_eq!(installer_choice(" de "), Some("de"));
        assert_eq!(installer_choice("klingon"), None);
        assert_eq!(installer_choice(""), None);
    }

    /// Picked in Options > installer > system > English.
    #[test]
    fn the_start_language_follows_the_precedence() {
        let system = ["sv-SE", "fr-FR"];
        // A language picked in Options always wins.
        assert_eq!(startup_language("de", Some("ja"), &system), "de");
        assert_eq!(startup_language("zh-TW", Some("system"), &system), "zh-TW");
        assert_eq!(startup_language("lo", Some("en"), &system), "lo");
        // An English that was picked (or saved by an older version) stays.
        assert_eq!(startup_language("en", Some("ja"), &system), "en");
        // System language picked in Options: the system, not the installer.
        assert_eq!(startup_language("system", Some("ja"), &system), "fr");
        assert_eq!(startup_language("system", None, &["sv"]), "en");
        // Nothing picked: the installer's language.
        assert_eq!(startup_language("", Some("ja"), &system), "ja");
        // The installer left at System language: the system's.
        assert_eq!(startup_language("", Some("system"), &system), "fr");
        // No installer (zip, Linux, macOS, Store), or a value we do not
        // know: the system's.
        assert_eq!(startup_language("", None, &system), "fr");
        assert_eq!(startup_language("", Some("xx"), &system), "fr");
        assert_eq!(startup_language("", None, &["pt-PT"]), "pt-BR");
        // An unknown saved language counts as none picked.
        assert_eq!(startup_language("sv", Some("it"), &system), "it");
        // Nothing at all: English.
        assert_eq!(startup_language::<&str>("", None, &[]), "en");
    }

    #[test]
    fn parses_escapes_and_comments() {
        let m = parse("# c\na.b = \"x \\\"y\\\" \\n z\"\nbad\n");
        assert_eq!(m.get("a.b").map(String::as_str), Some("x \"y\" \n z"));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn every_language_covers_english_keys() {
        let en = parse(FILES[0].1);
        assert!(en.len() > 100);
        for (code, src) in FILES.iter().skip(1) {
            let t = parse(src);
            let missing: Vec<_> = en.keys().filter(|k| !t.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{code} is missing {missing:?}");
        }
    }

    /// The desktop keeps every language built in, in the order of the
    /// language list.
    #[test]
    fn the_desktop_builds_every_language_in() {
        assert_eq!(FILES.len(), LANGUAGES.len());
        for ((file, _), (code, _)) in FILES.iter().zip(LANGUAGES.iter()) {
            assert_eq!(file, code);
            assert!(builtin(code).is_some(), "{code}");
        }
    }

    #[test]
    fn the_browser_fetches_every_language_but_english() {
        assert_eq!(file_url("en"), None);
        assert_eq!(file_url("zz"), None);
        assert_eq!(file_url("pt-BR").as_deref(), Some("lang/pt-BR.toml"));
        for (code, _) in LANGUAGES.iter().skip(1) {
            let url = file_url(code).expect("a file to fetch");
            let path = format!("{}/{url}", env!("CARGO_MANIFEST_DIR"));
            assert!(std::path::Path::new(&path).is_file(), "{path}");
        }
    }

    /// The browser build: English shows until the chosen language's file
    /// arrives, then the interface switches; the file is asked for once.
    #[test]
    fn a_fetched_language_shows_english_until_it_arrives() {
        let de = builtin("de").expect("German");
        let mut st = State::new(parse(FILES[0].1));
        assert_eq!(st.choose("de", None).as_deref(), Some("lang/de.toml"));
        assert_eq!(st.code, "de");
        assert_eq!(st.shown, "en");
        assert_eq!(st.table.get("menu.file").map(String::as_str), Some("File"));
        // Chosen again while on its way: not asked for twice.
        assert_eq!(st.choose("de", None), None);
        assert_eq!(st.arrived("de", Some(de)), Ok(true));
        assert_eq!(st.shown, "de");
        assert_eq!(st.table, parse(de));
        // Back to English and to German again: no new fetch.
        assert_eq!(st.choose("en", builtin("en")), None);
        assert_eq!(st.shown, "en");
        assert_eq!(st.choose("de", None), None);
        assert_eq!(st.table, parse(de));
    }

    /// A file that arrives after another language was chosen is kept for
    /// later and does not change the interface.
    #[test]
    fn a_late_language_file_is_kept_but_not_shown() {
        let fr = builtin("fr").expect("French");
        let mut st = State::new(parse(FILES[0].1));
        assert!(st.choose("fr", None).is_some());
        assert_eq!(st.choose("it", None).as_deref(), Some("lang/it.toml"));
        assert_eq!(st.arrived("fr", Some(fr)), Ok(false));
        assert_eq!(st.shown, "en");
        assert_eq!(st.choose("fr", None), None);
        assert_eq!(st.shown, "fr");
    }

    /// A missing or broken language file leaves the interface in English,
    /// and choosing the language again asks for the file again.
    #[test]
    fn a_missing_language_falls_back_to_english() {
        let mut st = State::new(parse(FILES[0].1));
        assert!(st.choose("ja", None).is_some());
        assert!(st.arrived("ja", None).is_err());
        assert_eq!((st.code.as_str(), st.shown.as_str()), ("ja", "en"));
        assert_eq!(st.table.get("menu.file").map(String::as_str), Some("File"));
        assert!(st.choose("ja", None).is_some());
        let page = "<!doctype html>\n<meta charset=\"utf-8\">\n<title>404</title>";
        assert!(st.arrived("ja", Some(page)).is_err());
        assert_eq!(st.shown, "en");
        assert!(st.choose("ja", None).is_some());
        // Unknown codes have nothing to fetch.
        assert_eq!(st.choose("zz", None), None);
        assert_eq!(st.shown, "en");
    }

    #[test]
    fn fallback_to_english_then_key() {
        set_language("zz");
        assert_eq!(tr("menu.file"), "File");
        assert_eq!(tr("no.such.key"), "key");
    }
}
