//! `status --json` (port Phase 3c): the ported `status-json.test.mjs` tests over fakes, the
//! running gate and the pid probe.

use std::cell::Cell;

use temper_domain::PathStyle::Posix;
use temper_domain::{JsValue, Scanner};

use super::{HEAD, OTHER, repo};
use crate::engine::{CommandError, CommandOutput, Engine, Exit, OutputFormat, RunEnv, StatusArgs};
use crate::fakes::{FakeFiles, FakePorts};
use crate::ports::EntryKind;

const SCORES: &str = r#"{"correctness":4,"escape_hatches":4,"ownership":3,"errors":3,"api":3,"architecture":"n/a","tests":3,"performance":"n/a","concurrency":"n/a","observability":"n/a","craft":4,"docs":3}"#;

fn json_args(session: Option<&str>) -> StatusArgs {
    StatusArgs {
        format: OutputFormat::Json,
        session: session.map(str::to_owned),
    }
}

/// An engine at `/w` reporting the version 1.6.0, as the launcher passes it.
fn engine_in(ports: FakePorts, env: impl FnOnce(RunEnv) -> RunEnv) -> Engine<FakePorts> {
    let env = env(RunEnv {
        plugin_version: Some("1.6.0".to_owned()),
        ..RunEnv::at("/w", Posix)
    });
    Engine::new(ports, env, Scanner::new().expect("the scanner builds"))
}

fn run(engine: &Engine<FakePorts>, session: Option<&str>) -> CommandOutput {
    engine.status(&json_args(session)).expect("status answers")
}

/// The document's `key`, as JSON.
fn field(out: &CommandOutput, key: &str) -> String {
    let doc = JsValue::from_json(&out.stdout).expect("the document is JSON");
    doc.get(key)
        .map_or_else(|| "undefined".to_owned(), JsValue::to_json)
}

/// The tree's fingerprint in the repository `files` makes.
fn fp_of(files: FakeFiles) -> String {
    let out = run(&engine_in(repo(files, &[("HEAD", HEAD)]), |e| e), None);
    field(&out, "fingerprint").trim_matches('"').to_owned()
}

/// `files` plus `rel` under the state dir, holding `text`.
fn with_state(files: FakeFiles, rel: &str, text: &str) -> FakeFiles {
    files.with_file(&format!("/w/.git/temper/{rel}"), text)
}

fn answer(files: FakeFiles) -> CommandOutput {
    run(&engine_in(repo(files, &[("HEAD", HEAD)]), |e| e), None)
}

/// Ported "status --json: a green gate and an approved verdict for the current tree": the
/// whole document, byte for byte (no changes here: the fake git lists none).
#[test]
fn a_green_gate_and_an_approved_verdict_give_1xs_document() {
    let fp = fp_of(FakeFiles::new(Posix));
    let gate = format!(
        r#"{{"ok":true,"fp":"{fp}","at":"2026-10-11T10:00:00.000Z","steps":[{{"name":"fmt","ok":true,"ms":120,"summary":"applied"}},{{"name":"clippy","ok":true,"ms":9000,"summary":"clean","log":"/x/clippy.log"}}]}}"#
    );
    let verdict = format!(
        r#"{{"temper_verdict":1,"verdict":"approved","fingerprint":"{fp}","scores":{SCORES},"recorded_at":"2026-10-11T10:05:00.000Z","stale":false,"findings":[{{"severity":"nit","rule":"API-01","file":"src/lib.rs","line":2,"evidence":"pub fn b() {{}}","fix":"document b"}}],"evidence":{{"files_read":["src/lib.rs"],"commands_run":["temper gate"]}}}}"#
    );
    let files = with_state(FakeFiles::new(Posix), &format!("gate/{fp}.json"), &gate);
    let files = with_state(files, &format!("verdicts/{fp}.json"), &verdict);

    let out = answer(files);

    let want = format!(
        concat!(
            r#"{{"temper_status":1,"version":"1.6.0","root":"/w","stateDir":"/w/.git/temper","base":"{head}","fingerprint":"{fp}","changed":[],"#,
            r#""gate":{{"ok":true,"unavailable":false,"at":"2026-10-11T10:00:00.000Z","steps":[{{"name":"fmt","ok":true,"ms":120,"summary":"applied"}},{{"name":"clippy","ok":true,"ms":9000,"summary":"clean"}}],"running":null}},"#,
            r#""review":{{"verdict":"approved","stale":false,"fingerprint":"{fp}","recorded_at":"2026-10-11T10:05:00.000Z","scores":{scores},"findings":[{{"severity":"nit","rule":"API-01","file":"src/lib.rs","line":2,"fix":"document b"}}]}},"#,
            r#""baseline":null,"config":{{"review":"required","skillGate":true}}}}"#,
            "\n"
        ),
        head = HEAD,
        fp = fp,
        scores = SCORES
    );
    assert_eq!(
        (out.stdout, out.stderr, out.exit),
        (want, String::new(), Exit::Ok)
    );
}

