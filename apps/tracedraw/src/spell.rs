//! Spell check over the document's text. Dictionaries: Hunspell
//! dictionaries (a `.dic` word file with its `.aff` affix file, read by
//! [`crate::hunspell`]) found in the system's folders, in TraceDraw's own
//! `dictionaries` folder and in a folder the user names (Options > Text >
//! Spelling, which also switches each one off); plain word lists
//! (`/usr/share/dict/*`, a `.dic` with no `.aff`); a built-in core list of
//! very common English and Portuguese words; and the user's own additions
//! (`<config>/dictionary.txt`). A word is right when any of them knows it.

use crate::app::App;
use crate::hunspell::Hunspell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use tracedraw_core::{document::ShapeKind, Command, ShapeId, TextSpan};

const CORE_WORDS: &str = include_str!("../lang/core-words.txt");

/// Which dictionaries to use: a folder of the user's and the names of
/// those switched off.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpellConfig {
    pub folder: Option<PathBuf>,
    pub disabled: Vec<String>,
}

/// A dictionary found on disk: its name (the file name without `.dic`,
/// such as `pt_BR`), its word file and its affix file (none for a plain
/// word list).
#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryFile {
    pub name: String,
    pub dic: PathBuf,
    pub aff: Option<PathBuf>,
}

struct Loaded {
    plain: HashSet<String>,
    hunspell: Vec<Hunspell>,
    size: usize,
}

type State = RwLock<(SpellConfig, Option<Arc<Loaded>>)>;

#[cfg(not(test))]
fn state() -> &'static State {
    static STATE: OnceLock<State> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new((SpellConfig::default(), None)))
}

/// In tests each thread (each test) has dictionaries of its own: an app
/// made by another test configures them from its settings, and shared
/// they changed under a test checking a folder of its own.
#[cfg(test)]
fn state() -> &'static State {
    thread_local! {
        static STATE: &'static State =
            Box::leak(Box::new(RwLock::new((SpellConfig::default(), None))));
    }
    STATE.with(|s| *s)
}

/// Use the dictionaries `cfg` names from now on (read again on the next
/// check when they changed).
pub fn configure(cfg: SpellConfig) {
    let mut st = state().write().unwrap_or_else(|e| e.into_inner());
    if st.0 != cfg {
        *st = (cfg, None);
    }
}

/// Read the dictionaries again on the next check (files were added).
pub fn reload() {
    let mut st = state().write().unwrap_or_else(|e| e.into_inner());
    st.1 = None;
}

fn loaded() -> Arc<Loaded> {
    if let Some(l) = state().read().unwrap_or_else(|e| e.into_inner()).1.clone() {
        return l;
    }
    let cfg = state().read().unwrap_or_else(|e| e.into_inner()).0.clone();
    let l = Arc::new(load(&cfg));
    let mut st = state().write().unwrap_or_else(|e| e.into_inner());
    if st.0 == cfg {
        st.1 = Some(l.clone());
    }
    l
}

fn load(cfg: &SpellConfig) -> Loaded {
    let mut plain: HashSet<String> = CORE_WORDS
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect();
    for path in word_lists() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            plain.extend(
                text.lines()
                    .map(|l| l.trim().to_lowercase())
                    .filter(|w| !w.is_empty()),
            );
        }
    }
    let mut hunspell = Vec::new();
    for d in find_dictionaries(cfg.folder.as_deref()) {
        if cfg.disabled.contains(&d.name) {
            continue;
        }
        let Ok(dic) = std::fs::read(&d.dic) else {
            continue;
        };
        let h = match d.aff.as_ref().and_then(|a| std::fs::read(a).ok()) {
            Some(aff) => Hunspell::from_bytes(&aff, &dic),
            None => Hunspell::from_word_list(&dic),
        };
        if !h.is_empty() {
            hunspell.push(h);
        }
    }
    let size = plain.len() + hunspell.iter().map(Hunspell::len).sum::<usize>();
    Loaded {
        plain,
        hunspell,
        size,
    }
}

