//! Ported from 1.x `tests/state-retention.test.mjs` (the prune tests) and
//! `tests/gate-progress.test.mjs` (pruning), over the fakes at [`NOW`].

use temper_domain::PathStyle::Posix;
use temper_domain::{Config, ModTime};

use super::{
    GATE, GRACE_MINUTES, Limits, VERDICTS, prune_fingerprint_state, prune_markers, prune_sessions,
};
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, NOW};
use crate::project::Workspace;

const STATE: &str = "/s";
const DAY: i64 = 86_400_000;
const MINUTE: i64 = 60_000;
/// Past the grace period, well inside the age limits.
const HOURS2: i64 = 2 * 60 * MINUTE;

fn workspace() -> Workspace {
    Workspace {
        root: "/w".to_owned(),
        state_dir: STATE.to_owned(),
        config: Config::defaults(),
        config_problem: None,
        git: true,
    }
}

/// [`NOW`] less `age_ms`, as a modification time.
fn mtime(age_ms: i64) -> ModTime {
    ModTime::from_unix_nanos(i128::from(NOW.unix_millis() - age_ms) * 1_000_000)
}

/// 1.x's `put(proj, rel, ageDays, text, i)`: a file under the state dir with the mtime
/// `age_ms` ago. Callers add [`nth`] for 1.x's `i`, so ages are distinct.
fn put(files: FakeFiles, rel: &str, age_ms: i64, text: &str) -> FakeFiles {
    let path = format!("{STATE}/{rel}");
    files
        .with_file(&path, text)
        .with_mtime(&path, mtime(age_ms))
}

/// 1.x's `i` ms older.
fn nth(i: u32) -> i64 {
    i64::from(i)
}

fn fp(n: u32) -> String {
    format!("{n:016x}")
}

/// The names in `dir` under the state dir, sorted.
fn list(ports: &FakePorts, dir: &str) -> Vec<String> {
    let prefix = format!("{STATE}/{dir}/");
    let mut names: Vec<String> = ports
        .files
        .paths()
        .iter()
        .filter_map(|p| p.strip_prefix(&prefix).map(str::to_owned))
        .collect();
    names.sort();
    names
}

fn has(ports: &FakePorts, rel: &str) -> bool {
    ports.files.text(&format!("{STATE}/{rel}")).is_some()
}

fn prune(files: FakeFiles, protect: Option<&str>) -> FakePorts {
    let ports = FakePorts::new(files);
    prune_fingerprint_state(&ports, Posix, &workspace(), protect);
    ports
}

#[test]
fn retention_limits_are_stated_and_ordered() {
    assert_eq!(
        (VERDICTS, GATE, GRACE_MINUTES),
        (
            Limits {
                max_age_days: 60,
                keep: 300,
                max_bytes: None
            },
            Limits {
                max_age_days: 30,
                keep: 300,
                max_bytes: Some(64 * 1024 * 1024)
            },
            60
        ),
        "1.x's RETENTION: verdicts outlive gate results, whose logs go with them"
    );
}

#[test]
fn records_past_their_age_go_and_fresher_ones_stay() {
    let mut files = FakeFiles::new(Posix);
    for (rel, age) in [
        (format!("gate/{}.json", fp(1)), 31 * DAY),
        (format!("gate/{}.json", fp(2)), 29 * DAY),
        (format!("verdicts/{}.json", fp(1)), 61 * DAY),
        (format!("verdicts/{}.json", fp(2)), 59 * DAY),
        (format!("verdicts/{}.json", fp(3)), 31 * DAY),
        (format!("verdicts/{}.history.jsonl", fp(1)), 61 * DAY),
        (format!("verdicts/{}.history.jsonl", fp(2)), 59 * DAY),
        (format!("logs/{}-clippy.log", fp(1)), 31 * DAY),
        (format!("logs/{}-test.log", fp(2)), 29 * DAY),
        (format!("logs/{}-test.log", fp(4)), 31 * DAY),
        (format!("logs/{}-test.log", fp(5)), 15 * DAY),
    ] {
        files = put(files, &rel, age, "{}");
    }

    let ports = prune(files, None);

    assert_eq!(list(&ports, "gate"), [format!("{}.json", fp(2))], "gate");
    assert_eq!(
        list(&ports, "verdicts"),
        [
            format!("{}.history.jsonl", fp(2)),
            format!("{}.json", fp(2)),
            format!("{}.json", fp(3))
        ],
        "verdicts, a history going with its verdict"
    );
    assert_eq!(
        list(&ports, "logs"),
        [format!("{}-test.log", fp(2)), format!("{}-test.log", fp(5))],
        "logs"
    );
}

