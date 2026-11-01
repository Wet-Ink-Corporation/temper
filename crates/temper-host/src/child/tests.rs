use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use temper_app::{
    ByteCap, ExitKind, ProcessError, ProcessOutput, ProcessRequest, ProcessRunner, Stream,
};

use super::{Captured, StdProcess, read_capped};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Enough for any output these tests see.
const ROOMY: ByteCap = ByteCap::new(1 << 20);

/// Long enough that only a hung child reaches it.
const PATIENT: Duration = Duration::from_secs(60);

/// What one request asks for, beyond the program and its arguments.
#[derive(Clone, Copy)]
struct Limits {
    stdout_cap: ByteCap,
    stderr_cap: ByteCap,
    timeout: Duration,
}

const GENEROUS: Limits = Limits {
    stdout_cap: ROOMY,
    stderr_cap: ROOMY,
    timeout: PATIENT,
};

fn run(command: &[&str], limits: Limits) -> Result<ProcessOutput, ProcessError> {
    let (program, args) = command.split_first().expect("a program");
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    StdProcess.run(&ProcessRequest {
        program,
        args: &args,
        cwd: Path::new(env!("CARGO_MANIFEST_DIR")),
        stdout_cap: limits.stdout_cap,
        stderr_cap: limits.stderr_cap,
        timeout: limits.timeout,
    })
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn captures_stdout_and_exit_code() -> TestResult {
    let output = run(&["git", "--version"], GENEROUS)?;

    assert_eq!(output.status, ExitKind::Code(0), "git --version succeeds");
    assert!(
        output.stdout.starts_with(b"git version "),
        "{:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(output.stderr, b"", "nothing on stderr");
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn reports_a_failing_exit_code_and_its_stderr() -> TestResult {
    let output = run(&["git", "--no-such-flag"], GENEROUS)?;

    assert_eq!(output.status, ExitKind::Code(129), "git's usage error");
    // The option's name is the part of the message no locale translates.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--no-such-flag"), "{stderr:?}");
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn a_missing_program_is_a_spawn_error() {
    let result = run(&["temper-test-no-such-program"], GENEROUS);

    assert!(
        matches!(
            &result,
            Err(ProcessError::Spawn { program, .. }) if program == "temper-test-no-such-program"
        ),
        "{result:?}"
    );
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn stdin_is_null() -> TestResult {
    let output = run(&["git", "hash-object", "--stdin"], GENEROUS)?;

    assert_eq!(
        (output.status, output.stdout),
        (
            ExitKind::Code(0),
            b"e69de29bb2d1d6434b8b29ae775ad8c2e48c5391\n".to_vec()
        ),
        "the hash of the empty blob: git read nothing"
    );
    Ok(())
}

/// The test returning at all is the evidence that the child was killed and reaped and both
/// readers saw the end of their pipes; there is no wall-clock assertion (TST-10).
///
/// On Unix the child is `sh`, which would print `late` once its `sleep` ends, so an empty
/// stdout shows that `sh` was killed rather than waited for (1e-10 A, wi-0f7564). `kill`
/// reaches only `sh`, not the `sleep` it forks, so `sleep`'s streams go to `/dev/null`: had
/// it inherited stdout, the reader would wait the full 30 s for it to close the pipe.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn a_child_past_its_timeout_is_killed_and_reaped() -> TestResult {
    let hang: &[&str] = if cfg!(windows) {
        &["ping", "-n", "30", "127.0.0.1"]
    } else {
        &["sh", "-c", "sleep 30 >/dev/null 2>&1; echo late"]
    };
    let limits = Limits {
        timeout: Duration::from_millis(100),
        ..GENEROUS
    };

    let output = run(hang, limits)?;

    assert_eq!(output.status, ExitKind::TimedOut, "killed at its timeout");
    if cfg!(not(windows)) {
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "",
            "sh was killed before it could print"
        );
    }
    Ok(())
}

/// The cap's boundary on an in-memory stream: at most `cap` bytes are kept, and `overflowed`
/// is set only when the stream held more.
#[test]
fn read_capped_keeps_at_most_its_cap_and_flags_the_rest() -> TestResult {
    let cases: [(usize, &[u8], Captured); 5] = [
        (4, b"abc", captured(b"abc", false)),
        (4, b"abcd", captured(b"abcd", false)),
        (4, b"abcde", captured(b"abcd", true)),
        (0, b"", captured(b"", false)),
        (0, b"a", captured(b"", true)),
    ];

    for (cap, input, expected) in cases {
        let got = read_capped(input, ByteCap::new(cap))?;
        assert_eq!(got, expected, "cap {cap}, input {input:?}");
    }
    Ok(())
}

fn captured(bytes: &[u8], overflowed: bool) -> Captured {
    Captured {
        bytes: bytes.to_vec(),
        overflowed,
    }
}

/// OC-3 A (wi-129daa): output past a cap is an error naming the stream, as 1.x's
/// `maxBuffer` is, never silently truncated output.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn output_past_the_cap_is_an_overflow() {
    let tiny = ByteCap::new(4);

    let results = [
        run(
            &["git", "--version"],
            Limits {
                stdout_cap: tiny,
                ..GENEROUS
            },
        ),
        run(
            &["git", "--no-such-flag"],
            Limits {
                stderr_cap: tiny,
                ..GENEROUS
            },
        ),
    ];

    assert!(
        matches!(
            &results,
            [
                Err(ProcessError::Overflow {
                    stream: Stream::Stdout,
                    cap: 4,
                    ..
                }),
                Err(ProcessError::Overflow {
                    stream: Stream::Stderr,
                    cap: 4,
                    ..
                }),
            ]
        ),
        "{results:?}"
    );
}