/// Plain word lists of the system, one word per line.
fn word_lists() -> Vec<PathBuf> {
    [
        "/usr/share/dict/words",
        "/usr/share/dict/american-english",
        "/usr/share/dict/british-english",
        "/usr/share/dict/portuguese",
        "/usr/share/dict/brazilian",
    ]
    .into_iter()
    .map(PathBuf::from)
    .filter(|p| p.is_file())
    .collect()
}

/// TraceDraw's own folder for dictionaries (`<config>/dictionaries`).
pub fn own_folder() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("dictionaries"))
}

/// The folders searched, the user's first: TraceDraw's own folder and the
/// one named in the options (each with the folders inside it, three deep,
/// so a dictionary package unpacked there or another program's dictionary
/// folder works), then the system's.
fn folders(extra: Option<&Path>) -> Vec<(PathBuf, usize)> {
    let mut v: Vec<(PathBuf, usize)> = Vec::new();
    if let Some(own) = own_folder() {
        v.push((own, 3));
    }
    if let Some(e) = extra {
        v.push((e.to_path_buf(), 3));
    }
    for dir in [
        "/usr/share/hunspell",
        "/usr/share/myspell",
        "/usr/share/myspell/dicts",
        "/Library/Spelling",
        "/usr/local/share/hunspell",
    ] {
        v.push((PathBuf::from(dir), 0));
    }
    v
}

/// What a folder scan looks for.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Spelling dictionaries and word lists.
    Spelling,
    /// Hyphenation patterns (`hyph_*.dic`).
    Hyphenation,
}

fn scan(dir: &Path, depth: usize, out: &mut Vec<DictionaryFile>) {
    scan_for(dir, depth, Kind::Spelling, out);
}

fn scan_for(dir: &Path, depth: usize, kind: Kind, out: &mut Vec<DictionaryFile>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            if depth > 0 {
                scan_for(&p, depth - 1, kind, out);
            }
            continue;
        }
        let is_dic = p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("dic"));
        let Some(name) = p.file_stem().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        // Hyphenation patterns and thesauri share the extension.
        let lower = name.to_lowercase();
        let hyph = lower.starts_with("hyph_");
        let wanted = match kind {
            Kind::Spelling => !hyph && !lower.starts_with("th_"),
            Kind::Hyphenation => hyph,
        };
        if !is_dic || !wanted {
            continue;
        }
        if out.iter().any(|d| d.name == name) {
            continue;
        }
        let aff = p.with_extension("aff");
        out.push(DictionaryFile {
            name,
            aff: (kind == Kind::Spelling && aff.is_file()).then_some(aff),
            dic: p,
        });
    }
}

/// The dictionaries found, each name once (the first found wins).
pub fn find_dictionaries(extra: Option<&Path>) -> Vec<DictionaryFile> {
    let mut out = Vec::new();
    for (dir, depth) in folders(extra) {
        scan(&dir, depth, &mut out);
    }
    out
}

/// The hyphenation pattern files found (`hyph_pt_BR`), each name once.
pub fn find_hyphenation(extra: Option<&Path>) -> Vec<DictionaryFile> {
    let mut out = Vec::new();
    for (dir, depth) in folders(extra) {
        scan_for(&dir, depth, Kind::Hyphenation, &mut out);
    }
    out
}

/// The patterns of a hyphenation file: its first line names the
/// character set the rest is written in.
pub fn read_hyphenation(path: &Path) -> Option<tracedraw_text::hyphen::PatternSet> {
    let bytes = std::fs::read(path).ok()?;
    let first = bytes.split(|b| *b == b'\n').next().unwrap_or(&[]);
    let enc = crate::hunspell::encoding_for(&String::from_utf8_lossy(first));
    let (text, _, _) = enc.decode(&bytes);
    tracedraw_text::hyphen::PatternSet::parse(&text)
}

