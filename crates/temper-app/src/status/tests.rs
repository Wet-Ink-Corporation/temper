use temper_domain::PathStyle::Posix;
use temper_domain::{NOT_A_RUST_PROJECT, Scanner};

use crate::engine::{CommandError, Engine, Exit, OutputFormat, RunEnv, StatusArgs};
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::ports::ExitKind;

mod json;

const HEAD: &str = "1111111111111111111111111111111111111111";
const OTHER: &str = "3333333333333333333333333333333333333333";

/// `temper status`, no session.
const TEXT: StatusArgs = StatusArgs {
    format: OutputFormat::Text,
    session: None,
};

fn ran(stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    }
}

fn failed() -> Reply {
    Reply::Exit {
        status: ExitKind::Code(1),
        stdout: String::new(),
    }
}

fn engine(ports: FakePorts, temper_base: Option<&str>) -> Engine<FakePorts> {
    let env = RunEnv {
        temper_base: temper_base.map(str::to_owned),
        ..RunEnv::at("/w", Posix)
    };
    Engine::new(ports, env, Scanner::new().expect("the scanner builds"))
}

fn text(engine: &Engine<FakePorts>, session: Option<&str>) -> (String, String) {
    let out = engine
        .status(&StatusArgs {
            format: OutputFormat::Text,
            session: session.map(str::to_owned),
        })
        .expect("status answers");
    assert_eq!(out.exit, Exit::Ok, "exit 0");
    (out.stdout, out.stderr)
}

/// A repository at `/w` whose HEAD is [`HEAD`], where `rev` resolves to `sha` too.
fn repo(files: FakeFiles, resolves: &[(&str, &str)]) -> FakePorts {
    let mut process = FakeProcess::new()
        .answering(&["git", "rev-parse", "--show-toplevel"], "/w", ran("/w\n"))
        .answering(
            &["git", "rev-parse", "--git-common-dir"],
            "/w",
            ran(".git\n"),
        )
        .answering(
            &["git", "rev-parse", "HEAD"],
            "/w",
            ran(&format!("{HEAD}\n")),
        )
        .otherwise(failed());
    for &(rev, sha) in resolves {
        let commit = format!("{rev}^{{commit}}");
        process = process.answering(
            &["git", "rev-parse", "--verify", "--quiet", &commit],
            "/w",
            ran(&format!("{sha}\n")),
        );
    }
    FakePorts {
        process,
        ..FakePorts::new(files.with_file("/w/Cargo.toml", "[workspace]"))
    }
}

fn line<'t>(text: &'t str, key: &str) -> &'t str {
    text.lines()
        .find(|l| l.starts_with(key))
        .unwrap_or_else(|| panic!("no {key} line in {text}"))
}

#[test]
fn outside_a_rust_project_status_refuses_with_exit_2() {
    let engine = engine(FakePorts::new(FakeFiles::new(Posix)), None);

    let refused = engine.status(&TEXT);

    assert!(
        matches!(&refused, Err(CommandError::Refused { exit: Exit::Usage, message }) if message == NOT_A_RUST_PROJECT),
        "{refused:?}"
    );
}

/// Item 18: a `TEMPER_BASE` that resolves is the base, as given.
#[test]
fn a_temper_base_that_resolves_is_the_base_as_given() {
    let engine = engine(
        repo(FakeFiles::new(Posix), &[("HEAD~1", OTHER), ("HEAD", HEAD)]),
        Some("HEAD~1"),
    );

    let (stdout, stderr) = text(&engine, None);

    assert_eq!(line(&stdout, "base:"), "base:        HEAD~1");
    assert_eq!(stderr, "");
}

/// One that does not gives HEAD, and the note once per process however often it is read.
#[test]
fn a_temper_base_that_does_not_resolve_gives_head_and_one_note() {
    let engine = engine(
        repo(FakeFiles::new(Posix), &[("HEAD", HEAD)]),
        Some("garbage-base-xyz"),
    );

    let first = text(&engine, None);
    let second = text(&engine, None);

    assert_eq!(line(&first.0, "base:"), format!("base:        {HEAD}"));
    assert_eq!(
        [first.1.as_str(), second.1.as_str()],
        [
            "temper: TEMPER_BASE garbage-base is not a commit in this repository (another session's base?); using HEAD.\n",
            ""
        ]
    );
}

