//! `check-seams`: the modules bound for redkiln's kits reach only what the kit will hold.
//!
//! wi-59cb6e (owner, 2026-11-08): when temper moves into redkiln, the language-neutral half of
//! the engine goes to the `redkiln-craft` and `redkiln-guard` kits, and the Rust policy stays in
//! Redkiln Rust's crates. This check keeps that a cut along module lines. Each kit-bound file may
//! name, outside its own module, only the modules and items listed for its crate; any other
//! `crate::` or `temper_domain::` path is a violation. It is a text scan (comments skipped, `use`
//! groups expanded), so a path built by a macro escapes it: a tripwire for review, not a proof.

use std::fmt;

/// Where a kit-bound module goes in redkiln.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kit {
    /// `redkiln-craft`: the completion gate, verdicts, the fingerprint, the ratchet, the cache.
    Craft,
    /// `redkiln-guard`: the shell reader and protected globs.
    Guard,
    /// Language-neutral helpers both kits need.
    Support,
}

impl fmt::Display for Kit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Craft => "redkiln-craft",
            Self::Guard => "redkiln-guard",
            Self::Support => "kit support",
        })
    }
}

/// The seam of one crate: its kit-bound modules and what they may reach.
#[derive(Debug)]
pub(crate) struct Seam {
    /// The crate's directory under `crates/`.
    pub(crate) krate: &'static str,
    /// Each kit-bound module, as a path under `src/` without `.rs`.
    pub(crate) modules: &'static [(&'static str, Kit)],
    /// First segments a `crate::` path may start with: kit-bound modules, and items `lib.rs`
    /// re-exports from them.
    pub(crate) crate_paths: &'static [&'static str],
    /// First segments a `temper_domain::` path may start with.
    pub(crate) domain_paths: &'static [&'static str],
}

/// The domain's kit-bound modules reach only each other, through module paths or the items
/// `lib.rs` re-exports from them.
const DOMAIN: Seam = Seam {
    krate: "temper-domain",
    modules: &[
        ("fingerprint", Kit::Craft),
        ("verdict", Kit::Craft),
        ("state", Kit::Craft),
        ("glob", Kit::Guard),
        ("hooks/bash/code", Kit::Guard),
        ("hooks/setup", Kit::Support),
        ("time", Kit::Support),
        ("js", Kit::Support),
        ("paths", Kit::Support),
    ],
    crate_paths: &[
        "fingerprint",
        "verdict",
        "state",
        "glob",
        "hooks::setup",
        "time",
        "js",
        "js_sort",
        "paths",
        "Fingerprint",
        "Timestamp",
    ],
    domain_paths: &[],
};

/// The application's kit-bound modules reach each other and the domain's kit-bound items.
const APP: Seam = Seam {
    krate: "temper-app",
    modules: &[
        ("hooks/shell", Kit::Guard),
        ("memo", Kit::Craft),
        ("paths", Kit::Support),
        ("relpath", Kit::Support),
        ("ports", Kit::Support),
    ],
    // `fakes` is `#[cfg(test)]` in `lib.rs`: the in-file tests of kit-bound modules use it, and
    // it goes with them as test support.
    crate_paths: &["hooks::shell", "memo", "paths", "relpath", "ports", "fakes"],
    domain_paths: &[
        "GlobError",
        "InterpreterCode",
        "ModTime",
        "OpaqueKind",
        "PathStyle",
        "Timestamp",
        "is_js_space",
        "js_trim",
        "unverbatim",
    ],
};

/// Every seam this check holds.
pub(crate) const SEAMS: [Seam; 2] = [DOMAIN, APP];

/// One path a kit-bound file names that its seam does not allow.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Violation {
    /// The file, relative to the workspace root.
    pub(crate) file: String,
    /// 1-based line of the path's first segment.
    pub(crate) line: usize,
    /// The path as written, with any `use` group expanded.
    pub(crate) path: String,
    /// The kit the file is bound for.
    pub(crate) kit: Kit,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: `{}` is outside the {} seam (xtask/src/seams.rs lists what it may reach)",
            self.file, self.line, self.path, self.kit
        )
    }
}

/// Whether `rel` (a path under the crate's `src/`, `/`-separated) is test-only: a `tests.rs` or
/// a file under a `tests/` directory. Fakes and fixtures there are test support, not kit code.
pub(crate) fn is_test_file(rel: &str) -> bool {
    let mut parts = rel.split('/').peekable();
    while let Some(part) = parts.next() {
        let is_dir = parts.peek().is_some();
        if part == "tests.rs" || (is_dir && part == "tests") {
            return true;
        }
    }
    false
}