/// Hyphenate with the pattern file named `name` (found in the dictionary
/// folders; empty: the built-in patterns). False when it was not found or
/// held no patterns, the built-in ones then being used.
pub fn use_hyphenation(name: &str, extra: Option<&Path>) -> bool {
    static CURRENT: OnceLock<RwLock<(String, Option<PathBuf>)>> = OnceLock::new();
    let current = CURRENT.get_or_init(|| RwLock::new((String::new(), None)));
    let key = (name.to_string(), extra.map(Path::to_path_buf));
    if *current.read().unwrap_or_else(|e| e.into_inner()) == key {
        return true;
    }
    let set = if name.is_empty() {
        None
    } else {
        find_hyphenation(extra)
            .into_iter()
            .find(|d| d.name == name)
            .and_then(|d| read_hyphenation(&d.dic))
    };
    let found = name.is_empty() || set.is_some();
    tracedraw_text::hyphen::set_patterns(set);
    *current.write().unwrap_or_else(|e| e.into_inner()) = key;
    found
}

fn user_dictionary_path() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("dictionary.txt"))
}

/// The user's own words (Add in the Spell Checker), lower case.
fn user_words() -> &'static RwLock<HashSet<String>> {
    static USER: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();
    USER.get_or_init(|| {
        let words = user_dictionary_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| t.split_whitespace().map(str::to_lowercase).collect())
            .unwrap_or_default();
        RwLock::new(words)
    })
}

pub fn add_user_word(word: &str) {
    user_words()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(word.to_lowercase());
    if let Some(p) = user_dictionary_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut text = std::fs::read_to_string(&p).unwrap_or_default();
        text.push_str(word);
        text.push('\n');
        let _ = std::fs::write(p, text);
    }
}

pub fn is_known(word: &str) -> bool {
    let w = word.to_lowercase();
    if w.chars().any(|c| c.is_ascii_digit()) || w.chars().count() < 2 {
        return true;
    }
    let l = loaded();
    let user = user_words().read().unwrap_or_else(|e| e.into_inner());
    let known = |p: &str| {
        let lower = p.to_lowercase();
        l.plain.contains(&lower) || user.contains(&lower) || l.hunspell.iter().any(|h| h.check(p))
    };
    if known(word) {
        return true;
    }
    // A hyphenated word whose parts are words.
    word.contains('-')
        && word
            .split('-')
            .filter(|p| !p.is_empty())
            .all(|p| p.chars().count() < 2 || known(p))
}

/// Words in a text that the dictionary does not know.
pub fn unknown_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split(|c: char| !(c.is_alphabetic() || c == '\'' || c == '-')) {
        let w = raw.trim_matches(|c: char| c == '\'' || c == '-');
        if w.is_empty() || is_known(w) {
            continue;
        }
        if !out.iter().any(|x: &String| x == w) {
            out.push(w.to_string());
        }
    }
    out
}

/// Check every text object on the current page: (shape, word) pairs and
/// the dictionary size (0 means no dictionary was found).
pub fn check_document(app: &App) -> (Vec<(ShapeId, String)>, usize) {
    let size = loaded().size + user_words().read().map(|u| u.len()).unwrap_or(0);
    let mut out = Vec::new();
    if let Ok(page) = app.doc().page(app.page) {
        for s in page.layers.iter().flat_map(|l| &l.shapes) {
            if let ShapeKind::Text { spans, .. } = &s.kind {
                let t: String = spans.iter().map(|x| x.text.as_str()).collect();
                for w in unknown_words(&t) {
                    out.push((s.id, w));
                }
            }
        }
    }
    (out, size)
}

