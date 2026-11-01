//! Node's `path` functions, as 1.x calls them: a lexical port of `path.posix` and `path.win32`
//! (Node 22's `lib/path.js`), over `&str` so both styles are tested on every platform (OC-1 B).
//!
//! `resolve` and `relative` take the working directory as an argument where Node reads
//! `process.cwd()`. `dirname`, `basename` and `join` are defined only on resolved, absolute
//! paths and plain names, which is all 1.x passes them.

mod win32;

use std::borrow::Cow;

use temper_domain::PathStyle;

/// What Node's `path` reads from the process: the platform's rules and `process.cwd()`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PathEnv<'a> {
    pub(crate) style: PathStyle,
    /// The working directory, absolute.
    pub(crate) cwd: &'a str,
}

/// `path.resolve(path)` with `process.cwd()` = `cwd`.
pub(crate) fn resolve(style: PathStyle, cwd: &str, path: &str) -> String {
    match style {
        PathStyle::Windows => win32::resolve(cwd, path),
        PathStyle::Posix => posix_resolve(cwd, path),
    }
}

/// `path.relative(from, to)` with `process.cwd()` = `cwd`. Windows compares case-insensitively
/// and keeps `to`'s case; when the roots differ it returns `to`, resolved.
pub(crate) fn relative(style: PathStyle, cwd: &str, from: &str, to: &str) -> String {
    match style {
        PathStyle::Windows => win32::relative(cwd, from, to),
        PathStyle::Posix => posix_relative(cwd, from, to),
    }
}

/// `path.isAbsolute(path)`.
pub(crate) fn is_absolute(style: PathStyle, path: &str) -> bool {
    match style {
        PathStyle::Windows => win32::is_absolute(path),
        PathStyle::Posix => path.starts_with('/'),
    }
}

/// `path.dirname(path)` of a resolved, absolute path; the parent of a root is the root.
pub(crate) fn dirname(style: PathStyle, path: &str) -> &str {
    let root = root_len(style, path);
    let parent = path
        .get(root..)
        .and_then(|rest| {
            // Node passes over trailing separators before it looks for the last one.
            rest.trim_end_matches(|c| is_sep(style, c))
                .rfind(|c| is_sep(style, c))
        })
        .map_or(root, |at| root.saturating_add(at).max(root));
    // The cut is at a separator or after the root, both ASCII, so it is a char boundary.
    path.get(..parent).unwrap_or(path)
}

/// `path.basename(path)` of a resolved, absolute path: the last name, or `""` for a root.
pub(crate) fn basename(style: PathStyle, path: &str) -> &str {
    let rest = path.get(root_len(style, path)..).unwrap_or("");
    rest.rsplit(|c| is_sep(style, c)).next().unwrap_or(rest)
}

/// `path.join(dir, name)` for a resolved, absolute `dir` and a plain `name` (no separator, not
/// `.` or `..`), where Node's normalisation changes nothing but the separator between them.
pub(crate) fn join(style: PathStyle, dir: &str, name: &str) -> String {
    let sep = separator(style);
    if dir.ends_with(|c| is_sep(style, c)) {
        [dir, name].concat()
    } else {
        [dir, sep, name].concat()
    }
}

/// 1.x `norm`: `\` becomes `/`.
pub(crate) fn norm(path: &str) -> Cow<'_, str> {
    if path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        Cow::Borrowed(path)
    }
}

/// The style's own separator, which `resolve` and `join` write.
fn separator(style: PathStyle) -> &'static str {
    match style {
        PathStyle::Windows => "\\",
        PathStyle::Posix => "/",
    }
}

/// Does the style read `c` as a separator? Windows reads both.
fn is_sep(style: PathStyle, c: char) -> bool {
    match style {
        PathStyle::Windows => c == '\\' || c == '/',
        PathStyle::Posix => c == '/',
    }
}

/// The byte length of a resolved, absolute path's root, separator included: `/`, `C:\` or
/// `\\host\share\`.
fn root_len(style: PathStyle, path: &str) -> usize {
    match style {
        PathStyle::Windows => win32::root_len(path),
        PathStyle::Posix => usize::from(path.starts_with('/')),
    }
}

/// Node's posix `resolve` with one argument.
fn posix_resolve(cwd: &str, path: &str) -> String {
    if (path.is_empty() || path == ".") && cwd.starts_with('/') {
        return cwd.to_owned();
    }
    let full = if path.starts_with('/') {
        [path, "/"].concat()
    } else {
        [cwd, "/", path, "/"].concat()
    };
    let absolute = full.starts_with('/');
    let normal = normalize_string(&full, !absolute, '/', |b| b == b'/');
    if absolute {
        ["/", &normal].concat()
    } else if normal.is_empty() {
        ".".to_owned()
    } else {
        normal
    }
}

