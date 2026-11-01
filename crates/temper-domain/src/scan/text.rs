//! Hits as 1.x prints them: one hit (`formatHit`) and the `temper scan` text report.

use super::hit::Hit;

/// One hit as 1.x's `formatHit` prints it, on two lines with no newline at the end.
///
/// The first line is the rule id, `file:line`, the message and the source text in backticks;
/// the second is `    fix: ` and the fix. Without a file, or with an empty one, the place is
/// `line N`.
///
/// # Examples
///
/// ```
/// use temper_domain::{CodeKind, FileRole, ScanOptions, Scanner, format_hit};
///
/// let scanner = Scanner::new()?;
/// let options = ScanOptions { kind: CodeKind::Prod, file: FileRole::Source, forbid: None };
/// let report = scanner.scan("fn f() { dbg!(1); }", &options);
///
/// let printed: Vec<String> = report.hits().iter().map(|h| format_hit(h, None)).collect();
/// assert_eq!(printed, ["H-06 line 1: `dbg!` left in code. `fn f() { dbg!(1); }`\n    fix: Remove it. \
///     Use `tracing::debug!` with structured fields for diagnostics that should stay."]);
/// # Ok::<(), temper_domain::ScannerSetupError>(())
/// ```
#[must_use]
pub fn format_hit(hit: &Hit, file: Option<&str>) -> String {
    let (id, line, msg, text, fix) = (hit.id(), hit.line(), hit.msg(), hit.text(), hit.fix());
    match file {
        Some(file) if !file.is_empty() => {
            format!("{id} {file}:{line}: {msg}. `{text}`\n    fix: {fix}")
        }
        Some(_) | None => format!("{id} line {line}: {msg}. `{text}`\n    fix: {fix}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{CodeKind, FileRole, ScanOptions, Scanner};

    fn scan(src: &str) -> Vec<Hit> {
        let scanner = Scanner::new().expect("every built-in pattern compiles");
        let options = ScanOptions {
            kind: CodeKind::Prod,
            file: FileRole::Source,
            forbid: None,
        };
        scanner.scan(src, &options).into_hits()
    }

    /// The `lib.rs` hit of 1.x's `scan` text on the 1d.2 fixture (`oracle/files.txt`), and the
    /// same hit with no file or an empty one.
    #[test]
    fn format_hit_like_1x() {
        let hits = scan("pub fn parse(v: Option<u8>) -> u8 {\n    v.unwrap()\n}\n");
        let fix = "Propagate with `?`, convert with `ok_or_else`/`let ... else`, or make the invariant a type. See references/errors.md \"Eliminating unwrap/expect\".";

        let printed: Vec<[String; 3]> = hits
            .iter()
            .map(|hit| {
                [
                    format_hit(hit, Some("crates/core/src/lib.rs")),
                    format_hit(hit, None),
                    format_hit(hit, Some("")),
                ]
            })
            .collect();

        let at_line = format!("unwrap/expect in production code. `v.unwrap()`\n    fix: {fix}");
        assert_eq!(
            printed,
            [[
                format!("H-02 crates/core/src/lib.rs:2: {at_line}"),
                format!("H-02 line 2: {at_line}"),
                format!("H-02 line 2: {at_line}"),
            ]],
            "with a file, without one, and with an empty one"
        );
    }
}