/// Suggestions: the Hunspell dictionaries' own (their replacement
/// tables and edits with their letters), then edits by one letter found
/// in the word lists; eight at most.
pub fn suggest(word: &str) -> Vec<String> {
    const MAX: usize = 8;
    let l = loaded();
    let mut out: Vec<String> = Vec::new();
    for h in &l.hunspell {
        for s in h.suggest(word, MAX) {
            if !out.contains(&s) && out.len() < MAX {
                out.push(s);
            }
        }
    }
    let w = word.to_lowercase();
    let d = &l.plain;
    let chars: Vec<char> = w.chars().collect();
    let alphabet = "abcdefghijklmnopqrstuvwxyzáéíóúâêôãõçàü";
    let mut push = |s: String| {
        if s != w && d.contains(&s) && !out.contains(&s) && out.len() < MAX {
            out.push(s);
        }
    };
    for i in 0..chars.len() {
        // Deletion.
        let mut s: String = chars[..i].iter().collect();
        s.extend(chars[i + 1..].iter());
        push(s);
        // Transposition.
        if i + 1 < chars.len() {
            let mut t = chars.clone();
            t.swap(i, i + 1);
            push(t.iter().collect());
        }
        // Replacement.
        for a in alphabet.chars() {
            let mut t = chars.clone();
            t[i] = a;
            push(t.iter().collect());
        }
    }
    for i in 0..=chars.len() {
        for a in alphabet.chars() {
            let mut t = chars.clone();
            t.insert(i, a);
            push(t.iter().collect());
        }
    }
    out
}

pub fn replace_word(app: &mut App, id: ShapeId, word: &str, replacement: &str) {
    let Some(s) = app.doc().find_shape(id).cloned() else {
        return;
    };
    if let ShapeKind::Text {
        spans,
        origin,
        frame,
        align,
        para,
        on_path,
    } = s.kind
    {
        let new_spans: Vec<TextSpan> = spans
            .into_iter()
            .map(|sp| {
                let text = replace_whole_word(&sp.text, word, replacement);
                TextSpan { text, ..sp }
            })
            .collect();
        app.run(Command::SetShapeKind {
            shape: id,
            kind: ShapeKind::Text {
                spans: new_spans,
                origin,
                frame,
                align,
                para,
                on_path,
            },
        });
    }
}

