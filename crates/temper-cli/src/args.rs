//! The command line, parsed into what `main` runs.

use std::ffi::OsString;

use temper_app::{OutputFormat, ScanArgs, ScanTargets};

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
    /// `temper scan --all` or `temper scan FILES..`.
    Scan(ScanArgs),
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
    let mut args = args.into_iter().peekable();
    let Some(first) = args.next() else {
        return Command::Usage(UsageExit::Requested);
    };
    if first.is_empty() {
        return Command::Usage(UsageExit::Requested);
    }
    if first == "scan" {
        return scan(args);
    }
    match args.peek() {
        None if first == "--version" => Command::Version,
        _ => Command::Usage(UsageExit::Unknown),
    }
}

/// `scan`'s arguments, read as 1.x reads them: `--json` and `--all` anywhere, and every
/// argument that does not start with `--` a file, decoded lossily as Node decodes its argv.
/// Files win over `--all`; with neither, bare `scan` (1.x `scanChanged`) waits for Phase 4.
fn scan(args: impl Iterator<Item = OsString>) -> Command {
    let (mut json, mut all, mut files) = (false, false, Vec::new());
    for arg in args {
        let arg = arg
            .into_string()
            .unwrap_or_else(|raw| raw.to_string_lossy().into_owned());
        match arg.as_str() {
            "--json" => json = true,
            "--all" => all = true,
            flag if flag.starts_with("--") => {}
            _ => files.push(arg),
        }
    }
    let targets = match (files.is_empty(), all) {
        (false, _) => ScanTargets::Files(files),
        (true, true) => ScanTargets::All,
        (true, false) => return Command::Usage(UsageExit::Unknown),
    };
    let format = if json {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    };
    Command::Scan(ScanArgs { targets, format })
}

#[cfg(test)]
mod tests {
    use super::{Command, UsageExit, parse};
    use std::ffi::OsString;
    use temper_app::{OutputFormat, ScanArgs, ScanTargets};

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

    fn scan_of(args: &[&str]) -> Command {
        parse(["scan"].iter().chain(args).map(OsString::from))
    }

    fn files(names: &[&str]) -> ScanTargets {
        ScanTargets::Files(names.iter().map(|&name| name.to_owned()).collect())
    }

    /// 1.x: `--json` and `--all` anywhere; every argument not starting with `--` is a file, so
    /// `-x` is one and `--bogus` or `--json=1` is ignored; files win over `--all`.
    #[test]
    fn scan_flags_and_files_like_1x() {
        let scan = |targets, format| Command::Scan(ScanArgs { targets, format });

        assert_eq!(
            [
                scan_of(&["--all"]),
                scan_of(&["--json", "--all"]),
                scan_of(&["--all", "--json=1", "--bogus"]),
                scan_of(&["a.rs", "-x", "--json"]),
                scan_of(&["--all", "a.rs"]),
            ],
            [
                scan(ScanTargets::All, OutputFormat::Text),
                scan(ScanTargets::All, OutputFormat::Json),
                scan(ScanTargets::All, OutputFormat::Text),
                scan(files(&["a.rs", "-x"]), OutputFormat::Json),
                scan(files(&["a.rs"]), OutputFormat::Text),
            ],
            "flags and files"
        );
    }

    /// Bare `temper scan` (1.x `scanChanged`) waits for Phase 4 (1d-14 A).
    #[test]
    fn bare_scan_is_not_understood() {
        assert_eq!(
            [scan_of(&[]), scan_of(&["--json"]), scan_of(&["--bogus"])],
            [
                Command::Usage(UsageExit::Unknown),
                Command::Usage(UsageExit::Unknown),
                Command::Usage(UsageExit::Unknown),
            ],
            "no file and no --all"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_file_argument_is_kept_lossily() {
        use std::os::unix::ffi::OsStringExt;
        let args = [OsString::from("scan"), OsString::from_vec(vec![0x66, 0x80])];

        assert_eq!(
            parse(args),
            Command::Scan(ScanArgs {
                targets: files(&["f\u{fffd}"]),
                format: OutputFormat::Text,
            }),
            "decoded as Node decodes its arguments"
        );
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