#[test]
fn a_busy_repository_keeps_only_the_newest_gate_runs_each_with_all_its_logs() {
    let keep = GATE.keep;
    let mut files = FakeFiles::new(Posix);
    for i in 0..keep + 5 {
        let gate = format!("gate/{}.json", fp(i));
        files = put(files, &gate, HOURS2 + nth(i * 1000), "{}");
        for step in ["clippy", "test", "doctest", "doc"] {
            let log = format!("logs/{}-{step}.log", fp(i));
            files = put(files, &log, HOURS2 + nth(i * 1000), "x");
        }
    }
    for i in 0..keep + 3 {
        let verdict = format!("verdicts/{}.json", fp(i));
        files = put(files, &verdict, HOURS2 + nth(i * 1000), "{}");
    }

    let ports = prune(files, None);

    let gates = list(&ports, "gate");
    assert_eq!(gates.len(), 300, "the newest 300");
    assert!(gates.contains(&format!("{}.json", fp(0))), "the newest");
    for i in keep..keep + 5 {
        assert!(!gates.contains(&format!("{}.json", fp(i))), "gate {i}");
    }
    assert_eq!(list(&ports, "logs").len(), 1200, "logs and all");
    assert_eq!(list(&ports, "verdicts").len(), 300, "verdicts by count");
}

#[test]
fn a_kept_gate_result_keeps_the_logs_it_cites() {
    let red = fp(0xbad);
    let mut files = FakeFiles::new(Posix);
    files = put(
        files,
        &format!("gate/{red}.json"),
        DAY / 5,
        r#"{"ok":false}"#,
    );
    files = put(files, &format!("logs/{red}-clippy.log"), DAY / 5, "x");
    files = put(files, &format!("logs/{red}-test.log"), DAY / 5, "x");
    for i in 0..40 {
        files = put(files, &format!("gate/{}.json", fp(i)), nth(i * 1000), "{}");
        for step in ["clippy", "test", "doctest", "doc", "rustdoc"] {
            let log = format!("logs/{}-{step}.log", fp(i));
            files = put(files, &log, nth(i * 1000), "x");
        }
    }

    let ports = prune(files, None);

    assert_eq!(list(&ports, "gate").len(), 41, "every result stays");
    assert!(
        has(&ports, &format!("logs/{red}-clippy.log"))
            && has(&ports, &format!("logs/{red}-test.log")),
        "the red result still points at its logs"
    );
}

#[test]
fn a_gate_run_in_progress_within_the_grace_period_is_not_touched() {
    let keep = GATE.keep;
    let running = fp(0xfeed);
    let old = fp(0x01_d000);
    let mut files = FakeFiles::new(Posix);
    for (rel, age) in [
        (format!("logs/{running}-clippy.log"), 30 * MINUTE),
        (format!("logs/{running}-test.log"), 30 * MINUTE),
        (format!("verdicts/{running}.json"), 30 * MINUTE),
        (format!("gate/{old}.json"), HOURS2),
        (format!("logs/{old}-test.log"), HOURS2),
    ] {
        files = put(files, &rel, age, "{}");
    }
    for i in 0..keep {
        let age = 10 * MINUTE + nth(i);
        files = put(files, &format!("gate/{}.json", fp(i)), age, "{}");
        files = put(files, &format!("logs/{}-test.log", fp(i)), age, "x");
    }
    for i in 0..VERDICTS.keep {
        let age = 10 * MINUTE + nth(i);
        files = put(files, &format!("verdicts/{}.json", fp(i)), age, "{}");
    }

    let ports = prune(files, None);

    assert!(
        has(&ports, &format!("logs/{running}-clippy.log"))
            && has(&ports, &format!("logs/{running}-test.log")),
        "the running gate keeps its logs"
    );
    assert!(
        has(&ports, &format!("verdicts/{running}.json")),
        "a fresh verdict over the count"
    );
    assert!(
        !has(&ports, &format!("gate/{old}.json")) && !has(&ports, &format!("logs/{old}-test.log")),
        "a run past the grace period still goes"
    );
    assert_eq!(list(&ports, "gate").len(), 300, "the count");
}

#[test]
fn past_the_byte_budget_the_oldest_gate_runs_go_whole() {
    let mut files = FakeFiles::new(Posix);
    for i in [1, 2, 3] {
        let age = i64::from(4 - i) * DAY;
        files = put(files, &format!("gate/{}.json", fp(i)), age, "{}");
        let log = format!("logs/{}-test.log", fp(i));
        files = put(files, &log, age, "x").with_len(&format!("{STATE}/{log}"), 30 << 20);
    }

    let ports = prune(files, None);

    assert_eq!(
        list(&ports, "gate"),
        [format!("{}.json", fp(2)), format!("{}.json", fp(3))],
        "the oldest result went"
    );
    assert_eq!(
        list(&ports, "logs"),
        [format!("{}-test.log", fp(2)), format!("{}-test.log", fp(3))],
        "with its log"
    );
}

#[test]
fn the_newest_gate_run_stays_even_alone_over_the_byte_budget() {
    let log = format!("logs/{}-test.log", fp(1));
    let files = put(
        FakeFiles::new(Posix),
        &format!("gate/{}.json", fp(1)),
        0,
        "{}",
    );
    let files = put(files, &log, 0, "x").with_len(&format!("{STATE}/{log}"), (64 << 20) + 1);

    let ports = prune(files, None);

    assert_eq!(list(&ports, "gate"), [format!("{}.json", fp(1))], "kept");
    assert_eq!(
        list(&ports, "logs"),
        [format!("{}-test.log", fp(1))],
        "with its log"
    );
}