/// The kit of the module `rel` belongs to, if it is kit-bound (`rel` is under `src/`, without
/// the `src/` prefix, `/`-separated).
pub(crate) fn kit_of(seam: &Seam, rel: &str) -> Option<Kit> {
    let stem = rel.strip_suffix(".rs")?;
    seam.modules
        .iter()
        .find(|(module, _)| {
            stem == *module
                || stem
                    .strip_prefix(module)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .map(|&(_, kit)| kit)
}

/// The violations in one kit-bound file's text.
pub(crate) fn check_file(seam: &Seam, file: &str, kit: Kit, text: &str) -> Vec<Violation> {
    references(text)
        .into_iter()
        .filter(|(_, path)| !allowed(seam, path))
        .map(|(line, path)| Violation {
            file: file.to_owned(),
            line,
            path,
            kit,
        })
        .collect()
}

fn allowed(seam: &Seam, path: &str) -> bool {
    if path == "crate" || path == "temper_domain" {
        return false;
    }
    let (list, rest) = if let Some(rest) = path.strip_prefix("crate::") {
        (seam.crate_paths, rest)
    } else if let Some(rest) = path.strip_prefix("temper_domain::") {
        (seam.domain_paths, rest)
    } else {
        return true;
    };
    list.iter().any(|allowed| {
        rest == *allowed
            || rest
                .strip_prefix(allowed)
                .is_some_and(|tail| tail.starts_with("::"))
    })
}

/// Every `crate::` and `temper_domain::` path in `text`, with its line, comments skipped and
/// `use` groups expanded (`crate::{a, b::{c, self}}` gives `crate::a`, `crate::b::c` and
/// `crate::b`).
pub(crate) fn references(text: &str) -> Vec<(usize, String)> {
    let code: Vec<char> = text
        .lines()
        .flat_map(|line| {
            let code = line.split_once("//").map_or(line, |(code, _)| code);
            code.chars().chain(['\n'])
        })
        .collect();
    let mut out = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while let Some(&c) = code.get(i) {
        let boundary = i == 0 || code.get(i - 1).is_some_and(|p| !is_ident(*p) && *p != ':');
        let root = ["crate::", "temper_domain::"]
            .into_iter()
            .find(|root| boundary && starts_with(&code, i, root));
        if let Some(root) = root {
            let (paths, end) = parse_path(&code, i + root.len());
            let prefix = root.trim_end_matches("::");
            out.extend(paths.into_iter().map(|p| (line, join(prefix, &p))));
            line += code
                .iter()
                .take(end)
                .skip(i)
                .filter(|&&ch| ch == '\n')
                .count();
            i = end;
            continue;
        }
        if c == '\n' {
            line += 1;
        }
        i += 1;
    }
    out
}

fn join(prefix: &str, rest: &str) -> String {
    if rest.is_empty() {
        prefix.to_owned()
    } else {
        format!("{prefix}::{rest}")
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn starts_with(code: &[char], at: usize, word: &str) -> bool {
    word.chars()
        .enumerate()
        .all(|(k, w)| code.get(at + k) == Some(&w))
}

/// Parses a path tail at `at` (after `root::`): idents joined by `::`, maybe ending in a
/// `{...}` group. Returns the expanded tails and the index just past the path.
fn parse_path(code: &[char], at: usize) -> (Vec<String>, usize) {
    let mut i = skip_space(code, at);
    let mut segments: Vec<String> = Vec::new();
    loop {
        if code.get(i) == Some(&'{') {
            let (inner, end) = parse_group(code, i + 1);
            let base = segments.join("::");
            let paths = inner
                .into_iter()
                .map(|p| match (base.is_empty(), p.as_str()) {
                    (_, "self") => base.as_str().into(),
                    (true, _) => p,
                    (false, _) => format!("{base}::{p}"),
                })
                .collect();
            return (paths, end);
        }
        let start = i;
        while code.get(i).is_some_and(|c| is_ident(*c)) {
            i += 1;
        }
        let ident: String = code.iter().take(i).skip(start).collect();
        if ident.is_empty() {
            break;
        }
        segments.push(ident);
        let after = skip_space(code, i);
        if starts_with(code, after, "::") {
            i = skip_space(code, after + 2);
        } else {
            break;
        }
    }
    (vec![segments.join("::")], i)
}

/// Parses the entries of a `{...}` group starting just after `{`; returns them and the index
/// past the closing `}`. `x as y` keeps `x`.
fn parse_group(code: &[char], at: usize) -> (Vec<String>, usize) {
    let mut out = Vec::new();
    let mut i = at;
    loop {
        i = skip_space(code, i);
        match code.get(i) {
            None => return (out, i),
            Some('}') => return (out, i + 1),
            Some(',') => i += 1,
            Some(_) => {
                let (paths, end) = parse_path(code, i);
                out.extend(paths.into_iter().filter(|p| !p.is_empty()));
                i = skip_alias(code, end);
                if i == end && !matches!(code.get(i), Some(',' | '}')) {
                    i += 1;
                }
            }
        }
    }
}

fn skip_alias(code: &[char], at: usize) -> usize {
    let i = skip_space(code, at);
    if starts_with(code, i, "as") && code.get(i + 2).is_some_and(|c| c.is_whitespace()) {
        let mut j = skip_space(code, i + 2);
        while code.get(j).is_some_and(|c| is_ident(*c)) {
            j += 1;
        }
        return j;
    }
    at
}

fn skip_space(code: &[char], at: usize) -> usize {
    let mut i = at;
    while code.get(i).is_some_and(|c| c.is_whitespace()) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests;
