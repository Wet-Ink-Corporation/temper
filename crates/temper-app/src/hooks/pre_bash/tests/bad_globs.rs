//! A `protected` or `scanner.exempt` glob that does not compile. 1.x compiles each glob where
//! `checkFile` first tries it, so its throw (caught by `readerRule`) stops the copy scan only
//! once a copy is read: an unread copy, an unread patch or a direct write is refused as with no
//! such glob, and a copy that reaches the glob gives the rule no answer. Each answer was probed
//! against 1.x on the same workspace.

use temper_domain::HookOutcome;

use super::{Ws, files};

/// A glob past `regex`'s size limit, as `"**/" + "a*?".repeat(40000) + "/x"`.
fn uncompilable() -> String {
    format!("**/{}/x", "a*?".repeat(40_000))
}

/// The workspace with `config` as its `.claude/temper.json`, and a clean and an unclean
/// source at its root.
fn ws_with(config: &str) -> Ws {
    Ws::with(
        files()
            .with_file("/ws/.claude/temper.json", config)
            .with_file("/ws/bad.rs", "fn f() { x.unwrap(); }\n")
            .with_file("/ws/ok.rs", "//! ok\n"),
    )
}

/// A workspace whose `protected` list holds the uncompilable glob.
fn protected_ws() -> Ws {
    let glob = temper_domain::JsValue::String(uncompilable()).to_json();
    ws_with(&format!(r#"{{"protected":[{glob}]}}"#))
}

/// A workspace whose one `scanner.exempt` entry, for `rule`, holds the uncompilable glob.
fn exempt_ws(rule: &str) -> Ws {
    let glob = temper_domain::JsValue::String(uncompilable()).to_json();
    ws_with(&format!(
        r#"{{"scanner":{{"exempt":[{{"rule":"{rule}","paths":[{glob}]}}]}}}}"#
    ))
}

const UNREAD: &[&str] = &[
    "cp $X crates/ledger-domain/src/x.rs",
    "git apply missing.patch",
    r"printf x > crates/ledger-domain/src/x.r\s",
];

/// `cmd` is denied, as it is with no such glob.
fn denied_as_without(ws: &Ws, cmd: &str) {
    let plain = ws_with("{}").bash(cmd);
    assert!(
        matches!(plain, HookOutcome::Deny { .. }),
        "{cmd:?}: {plain:?}"
    );
    assert_eq!(ws.bash(cmd), plain, "{cmd:?}");
}

#[test]
fn an_uncompilable_protected_glob_still_refuses_what_temper_cannot_read() {
    let ws = protected_ws();
    for cmd in UNREAD {
        denied_as_without(&ws, cmd);
    }
}

#[test]
fn an_uncompilable_protected_glob_stops_the_scan_at_the_first_copy_read() {
    let ws = protected_ws();
    denied_as_without(&ws_with("{}"), "cp bad.rs crates/ledger-domain/src/x.rs");
    for cmd in [
        "cp bad.rs crates/ledger-domain/src/x.rs",
        "cp ok.rs crates/ledger-domain/src/x.rs",
        "cp ok.rs crates/ledger-domain/src/x.rs; cp $X crates/ledger-domain/src/y.rs",
        "cp $X crates/ledger-domain/src/y.rs; cp ok.rs crates/ledger-domain/src/x.rs",
    ] {
        assert_eq!(ws.bash(cmd), HookOutcome::Pass, "{cmd:?}");
    }
}

#[test]
fn an_uncompilable_exempt_glob_is_compiled_only_for_a_new_hit_of_its_rule() {
    let unwrap_rule = exempt_ws("H-02");
    for cmd in UNREAD {
        denied_as_without(&unwrap_rule, cmd);
    }
    for cmd in [
        "cp bad.rs crates/ledger-domain/src/x.rs",
        "cp ok.rs crates/ledger-domain/src/x.rs",
    ] {
        assert_eq!(unwrap_rule.bash(cmd), HookOutcome::Pass, "{cmd:?}");
    }
    // Another rule's glob is never tried, so the `unwrap` is denied.
    denied_as_without(
        &exempt_ws("H-03"),
        "cp bad.rs crates/ledger-domain/src/x.rs",
    );
}
