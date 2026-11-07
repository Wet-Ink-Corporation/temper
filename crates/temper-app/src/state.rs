//! Reads of the files temper keeps, with 1.x's fallbacks (`readText` and the readers on it).

use std::path::Path;

use crate::ports::Files;

/// 1.x `readText(p)`: the file's text, or `None` for any failure, which 1.x's `catch` turns
/// into `null`. The `Files` port already refuses a text past V8's longest string (3-24), so a
/// file 1.x cannot read is `None` here too.
pub(crate) fn read_js_text(files: &impl Files, path: &str) -> Option<String> {
    match files.read_text(Path::new(path)) {
        Ok(text) => text,
        // Unreadable, or longer than a JS string: 1.x's `catch { return null; }`.
        Err(_) => None,
    }
}
