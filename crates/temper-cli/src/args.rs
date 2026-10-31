//! The command line, parsed into what `main` runs.

use std::ffi::OsString;

/// The 1.x usage text, verbatim from `scripts/temper.mjs`, so a script that matches it keeps
/// working. It lists every 1.x subcommand, including those not yet ported.
pub(crate) const USAGE: &str = "usage: temper <hook EVENT | gate [--check] [--no-cache] [--json] | scan [FILES..] [--all] [--json] | status [--json] [--session ID] | setup [--dry-run] [--force | --vendor-only] | stats [--json] [--session ID] [--branch [NAME]] | telemetry push --session ID
              | baseline [--init | --update | --tighten | --check [--base REF] [--findings-out FILE] [--json]]
              | report [--findings FILE] [--base REF] [--out DIR] [--format json,md,html]>
";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// `temper --version`: print the version.
    Version,
    /// Print the usage text, as asked for or because the command line is not understood.
    Usage(UsageExit),
}

/// Why the usage text is printed, which decides the exit code.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UsageExit {
    /// No subcommand, or an empty one: the usage text is the answer (exit 0, as in 1.x).
    Requested,
    /// An unknown or not yet ported subcommand (exit 2, as in 1.x).
    Unknown,
}

/// Parse the arguments after the program name.
///
/// As in 1.x, a missing or empty first argument means no subcommand, whatever follows it.
pub(crate) fn parse<I: IntoIterator<Item = OsString>>(args: I) -> Command {
    let mut args = args.into_iter();
    match (args.next(), args.next()) {
        (None, _) => Command::Usage(UsageExit::Requested),
        (Some(first), _) if first.is_empty() => Command::Usage(UsageExit::Requested),
        (Some(first), None) if first == "--version" => Command::Version,
        _ => Command::Usage(UsageExit::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, UsageExit, parse};
    use std::ffi::OsString;

    fn parse_strs(args: &[&str]) -> Command {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_ask_for_the_usage_text() {
        assert_eq!(
            parse_strs(&[]),
            Command::Usage(UsageExit::Requested),
            "empty command line"
        );
    }

    #[test]
    fn an_empty_first_argument_asks_for_the_usage_text() {
        for args in [&[""][..], &["", "x"]] {
            assert_eq!(
                parse_strs(args),
                Command::Usage(UsageExit::Requested),
                "{args:?}"
            );
        }
    }

    #[test]
    fn version_alone_asks_for_the_version() {
        assert_eq!(parse_strs(&["--version"]), Command::Version, "--version");
    }

    #[test]
    fn anything_else_is_not_understood() {
        for args in [&["x"][..], &["--version", "x"], &["hook", "stop"], &["-V"]] {
            assert_eq!(
                parse_strs(args),
                Command::Usage(UsageExit::Unknown),
                "{args:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_argument_is_not_understood() {
        use std::os::unix::ffi::OsStringExt;
        let arg = OsString::from_vec(vec![0x66, 0x6f, 0x80]);
        assert_eq!(
            parse([arg]),
            Command::Usage(UsageExit::Unknown),
            "non-UTF-8 argument"
        );
    }
}
