//! What the macros' object model offers, read from its source
//! (`vg_model.js`): each member's name, whether it is a method, its
//! parameters, and whether it works, does nothing, or raises an error
//! that it is not available. The Macro Editor's compatibility check and
//! the conversion to JavaScript use it (docs/behavior/vba.md,
//! "Converting macros").

use super::vg::PRELUDE;
use std::collections::HashMap;

/// What a member does here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Works,
    /// Accepted without effect.
    NoEffect,
    /// Raises an error naming it; the reason, when the model gives one.
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub name: String,
    pub owner: String,
    pub method: bool,
    pub params: Vec<String>,
    pub status: Status,
}

fn ident(s: &str) -> Option<&str> {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(s.len());
    let id = &s[..end];
    (!id.is_empty() && !id.starts_with(|c: char| c.is_ascii_digit())).then_some(id)
}

fn params(list: &str) -> Vec<String> {
    list.split(',')
        .map(|p| {
            p.split('=')
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches("...")
        })
        .filter(|p| !p.is_empty())
        .map(|p| p.to_ascii_lowercase())
        .collect()
}

/// The status of a member from its body (what follows its parameter
/// list).
fn status_of(body: &str) -> Status {
    let b = body.trim_start();
    let b = b.strip_prefix('{').unwrap_or(b).trim_start();
    if b.starts_with('}') {
        return Status::NoEffect;
    }
    let inner = b.strip_prefix("return ").unwrap_or(b);
    if let Some(rest) = inner.strip_prefix("__no(") {
        // __no('Owner.Member', 'why')
        let parts: Vec<&str> = rest.split('\'').collect();
        let why = parts.get(3).copied().unwrap_or("");
        return Status::Unavailable(why.to_string());
    }
    if let Some(rest) = inner.strip_prefix("throw new Error(") {
        let why = rest
            .trim_start_matches(['\'', '"'])
            .split(['\'', '"'])
            .next()
            .unwrap_or("");
        return Status::Unavailable(why.to_string());
    }
    Status::Works
}

fn scan(src: &str) -> Vec<Member> {
    let mut out = Vec::new();
    let mut owner: Option<String> = None;
    let lines: Vec<&str> = src.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if let Some(rest) = line.strip_prefix("class ") {
            owner = ident(rest).map(str::to_string);
            continue;
        }
        if let Some(rest) = line.strip_prefix("const ") {
            if let Some(n) = ident(rest) {
                if rest[n.len()..].trim_start().starts_with("= {")
                    && (!n.starts_with("__") || n == "__status")
                {
                    owner = Some(n.to_string());
                    continue;
                }
            }
        }
        if line.starts_with('}') {
            owner = None;
        }
        // __def(Cls, 'Name', function () { ... }, ...)
        if let Some(rest) = line.strip_prefix("__def(") {
            let parts: Vec<&str> = rest.splitn(3, ',').collect();
            if parts.len() == 3 {
                let cls = parts[0].trim();
                let name = parts[1].trim().trim_matches(['\'', '"']);
                let body = parts[2].split_once(')').map(|(_, b)| b).unwrap_or("");
                out.push(Member {
                    name: name.to_string(),
                    owner: cls.to_string(),
                    method: false,
                    params: Vec::new(),
                    status: status_of(body),
                });
            }
            continue;
        }
        // Owner.prototype.Method = function (a) { ... } or Owner.Method = function
        if !line.starts_with(' ') {
            if let Some((head, tail)) = line.split_once(" = function") {
                let path: Vec<&str> = head.split('.').collect();
                if path.len() >= 2 && path.iter().all(|p| ident(p) == Some(*p)) {
                    let name = path[path.len() - 1];
                    let (args, body) = match (tail.find('('), tail.find(')')) {
                        (Some(a), Some(b)) if a < b => (&tail[a + 1..b], &tail[b + 1..]),
                        _ => ("", tail),
                    };
                    out.push(Member {
                        name: name.to_string(),
                        owner: path[0].to_string(),
                        method: true,
                        params: params(args),
                        status: status_of(body),
                    });
                }
            }
            continue;
        }
        let Some(cls) = &owner else {
            continue;
        };
        let Some(member) = line.strip_prefix("  ") else {
            continue;
        };
        if member.starts_with(' ') {
            continue;
        }
        let member = member.strip_prefix("static ").unwrap_or(member);
        let (member, prop) = match member
            .strip_prefix("get ")
            .or_else(|| member.strip_prefix("set "))
        {
            Some(m) => (m, true),
            None => (member, false),
        };
        let Some(m) = ident(member) else {
            continue;
        };
        if m == "constructor" || m.starts_with('_') {
            continue;
        }
        let tail = &member[m.len()..];
        // Name: value in an object literal: a property.
        if !prop && tail.trim_start().starts_with(':') {
            out.push(Member {
                name: m.to_string(),
                owner: cls.clone(),
                method: false,
                params: Vec::new(),
                status: Status::Works,
            });
            continue;
        }
        let Some(args) = tail.strip_prefix('(') else {
            continue;
        };
        let Some(close) = args.find(')') else {
            continue;
        };
        let mut body = args[close + 1..].to_string();
        if body.trim() == "{" {
            // A body over several lines: the next line starts it.
            if let Some(next) = lines.get(i + 1) {
                body.push_str(next);
            }
        }
        if !body.trim_start().starts_with('{') {
            continue;
        }
        out.push(Member {
            name: m.to_string(),
            owner: cls.clone(),
            method: !prop,
            params: if prop {
                Vec::new()
            } else {
                params(&args[..close])
            },
            status: status_of(&body),
        });
    }
    out
}