fn replace_whole_word(text: &str, word: &str, replacement: &str) -> String {
    let mut out = String::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut String| {
        if !cur.is_empty() {
            if *cur == word {
                out.push_str(replacement);
            } else {
                out.push_str(cur);
            }
            cur.clear();
        }
    };
    for c in text.chars() {
        if c.is_alphabetic() || c == '\'' || c == '-' {
            cur.push(c);
        } else {
            flush(&mut cur, &mut out);
            out.push(c);
        }
    }
    flush(&mut cur, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_words_are_known_and_nonsense_is_not() {
        assert!(is_known("the"));
        assert!(is_known("casa"));
        assert!(!is_known("qzxvbnm"));
        assert_eq!(unknown_words("the qzxvbnm casa 42"), vec!["qzxvbnm"]);
    }

    /// Dictionaries in a folder of the user's: found with the folders
    /// inside it, hyphenation patterns and thesauri left out, a `.dic`
    /// without `.aff` read as a word list; used once configured, and not
    /// once switched off.
    #[test]
    fn dictionaries_in_a_folder_are_found_and_used() {
        let dir = std::env::temp_dir().join(format!("tracedraw-dicts-{}", std::process::id()));
        let inner = dir.join("blorfish");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(
            inner.join("xx_TEST.aff"),
            "SET UTF-8\nSFX S Y 1\nSFX S 0 s .\nPFX U Y 1\nPFX U 0 un .\n",
        )
        .unwrap();
        std::fs::write(inner.join("xx_TEST.dic"), "2\nblorf/SU\nZquee\n").unwrap();
        std::fs::write(dir.join("hyph_xx.dic"), "UTF-8\n1a\n").unwrap();
        std::fs::write(dir.join("th_xx.dic"), "x").unwrap();
        std::fs::write(dir.join("plainlist.dic"), "wibblewort\n").unwrap();
        let found = find_dictionaries(Some(&dir));
        let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains(&"xx_TEST") && names.contains(&"plainlist"),
            "{names:?}"
        );
        assert!(!names
            .iter()
            .any(|n| n.starts_with("hyph_") || n.starts_with("th_")));
        let test = found.iter().find(|d| d.name == "xx_TEST").unwrap();
        assert!(test.aff.is_some());
        assert!(found
            .iter()
            .find(|d| d.name == "plainlist")
            .unwrap()
            .aff
            .is_none());

        assert!(!is_known("unblorfs"));
        configure(SpellConfig {
            folder: Some(dir.clone()),
            disabled: Vec::new(),
        });
        assert!(is_known("unblorfs"));
        assert!(is_known("Blorf"));
        assert!(is_known("Zquee"));
        assert!(is_known("wibblewort"));
        assert!(is_known("blorf-wibblewort"));
        assert!(suggest("blorff").contains(&"blorf".to_string()));
        configure(SpellConfig {
            folder: Some(dir.clone()),
            disabled: vec!["xx_TEST".into()],
        });
        assert!(!is_known("unblorfs"));
        assert!(is_known("wibblewort"));
        configure(SpellConfig::default());
        // Hyphenation files beside them, in their own character set.
        std::fs::write(
            inner.join("hyph_yy.dic"),
            b"ISO8859-1\nLEFTHYPHENMIN 1\n1\xe7a\n",
        )
        .unwrap();
        let hyph = find_hyphenation(Some(&dir));
        let names: Vec<&str> = hyph.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains(&"hyph_xx") && names.contains(&"hyph_yy"),
            "{names:?}"
        );
        assert_eq!(names.len(), 2);
        let yy = hyph.iter().find(|d| d.name == "hyph_yy").unwrap();
        let set = read_hyphenation(&yy.dic).expect("patterns");
        assert_eq!((set.len(), set.left_min), (1, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real hyphenation dictionaries, where `HUNSPELL_TEST_DIR` holds
    /// them (not in the repository): English, Brazilian Portuguese,
    /// German (eight-bit, two levels) and Italian (apostrophes, words
    /// followed by punctuation, #255).
    #[test]
    #[ignore]
    fn real_hyphenation_files() {
        let Some(dir) = std::env::var_os("HUNSPELL_TEST_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let found = find_hyphenation(Some(&dir));
        let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
        eprintln!("{names:?}");
        let cases: [(&str, &str, &[usize]); 8] = [
            ("hyph_en_US", "hyphenation", &[2, 6]),
            ("hyph_pt_BR", "computador", &[3, 5, 7]),
            ("hyph_de_DE", "Haustür", &[4]),
            ("hyph_de_DE", "Schifffahrt", &[6]),
            ("hyph_it_IT", "informazione", &[2, 5, 7]),
            ("hyph_it_IT", "informazione,", &[2, 5, 7]),
            ("hyph_it_IT", "dell'università", &[8, 11]),
            ("hyph_it_IT", "«sillabazione»", &[4, 6, 8]),
        ];
        for (name, word, want) in cases {
            let d = found.iter().find(|d| d.name == name).expect(name);
            let set = read_hyphenation(&d.dic).expect("patterns");
            tracedraw_text::hyphen::set_patterns(Some(set));
            let got = tracedraw_text::hyphen::break_points(word);
            tracedraw_text::hyphen::set_patterns(None);
            eprintln!("{name}: {word} -> {got:?}");
            for w in want {
                assert!(got.contains(w), "{name}: {word} {got:?} lacks {w}");
            }
        }
    }

    #[test]
    fn suggestions_fix_a_typo() {
        let s = suggest("hte");
        assert!(s.contains(&"the".to_string()), "{s:?}");
    }

    #[test]
    fn whole_word_replace() {
        assert_eq!(
            replace_whole_word("a cat catalog", "cat", "dog"),
            "a dog catalog"
        );
    }
}
