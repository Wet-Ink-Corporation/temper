//! `check-seams`: the modules bound for redkiln's kits reach only what the kit will hold.
//!
//! wi-59cb6e (owner, 2026-11-08): when temper moves into redkiln, the language-neutral half of
//! the engine goes to the `redkiln-craft` and `redkiln-guard` kits, and the Rust policy stays in
//! Redkiln Rust's crates. This check keeps that a cut along module lines. Each kit-bound file may
//! name, outside its own module, only the modules and items listed for its crate; any other
//! `crate::`, `temper_domain::` or resolved `super::` path is a violation, and so is renaming a
//! root. It is a text scan (comments and literals skipped, `use` groups expanded, inline modules
//! tracked), so a path built by a macro, or one reached through a `use` glob of a listed module
//! that re-exports more, escapes it: a tripwire for review, not a proof. A kit-bound module's
//! tests are scanned too, because they move with it.

mod lex;

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
    /// First segments a `crate::` path may start with besides the kit-bound modules themselves:
    /// items `lib.rs` re-exports from them.
    pub(crate) crate_paths: &'static [&'static str],
    /// First segments a `temper_domain::` path may start with.
    pub(crate) domain_paths: &'static [&'static str],
}

/// The domain's kit-bound modules reach only each other, through module paths or the items
/// `lib.rs` re-exports from them. `hooks/bash/opaque` holds `OpaqueKind`, which the app's shell
/// reader names; the reviewer's texts that use it stay in `hooks/bash/reviewer`.
const DOMAIN: Seam = Seam {
    krate: "temper-domain",
    modules: &[
        ("fingerprint", Kit::Craft),
        ("verdict", Kit::Craft),
        ("state", Kit::Craft),
        ("glob", Kit::Guard),
        ("hooks/bash/code", Kit::Guard),
        ("hooks/bash/opaque", Kit::Guard),
        ("hooks/setup", Kit::Support),
        ("time", Kit::Support),
        ("js", Kit::Support),
        ("paths", Kit::Support),
    ],
    crate_paths: &["js_sort", "Fingerprint", "Timestamp"],
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
    crate_paths: &["fakes"],
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

/// The listed modules of `seam` that none of `files` (paths under `src/`) belongs to: a module
/// renamed or deleted without its entry, which would otherwise go unchecked.
pub(crate) fn unmatched_modules<'s>(seam: &'s Seam, files: &[String]) -> Vec<&'s str> {
    seam.modules
        .iter()
        .filter(|&&(module, _)| {
            !files.iter().any(|file| {
                let stem = file.strip_suffix(".rs").unwrap_or(file);
                stem == module
                    || stem
                        .strip_prefix(module)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
        })
        .map(|&(module, _)| module)
        .collect()
}

/// The violations in the text of one kit-bound file, `rel` under its crate's `src/`
/// (`/`-separated). Its tests are checked too: they move with the kit.
pub(crate) fn check_file(seam: &Seam, rel: &str, kit: Kit, text: &str) -> Vec<Violation> {
    let file = format!("crates/{}/src/{rel}", seam.krate);
    let module: Vec<&str> = rel.strip_suffix(".rs").unwrap_or(rel).split('/').collect();
    references(text, &module)
        .into_iter()
        .filter(|(_, path)| !allowed(seam, path))
        .map(|(line, path)| Violation {
            // Each violation owns its file name: genuine duplication, one per finding.
            file: file.clone(),
            line,
            path,
            kit,
        })
        .collect()
}

fn allowed(seam: &Seam, path: &str) -> bool {
    if path == "crate" || path == "temper_domain" || path.starts_with("super") {
        return false;
    }
    let under = |rest: &str, allowed: &str| {
        rest == allowed
            || rest
                .strip_prefix(allowed)
                .is_some_and(|tail| tail.starts_with("::"))
    };
    if let Some(rest) = path.strip_prefix("crate::") {
        seam.crate_paths.iter().any(|allowed| under(rest, allowed))
            || seam
                .modules
                .iter()
                .any(|(module, _)| under(rest, &module.replace('/', "::")))
    } else if let Some(rest) = path.strip_prefix("temper_domain::") {
        seam.domain_paths.iter().any(|allowed| under(rest, allowed))
    } else {
        true
    }
}

/// Every `crate::`, `temper_domain::` and `super::` path in `text`, the source of the module
/// `module` (its segments under `src/`), with its line. Comments and literals are skipped
/// ([`lex::code_only`]), `use` groups expanded (`crate::{a, b::{c, self}}` gives `crate::a`,
/// `crate::b::c` and `crate::b`), and `super::` resolved against `module` and the inline
/// `mod name { .. }` blocks around it into a `crate::` path; one that climbs past the crate root
/// stays `super::..`. A renamed root (`use temper_domain as d;`) gives the bare root, which no
/// seam allows, since paths through the new name would escape the scan.
pub(crate) fn references(text: &str, module: &[&str]) -> Vec<(usize, String)> {
    let code = lex::code_only(text);
    let mut out = Vec::new();
    let mut scope = Scope::default();
    let mut line = 1_usize;
    let mut i = 0;
    while let Some(&c) = code.get(i) {
        let at_root = at_boundary(&code, i);
        if let Some((paths, end)) = at_root
            .then(|| root_paths(&code, i, module, &scope))
            .flatten()
        {
            out.extend(paths.into_iter().map(|p| (line, p)));
            line = line.saturating_add(newlines(&code, i, end));
            i = end;
            continue;
        }
        if at_root && starts_word(&code, i, "mod") {
            scope.note_mod(&code, i);
        }
        match c {
            '{' => scope.open(),
            '}' => scope.close(),
            '\n' => line = line.saturating_add(1),
            _ => {}
        }
        i = i.saturating_add(1);
    }
    out
}

/// The inline modules open around the scan: each `mod name {` block's name and the brace depth
/// inside it.
#[derive(Debug, Default)]
struct Scope {
    depth: usize,
    pending: Option<String>,
    open: Vec<(String, usize)>,
}

impl Scope {
    /// Records `mod name` at `at`; it becomes an open module if a `{` follows its name.
    fn note_mod(&mut self, code: &[char], at: usize) {
        let start = skip_space(code, at.saturating_add(3));
        let name = ident_at(code, start);
        let after = skip_space(code, start.saturating_add(name.chars().count()));
        if !name.is_empty() && code.get(after) == Some(&'{') {
            self.pending = Some(name);
        }
    }

    fn open(&mut self) {
        self.depth = self.depth.saturating_add(1);
        if let Some(name) = self.pending.take() {
            self.open.push((name, self.depth));
        }
    }

    fn close(&mut self) {
        if self
            .open
            .last()
            .is_some_and(|&(_, depth)| depth == self.depth)
        {
            self.open.pop();
        }
        self.depth = self.depth.saturating_sub(1);
    }

    fn names(&self) -> impl Iterator<Item = &str> {
        self.open.iter().map(|(name, _)| name.as_str())
    }
}

/// The paths a root at `at` starts (`crate::`, `temper_domain::`, a `super::` chain, or a root
/// renamed with `as`), and the index past them; `None` when no root starts there.
fn root_paths(
    code: &[char],
    at: usize,
    module: &[&str],
    scope: &Scope,
) -> Option<(Vec<String>, usize)> {
    for root in ["crate", "temper_domain"] {
        if !starts_word(code, at, root) {
            continue;
        }
        let after = skip_space(code, at.saturating_add(root.len()));
        if starts_with(code, after, "::") {
            let (paths, end) = parse_path(code, after.saturating_add(2));
            return Some((paths.iter().map(|p| join(root, p)).collect(), end));
        }
        if starts_word(code, after, "as") {
            return Some((vec![root.to_owned()], after.saturating_add(2)));
        }
    }
    let mut i = at;
    // `self::super::x` is `super::x`.
    if starts_word(code, i, "self") {
        let after = skip_space(code, i.saturating_add(4));
        let next = skip_space(code, after.saturating_add(2));
        if starts_with(code, after, "::") && starts_word(code, next, "super") {
            i = next;
        }
    }
    let mut supers = 0_usize;
    let mut renamed = None;
    while starts_word(code, i, "super") {
        supers = supers.saturating_add(1);
        let after = skip_space(code, i.saturating_add(5));
        if starts_word(code, after, "as") {
            renamed = Some(after.saturating_add(2));
            break;
        }
        if !starts_with(code, after, "::") {
            // The chain names a module itself (`use super::super;`).
            renamed = Some(i.saturating_add(5));
            break;
        }
        i = skip_space(code, after.saturating_add(2));
    }
    if supers == 0 {
        return None;
    }
    // A chain that ends in `super` (renamed or not) names its whole module, so it is reported
    // bare, as `crate as` is.
    let (paths, end) = match renamed {
        Some(end) => (vec![String::new()], end),
        None => parse_path(code, i),
    };
    let base: Vec<&str> = module.iter().copied().chain(scope.names()).collect();
    let resolved = base
        .len()
        .checked_sub(supers)
        .and_then(|keep| base.get(..keep))
        .map(|kept| {
            ["crate"]
                .iter()
                .chain(kept)
                .copied()
                .collect::<Vec<_>>()
                .join("::")
        });
    let prefix = resolved.unwrap_or_else(|| vec!["super"; supers].join("::"));
    Some((paths.iter().map(|p| join(&prefix, p)).collect(), end))
}

/// Whether a path may start at `at`: not inside an identifier, and not after `::` unless that
/// `::` is itself a path's start (`::temper_domain::x`).
fn at_boundary(code: &[char], at: usize) -> bool {
    let before = |back: usize| at.checked_sub(back).and_then(|p| code.get(p)).copied();
    match before(1) {
        None => true,
        Some(':') => {
            before(2) == Some(':') && before(3).is_none_or(|p| !is_ident(p) && p != ':' && p != '>')
        }
        Some(p) => !is_ident(p),
    }
}

/// Whether the identifier `word` stands at `at` (not followed by more identifier characters).
fn starts_word(code: &[char], at: usize, word: &str) -> bool {
    starts_with(code, at, word)
        && code
            .get(at.saturating_add(word.chars().count()))
            .is_none_or(|&c| !is_ident(c))
}

fn ident_at(code: &[char], at: usize) -> String {
    code.iter().skip(at).take_while(|&&c| is_ident(c)).collect()
}

fn newlines(code: &[char], from: usize, to: usize) -> usize {
    code.iter()
        .take(to)
        .skip(from)
        .filter(|&&ch| ch == '\n')
        .count()
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
