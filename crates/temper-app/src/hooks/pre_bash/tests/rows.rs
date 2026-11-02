//! `tests/telemetry.test.mjs`'s "shell deny rows carry the command, redacted and clipped", on
//! that test's one-crate workspace.

use temper_domain::{HookEvent, HookOutcome, JsValue, PathStyle::Posix};

use super::Ws;
use crate::fakes::FakeFiles;

/// The telemetry test's workspace: one crate at the root.
fn one_crate() -> Ws {
    Ws::with(
        FakeFiles::new(Posix)
            .with_file(
                "/ws/Cargo.toml",
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )
            .with_file("/ws/src/lib.rs", "pub fn a() {}\n"),
    )
}

/// The telemetry rows logged so far, and the log they were read from.
fn rows(ws: &Ws) -> (Vec<JsValue>, String) {
    let log = ws
        .ports
        .files
        .text("/ws/target/temper/telemetry.jsonl")
        .expect("the denials are logged");
    let rows = log
        .lines()
        .map(|line| JsValue::from_json(line).expect("each row is JSON"))
        .collect();
    (rows, log)
}

/// The string `key` of each row, empty where it is absent.
fn field(rows: &[JsValue], key: &str) -> Vec<String> {
    rows.iter()
        .map(|row| {
            row.get(key)
                .and_then(JsValue::as_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

/// 1.x "shell deny rows carry the command, redacted and clipped".
#[test]
fn shell_deny_rows_carry_the_command_redacted_and_clipped() {
    let ws = one_crate();
    let cmds = [
        "GH_TOKEN=s3cret cat > src/lib.rs <<'EOF'\npub fn x() {}\nEOF".to_owned(),
        "echo x > Cargo.lock".to_owned(),
        format!(
            "curl -H \"Authorization: Bearer s3cret\" https://example.com/{} > Cargo.toml",
            "p".repeat(300)
        ),
        "cargo test --ignore-rust-version".to_owned(),
    ];
    for cmd in &cmds {
        let json = format!(
            r#"{{"tool_name":"Bash","cwd":"/ws","session_id":"s-cmd","tool_input":{{"command":{}}}}}"#,
            JsValue::String(cmd.to_owned()).to_json()
        );
        let res = ws.run(HookEvent::PreBash, &json);
        assert!(matches!(res, HookOutcome::Deny { .. }), "{cmd:?}: {res:?}");
    }

    let (rows, log) = rows(&ws);
    assert_eq!(
        field(&rows, "event"),
        [
            "deny-shell-write",
            "deny-lockfile",
            "deny-shell-write",
            "deny-ignore-rust-version"
        ]
    );
    let cmd = field(&rows, "cmd");
    assert_eq!(
        cmd,
        [
            "GH_TOKEN=*** cat > src/lib.rs <<…".to_owned(),
            "echo x > Cargo.lock".to_owned(),
            format!("curl -H \"…\" https://example.com/{}…", "p".repeat(168)),
            "cargo test --ignore-rust-version".to_owned(),
        ]
    );
    assert_eq!(cmd.get(2).map(|c| c.encode_utf16().count()), Some(201));
    assert!(
        field(&rows, "sid").iter().all(|sid| sid == "s-cmd"),
        "{log}"
    );
    assert!(!log.contains("s3cret"), "{log}");
}
