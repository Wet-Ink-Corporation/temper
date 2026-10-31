//! The binary's arguments, output and exit codes, run as a real process.

use crate::support::{TestResult, temper};

/// The 1.x usage text, copied verbatim from `scripts/temper.mjs` (the `default:` case).
const USAGE: &str = "usage: temper <hook EVENT | gate [--check] [--no-cache] [--json] | scan [FILES..] [--all] [--json] | status [--json] [--session ID] | setup [--dry-run] [--force | --vendor-only] | stats [--json] [--session ID] [--branch [NAME]] | telemetry push --session ID\n              | baseline [--init | --update | --tighten | --check [--base REF] [--findings-out FILE] [--json]]\n              | report [--findings FILE] [--base REF] [--out DIR] [--format json,md,html]>\n";

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn version_prints_the_package_version_and_exits_0() -> TestResult {
    let run = temper(&["--version"])?;
    assert_eq!(
        run.stdout,
        concat!("temper ", env!("CARGO_PKG_VERSION"), "\n"),
        "version line"
    );
    assert_eq!(run.code, Some(0), "exit code");
    assert_eq!(run.stderr, "", "nothing on stderr");
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn no_subcommand_prints_the_usage_text_and_exits_0() -> TestResult {
    let run = temper(&[])?;
    assert_eq!(run.stdout, USAGE, "usage text");
    assert_eq!(run.code, Some(0), "exit code");
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn an_unknown_or_unported_subcommand_prints_the_usage_text_and_exits_2() -> TestResult {
    for args in [&["bogus"][..], &["hook", "stop"], &["--version", "x"]] {
        let run = temper(args)?;
        assert_eq!(run.stdout, USAGE, "usage text for {args:?}");
        assert_eq!(run.code, Some(2), "exit code for {args:?}");
    }
    Ok(())
}
