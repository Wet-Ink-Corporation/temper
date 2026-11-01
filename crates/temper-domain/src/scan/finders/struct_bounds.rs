//! `struct-bounds`: trait bounds on a generic type definition (1.x `structBounds`).
//!
//! 1.x builds two regexes per bounded type at run time from the type's names. They are
//! hand-written here as [`mentions_assoc`] and [`has_drop_impl`], so a scan compiles nothing and
//! cannot fail (decision 1c-4, wi-5842d0).

use regex::Regex;

use super::Found;
use crate::js::{is_js_space, js_trim};
use crate::scan::env::Env;
use crate::scan::lexer::{match_angle, match_brace, match_pair, next_char, slice, split_top};
use crate::scan::scanner::{ScannerSetupError, compile};

/// The compiled patterns of the finder.
#[derive(Debug)]
pub(crate) struct StructBoundPatterns {
    head: Regex,
    const_kw: Regex,
    param: Regex,
    where_clause: Regex,
    where_pred: Regex,
    drop_for: Regex,
    impl_kw: Regex,
}

impl StructBoundPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            head: compile(
                "struct-bounds:head",
                r"\b(?:struct|enum)\s+([A-Za-z_]\w*)\s*<",
            )?,
            const_kw: compile("struct-bounds:const", r"^const\b")?,
            param: compile("struct-bounds:param", r"^([A-Za-z_]\w*)\s*:([^=]*)")?,
            where_clause: compile("struct-bounds:where", r"^\s*where\b([^{;]*)")?,
            where_pred: compile("struct-bounds:pred", r"^\s*([^:]+?)\s*:([\s\S]*)$")?,
            drop_for: compile("struct-bounds:drop-for", r"\bDrop\s+for\s+")?,
            impl_kw: compile("struct-bounds:impl", r"\bimpl\b")?,
        })
    }
}

/// Pushes every generic `struct` or `enum` with a trait bound on a parameter, unless a field
/// names an associated type of a bounded name or the type has a `Drop` impl.
pub(crate) fn find(env: &Env<'_>, p: &StructBoundPatterns, out: &mut Vec<Found>) {
    let code = env.code;
    for caps in p.head.captures_iter(code) {
        let head = caps.get_match();
        let name = caps.get(1).map_or("", |n| n.as_str());
        // The match ends with the `<`.
        let open = head.end().saturating_sub(1);
        let Some(close) = match_angle(code, open) else {
            continue;
        };
        let mut bounded = bounded_params(slice(code, open.saturating_add(1), close), p);
        let mut k = skip_js_spaces(code, close.saturating_add(1));
        let mut body = "";
        if code.as_bytes().get(k) == Some(&b'(') {
            let pc = match_pair(code, k, b'(', b')');
            body = slice(code, k, pc.saturating_add(1));
            k = pc.saturating_add(1);
        }
        k = where_bounds(code, k, p, &mut bounded);
        if bounded.is_empty() {
            continue;
        }
        if body.is_empty() {
            body = braced_body(code, skip_js_spaces(code, k));
        }
        let assoc = bounded.iter().any(|n| mentions_assoc(body, n));
        if !assoc && !has_drop_impl(code, name, p) {
            out.push(Found::at(head.start()));
        }
    }
}

/// The names of the parameters in `params` (the text between `<` and `>`) with a trait bound.
fn bounded_params<'c>(params: &'c str, p: &StructBoundPatterns) -> Vec<&'c str> {
    split_top(params, b',')
        .map(js_trim)
        .filter(|t| !t.is_empty() && !t.starts_with('\'') && !p.const_kw.is_match(t))
        .filter_map(|t| p.param.captures(t))
        .filter(|caps| caps.get(2).is_some_and(|b| trait_bound(b.as_str())))
        .filter_map(|caps| caps.get(1).map(|n| n.as_str()))
        .collect()
}

