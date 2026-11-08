use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use temper_domain::PathStyle::Posix;

use super::{GitMemo, MemoPorts};
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::ports::{
    ByteCap, ExitKind, Files, Ports, ProcessError, ProcessOutput, ProcessRequest, ProcessRunner,
    StderrSink, StdoutSink,
};

const CAP: ByteCap = ByteCap::new(1 << 20);

fn exit(code: i32, stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(code),
        stdout: stdout.to_owned(),
    }
}

fn output(code: i32, stdout: &str) -> ProcessOutput {
    ProcessOutput {
        status: ExitKind::Code(code),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

/// `command` (the program, then its arguments) in `cwd`, its stdout captured.
fn run(
    process: &impl ProcessRunner,
    command: &[&str],
    cwd: &str,
) -> Result<ProcessOutput, ProcessError> {
    let (program, args) = command.split_first().expect("a program");
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    process.run(ProcessRequest {
        program,
        args: &args,
        cwd: Path::new(cwd),
        stdout: StdoutSink::Capture(CAP),
        stderr: StderrSink::Capture(CAP),
        timeout: Duration::from_secs(5),
    })
}

fn ports(process: FakeProcess) -> FakePorts {
    FakePorts {
        process,
        ..FakePorts::new(FakeFiles::new(Posix))
    }
}

const HEAD: &[&str] = &["git", "rev-parse", "--verify", "--quiet", "HEAD^{commit}"];
const BRANCH: &[&str] = &["git", "symbolic-ref", "--short", "-q", "HEAD"];
const UNTRACKED: &[&str] = &["git", "ls-files", "--others", "--exclude-standard", "-z"];
const FMT: &[&str] = &["cargo", "fmt", "--all"];

#[test]
fn a_repeated_ref_query_runs_once_and_gives_the_first_answer_again() {
    let fake = ports(
        FakeProcess::new()
            .answering(HEAD, "/r", exit(0, "abc\n"))
            .answering(BRANCH, "/r", exit(1, "")),
    );
    let memo = GitMemo::default();
    let ports = MemoPorts::new(&fake, &memo);

    let answers = [HEAD, HEAD, BRANCH, BRANCH].map(|c| run(ports.process(), c, "/r").ok());

    assert_eq!(
        answers,
        [
            Some(output(0, "abc\n")),
            Some(output(0, "abc\n")),
            Some(output(1, "")),
            Some(output(1, "")),
        ],
        "a failing exit is an answer too"
    );
    assert_eq!(fake.process.calls(), 2, "one run per question");
}

#[test]
fn answers_are_kept_per_directory_and_arguments() {
    let fake = ports(FakeProcess::new().otherwise(exit(0, "x")));
    let memo = GitMemo::default();
    let ports = MemoPorts::new(&fake, &memo);

    for (command, cwd) in [(HEAD, "/r"), (HEAD, "/w"), (BRANCH, "/r"), (HEAD, "/w")] {
        assert_eq!(
            run(ports.process(), command, cwd).ok(),
            Some(output(0, "x"))
        );
    }

    assert_eq!(
        fake.process.calls(),
        3,
        "the fourth question was the second's"
    );
}

#[test]
fn a_work_tree_answer_holds_until_another_program_runs_and_a_ref_answer_past_it() {
    let fake = ports(
        FakeProcess::new()
            .answering(UNTRACKED, "/r", exit(0, "a.rs\0"))
            .answering(HEAD, "/r", exit(0, "abc\n"))
            .answering(FMT, "/r", exit(0, "")),
    );
    let memo = GitMemo::default();
    let ports = MemoPorts::new(&fake, &memo);
    let process = ports.process();

    let first = [UNTRACKED, HEAD, UNTRACKED].map(|c| run(process, c, "/r").ok());
    let calls_before = fake.process.calls();
    let rest = [FMT, UNTRACKED, HEAD, FMT].map(|c| run(process, c, "/r").ok());

    assert_eq!(calls_before, 2, "ls-files and rev-parse, once each");
    assert_eq!(
        (first, rest),
        (
            [
                Some(output(0, "a.rs\0")),
                Some(output(0, "abc\n")),
                Some(output(0, "a.rs\0")),
            ],
            [
                Some(output(0, "")),
                Some(output(0, "a.rs\0")),
                Some(output(0, "abc\n")),
                Some(output(0, "")),
            ]
        )
    );
    assert_eq!(
        fake.process.calls(),
        5,
        "after cargo: ls-files again, rev-parse kept, cargo itself never kept"
    );
}

#[test]
fn a_file_write_drops_the_work_tree_answers_only() {
    let fake = ports(
        FakeProcess::new()
            .answering(UNTRACKED, "/r", exit(0, "a.rs\0"))
            .answering(HEAD, "/r", exit(0, "abc\n")),
    );
    let memo = GitMemo::default();
    let ports = MemoPorts::new(&fake, &memo);

    let first = [UNTRACKED, HEAD].map(|c| run(ports.process(), c, "/r").ok());
    let wrote = ports
        .files()
        .write_atomic(Path::new("/r/.git/temper/x.json"), "{}");
    let second = [UNTRACKED, HEAD].map(|c| run(ports.process(), c, "/r").ok());

    assert_eq!(wrote.ok(), Some(()), "the write went through");
    assert_eq!(first, second, "the same answers");
    assert_eq!(
        fake.process.calls(),
        3,
        "ls-files ran again after the write"
    );
    assert_eq!(
        fake.files.text("/r/.git/temper/x.json").as_deref(),
        Some("{}")
    );
}

/// #210 review nit: `show :path` and `rev-parse :path` read the index, which a run can change,
/// so they hold only as long as a work-tree answer.
#[test]
fn a_ref_question_that_names_the_index_holds_only_until_a_write() {
    const STAGED: &[&str] = &["git", "show", ":src/lib.rs"];
    const STAGE_0: &[&str] = &["git", "rev-parse", ":0:src/lib.rs"];
    let fake = ports(FakeProcess::new().otherwise(exit(0, "x")));
    let memo = GitMemo::default();
    let ports = MemoPorts::new(&fake, &memo);

    let first = [STAGED, STAGE_0].map(|c| run(ports.process(), c, "/r").ok());
    let wrote = ports
        .files()
        .write_atomic(Path::new("/r/.git/temper/x.json"), "{}");
    let second = [STAGED, STAGE_0].map(|c| run(ports.process(), c, "/r").ok());

    assert_eq!(wrote.ok(), Some(()), "the write went through");
    assert_eq!(first, second, "the same answers");
    assert_eq!(fake.process.calls(), 4, "both ran again after the write");
}

#[test]
fn a_run_that_failed_or_timed_out_is_never_replayed() {
    let fake = ports(
        FakeProcess::new()
            .answering(
                HEAD,
                "/r",
                Reply::Exit {
                    status: ExitKind::TimedOut,
                    stdout: String::new(),
                },
            )
            .otherwise(Reply::SpawnFails),
    );
    let memo = GitMemo::default();
    let ports = MemoPorts::new(&fake, &memo);

    let answers = [HEAD, HEAD, BRANCH, BRANCH].map(|c| run(ports.process(), c, "/r"));

    assert!(
        matches!(
            answers,
            [
                Ok(ProcessOutput {
                    status: ExitKind::TimedOut,
                    ..
                }),
                Ok(ProcessOutput {
                    status: ExitKind::TimedOut,
                    ..
                }),
                Err(ProcessError::Spawn { .. }),
                Err(ProcessError::Spawn { .. }),
            ]
        ),
        "{answers:?}"
    );
    assert_eq!(fake.process.calls(), 4, "each ran");
}

/// #210 review nit: only an exit code is an answer; a git a signal ended is asked again.
#[test]
fn a_run_a_signal_ended_is_never_replayed() {
    let fake = ports(FakeProcess::new().otherwise(Reply::Exit {
        status: ExitKind::Signal,
        stdout: String::new(),
    }));
    let memo = GitMemo::default();
    let ports = MemoPorts::new(&fake, &memo);

    let answers = [HEAD, HEAD].map(|c| run(ports.process(), c, "/r"));

    assert!(
        matches!(
            answers,
            [
                Ok(ProcessOutput {
                    status: ExitKind::Signal,
                    ..
                }),
                Ok(ProcessOutput {
                    status: ExitKind::Signal,
                    ..
                }),
            ]
        ),
        "{answers:?}"
    );
    assert_eq!(fake.process.calls(), 2, "each ran");
}

#[test]
fn a_streamed_git_call_always_runs() {
    let fake = ports(FakeProcess::new().otherwise(exit(0, "diff")));
    let memo = GitMemo::default();
    let ports = MemoPorts::new(&fake, &memo);
    let args = [OsString::from("diff")];
    let mut sink = Vec::new();

    for _ in 0..2 {
        let streamed = ports.process().run(ProcessRequest {
            program: "git",
            args: &args,
            cwd: Path::new("/r"),
            stdout: StdoutSink::Stream(&mut sink),
            stderr: StderrSink::Capture(CAP),
            timeout: Duration::from_secs(5),
        });
        assert_eq!(streamed.ok().map(|o| o.status), Some(ExitKind::Code(0)));
    }

    assert_eq!(sink, b"diffdiff", "both runs streamed");
    assert_eq!(fake.process.calls(), 2);
}
