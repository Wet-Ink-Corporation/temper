//! Ported from 1.x `tests/telemetry.test.mjs` "`SessionStart` on resume logs gate-lost for the
//! session's killed gates, once, with the step and heartbeat" and "a lost gate another hook has
//! claimed is not logged again", at the level of `logLostGates`.

use temper_domain::PathStyle::Posix;
use temper_domain::{Config, ModTime, Timestamp};

use super::log_lost_gates;
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, NOW};
use crate::hooks::{HookCall, HookInput};
use crate::ports::EntryKind;
use crate::project::Workspace;

const STATE: &str = "/w/.git/temper";
const LOG: &str = "/w/.git/temper/telemetry.jsonl";
const PROGRESS: &str = "/w/.git/temper/gate-progress-00000000000000aa.json";
const SESSION: &str = "/w/.git/temper/sessions/s-lost";
/// Dead pids; the fakes' own is 4242 and 77 runs.
const MAIN: u32 = 1001;
const SUB: u32 = 1002;

fn workspace() -> Workspace {
    Workspace {
        root: "/w".to_owned(),
        state_dir: STATE.to_owned(),
        config: Config::defaults(),
        config_problem: None,
        git: true,
    }
}

/// A minute before the fakes' clock, when every gate here started.
fn t0() -> i64 {
    NOW.unix_millis() - 60_000
}

fn at(ms: i64) -> String {
    Timestamp::from_unix_millis(t0() + ms).to_string()
}

/// `/proc` listing this process and pid 77.
fn with_proc(files: FakeFiles) -> FakeFiles {
    files
        .with_file("/proc/4242/status", "")
        .with_listing("/proc", &[("77", EntryKind::Dir), ("4242", EntryKind::Dir)])
        .with_link("/proc/self", "/proc/4242")
}

/// A `gate-start` row of session `sid` with `fields` after its `event`.
fn start(sid: &str, fields: &str) -> String {
    format!(
        r#"{{"t":"{}","sid":"{sid}","agent":"main","event":"gate-start",{fields},"timeoutMs":1200000}}"#,
        at(0)
    )
}