/// Adds the bounded names of a `where` clause at `k` to `bounded`, and returns the offset after
/// the clause (`k` when there is none). A name is any text before the `:`, even empty.
fn where_bounds<'c>(
    code: &'c str,
    k: usize,
    p: &StructBoundPatterns,
    bounded: &mut Vec<&'c str>,
) -> usize {
    let Some(clause) = p.where_clause.captures(slice(code, k, code.len())) else {
        return k;
    };
    let preds = clause.get(1).map_or("", |preds| preds.as_str());
    let names = split_top(preds, b',')
        .filter_map(|pred| p.where_pred.captures(pred))
        .filter_map(|caps| {
            let name = js_trim(caps.get(1)?.as_str());
            let bounds = caps.get(2)?.as_str();
            (!name.starts_with('\'') && trait_bound(bounds)).then_some(name)
        });
    bounded.extend(names);
    k.saturating_add(clause.get_match().len())
}

/// The `{ .. }` body at `k`, both braces included, or `""` when no `{` is there.
fn braced_body(code: &str, k: usize) -> &str {
    if code.as_bytes().get(k) == Some(&b'{') {
        slice(code, k, match_brace(code, k).saturating_add(1))
    } else {
        ""
    }
}

/// The offset of the first char at or after `k` that is not a JS space.
fn skip_js_spaces(code: &str, k: usize) -> usize {
    let skipped = code.get(k..).map_or(0, |rest| {
        rest.len()
            .saturating_sub(rest.trim_start_matches(is_js_space).len())
    });
    k.saturating_add(skipped)
}

/// Does a bound list hold a trait bound, anything but a lifetime and `?Sized` (1.x `traitBound`)?
fn trait_bound(bounds: &str) -> bool {
    split_top(bounds, b'+')
        .map(js_trim)
        .any(|t| !t.is_empty() && !t.starts_with('\'') && t != "?Sized")
}

/// Is `c` in JS's `\w` (ASCII word chars)?
fn is_js_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// 1.x `(?:^|[^\w:])n\s*::|<\s*n\s+as\b` with `n` escaped: does `body` name an associated type
/// of `n`, as `n::X` or `<n as Tr>`? Every position where `n` starts is tried, overlapping ones
/// included; an empty `n` tries every char boundary.
fn mentions_assoc(body: &str, n: &str) -> bool {
    let mut from = 0;
    while let Some(found) = body.get(from..).and_then(|rest| rest.find(n)) {
        let at = from.saturating_add(found);
        let before = body.get(..at).unwrap_or("");
        let after = body.get(at.saturating_add(n.len())..).unwrap_or("");
        let path_start = before
            .chars()
            .next_back()
            .is_none_or(|c| !is_js_word(c) && c != ':');
        if path_start && after.trim_start_matches(is_js_space).starts_with("::") {
            return true;
        }
        if before.trim_end_matches(is_js_space).ends_with('<') && qualified_as(after) {
            return true;
        }
        from = next_char(body, at);
    }
    false
}

/// Does `after` start with one or more JS spaces, then `as` as a whole word?
fn qualified_as(after: &str) -> bool {
    let rest = after.trim_start_matches(is_js_space);
    rest.len() < after.len()
        && rest
            .strip_prefix("as")
            .is_some_and(|tail| !tail.chars().next().is_some_and(is_js_word))
}

/// 1.x `\bimpl\b[^{;]*\bDrop\s+for\s+name\b`: does `code` have a `Drop` impl for `name`?
fn has_drop_impl(code: &str, name: &str, p: &StructBoundPatterns) -> bool {
    p.drop_for.find_iter(code).any(|d| {
        let names_it = code.get(d.end()..).and_then(|rest| rest.strip_prefix(name));
        let whole = names_it.is_some_and(|tail| !tail.chars().next().is_some_and(is_js_word));
        let head = code.get(..d.start()).unwrap_or("");
        // No `{` or `;` may come between `impl` and `Drop`.
        let from = head.rfind(['{', ';']).map_or(0, |at| at.saturating_add(1));
        whole
            && p.impl_kw
                .find_at(code, from)
                .is_some_and(|i| i.end() <= d.start())
    })
}
