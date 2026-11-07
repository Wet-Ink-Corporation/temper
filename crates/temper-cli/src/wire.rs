//! The composition root: the standard-library adapters and the run environment, wired into
//! the engine once per process.

use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::PathBuf;

use temper_app::{Engine, RunEnv};
use temper_domain::{PathStyle, Scanner, ScannerSetupError};
use temper_host::HostPorts;

/// The engine on the host's adapters, in this process's working directory and environment.
pub(crate) fn engine() -> Result<Engine<HostPorts>, WireError> {
    let cwd = std::env::current_dir().map_err(WireError::Cwd)?;
    let windows = cfg!(windows);
    let style = if windows {
        PathStyle::Windows
    } else {
        PathStyle::Posix
    };
    let var = |name: &str| non_empty(name).map(|value| value.to_string_lossy().into_owned());
    let env = RunEnv {
        cwd,
        style,
        plugin_data: non_empty("CLAUDE_PLUGIN_DATA").map(PathBuf::from),
        temp_dir: PathBuf::from(node_tmpdir(windows, var)),
        // With the variable unset, `home_dir` asks the OS (passwd, or the user profile).
        home: node_homedir(windows, |name| std::env::var_os(name), std::env::home_dir),
        remote_session: var("CLAUDE_CODE_REMOTE_SESSION_ID"),
        temper_base: var("TEMPER_BASE"),
        pid: std::process::id(),
        plugin_version: var("TEMPER_PLUGIN_VERSION"),
        disabled: hooks_disabled(),
    };
    let scanner = Scanner::new().map_err(WireError::Scanner)?;
    Ok(Engine::new(HostPorts::default(), env, scanner))
}

/// `TEMPER_DISABLE=1`: every hook answers nothing.
pub(crate) fn hooks_disabled() -> bool {
    std::env::var_os("TEMPER_DISABLE").is_some_and(|value| value == "1")
}

/// The variable `name` unless it is unset or empty, as JS's `process.env[name] || ...` reads it.
fn non_empty(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}

/// Node 22's `os.tmpdir()`, over `var` (a variable's value, `None` when unset or empty).
///
/// Unix: `TMPDIR`, `TMP`, `TEMP`, else `/tmp`, without one trailing `/`. Windows: `TEMP`, `TMP`,
/// else `%SystemRoot%\temp` (`%windir%`, or JS's `undefined`, standing in), without one trailing
/// `\` unless a `:` precedes it. `std::env::temp_dir` reads neither `TMP` nor `TEMP` on Unix.
pub(crate) fn node_tmpdir(windows: bool, var: impl Fn(&str) -> Option<String>) -> String {
    if windows {
        let dir = var("TEMP").or_else(|| var("TMP")).unwrap_or_else(|| {
            let root = var("SystemRoot")
                .or_else(|| var("windir"))
                .unwrap_or_else(|| "undefined".to_owned());
            format!("{root}\\temp")
        });
        match dir.strip_suffix('\\') {
            Some(rest) if !rest.is_empty() && !rest.ends_with(':') => rest.to_owned(),
            Some(_) | None => dir,
        }
    } else {
        let dir = var("TMPDIR")
            .or_else(|| var("TMP"))
            .or_else(|| var("TEMP"))
            .unwrap_or_else(|| "/tmp".to_owned());
        match dir.strip_suffix('/') {
            Some(rest) if !rest.is_empty() => rest.to_owned(),
            Some(_) | None => dir,
        }
    }
}

/// Node 22's `os.homedir()` (libuv's `uv_os_homedir`): `HOME` on Unix and `USERPROFILE` on
/// Windows when the variable is set, even to the empty string (`set` gives its value, `None`
/// when unset), else the OS's answer (`os`). With neither, Node throws; temper reads `""`, an
/// empty home: `~` is then unknown and `$HOME` empty.
pub(crate) fn node_homedir(
    windows: bool,
    set: impl Fn(&str) -> Option<OsString>,
    os: impl FnOnce() -> Option<PathBuf>,
) -> PathBuf {
    let name = if windows { "USERPROFILE" } else { "HOME" };
    set(name).map(PathBuf::from).or_else(os).unwrap_or_default()
}

