use temper_domain::Config;
use temper_domain::PathStyle::Posix;

use super::{Force, set_base, start_fingerprint};
use crate::changes::fingerprint;
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, NOW, Reply};
use crate::ports::{ExitKind, FsOp, Ports};
use crate::project::Workspace;

const HEAD: &str = "1111111111111111111111111111111111111111";
const SESSION: &str = "/w/.git/temper/sessions/s1";

fn workspace() -> Workspace {
    Workspace {
        root: "/w".to_owned(),
        state_dir: "/w/.git/temper".to_owned(),
        config: Config::defaults(),
        config_problem: None,
        git: true,
    }
}

fn ran(stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    }
}

fn failed() -> Reply {
    Reply::Exit {
        status: ExitKind::Code(128),
        stdout: String::new(),
    }
}

/// A repository at `/w` whose HEAD is `head`, or has none; every other git call fails.
fn repo(files: FakeFiles, head: Option<&str>) -> FakePorts {
    let rev_parse = head.map_or_else(failed, |sha| ran(&format!("{sha}\n")));
    FakePorts {
        process: FakeProcess::new()
            .answering(&["git", "rev-parse", "HEAD"], "/w", rev_parse)
            .otherwise(failed()),
        ..FakePorts::new(files)
    }
}

fn file(ports: &FakePorts, name: &str) -> Option<String> {
    ports.files.text(&format!("{SESSION}/{name}"))
}

#[test]
fn if_absent_keeps_an_existing_base_and_writes_nothing() {
    let files = FakeFiles::new(Posix).with_file(&format!("{SESSION}/base"), "abc");
    let ports = repo(files, Some(HEAD));
    let env = RunEnv::at("/w", Posix);

    set_base(&ports, &env, &workspace(), "s1", Force::IfAbsent).expect("nothing to write");

    assert_eq!(file(&ports, "base").as_deref(), Some("abc"), "kept");
    assert_eq!(file(&ports, "started"), None, "no start time");
    assert_eq!(file(&ports, "start-fp"), None, "no start fingerprint");
    assert_eq!(ports.process.calls(), 0, "not even HEAD is asked");
}

#[test]
fn always_rewrites_the_base_the_start_time_and_the_start_fingerprint() {
    let files = FakeFiles::new(Posix)
        .with_file(&format!("{SESSION}/base"), "abc")
        .with_file(&format!("{SESSION}/started"), "1")
        .with_file(&format!("{SESSION}/start-fp"), "x");
    let ports = repo(files, Some(HEAD));
    let env = RunEnv::at("/w", Posix);
    let ws = workspace();

    set_base(&ports, &env, &ws, "s1", Force::Always).expect("written");

    let expected = fingerprint(&ports, &env, &ws, Some(HEAD)).to_string();
    let started = NOW.unix_millis().to_string();
    let written = [
        file(&ports, "base"),
        file(&ports, "started"),
        file(&ports, "start-fp"),
    ];
    assert_eq!(
        written.each_ref().map(Option::as_deref),
        [Some(HEAD), Some(started.as_str()), Some(expected.as_str())],
        "1.x writes HEAD, String(Date.now()) and fingerprint(proj, head)"
    );
    assert_eq!(
        start_fingerprint(ports.files(), Posix, &ws, "s1").as_deref(),
        Some(expected.as_str()),
        "read back trimmed"
    );
}

#[test]
fn if_absent_writes_a_session_that_has_no_base_yet() {
    let ports = repo(FakeFiles::new(Posix), Some(HEAD));
    let env = RunEnv::at("/w", Posix);

    set_base(&ports, &env, &workspace(), "s1", Force::IfAbsent).expect("written");

    assert_eq!(file(&ports, "base").as_deref(), Some(HEAD), "the base is HEAD");
}

#[test]
fn a_missing_head_writes_an_empty_base() {
    let ports = repo(FakeFiles::new(Posix), None);
    let env = RunEnv::at("/w", Posix);
    let ws = workspace();

    set_base(&ports, &env, &ws, "s1", Force::Always).expect("written");

    assert_eq!(file(&ports, "base").as_deref(), Some(""), "`head ?? ''`");
    assert_eq!(
        file(&ports, "start-fp"),
        Some(fingerprint(&ports, &env, &ws, None).to_string()),
        "fingerprint(proj, null)"
    );
}

#[test]
fn a_write_failure_is_an_error_with_nothing_after_it_written() {
    let files = FakeFiles::new(Posix).with_read_only(&format!("{SESSION}/started"));
    let ports = repo(files, Some(HEAD));
    let env = RunEnv::at("/w", Posix);

    let error =
        set_base(&ports, &env, &workspace(), "s1", Force::Always).expect_err("started fails");

    assert_eq!(error.op(), FsOp::Write, "1.x's writeAtomic throws");
    assert_eq!(file(&ports, "base").as_deref(), Some(HEAD), "written before");
    assert_eq!(file(&ports, "start-fp"), None, "never reached");
}
