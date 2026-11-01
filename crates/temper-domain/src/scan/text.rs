//! Hits as 1.x prints them: one hit (`formatHit`) and the `temper scan` text report.

use super::hit::{Hit, HitAt, Severity, severity_name};

/// What `temper` prints on stderr, with a newline, when the working directory is not inside a
/// Rust project: no `Cargo.toml` in it or above it (1.x `project()`, exit 2).
pub const NOT_A_RUST_PROJECT: &str = "temper: not inside a Rust project (no Cargo.toml found)";

/// The text report of `temper scan`: one `[severity] ` line per hit, then an empty line and the
/// counts.
///
/// Hits are sorted, stably, by severity (hard, ask, soft), then by file in byte order (port
/// plan D16(l); 1.x's `localeCompare` order depends on the locale), then by line. Takes the
/// hits by value because it sorts them in place.
///
/// # Examples
///
/// ```
/// use temper_domain::render_scan_text;
///
/// assert_eq!(render_scan_text(Vec::new()), "\n0 blocking, 0 exception request(s), 0 advisory\n");
/// ```
#[must_use]
pub fn render_scan_text(mut hits: Vec<HitAt<'_>>) -> String {
    hits.sort_by_key(|at| (rank(at.hit.severity()), at.file, at.hit.line()));
    let (mut hard, mut ask, mut soft) = (0_usize, 0_usize, 0_usize);
    let mut lines = String::new();
    for at in &hits {
        let count = match at.hit.severity() {
            Severity::Hard => &mut hard,
            Severity::Ask => &mut ask,
            Severity::Soft => &mut soft,
        };
        *count = count.saturating_add(1);
        lines.push('[');
        lines.push_str(severity_name(at.hit.severity()));
        lines.push_str("] ");
        lines.push_str(&format_hit(at.hit, Some(at.file)));
        lines.push('\n');
    }
    format!("{lines}\n{hard} blocking, {ask} exception request(s), {soft} advisory\n")
}

/// 1.x's `{ hard: 0, ask: 1, soft: 2 }` sort order.
fn rank(severity: Severity) -> u8 {
    match severity {
        Severity::Hard => 0,
        Severity::Ask => 1,
        Severity::Soft => 2,
    }
}

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

    /// 1.x's `scan` sort, with D16(l): severity (hard, ask, soft), then the file in byte order
    /// (where 1.x's `localeCompare` puts `a.rs` before `B.rs`), then the line as a number.
    #[test]
    fn scan_text_sorts_by_severity_then_file_bytes_then_line() {
        let hits = scan(concat!(
            "fn b() { std::thread::sleep(d); }\n\n\n\n\n\n\n\n",
            "fn d() { y.unwrap(); dbg!(9); }\nfn e() { dbg!(10); }\n",
            "#[expect(clippy::unwrap_used, reason = \"r\")]\nfn a() { x.unwrap(); }\n",
        ));
        let find = |key: &str, line: usize| {
            hits.iter()
                .find(|h| h.key().as_str() == key && h.line().get() == line)
                .unwrap_or_else(|| panic!("{key}@{line} in {hits:?}"))
        };
        let (unwrap9, ask, sleep) = (find("unwrap", 9), find("unwrap", 12), find("sleep", 1));
        let (dbg9, dbg10, attr) = (find("dbg", 9), find("dbg", 10), find("expect-attr", 11));
        let at = |hit, file| HitAt { hit, file };
        // Equal keys keep their input order: dbg@9 comes before unwrap@9 here, unlike the table.
        let input = vec![
            at(sleep, "a.rs"),
            at(dbg10, "a.rs"),
            at(ask, "a.rs"),
            at(attr, "a.rs"),
            at(dbg9, "a.rs"),
            at(dbg10, "B.rs"),
            at(unwrap9, "a.rs"),
        ];

        let text = render_scan_text(input);

        let places: Vec<&str> = text
            .lines()
            .filter(|line| line.starts_with('['))
            .map(|line| line.split(": ").next().unwrap_or(line))
            .collect();
        assert_eq!(
            places,
            [
                "[hard] H-06 B.rs:10",
                "[hard] H-06 a.rs:9",
                "[hard] H-02 a.rs:9",
                "[hard] H-06 a.rs:10",
                "[ask] H-05 a.rs:11",
                "[ask] H-02 a.rs:12",
                "[soft] TST-10 a.rs:1",
            ],
            "the sorted places"
        );
        assert!(
            text.starts_with(&format!("[hard] {}\n", format_hit(dbg10, Some("B.rs")))),
            "each hit is `[sev] ` and format_hit: {text}"
        );
        assert!(
            text.ends_with("(TST-10).\n\n4 blocking, 2 exception request(s), 1 advisory\n"),
            "the summary after an empty line: {text}"
        );
    }

    #[test]
    fn scan_text_without_hits_is_the_summary() {
        assert_eq!(
            render_scan_text(Vec::new()),
            "\n0 blocking, 0 exception request(s), 0 advisory\n",
            "no hits"
        );
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