fn log(ports: &FakePorts, sid: &str) -> usize {
    let input =
        HookInput::parse(&format!(r#"{{"session_id":"{sid}","source":"resume"}}"#)).expect("JSON");
    let call = HookCall {
        ports,
        env: &RunEnv::at("/w", Posix),
        input: &input,
    };
    log_lost_gates(&call, &workspace()).rows
}

/// The `gate-lost` rows in the log.
fn lost_rows(ports: &FakePorts) -> Vec<String> {
    let text = ports.files.text(LOG).unwrap_or_default();
    text.lines()
        .filter(|line| line.contains(r#""event":"gate-lost""#))
        .map(str::to_owned)
        .collect()
}

fn killed_gates() -> FakeFiles {
    let rows = [
        start("s-lost", &format!(r#""pid":{MAIN}"#)),
        format!(
            r#"{{"t":"{}","sid":"s-lost","agent":"temper:rust-implementer","event":"gate-start","agent_id":"im1","pid":{SUB},"timeoutMs":1200000}}"#,
            at(0)
        ),
        // A gate whose process still runs (this one, or pid 77) is not lost; nor is another
        // session's.
        start("s-lost", r#""agent_id":"im2","pid":4242"#),
        start("s-lost", r#""pid":77"#),
        start("other", r#""pid":1003"#),
    ];
    let progress = format!(
        r#"{{"fp":"00000000000000aa","pid":{MAIN},"startedAt":"{}","steps":[{{"name":"fmt","ok":true,"ms":800}}],"current":{{"name":"test","startedAt":"{}"}}}}"#,
        at(0),
        at(20_000)
    );
    let beat = format!(r#"{{"pid":{SUB},"at":{}}}"#, t0() + 45_000);
    with_proc(FakeFiles::new(Posix))
        .with_file(LOG, &(rows.join("\n") + "\n"))
        .with_file(PROGRESS, &progress)
        .with_mtime(
            PROGRESS,
            ModTime::from_unix_nanos(i128::from(t0() + 20_000) * 1_000_000),
        )
        .with_file(&format!("{SESSION}/gate-alive-agent-im1.json"), &beat)
}

#[test]
fn the_session_s_killed_gates_are_logged_once_with_the_step_and_heartbeat() {
    let ports = FakePorts::new(killed_gates());

    let first = log(&ports, "s-lost");
    let second = log(&ports, "s-lost");

    let now = NOW.to_string();
    assert_eq!(
        lost_rows(&ports),
        [
            format!(
                r#"{{"t":"{now}","sid":"s-lost","agent":"main","event":"gate-lost","pid":{MAIN},"reason":"interrupted","ranMs":20000,"foundMs":60000,"timeoutMs":1200000,"heartbeat":false,"progress":true,"step":"test","steps":{{"fmt":800}}}}"#
            ),
            format!(
                r#"{{"t":"{now}","sid":"s-lost","agent":"temper:rust-implementer","event":"gate-lost","pid":{SUB},"reason":"interrupted","ranMs":45000,"foundMs":60000,"timeoutMs":1200000,"heartbeat":true,"progress":false,"step":null,"steps":{{}}}}"#
            ),
        ],
        "1.x's rows, the second hook's claims finding them logged"
    );
    assert_eq!([first, second], [2, 0], "logged once");
    assert_eq!(
        ports.files.text(PROGRESS),
        None,
        "the progress file has served"
    );
    let claim = format!("{SESSION}/gate-lost-{MAIN}-{}", t0());
    assert_eq!(ports.files.text(&claim).as_deref(), Some(""), "the claim");
}

#[test]
fn a_gate_another_hook_has_claimed_is_not_logged_and_keeps_its_progress_file() {
    let claim = format!("{SESSION}/gate-lost-{MAIN}-{}", t0());
    let files = killed_gates().with_file(&claim, "");
    let ports = FakePorts::new(files);

    let logged = log(&ports, "s-lost");

    assert_eq!(logged, 1, "only the implementer's gate");
    assert!(
        ports.files.text(PROGRESS).is_some(),
        "the claimant removes it, not this hook"
    );
}

#[test]
fn a_progress_file_of_another_pid_stays() {
    let ports = FakePorts::new(killed_gates());

    log(&ports, "other");

    assert_eq!(lost_rows(&ports).len(), 1, "pid 1003's gate");
    assert!(ports.files.text(PROGRESS).is_some(), "pid 1001's file");
}

#[test]
fn without_a_foreign_open_gate_no_pid_is_probed() {
    // No `/proc` and no `ps`: a probe would read as Unknown, and count a process call.
    let rows = [
        start("s1", r#""pid":4242"#),
        start("s1", r#""pid":"7""#),
        start("s1", r#""pid":1.5"#),
        start("s1", r#""pid":9"#),
        r#"{"sid":"s1","agent":"main","event":"stop-green","pid":9}"#.to_owned(),
    ];
    let files = FakeFiles::new(Posix).with_file(LOG, &rows.join("\n"));
    let ports = FakePorts {
        process: FakeProcess::new(),
        ..FakePorts::new(files)
    };

    let logged = log(&ports, "s1");

    assert_eq!(logged, 0, "nothing lost");
    assert_eq!(ports.process.calls(), 0, "no probe");
    assert_eq!(ports.files.paths(), [LOG], "no claim, no progress read");
}

#[test]
fn a_throw_ends_the_loop_after_its_claim() {
    // 1.x's `lostGateRow` reads `s.name` of a null step and throws; its `catch` ends the loop.
    let rows = [
        start("s1", &format!(r#""pid":{MAIN}"#)),
        start("s1", &format!(r#""pid":{SUB}"#)),
    ];
    let files = with_proc(FakeFiles::new(Posix))
        .with_file(LOG, &rows.join("\n"))
        .with_file(PROGRESS, &format!(r#"{{"pid":{MAIN},"steps":[null]}}"#))
        .with_mtime(PROGRESS, ModTime::from_unix_nanos(0));
    let ports = FakePorts::new(files);

    let logged = log(&ports, "s1");

    let claim = |pid: u32| format!("{STATE}/sessions/s1/gate-lost-{pid}-{}", t0());
    assert_eq!(logged, 0, "no row");
    assert_eq!(
        [MAIN, SUB].map(|pid| ports.files.text(&claim(pid)).is_some()),
        [true, false],
        "the first claimed, the second never reached"
    );
}
