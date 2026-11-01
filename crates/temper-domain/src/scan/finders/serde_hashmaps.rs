//! `serde-hashmap`: `HashMap` and `HashSet` fields of a type that derives `Serialize`, also
//! through `cfg_attr`, unless skipped or serialized `with` a function (1.x `serializedHashMaps`).

use regex::Regex;

use super::Found;
use crate::scan::env::Env;
use crate::scan::lexer::{match_angle, match_brace, match_pair, slice, split_top};
use crate::scan::scanner::{ScannerSetupError, compile};

/// The compiled patterns of the finder.
#[derive(Debug)]
pub(crate) struct SerdePatterns {
    derive: Regex,
    item: Regex,
    hash: Regex,
    skip: Regex,
}

impl SerdePatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            derive: compile(
                "serde:derive",
                r"#\s*\[[^\]]*\bderive\s*\([^\]]*\bSerialize\b[^\]]*\]",
            )?,
            item: compile(
                "serde:item",
                r"^(?:\s*#\s*\[[^\]]*\])*\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:struct|enum)\s+[A-Za-z_]\w*\s*",
            )?,
            hash: compile("serde:hash", r"\bHash(?:Map|Set)\s*<")?,
            skip: compile(
                "serde:skip",
                r"\bskip(?:_serializing)?\b|\b(?:serialize_)?with\s*=",
            )?,
        })
    }
}

/// Pushes the `HashMap<`/`HashSet<` of every field of a `Serialize` type that is not skipped.
pub(crate) fn find(env: &Env<'_>, p: &SerdePatterns, out: &mut Vec<Found>) {
    let code = env.code;
    for attr in p.derive.find_iter(code) {
        let Some((k, close)) = item_fields(code, attr.end(), p) else {
            continue;
        };
        let mut at = k.saturating_add(1);
        for field in split_top(slice(code, at, close), b',') {
            if let Some(h) = p.hash.find(field) {
                let before = field.get(..h.start()).unwrap_or("");
                if !p.skip.is_match(before) {
                    out.push(Found::at(at.saturating_add(h.start())));
                }
            }
            at = at.saturating_add(field.len()).saturating_add(1);
        }
    }
}

/// The opening `{` or `(` of the fields of the `struct` or `enum` item after the attribute
/// ending at `from`, and the offset of its closer; `None` when no item or no fields follow.
///
/// When the item's generic list does not close, 1.x's `matchAngle(..) + 1` is 0, so the walk to
/// the fields restarts at the start of the code.
fn item_fields(code: &str, from: usize, p: &SerdePatterns) -> Option<(usize, usize)> {
    let item = p.item.find(slice(code, from, code.len()))?;
    let bytes = code.as_bytes();
    let mut k = from.saturating_add(item.len());
    if bytes.get(k) == Some(&b'<') {
        k = match_angle(code, k).map_or(0, |close| close.saturating_add(1));
    }
    let stop = bytes
        .get(k..)?
        .iter()
        .position(|b| matches!(b, b'{' | b'(' | b';'))?;
    k = k.saturating_add(stop);
    match bytes.get(k) {
        Some(b'{') => Some((k, match_brace(code, k))),
        Some(b'(') => Some((k, match_pair(code, k, b'(', b')'))),
        _ => None,
    }
}