/// Every member of the object model, by lower-case name; a name several
/// objects have takes the best status any of them gives it.
pub fn catalog() -> &'static HashMap<String, Member> {
    static CAT: std::sync::OnceLock<HashMap<String, Member>> = std::sync::OnceLock::new();
    CAT.get_or_init(|| {
        let mut map: HashMap<String, Member> = HashMap::new();
        for m in scan(PRELUDE) {
            let k = m.name.to_ascii_lowercase();
            let rank = |s: &Status| match s {
                Status::Works => 0,
                Status::NoEffect => 1,
                Status::Unavailable(_) => 2,
            };
            match map.get_mut(&k) {
                Some(old) => {
                    if rank(&m.status) < rank(&old.status) {
                        old.status = m.status.clone();
                    }
                    if m.method && !old.method && old.params.is_empty() {
                        // A method on one object, a property on another:
                        // keep it a property (indexing reads its item).
                    }
                    if old.params.is_empty() && !m.params.is_empty() {
                        old.params = m.params.clone();
                    }
                }
                None => {
                    map.insert(k, m);
                }
            }
        }
        map
    })
}

/// Every word the object model's source and the VBA runtime use: a
/// member a macro names that is in neither may not exist here.
pub fn known_words() -> &'static std::collections::HashSet<String> {
    static W: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    W.get_or_init(|| {
        let mut set = std::collections::HashSet::new();
        for src in [
            PRELUDE,
            tracedraw_vba::RUNTIME_JS,
            super::vba_run::GLUE,
            include_str!("../macro_forms.rs"),
        ] {
            for w in src.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if !w.is_empty() {
                    set.insert(w.to_ascii_lowercase());
                }
            }
        }
        set
    })
}

/// JavaScript that lets `for (x of collection)` walk the object model's
/// collections (those with `Item` or `Count`), as VBA's For Each does.
pub fn iterable_js() -> String {
    let names: Vec<&str> = PRELUDE
        .lines()
        .filter_map(|l| l.strip_prefix("class ").and_then(ident))
        .collect();
    format!(
        "for (const __c of [{}]) {{ try {{ const p = __c.prototype; if (p && !p[Symbol.iterator] && ('Item' in p || 'Count' in p)) p[Symbol.iterator] = function () {{ return __v.each(this)[Symbol.iterator](); }}; }} catch (e) {{}} }}\n",
        names.join(", ")
    )
}

/// The member another name of a member stands for (`__member_alias` in
/// the model: names kept as FNV-1a hashes of their lower-case spelling).
pub fn alias_of(name: &str) -> Option<&'static str> {
    let mut h: u32 = 0x811c9dc5;
    for b in name.to_lowercase().bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x01000193);
    }
    aliases().get(&h).copied()
}

fn aliases() -> &'static HashMap<u32, &'static str> {
    static A: std::sync::OnceLock<HashMap<u32, &'static str>> = std::sync::OnceLock::new();
    A.get_or_init(|| {
        let mut map = HashMap::new();
        let Some(start) = PRELUDE.find("const __memberAliases = new Map([") else {
            return map;
        };
        let rest = &PRELUDE[start..];
        let end = rest.find("]);").unwrap_or(rest.len());
        for line in rest[..end].lines() {
            let l = line.trim().trim_start_matches('[');
            let Some((num, name)) = l.split_once(',') else {
                continue;
            };
            let (Ok(n), Some(name)) = (num.trim().parse::<u32>(), name.split('\'').nth(1)) else {
                continue;
            };
            // The name lives in the static source text.
            if let Some(at) = PRELUDE.find(&format!("'{name}'")) {
                map.insert(n, &PRELUDE[at + 1..at + 1 + name.len()]);
            }
        }
        map
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_knows_what_works_and_what_does_not() {
        let c = catalog();
        assert_eq!(c["createrectangle"].status, Status::Works);
        assert!(c["createrectangle"].method);
        assert_eq!(c["createrectangle"].params[0], "left");
        assert!(!c["positionx"].method);
        assert!(matches!(c["getuserclick"].status, Status::Unavailable(_)));
        assert!(matches!(c["undo"].status, Status::Unavailable(_)));
        assert_eq!(c["tofitshaperange"].status, Status::NoEffect);
        assert!(known_words().contains("addtoclipframe"));
        assert_eq!(alias_of("AddToClipFrame"), None);
    }
}
