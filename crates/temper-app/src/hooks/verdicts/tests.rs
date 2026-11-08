//! `check_verdict` and `store_verdict` through the two hooks that use them, on fakes: git's
//! questions about refs and objects are asked once per run (3f-17, as 3e-16 for
//! `subagent-start`).

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;

use temper_domain::PathStyle::Posix;
use temper_domain::{HookEvent, HookOutcome, Scanner};

use crate::engine::{HookError, RunEnv};
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::hooks::{HookCall, HookInput};
use crate::ports::ExitKind;

const HEAD: &str = "1111111111111111111111111111111111111111";

/// The git subcommands that read only refs and objects.
const LASTING: [&str; 5] = [
    "rev-parse",
    "symbolic-ref",
    "merge-base",
    "rev-list",
    "show",
];

fn ran(stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    }
}

/// A one-crate repository at `/r` with HEAD at [`HEAD`] and an untracked `src/lib.rs`; every
/// other git question exits 0 with nothing to say.
fn repository() -> FakePorts {
    let git = |args: &[&'static str]| [&["git"][..], args].concat();
    let head_commit = format!("{HEAD}^{{commit}}");
    let process = FakeProcess::new()
        .answering(&git(&["rev-parse", "--show-toplevel"]), "/r", ran("/r\n"))
        .answering(
            &git(&["rev-parse", "--git-common-dir"]),
            "/r",
            ran(".git\n"),
        )
        .answering(
            &git(&["rev-parse", "--git-dir", "--git-common-dir"]),
            "/r",
            ran(".git\n.git\n"),
        )
        .answering(
            &git(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]),
            "/r",
            ran(&format!("{HEAD}\n")),
        )
        .answering(
            &["git", "rev-parse", "--verify", "--quiet", &head_commit],
            "/r",
            ran(&format!("{HEAD}\n")),
        )
        .answering(
            &git(&["ls-files", "--others", "--exclude-standard", "-z"]),
            "/r",
            ran("src/lib.rs\0"),
        )
        .otherwise(ran(""));
    let files = FakeFiles::new(Posix)
        .with_file("/r/Cargo.toml", "[package]\nname = \"r\"\n")
        .with_file("/r/src/lib.rs", "//! R.\n");
    FakePorts {
        process,
        ..FakePorts::new(files)
    }
}

/// A reviewer's approval of `0123456789abcdef` that read `src/lib.rs`, fenced as a report.
const REPORT: &str = concat!(
    "Review done.\\n\\n```json\\n",
    r#"{\"temper_verdict\":1,\"verdict\":\"approved\",\"fingerprint\":\"0123456789abcdef\","#,
    r#"\"findings\":[],\"scores\":{\"correctness\":4,\"escape_hatches\":4,\"ownership\":4,"#,
    r#"\"errors\":4,\"api\":4,\"architecture\":4,\"tests\":4,\"performance\":4,"#,
    r#"\"concurrency\":4,\"observability\":4,\"craft\":4,\"docs\":4},"#,
    r#"\"gate\":{\"fmt\":\"pass\",\"clippy\":\"pass\",\"test\":\"pass\",\"doc\":\"pass\"},"#,
    r#"\"evidence\":{\"files_read\":[\"src/lib.rs\"],\"commands_run\":[\"temper gate\"]}}"#,
    "\\n```\\n"
);

/// `event` for reviewer `rv1` in `/r` with `more` input properties; the outer result fails
/// only when the test's own input or patterns do.
fn reviewer_runs(
    ports: &FakePorts,
    event: HookEvent,
    more: &str,
) -> Result<Result<HookOutcome, HookError>, Box<dyn std::error::Error>> {
    let env = RunEnv::at("/r", Posix);
    let input = HookInput::parse(&format!(
        r#"{{"session_id":"s1","cwd":"/r","agent_type":"temper:rust-reviewer","agent_id":"rv1"{more}}}"#
    ))?;
    let call = HookCall {
        ports,
        env: &env,
        input: &input,
    };
    let scanner = Scanner::new()?;
    Ok(crate::hooks::run(&call, &scanner, event))
}

/// The ref and object questions `ports` was asked more than once (same arguments and directory).
fn asked_again(ports: &FakePorts) -> Vec<(String, Vec<OsString>, PathBuf)> {
    let requests = ports.process.requests();
    let mut seen: HashSet<(Vec<OsString>, PathBuf)> = HashSet::new();
    requests
        .into_iter()
        .filter(|(program, args, _)| {
            program == "git"
                && args
                    .first()
                    .is_some_and(|sub| LASTING.iter().any(|l| sub == l))
        })
        // The set keeps its own copy of each key: the request itself is what is collected.
        .filter(|(_, args, cwd)| !seen.insert((args.clone(), cwd.clone())))
        .collect()
}

#[test]
fn a_hand_back_recorded_asks_git_each_ref_question_once() {
    let ports = repository();
    let more = format!(r#","tool_name":"SubagentHandback","tool_input":{{"message":"{REPORT}"}}"#);

    let outcome = reviewer_runs(&ports, HookEvent::PreHandback, &more).expect("the test's input");

    assert!(matches!(outcome, Ok(HookOutcome::Pass)), "{outcome:?}");
    assert!(
        ports
            .files
            .text("/r/.git/temper/verdicts/0123456789abcdef.json")
            .is_some(),
        "the verdict is recorded"
    );
    assert_eq!(asked_again(&ports), Vec::new(), "asked again");
}

#[test]
fn a_stop_recorded_asks_git_each_ref_question_once() {
    let ports = repository();
    let more = format!(r#","last_assistant_message":"{REPORT}""#);

    let outcome = reviewer_runs(&ports, HookEvent::SubagentStop, &more).expect("the test's input");

    assert!(matches!(outcome, Ok(HookOutcome::Pass)), "{outcome:?}");
    assert!(
        ports
            .files
            .text("/r/.git/temper/verdicts/0123456789abcdef.json")
            .is_some(),
        "the verdict is recorded"
    );
    assert_eq!(asked_again(&ports), Vec::new(), "asked again");
}