/// Ported "status --json: a red gate, a stale verdict and a readable baseline".
#[test]
fn a_red_gate_a_stale_verdict_and_a_readable_baseline() {
    let baseline = r#"{"temper_baseline":1,"toolchain":"1.90.0","totals":{"a":{"clippy::x":2,"clippy::y":3}}}"#;
    let base = || FakeFiles::new(Posix).with_file("/w/.claude/temper-baseline.json", baseline);
    let fp = fp_of(base());
    let files = with_state(
        base(),
        &format!("gate/{fp}.json"),
        r#"{"ok":false,"at":"2026-10-11T10:00:00.000Z","steps":[{"name":"clippy","ok":false,"ms":50,"summary":"error: unused"}]}"#,
    );
    let verdict = format!(
        r#"{{"temper_verdict":1,"verdict":"changes-requested","fingerprint":"ffffffffffffffff","scores":{SCORES},"recorded_at":"2026-10-11T10:05:00.000Z","stale":true,"findings":[{{"severity":"major","rule":"ERR-02","file":"src/lib.rs","evidence":"x","fix":"type the error"}}]}}"#
    );
    let files = with_state(files, &format!("verdicts/{fp}.json"), &verdict);

    let out = answer(files);

    assert_eq!(
        field(&out, "gate"),
        r#"{"ok":false,"unavailable":false,"at":"2026-10-11T10:00:00.000Z","steps":[{"name":"clippy","ok":false,"ms":50,"summary":"error: unused"}],"running":null}"#
    );
    assert_eq!(
        field(&out, "review"),
        format!(
            r#"{{"verdict":"changes-requested","stale":true,"fingerprint":"ffffffffffffffff","recorded_at":"2026-10-11T10:05:00.000Z","scores":{SCORES},"findings":[{{"severity":"major","rule":"ERR-02","file":"src/lib.rs","line":null,"fix":"type the error"}}]}}"#
        )
    );
    assert_eq!(
        field(&out, "baseline"),
        r#"{"path":".claude/temper-baseline.json","total":5,"toolchain":"1.90.0"}"#
    );
}