/// Node's posix `relative`.
fn posix_relative(cwd: &str, from: &str, to: &str) -> String {
    if from == to {
        return String::new();
    }
    let from = posix_resolve(cwd, from);
    let to = posix_resolve(cwd, to);
    if from == to {
        return String::new();
    }
    // Both start with `/`; compare what follows it.
    let (f, t) = (
        from.as_bytes().get(1..).unwrap_or(&[]),
        to.as_bytes().get(1..).unwrap_or(&[]),
    );
    let common = f.iter().zip(t).take_while(|(a, b)| a == b).count();
    let mut last_sep = f
        .get(..common)
        .and_then(|shared| shared.iter().rposition(|&b| b == b'/'));
    let length = f.len().min(t.len());
    if common == length {
        if t.len() > length {
            match t.get(common) {
                // `from` is the exact base of `to`, or the root.
                Some(b'/') => return slice_from(&to, common.saturating_add(2)),
                _ if common == 0 => return slice_from(&to, 1),
                _ => {}
            }
        } else if f.len() > length {
            match f.get(common) {
                Some(b'/') => last_sep = Some(common),
                _ if common == 0 => last_sep = Some(0),
                _ => {}
            }
        }
    }
    // One `..` for each separator in `from` after the common part, and one for its end.
    let start = last_sep.map_or(0, |at| at.saturating_add(1));
    let ups = f
        .get(start..)
        .map_or(0, |rest| rest.iter().filter(|&&b| b == b'/').count())
        .saturating_add(1);
    let ups = if start > f.len() { 0 } else { ups };
    let mut out = vec![".."; ups].join("/");
    // Node: `to.slice(toStart + lastCommonSep)`, with `lastCommonSep` -1 as "none".
    let tail = match last_sep {
        Some(at) => to.get(at.saturating_add(1)..).unwrap_or(""),
        None => &to,
    };
    out.push_str(tail);
    out
}

/// `text` from byte `at` on (an ASCII boundary), owned; empty past the end.
fn slice_from(text: &str, at: usize) -> String {
    text.get(at..).unwrap_or("").to_owned()
}

/// Node's `normalizeString`: resolves `.` and `..` segments and squeezes separators, with `..`
/// kept at the start when `allow_above_root`. Byte offsets stand in for Node's UTF-16 ones: every
/// cut is at an ASCII separator, and the one length compared (`lastSegmentLength === 2`, with
/// the text ending in `..`) means the same in both.
fn normalize_string(
    path: &str,
    allow_above_root: bool,
    sep: char,
    is_sep: fn(u8) -> bool,
) -> String {
    let bytes = path.as_bytes();
    let mut res = String::with_capacity(path.len());
    let mut last_segment_len = 0_usize;
    // Node's `lastSlash + 1`: where the current segment starts.
    let mut seg_start = 0_usize;
    // Node's `dots`, with -1 (a segment that is not all dots) as `None`.
    let mut dots = Some(0_usize);
    let mut code = 0_u8;
    for i in 0..=bytes.len() {
        match bytes.get(i) {
            Some(&b) => code = b,
            None if is_sep(code) => break,
            None => code = b'/',
        }
        if !is_sep(code) {
            dots = match (code, dots) {
                (b'.', Some(n)) => Some(n.saturating_add(1)),
                _ => None,
            };
            continue;
        }
        if seg_start == i || dots == Some(1) {
            // An empty or `.` segment: nothing to add.
        } else if dots == Some(2) {
            let ends_in_dots = res.len() >= 2 && last_segment_len == 2 && res.ends_with("..");
            if !ends_in_dots && !res.is_empty() {
                // Drop the last segment (all of `res` when it has one).
                match res.rfind(sep) {
                    Some(at) if res.len() > 2 => {
                        res.truncate(at);
                        last_segment_len = res
                            .rfind(sep)
                            .map_or(res.len(), |j| res.len().saturating_sub(j).saturating_sub(1));
                    }
                    Some(_) | None => {
                        res.clear();
                        last_segment_len = 0;
                    }
                }
                seg_start = i.saturating_add(1);
                dots = Some(0);
                continue;
            }
            if allow_above_root {
                if !res.is_empty() {
                    res.push(sep);
                }
                res.push_str("..");
                last_segment_len = 2;
            }
        } else {
            if !res.is_empty() {
                res.push(sep);
            }
            res.push_str(path.get(seg_start..i).unwrap_or(""));
            last_segment_len = i.saturating_sub(seg_start);
        }
        seg_start = i.saturating_add(1);
        dots = Some(0);
    }
    res
}

#[cfg(test)]
mod tests;
