//! A hit as `temper scan --json` writes it: 1.x's key order, escapes and pretty printing,
//! checked against 1.x's own output on the 1d.2 fixture (`.temper/plans/port-phase-1d2-oracle`).

use super::scan;
use crate::scan::{CodeKind, HitAt};

/// The fixture's `crates/core/src/asks.rs`: an unwrap under a reasoned `#[expect]` (an exception
/// request, whose `exception` comes before `msg`), a soft sleep, and the `expect-attr` finder's
/// own hit (whose `exception` comes after `msg`).
const ASKS_RS: &str = "#[expect(clippy::unwrap_used, reason = \"the caller checked it\")]\npub fn checked(v: Option<u8>) -> u8 {\n    v.unwrap()\n}\n\npub fn wait(d: std::time::Duration) {\n    std::thread::sleep(d);\n}\n";

/// 1.x `scan --all --json`, the `asks.rs` hits.
const ASKS_JSON: &str = r##"[
  {
    "key": "unwrap",
    "id": "H-02",
    "severity": "ask",
    "line": 3,
    "text": "v.unwrap()",
    "exception": "clippy::unwrap_used",
    "msg": "unwrap/expect in production code",
    "fix": "Propagate with `?`, convert with `ok_or_else`/`let ... else`, or make the invariant a type. See references/errors.md \"Eliminating unwrap/expect\".",
    "file": "crates/core/src/asks.rs"
  },
  {
    "key": "sleep",
    "id": "TST-10",
    "severity": "soft",
    "line": 7,
    "text": "std::thread::sleep(d);",
    "msg": "`thread::sleep`",
    "fix": "In async code this blocks the executor (ASY-06); use `tokio::time::sleep`. In tests, inject a clock or use paused time (TST-10).",
    "file": "crates/core/src/asks.rs"
  },
  {
    "key": "expect-attr",
    "id": "H-05",
    "severity": "ask",
    "line": 1,
    "text": "#[expect(clippy::unwrap_used, reason = \"the caller checked it\")]",
    "msg": "lint exception requested for `clippy::unwrap_used`",
    "exception": "clippy::unwrap_used",
    "fix": "Exception requests need human approval (H-05).",
    "file": "crates/core/src/asks.rs"
  }
]"##;

/// The fixture's `crates/core/src/lib.rs`: a control character, a tab and a raw `é` in a text.
const LIB_RS: &str = "//! Core.\n\n/// Parses.\npub fn parse(v: Option<u8>) -> u8 {\n    v.unwrap()\n}\n\npub fn label(v: Option<&str>) -> &str {\n    v.expect(\"\u{e9}\u{1}\tx\")\n}\n";

/// 1.x `scan --all --json`, the `lib.rs` hits.
const LIB_JSON: &str = r#"[
  {
    "key": "unwrap",
    "id": "H-02",
    "severity": "hard",
    "line": 5,
    "text": "v.unwrap()",
    "msg": "unwrap/expect in production code",
    "fix": "Propagate with `?`, convert with `ok_or_else`/`let ... else`, or make the invariant a type. See references/errors.md \"Eliminating unwrap/expect\".",
    "file": "crates/core/src/lib.rs"
  },
  {
    "key": "unwrap",
    "id": "H-02",
    "severity": "hard",
    "line": 9,
    "text": "v.expect(\"é\u0001\tx\")",
    "msg": "unwrap/expect in production code",
    "fix": "Propagate with `?`, convert with `ok_or_else`/`let ... else`, or make the invariant a type. See references/errors.md \"Eliminating unwrap/expect\".",
    "file": "crates/core/src/lib.rs"
  }
]"#;

#[test]
fn hit_json_matches_1x_field_order() {
    for (src, file, want) in [
        (ASKS_RS, "crates/core/src/asks.rs", ASKS_JSON),
        (LIB_RS, "crates/core/src/lib.rs", LIB_JSON),
    ] {
        let report = scan(src, CodeKind::Prod);
        let at: Vec<HitAt<'_>> = report
            .hits()
            .iter()
            .map(|hit| HitAt { hit, file })
            .collect();

        let json = serde_json::to_string_pretty(&at).expect("a hit always serializes");

        assert_eq!(json, want, "{file}");
    }
}

#[test]
fn a_bare_hit_has_no_file_key() {
    let report = scan("fn f() { dbg!(1); }", CodeKind::Prod);

    let json = serde_json::to_string(report.hits()).expect("a hit always serializes");

    assert_eq!(
        json,
        concat!(
            r#"[{"key":"dbg","id":"H-06","severity":"hard","line":1,"text":"fn f() { dbg!(1); }","#,
            r#""msg":"`dbg!` left in code","fix":"Remove it. Use `tracing::debug!` with structured "#,
            r#"fields for diagnostics that should stay."}]"#
        ),
        "a hit without its file"
    );
}