/// Ported "status --json: no gate and no review for the tree are null; an unreadable baseline
/// is an error object".
#[test]
fn no_records_are_null_and_an_unreadable_baseline_is_an_error_object() {
    let files = FakeFiles::new(Posix).with_file("/w/.claude/temper-baseline.json", "{ not json");

    let out = answer(files);

    assert_eq!(
        [
            field(&out, "gate"),
            field(&out, "review"),
            field(&out, "disabled")
        ],
        ["null", "null", "undefined"]
    );
    assert!(
        field(&out, "baseline")
            .starts_with(r#"{"error":".claude/temper-baseline.json is not valid JSON ("#),
        "{}",
        out.stdout
    );
}

/// Ported "status --json: malformed gate and verdict files read as none, or lose their bad
/// entries" (F6: nothing fails).
#[test]
fn malformed_records_read_as_none_or_lose_their_bad_entries() {
    let fp = fp_of(FakeFiles::new(Posix));
    let gate = format!("gate/{fp}.json");
    let verdict = format!("verdicts/{fp}.json");
    let both = |g: &str, v: &str| {
        let files = with_state(FakeFiles::new(Posix), &gate, g);
        answer(with_state(files, &verdict, v))
    };

    let scalars = both("[1]", "7");
    let entries = both(
        r#"{"ok":false,"steps":[null,{"name":"clippy","ok":false,"ms":1,"summary":"x"}]}"#,
        r#"{"verdict":"approved","findings":[null,3]}"#,
    );
    let object_steps = both(r#"{"ok":true,"steps":{}}"#, "{not json");

    assert_eq!(
        [field(&scalars, "gate"), field(&scalars, "review")],
        ["null", "null"]
    );
    assert!(
        field(&entries, "gate")
            .contains(r#""steps":[{"name":"clippy","ok":false,"ms":1,"summary":"x"}]"#),
        "{}",
        entries.stdout
    );
    assert!(
        field(&entries, "review").ends_with(r#""findings":[]}"#),
        "{}",
        entries.stdout
    );
    assert!(
        field(&object_steps, "gate").contains(r#""steps":[]"#),
        "{}",
        object_steps.stdout
    );
    assert_eq!(field(&object_steps, "review"), "null");
}

/// Ported "status --json: outside a Rust project it says project: null and exits 0"; the text
/// form keeps its refusal.
#[test]
fn outside_a_rust_project_the_document_says_project_null() {
    let engine = engine_in(FakePorts::new(FakeFiles::new(Posix)), |e| e);

    let out = run(&engine, None);

    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.exit),
        (
            "{\"temper_status\":1,\"version\":\"1.6.0\",\"project\":null}\n",
            "",
            Exit::Ok
        )
    );
    assert!(matches!(
        engine.status(&super::TEXT),
        Err(CommandError::Refused {
            exit: Exit::Usage,
            ..
        })
    ));
}

/// Ported "status --json: `TEMPER_DISABLE=1` marks the document disabled", as its last key.
#[test]
fn temper_disable_marks_the_document_disabled() {
    let engine = engine_in(repo(FakeFiles::new(Posix), &[("HEAD", HEAD)]), |e| RunEnv {
        disabled: true,
        ..e
    });

    let out = run(&engine, None);

    assert!(
        out.stdout.ends_with(",\"disabled\":true}\n"),
        "{}",
        out.stdout
    );
    assert_eq!(field(&out, "temper_status"), "1");
}

/// Port plan 3-13: without `TEMPER_PLUGIN_VERSION`, the engine's own version.
#[test]
fn without_a_plugin_version_the_engines_own_is_reported() {
    let engine = engine_in(FakePorts::new(FakeFiles::new(Posix)), |e| RunEnv {
        plugin_version: None,
        ..e
    });

    let out = run(&engine, None);

    assert_eq!(
        field(&out, "version"),
        format!("\"{}\"", env!("CARGO_PKG_VERSION"))
    );
}

/// Ported "status --json --session: the session's recorded base wins over HEAD" (the base;
/// the changes and fingerprint with real git are in cli/it).
#[test]
fn a_sessions_recorded_base_wins_over_head() {
    let files = FakeFiles::new(Posix).with_file("/w/.git/temper/sessions/s1/base", OTHER);
    let engine = engine_in(repo(files, &[(OTHER, OTHER), ("HEAD", HEAD)]), |e| e);

    let session = run(&engine, Some("s1"));
    let nobody = run(&engine, Some("nobody"));
    let plain = run(&engine, None);

    assert_eq!(
        [
            field(&session, "base"),
            field(&nobody, "base"),
            field(&plain, "base")
        ],
        [
            format!("\"{OTHER}\""),
            format!("\"{HEAD}\""),
            format!("\"{HEAD}\"")
        ]
    );
}

/// `/proc` showing this process (4242) and the gate at pid 77.
fn with_proc(files: FakeFiles) -> FakeFiles {
    files
        .with_file("/proc/4242/status", "")
        .with_listing("/proc", &[("77", EntryKind::Dir), ("4242", EntryKind::Dir)])
}

/// A progress file for `fp` at `pid`, started a second before the fakes' clock.
fn progress(fp: &str, pid: u32) -> String {
    format!(
        r#"{{"fp":"{fp}","pid":{pid},"startedAt":"2024-10-27T03:33:19.123Z","steps":[{{"name":"fmt","ok":true,"ms":10}}],"current":{{"name":"clippy","startedAt":"2024-10-27T03:33:20.000Z"}}}}"#
    )
}

/// A gate running now for the tree shows as `gate.running`, with `ok: null` before any
/// result; one whose process is gone does not.
#[test]
fn a_gate_running_now_shows_as_running_and_a_dead_one_does_not() {
    let fp = fp_of(FakeFiles::new(Posix));
    let at = format!("gate-progress-{fp}.json");

    let live = answer(with_state(
        with_proc(FakeFiles::new(Posix)),
        &at,
        &progress(&fp, 77),
    ));
    let dead = answer(with_state(
        with_proc(FakeFiles::new(Posix)),
        &at,
        &progress(&fp, 78),
    ));

    assert_eq!(
        field(&live, "gate"),
        r#"{"ok":null,"unavailable":false,"at":null,"steps":[],"running":{"startedAt":"2024-10-27T03:33:19.123Z","steps":[{"name":"fmt","ok":true,"ms":10}],"current":{"name":"clippy","startedAt":"2024-10-27T03:33:20.000Z"}}}"#
    );
    assert_eq!(field(&dead, "gate"), "null");
}

/// The probe runs only when a progress file names this tree, and then once (OC-2 A): a
/// machine without `/proc` runs `ps` for it, and nothing else does.
#[test]
fn the_pid_probe_runs_once_and_only_for_this_trees_progress_file() {
    let fp = fp_of(FakeFiles::new(Posix));
    let at = format!("gate-progress-{fp}.json");
    let calls = |files: FakeFiles| {
        let ports = repo(files, &[("HEAD", HEAD)]);
        let env = RunEnv::at("/w", Posix);
        let out = crate::status::run(&ports, &env, &Cell::new(false), &json_args(None));
        assert!(out.is_ok_and(|out| out.exit == Exit::Ok), "status answers");
        ports.process.calls()
    };

    let none = calls(FakeFiles::new(Posix));
    let other = calls(with_state(
        FakeFiles::new(Posix),
        &at,
        &progress("fedcba9876543210", 77),
    ));
    let ours = calls(with_state(FakeFiles::new(Posix), &at, &progress(&fp, 77)));

    assert_eq!(
        (other, ours),
        (none, none + 1),
        "one ps call, for this tree only"
    );
}

/// A progress time 1.x's `Date.parse` cannot convert throws outside any `try` in 1.x, so
/// `status --json` fails there too (F6's one exception, as the text's verdict line).
#[test]
fn a_progress_time_1x_cannot_convert_fails_status() {
    let fp = fp_of(FakeFiles::new(Posix));
    let text = format!(r#"{{"fp":"{fp}","pid":77,"startedAt":{{"toString":1}}}}"#);
    let files = with_state(
        with_proc(FakeFiles::new(Posix)),
        &format!("gate-progress-{fp}.json"),
        &text,
    );
    let engine = engine_in(repo(files, &[("HEAD", HEAD)]), |e| e);

    let thrown = engine.status(&json_args(None));

    assert!(
        matches!(thrown, Err(CommandError::Unconvertible(_))),
        "{thrown:?}"
    );
}
