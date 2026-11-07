//! The verdict block of a reviewer's report (1.x `extractVerdict`).

use crate::js::{Value, is_js_space};

/// The last fenced block of a report that contains `temper_verdict`, read as `JSON.parse` reads
/// it.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractedVerdict {
    /// The block is JSON: any JSON value, as 1.x accepts any.
    Parsed(Value),
    /// The block is not JSON. 1.x stores `{ __parseError: message }`; the message is
    /// `serde_json`'s wording, not V8's (port plan D16(a)).
    Unparseable {
        /// Why the block does not parse.
        message: String,
    },
}

impl ExtractedVerdict {
    /// 1.x `v?.[key]`: a parsed object's own property. An unparseable block has none that a
    /// reader asks for.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Parsed(value) => value.get(key),
            Self::Unparseable { .. } => None,
        }
    }
}

/// 1.x `extractVerdict(text)`: the last block ``/```(?:json)?\s*\n([\s\S]*?)```/g`` finds that
/// contains `temper_verdict`, parsed. `None` for empty text or when no block contains it.
///
/// One pass over `text`: each fence is looked at once, and the search stops at the first block
/// that has no closing fence, since no later block can close either.
///
/// # Examples
///
/// ```
/// use temper_domain::{ExtractedVerdict, extract_verdict};
///
/// let report = "Done.\n```json\n{\"temper_verdict\": 1}\n```\n";
/// let verdict = extract_verdict(report);
/// assert!(matches!(&verdict, Some(ExtractedVerdict::Parsed(v)) if v.to_json() == r#"{"temper_verdict":1}"#));
/// assert_eq!(extract_verdict("no block"), None);
/// ```
#[must_use]
pub fn extract_verdict(text: &str) -> Option<ExtractedVerdict> {
    let raw = Blocks { text, at: 0 }
        .filter(|body| body.contains("temper_verdict"))
        .last()?;
    Some(match Value::from_json(raw) {
        Ok(value) => ExtractedVerdict::Parsed(value),
        Err(e) => ExtractedVerdict::Unparseable {
            message: e.detail(),
        },
    })
}

const FENCE: &str = "```";

/// The bodies of the blocks 1.x's global regex matches, in order, with its leftmost-first
/// backtracking: a start is any fence; `json` is taken when it follows; then the longest run of
/// JS whitespace that ends in a `\n`; then the shortest body up to the next fence.
struct Blocks<'t> {
    text: &'t str,
    at: usize,
}

impl<'t> Iterator for Blocks<'t> {
    type Item = &'t str;

    fn next(&mut self) -> Option<&'t str> {
        loop {
            let start = self.at + self.text.get(self.at..)?.find(FENCE)?;
            let Some(body) = body_start(self.text, start + FENCE.len()) else {
                // The fence's first byte is ASCII, so the next byte starts a char.
                self.at = start + 1;
                continue;
            };
            let Some(len) = self.text.get(body..).and_then(|rest| rest.find(FENCE)) else {
                self.at = self.text.len();
                return None;
            };
            self.at = body + len + FENCE.len();
            return self.text.get(body..body + len);
        }
    }
}

/// Where a block's body starts after the fence that ends at `after`: past `json` when it follows
/// (without it, the `j` matches neither `\s` nor `\n`), then past the last `\n` of the JS
/// whitespace run.
fn body_start(text: &str, after: usize) -> Option<usize> {
    let rest = text.get(after..)?;
    let header = rest
        .strip_prefix("json")
        .map_or(after, |_| after + "json".len());
    let mut body = None;
    for (at, c) in text.get(header..)?.char_indices() {
        if !is_js_space(c) {
            break;
        }
        if c == '\n' {
            body = Some(header + at + 1);
        }
    }
    body
}
