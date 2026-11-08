//! The command line, parsed into what `main` runs.

use std::ffi::OsString;

use temper_app::{OutputFormat, ScanArgs, ScanTargets, StatusArgs};
use temper_domain::{HookEvent, UnknownHookEvent};

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
    /// `temper hook <event>` for a ported event, or a name that is no event at all.
    Hook(Result<HookEvent, UnknownHookEvent>),
    /// `temper status` in text, or its usage error.
    Status(Result<StatusArgs, StatusUsage>),
    /// Print the usage text, as asked for or because the command line is not understood.
    Usage(UsageExit),
}

/// `temper status --session` without a usable id: 1.x prints `STATUS_USAGE` and exits 2.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StatusUsage;

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
    if first == "hook" {
        return hook(args.next());
    }
    if first == "status" {
        return status(args);
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

/// `status`'s arguments, read as 1.x's `statusArgs` reads them: the first `--session` takes the
/// argument after it, which must be there, not empty and not start with `--`; `--json`
/// anywhere asks for the JSON form; anything else is ignored. The session check comes first,
/// as in 1.x.
fn status(args: impl Iterator<Item = OsString>) -> Command {
    let args: Vec<String> = args
        .map(|arg| {
            arg.into_string()
                .unwrap_or_else(|raw| raw.to_string_lossy().into_owned())
        })
        .collect();
    let json = args.iter().any(|arg| arg == "--json");
    let session = match args.iter().position(|arg| arg == "--session") {
        None => None,
        Some(at) => match args.into_iter().nth(at.saturating_add(1)) {
            Some(sid) if !sid.is_empty() && !sid.starts_with("--") => Some(sid),
            Some(_) | None => return Command::Status(Err(StatusUsage)),
        },
    };
    let format = if json {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    };
    Command::Status(Ok(StatusArgs { format, session }))
}

/// `hook`'s event, decoded lossily, with a missing one read as JS's `undefined`, as 1.x's
/// `HOOKS[argv[1]]` sees it. 1.x ignores any further argument. An event whose handler is not
/// ported yet is an unported subcommand, like any other (port plan 2b-1).
fn hook(event: Option<OsString>) -> Command {
    let name = event.map_or_else(
        || "undefined".to_owned(),
        |raw| {
            raw.into_string()
                .unwrap_or_else(|raw| raw.to_string_lossy().into_owned())
        },
    );
    match name.parse::<HookEvent>() {
        Ok(
            event @ (HookEvent::PostTool
            | HookEvent::UserPrompt
            | HookEvent::PreEdit
            | HookEvent::PreBash
            | HookEvent::SessionStart
            | HookEvent::SubagentStart
            | HookEvent::PreHandback
            | HookEvent::SubagentStop),
        ) => Command::Hook(Ok(event)),
        Ok(HookEvent::Stop) => Command::Usage(UsageExit::Unknown),
        Err(unknown) => Command::Hook(Err(unknown)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, StatusUsage, UsageExit, parse};
    use std::ffi::OsString;
    use temper_app::{OutputFormat, ScanArgs, ScanTargets, StatusArgs};
    use temper_domain::HookEvent;

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

    /// 1.x looks the event up by name; a missing one is JS's `undefined`. Events not ported yet
    /// stay unported subcommands, as `hook stop` above.
    #[test]
    fn hook_reads_its_event_and_ignores_the_rest() {
        let hook = |name: &str| Command::Hook(name.parse::<HookEvent>());

        assert_eq!(
            [
                parse_strs(&["hook", "post-tool"]),
                parse_strs(&["hook", "user-prompt", "extra"]),
                parse_strs(&["hook"]),
                parse_strs(&["hook", ""]),
                parse_strs(&["hook", "bogus"]),
                parse_strs(&["hook", "pre-edit", "extra"]),
                parse_strs(&["hook", "pre-bash"]),
                parse_strs(&["hook", "session-start"]),
                parse_strs(&["hook", "stop"]),
            ],
            [
                hook("post-tool"),
                hook("user-prompt"),
                hook("undefined"),
                hook(""),
                hook("bogus"),
                hook("pre-edit"),
                hook("pre-bash"),
                hook("session-start"),
                Command::Usage(UsageExit::Unknown),
            ],
            "the hook command"
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

    fn status_of(args: &[&str]) -> Command {
        parse(["status"].iter().chain(args).map(OsString::from))
    }

    /// 1.x `statusArgs`: `--session` and the argument after it, anything else ignored.
    #[test]
    fn status_reads_its_session_and_ignores_the_rest() {
        let session = |sid: Option<&str>| {
            Command::Status(Ok(StatusArgs {
                format: OutputFormat::Text,
                session: sid.map(str::to_owned),
            }))
        };
        assert_eq!(status_of(&[]), session(None));
        assert_eq!(status_of(&["x", "--other"]), session(None));
        assert_eq!(status_of(&["--session", "s1", "x"]), session(Some("s1")));
        assert_eq!(
            status_of(&["--session", "a", "--session", "b"]),
            session(Some("a")),
            "indexOf finds the first"
        );
    }

    /// `--session` with no id, an empty one or one starting `--` is a usage error, checked
    /// before `--json`; `--json` anywhere asks for the JSON form (port Phase 3c; until 3c this
    /// row pinned `status --json` as an unported subcommand).
    #[test]
    fn status_with_a_bad_session_is_a_usage_error_and_json_anywhere_is_the_json_form() {
        for args in [
            &["--session"][..],
            &["--session", ""],
            &["--session", "--json"],
            &["--json", "--session"],
        ] {
            assert_eq!(
                status_of(args),
                Command::Status(Err(StatusUsage)),
                "{args:?}"
            );
        }
        let json = |sid: Option<&str>| {
            Command::Status(Ok(StatusArgs {
                format: OutputFormat::Json,
                session: sid.map(str::to_owned),
            }))
        };
        assert_eq!(status_of(&["--json"]), json(None));
        assert_eq!(
            status_of(&["x", "--json", "--session", "s"]),
            json(Some("s"))
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