#[test]
fn the_records_of_the_current_fingerprint_stay_and_count_toward_no_cap() {
    let cur = fp(0xabc);
    let mut files = FakeFiles::new(Posix);
    for rel in [
        format!("gate/{cur}.json"),
        format!("verdicts/{cur}.json"),
        format!("logs/{cur}-clippy.log"),
        format!("logs/{cur}-test.log"),
        format!("verdicts/{}.json", fp(1)),
    ] {
        files = put(files, &rel, 400 * DAY, "{}");
    }

    let ports = prune(files, Some(&cur));

    assert_eq!(list(&ports, "gate"), [format!("{cur}.json")], "gate");
    assert_eq!(
        list(&ports, "verdicts"),
        [format!("{cur}.json")],
        "verdicts"
    );
    assert_eq!(
        list(&ports, "logs"),
        [format!("{cur}-clippy.log"), format!("{cur}-test.log")],
        "logs"
    );
}

#[test]
fn sessions_the_findings_cache_telemetry_and_a_missing_state_dir_are_left_alone() {
    let mut files = FakeFiles::new(Posix);
    for (rel, text) in [
        ("sessions/s-old/base", "abc"),
        ("findings-0000000000000001.json", "{}"),
        ("telemetry.jsonl", "{\"event\":\"x\"}\n"),
        ("telemetry.1.jsonl", "{\"event\":\"y\"}\n"),
    ] {
        files = put(files, rel, 400 * DAY, text);
    }

    let ports = prune(files, None);
    let missing = Workspace {
        state_dir: "/s/missing".to_owned(),
        ..workspace()
    };
    prune_fingerprint_state(&ports, Posix, &missing, None);

    for rel in [
        "sessions/s-old/base",
        "findings-0000000000000001.json",
        "telemetry.jsonl",
        "telemetry.1.jsonl",
    ] {
        assert!(has(&ports, rel), "{rel} stays");
    }
}

#[test]
fn gate_progress_files_over_a_day_old_go_and_younger_ones_stay() {
    let mut files = FakeFiles::new(Posix);
    for (rel, age_ms) in [
        ("gate-progress-aaaaaaaaaaaaaaaa.json", DAY + 1),
        ("gate-progress-bbbbbbbbbbbbbbbb.json", DAY - MINUTE),
        ("gate-progress-cccc.json.bak", 2 * DAY),
        ("gate-progress-XYZ.json", 2 * DAY),
    ] {
        files = put(files, rel, age_ms, "{}");
    }

    let ports = prune(files, None);

    assert_eq!(
        [
            has(&ports, "gate-progress-aaaaaaaaaaaaaaaa.json"),
            has(&ports, "gate-progress-bbbbbbbbbbbbbbbb.json"),
            has(&ports, "gate-progress-cccc.json.bak"),
            has(&ports, "gate-progress-XYZ.json"),
        ],
        [false, true, true, true],
        "only /^gate-progress-[0-9a-f]+\\.json$/ over a day old"
    );
}

#[test]
fn every_removal_error_is_ignored() {
    let files = put(
        FakeFiles::new(Posix),
        &format!("gate/{}.json", fp(1)),
        31 * DAY,
        "{}",
    );
    let files = put(files, &format!("gate/{}.json", fp(2)), 31 * DAY, "{}")
        .with_read_only(&format!("{STATE}/gate/{}.json", fp(1)));

    let ports = prune(files, None);

    assert_eq!(
        list(&ports, "gate"),
        [format!("{}.json", fp(1))],
        "the next file still went"
    );
}

#[test]
fn sessions_and_markers_go_by_directory_mtime() {
    let files = FakeFiles::new(Posix)
        .with_file("/s/sessions/old/base", "a")
        .with_mtime("/s/sessions/old", mtime(14 * DAY + 1))
        .with_file("/s/sessions/new/base", "b")
        .with_mtime("/s/sessions/new", mtime(14 * DAY - MINUTE))
        .with_file("/data/sessions/old/skill-main", "t")
        .with_mtime("/data/sessions/old", mtime(7 * DAY + 1))
        .with_file("/data/sessions/new/skill-main", "t")
        .with_mtime("/data/sessions/new", mtime(7 * DAY - MINUTE));
    let ports = FakePorts::new(files);
    let env = RunEnv {
        plugin_data: Some("/data".into()),
        ..RunEnv::at("/w", Posix)
    };

    prune_sessions(&ports, Posix, &workspace());
    prune_markers(&ports, &env);

    assert_eq!(
        ports.files.paths(),
        ["/data/sessions/new/skill-main", "/s/sessions/new/base"],
        "14 days for sessions, 7 for markers"
    );
}

#[test]
fn markers_without_plugin_data_live_under_the_temporary_directory() {
    let files = FakeFiles::new(Posix)
        .with_file("/tmp/temper-plugin-data/sessions/old/skill-main", "t")
        .with_mtime("/tmp/temper-plugin-data/sessions/old", mtime(8 * DAY));
    let ports = FakePorts::new(files);

    prune_markers(&ports, &RunEnv::at("/w", Posix));

    assert!(ports.files.paths().is_empty(), "pruned");
}