/// `--session ID`: the session's recorded base when it resolves (its directory name sanitised
/// per UTF-16 unit), else HEAD.
#[test]
fn a_session_base_is_read_from_the_sessions_directory() {
    let files = FakeFiles::new(Posix)
        .with_file("/w/.git/temper/sessions/a__b/base", &format!(" {OTHER}\n"))
        .with_file("/w/.git/temper/sessions/gone/base", "deadbeef");
    let engine = engine(repo(files, &[(OTHER, OTHER), ("HEAD", HEAD)]), None);

    let recorded = text(&engine, Some("a\u{1F600}b")).0;
    let gone = text(&engine, Some("gone")).0;
    let none = text(&engine, Some("never")).0;

    assert_eq!(line(&recorded, "base:"), format!("base:        {OTHER}"));
    assert_eq!(line(&gone, "base:"), format!("base:        {HEAD}"));
    assert_eq!(line(&none, "base:"), format!("base:        {HEAD}"));
}

/// F6: malformed gate, verdict and baseline files read as none or as 1.x's `unreadable:` line;
/// `status` still answers.
#[test]
fn malformed_records_read_as_none_and_status_still_answers() {
    let files = FakeFiles::new(Posix)
        .with_unreadable("/w/.claude/temper-baseline.json")
        .with_file("/w/.claude/temper.json", "[]");
    let ports = repo(files, &[("HEAD", HEAD)]);
    let engine = engine(ports, None);
    let (stdout, _) = text(&engine, None);
    let fp = line(&stdout, "fingerprint:")
        .trim_start_matches("fingerprint: ")
        .to_owned();
    let gate = format!("/w/.git/temper/gate/{fp}.json");
    let verdict = format!("/w/.git/temper/verdicts/{fp}.json");
    let files = FakeFiles::new(Posix)
        .with_file(&gate, "{not json")
        .with_file(&verdict, "")
        .with_file("/w/.claude/temper-baseline.json", "")
        .with_file("/w/.claude/temper.json", "[]");
    let engine = self::engine(repo(files, &[("HEAD", HEAD)]), None);

    let (stdout, stderr) = text(&engine, None);

    assert_eq!(
        stdout.lines().skip(5).collect::<Vec<_>>(),
        [
            "gate:        not run for this fingerprint",
            "review:      none for this fingerprint",
            "baseline:    unreadable: .claude/temper-baseline.json is not valid JSON (EOF while parsing a value at line 1 column 0)",
            "config:      review=required skillGate=true; config unreadable (not a JSON object): defaults in use",
        ]
    );
    assert_eq!(stderr, "");
}

/// A verdict whose `verdict` has its own `toString` key throws in 1.x's template literal,
/// outside any `try`: status fails (exit 1) instead of answering. The same key in the
/// baseline's toolchain is caught, and reads as unreadable.
#[test]
fn a_verdict_1x_cannot_convert_fails_status_and_a_toolchain_reads_as_unreadable() {
    let fp_of = |files: FakeFiles| {
        let (stdout, _) = text(&engine(repo(files, &[("HEAD", HEAD)]), None), None);
        line(&stdout, "fingerprint:")
            .trim_start_matches("fingerprint: ")
            .to_owned()
    };
    let baseline = r#"{"temper_baseline":1,"totals":{},"toolchain":{"toString":"x"}}"#;
    let fp = fp_of(FakeFiles::new(Posix).with_file("/w/.claude/temper-baseline.json", baseline));
    let verdict = format!("/w/.git/temper/verdicts/{fp}.json");
    let with = |json: &str| {
        FakeFiles::new(Posix)
            .with_file("/w/.claude/temper-baseline.json", baseline)
            .with_file(&verdict, json)
    };

    let thrown = engine(
        repo(with(r#"{"verdict":{"toString":1}}"#), &[("HEAD", HEAD)]),
        None,
    )
    .status(&TEXT);
    let (stdout, _) = text(
        &engine(
            repo(with(r#"{"verdict":"approved"}"#), &[("HEAD", HEAD)]),
            None,
        ),
        None,
    );

    assert!(
        matches!(thrown, Err(CommandError::Unconvertible(_))),
        "{thrown:?}"
    );
    assert_eq!(
        line(&stdout, "baseline:"),
        "baseline:    unreadable: Cannot convert object to primitive value"
    );
    assert_eq!(line(&stdout, "review:"), "review:      approved");
}