/// The engine could not be built.
#[derive(Debug)]
pub(crate) enum WireError {
    /// The working directory cannot be read, for example because it was deleted.
    Cwd(io::Error),
    /// A built-in scanner pattern does not compile: a bug in temper.
    Scanner(ScannerSetupError),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cwd(_) => "cannot read the working directory",
            Self::Scanner(_) => "cannot build the scanner",
        })
    }
}

impl Error for WireError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cwd(source) => Some(source),
            Self::Scanner(source) => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{node_homedir, node_tmpdir};

    fn homedir(windows: bool, vars: &[(&str, &str)]) -> PathBuf {
        let set = |name: &str| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        };
        node_homedir(windows, set, || Some(PathBuf::from("/from/os")))
    }

    /// Node 22.22.0 on Linux: `HOME=` gives `""`, `HOME=/x/` gives `/x/`, and with `HOME`
    /// unset the passwd entry; Windows reads `USERPROFILE` the same way (libuv).
    #[test]
    fn homedir_reads_the_variable_when_set_even_empty_else_asks_the_os() {
        assert_eq!(
            [
                homedir(false, &[("HOME", "/x/")]),
                homedir(false, &[("HOME", "")]),
                homedir(false, &[("USERPROFILE", "/u")]),
                homedir(true, &[("USERPROFILE", r"C:\Users\me"), ("HOME", "/h")]),
                homedir(true, &[("HOME", "/h")]),
            ],
            ["/x/", "", "/from/os", r"C:\Users\me", "/from/os"].map(PathBuf::from)
        );
        assert_eq!(
            node_homedir(false, |_| None, || None),
            PathBuf::new(),
            "Node throws here"
        );
    }

    fn tmpdir(windows: bool, vars: &[(&str, &str)]) -> String {
        node_tmpdir(windows, |name| {
            vars.iter()
                .find(|(key, value)| *key == name && !value.is_empty())
                .map(|(_, value)| (*value).to_owned())
        })
    }

    /// Each row printed by Node 22.22.0 on Linux with that environment.
    #[test]
    fn unix_reads_tmpdir_tmp_temp_then_slash_tmp() {
        assert_eq!(
            [
                tmpdir(false, &[]),
                tmpdir(false, &[("TMPDIR", "/x/"), ("TMP", "/a")]),
                tmpdir(false, &[("TMPDIR", ""), ("TMP", "/t2/")]),
                tmpdir(false, &[("TMP", ""), ("TEMP", "/t3/")]),
                tmpdir(false, &[("TMPDIR", "/")]),
                tmpdir(false, &[("TMPDIR", "//")]),
                tmpdir(false, &[("TMPDIR", "/x//")]),
                tmpdir(false, &[("TMPDIR", "a/")]),
            ],
            ["/tmp", "/x", "/t2", "/t3", "/", "/", "/x/", "a"],
            "os.tmpdir() on Unix"
        );
    }

    /// Node 22's `lib/os.js` `tmpdir` on Windows, case by case.
    #[test]
    fn windows_reads_temp_tmp_then_the_system_root() {
        assert_eq!(
            [
                tmpdir(true, &[("TEMP", r"C:\T\"), ("TMP", r"D:\")]),
                tmpdir(true, &[("TMP", r"D:\")]),
                tmpdir(true, &[("SystemRoot", r"C:\Windows")]),
                tmpdir(true, &[("windir", r"C:\W")]),
                tmpdir(true, &[]),
                tmpdir(true, &[("TEMP", r"\")]),
            ],
            [
                r"C:\T",
                r"D:\",
                r"C:\Windows\temp",
                r"C:\W\temp",
                r"undefined\temp",
                r"\",
            ],
            "os.tmpdir() on Windows"
        );
    }
}
