//! `temper hook pre-handback` end to end, with real git (port Phase 3f). Ported from 1.x
//! `handlers.test.mjs`'s "SubagentHandback: only the reviewer is checked; an invalid verdict is
//! denied with the problem list" and `verdict.test.mjs`'s "validateVerdict: the paths
//! SubagentStart lists, as given or absolute, cover the changed files".

use std::fs;
use std::path::Path;

use temper_domain::JsValue;

use crate::subagent_start::{LIB, LIB_RS_2, fingerprint, hook, json_str, repo, state};
use crate::support::{TestResult, git, real_path, write_tree};

/// 1.x's `verdict(fp, over)`: a well-formed approval of `fp`, each `over` property replacing
/// its own or added last, as JSON text.
pub(crate) fn verdict(fp: &str, over: &[(&str, &str)]) -> String {
    let scores = "{\"correctness\":4,\"escape_hatches\":4,\"ownership\":4,\"errors\":4,\"api\":4,\"architecture\":4,\"tests\":4,\"performance\":4,\"concurrency\":4,\"observability\":4,\"craft\":4,\"docs\":4}";
    let files_read = format!(
        r#"{{"files_read":[{}],"commands_run":["temper gate"]}}"#,
        json_str(LIB)
    );
    let mut props: Vec<(String, String)> = [
        ("temper_verdict", "1".to_owned()),
        ("verdict", r#""approved""#.to_owned()),
        ("fingerprint", json_str(fp)),
        ("findings", "[]".to_owned()),
        ("scores", scores.to_owned()),
        (
            "gate",
            r#"{"fmt":"pass","clippy":"pass","test":"pass","doc":"pass"}"#.to_owned(),
        ),
        ("evidence", files_read),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    for (key, value) in over {
        match props.iter_mut().find(|(k, _)| k == key) {
            Some(prop) => prop.1 = (*value).to_owned(),
            None => props.push(((*key).to_owned(), (*value).to_owned())),
        }
    }
    let body: Vec<String> = props
        .iter()
        .map(|(k, v)| format!("{}:{v}", json_str(k)))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// 1.x's `fenced(v)`: a report ending with the verdict block.
pub(crate) fn fenced(verdict: &str) -> String {
    format!("Review done.\n\n```json\n{verdict}\n```\n")
}

/// The hook input of a reviewer `agent_id` in session `sid` at `dir`, with `more` properties.
pub(crate) fn input(dir: &Path, sid: &str, more: &str) -> String {
    format!(
        r#"{{"cwd":{},"session_id":"{sid}"{more}}}"#,
        json_str(&dir.to_string_lossy())
    )
}

/// `pre-handback` of a `SubagentHandback({ message })` call.
fn handback(
    dir: &Path,
    sid: &str,
    who: &str,
    message: &str,
) -> Result<JsValue, Box<dyn std::error::Error>> {
    let more = format!(
        r#",{who},"tool_name":"SubagentHandback","tool_input":{{"message":{}}}"#,
        json_str(message)
    );
    hook(dir, "pre-handback", &input(dir, sid, &more))
}

const REVIEWER: &str = r#""agent_type":"temper:rust-reviewer","agent_id":"rv1""#;

fn decision(out: &JsValue) -> Option<(&str, &str)> {
    let spec = out.get("hookSpecificOutput")?;
    Some((
        spec.get("permissionDecision")?.as_str()?,
        spec.get("permissionDecisionReason")?.as_str()?,
    ))
}

pub(crate) fn recorded(dir: &Path, fp: &str) -> Option<JsValue> {
    let text = fs::read_to_string(state(dir).join("verdicts").join(format!("{fp}.json"))).ok()?;
    JsValue::from_json(&text).ok()
}

/// The session's telemetry rows of `event`.
pub(crate) fn rows(
    dir: &Path,
    sid: &str,
    event: &str,
) -> Result<Vec<JsValue>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(state(dir).join("telemetry.jsonl"))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let row = JsValue::from_json(line)?;
        let of = |key: &str, want: &str| row.get(key).and_then(JsValue::as_str) == Some(want);
        if of("sid", sid) && of("event", event) {
            out.push(row);
        }
    }
    Ok(out)
}

#[test]
fn only_the_reviewer_is_checked_and_an_invalid_verdict_is_denied_with_the_problem_list()
-> TestResult {
    let r = repo()?;
    let dir = r.path();
    fs::write(dir.join(LIB), LIB_RS_2)?;
    let implementer = r#""agent_type":"temper:rust-implementer","agent_id":"rv1""#;
    assert_eq!(handback(dir, "s-hb", implementer, "done")?, JsValue::Null);
    let out = handback(dir, "s-hb", REVIEWER, &fenced(&verdict("nothex", &[])))?;
    let (kind, reason) = decision(&out).ok_or("a decision")?;
    assert_eq!(kind, "deny");
    assert!(
        reason.starts_with("temper: the review verdict is invalid, so the report was not delivered:\n- \"fingerprint\" must be the 16-hex fingerprint"),
        "{reason}"
    );
    assert!(
        reason.ends_with("\nFix the verdict block and hand back again."),
        "{reason}"
    );
    let fp = fingerprint(dir, "s-hb")?;
    assert_eq!(
        handback(dir, "s-hb", REVIEWER, &fenced(&verdict(&fp, &[])))?,
        JsValue::Null
    );
    let rec = recorded(dir, &fp).ok_or("a record")?;
    assert_eq!(rec.get("stale"), Some(&JsValue::Bool(false)));
    let session = state(dir).join("sessions").join("s-hb");
    assert_eq!(fs::read_to_string(session.join("verdict-agent-rv1"))?, fp);
    let hash = fs::read_to_string(session.join("verdict-agent-rv1.hash"))?;
    assert_eq!(hash.len(), 16, "{hash}");
    let reviewer = |row: &JsValue| row.get("reviewer").cloned();
    let handbacks: Vec<_> = rows(dir, "s-hb", "handback")?
        .iter()
        .map(reviewer)
        .collect();
    let b = |x| Some(JsValue::Bool(x));
    assert_eq!(handbacks, [b(false), b(true), b(true)]);
    let verdicts = rows(dir, "s-hb", "verdict")?;
    let via: Vec<_> = verdicts
        .iter()
        .map(|row| row.get("via").and_then(JsValue::as_str))
        .collect();
    assert_eq!(via, [Some("handback")]);
    assert_eq!(rows(dir, "s-hb", "verdict-invalid")?.len(), 1);
    Ok(())
}

#[test]
fn the_paths_subagent_start_lists_as_given_or_absolute_cover_the_changed_files() -> TestResult {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    write_tree(
        root,
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n",
            ),
            ("src/lib.rs", "//! App.\n"),
            (
                "crates/core/Cargo.toml",
                "[package]\nname = \"core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("crates/core/src/lib.rs", "//! Core.\n"),
        ],
    )?;
    git(root, &["init", "-q"])?;
    git(root, &["config", "core.autocrlf", "false"])?;
    git(root, &["add", "-A"])?;
    git(root, &["commit", "-qm", "base"])?;
    write_tree(
        root,
        &[
            ("crates/core/src/lib.rs", "//! Core, changed.\n"),
            ("crates/core/src/new.rs", "//! New.\n"),
            ("src/lib.rs", "//! App, changed.\n"),
        ],
    )?;
    let changed = [
        "crates/core/src/lib.rs",
        "crates/core/src/new.rs",
        "src/lib.rs",
    ];
    let real = real_path(root)?;
    let native = |base: &Path, f: &str| base.join(f).to_string_lossy().into_owned();
    // A changes request, so only the coverage decides; each call is a fresh agent.
    let check =
        |n: usize, files_read: Vec<String>| -> Result<Option<String>, Box<dyn std::error::Error>> {
            let fp = fingerprint(root, "s-cover")?;
            let list: Vec<String> = files_read.iter().map(|f| json_str(f)).collect();
            let evidence = format!(
                r#"{{"files_read":[{}],"commands_run":["gate"]}}"#,
                list.join(",")
            );
            let v = verdict(
                &fp,
                &[
                    ("verdict", r#""changes-requested""#),
                    ("evidence", &evidence),
                ],
            );
            let who = format!(r#""agent_type":"rust-reviewer","agent_id":"c{n}""#);
            let out = handback(root, "s-cover", &who, &fenced(&v))?;
            Ok(decision(&out).map(|(_, reason)| reason.to_owned()))
        };
    // the repo-relative paths SubagentStart prints, and the same files as native absolute paths
    assert_eq!(
        check(1, changed.iter().map(|f| (*f).to_owned()).collect())?,
        None
    );
    assert_eq!(
        check(2, changed.iter().map(|f| native(&real, f)).collect())?,
        None
    );
    assert_eq!(
        check(3, changed.iter().map(|f| native(root, f)).collect())?,
        None
    );
    let bare = check(4, vec!["lib.rs".to_owned(), "new.rs".to_owned()])?.ok_or("denied")?;
    assert!(bare.contains("\n- changed files not reviewed: "), "{bare}");
    // the member crate's lib.rs, relative or absolute, does not cover the root package's src/lib.rs
    let member: Vec<&str> = changed
        .iter()
        .copied()
        .filter(|f| f.starts_with("crates/"))
        .collect();
    let missed = "\n- changed files not reviewed: src/lib.rs\n";
    let relative = check(5, member.iter().map(|f| (*f).to_owned()).collect())?.ok_or("denied")?;
    assert!(relative.contains(missed), "{relative}");
    let absolute = check(6, member.iter().map(|f| native(root, f)).collect())?.ok_or("denied")?;
    assert!(absolute.contains(missed), "{absolute}");
    Ok(())
}

#[test]
fn a_record_that_cannot_be_written_fails_open_as_1x_throws() -> TestResult {
    let r = repo()?;
    let dir = r.path();
    fs::write(dir.join(LIB), LIB_RS_2)?;
    let fp = fingerprint(dir, "s-hb-ro")?;
    // A file where the verdicts directory goes: the write throws after the verdict is checked.
    fs::create_dir_all(state(dir))?;
    fs::write(state(dir).join("verdicts"), "not a directory")?;
    let out = handback(dir, "s-hb-ro", REVIEWER, &fenced(&verdict(&fp, &[])))?;
    let message = out
        .get("systemMessage")
        .and_then(JsValue::as_str)
        .unwrap_or_default();
    assert!(
        message.starts_with("temper hook pre-handback failed: "),
        "{out:?}"
    );
    assert!(
        !state(dir)
            .join("sessions")
            .join("s-hb-ro")
            .join("verdict-agent-rv1")
            .exists()
    );
    assert_eq!(rows(dir, "s-hb-ro", "verdict")?.len(), 0);
    Ok(())
}
