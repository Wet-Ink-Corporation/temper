//! `SessionStart`'s texts (1.x `hooks.mjs` `sessionStart` and `withRuntimeWarnings`): the
//! lines it appends to `CLAUDE_ENV_FILE`.

use crate::PathStyle;

/// What `SessionStart` exports to the session's shell through `CLAUDE_ENV_FILE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvExports<'a> {
    /// The 1.x CLI, `<plugin root>/scripts/temper.mjs` with `/` separators; `None` without a
    /// plugin root (the binary run directly), which writes no `TEMPER_CLI` line (OC-6 A).
    pub cli: Option<&'a str>,
    /// The session base, when there is one.
    pub base: Option<&'a str>,
    /// This engine's executable, as the OS reports it (D17).
    pub bin: Option<&'a str>,
    /// The directory that holds it (D17).
    pub bin_dir: Option<&'a str>,
    /// The platform's path rules: on Windows the directory goes on `PATH` in MSYS form.
    pub style: PathStyle,
}

/// The text `SessionStart` appends to `CLAUDE_ENV_FILE`, each line ending in `\n`: 1.x's
/// `export TEMPER_CLI="…"` and `export TEMPER_BASE="…"` byte for byte, then D17's
/// `export TEMPER_BIN="…"` and `export PATH="<dir>:$PATH"` (owner, wi-1d4cd6; from port Phase
/// 3d, OC-6 A, wi-168ff8; D16(w)). The D17 values use `/` separators, the directory is in MSYS
/// form on Windows (`/c/…`, as Git Bash splits `PATH` on `:`), and `\`, `"`, `$` and `` ` ``
/// in them are escaped for the shell's double quotes. 1.x's lines are written as 1.x writes
/// them, unescaped.
///
/// # Examples
///
/// ```
/// use temper_domain::{EnvExports, PathStyle, env_file_lines};
///
/// let lines = env_file_lines(&EnvExports {
///     cli: Some("/p/scripts/temper.mjs"),
///     base: Some("abc"),
///     bin: Some(r"C:\t\temper.exe"),
///     bin_dir: Some(r"C:\t"),
///     style: PathStyle::Windows,
/// });
/// assert_eq!(
///     lines,
///     "export TEMPER_CLI=\"/p/scripts/temper.mjs\"\nexport TEMPER_BASE=\"abc\"\n\
///      export TEMPER_BIN=\"C:/t/temper.exe\"\nexport PATH=\"/c/t:$PATH\"\n"
/// );
/// ```
#[must_use]
pub fn env_file_lines(exports: &EnvExports<'_>) -> String {
    let mut out = String::new();
    if let Some(cli) = exports.cli {
        export(&mut out, "TEMPER_CLI", cli, "");
    }
    if let Some(base) = exports.base.filter(|base| !base.is_empty()) {
        export(&mut out, "TEMPER_BASE", base, "");
    }
    if let Some(bin) = exports.bin {
        export(&mut out, "TEMPER_BIN", &shell_quoted(&bin.replace('\\', "/")), "");
    }
    if let Some(dir) = exports.bin_dir {
        let dir = dir.replace('\\', "/");
        let dir = match exports.style {
            PathStyle::Windows => msys_form(&dir),
            PathStyle::Posix => dir,
        };
        export(&mut out, "PATH", &shell_quoted(&dir), ":$PATH");
    }
    out
}

/// Appends `export <name>="<value><tail>"` and a newline.
fn export(out: &mut String, name: &str, value: &str, tail: &str) {
    for part in ["export ", name, "=\"", value, tail, "\"\n"] {
        out.push_str(part);
    }
}

/// `C:/x` as Git Bash names it, `/c/x`; anything without a drive is left as it is.
fn msys_form(path: &str) -> String {
    let mut chars = path.chars();
    match (chars.next(), chars.next()) {
        (Some(drive), Some(':')) if drive.is_ascii_alphabetic() => {
            let rest = chars.as_str();
            let rest = if rest.is_empty() { "/" } else { rest };
            format!("/{}{rest}", drive.to_ascii_lowercase())
        }
        _ => path.to_owned(),
    }
}

/// `text` safe inside a shell's double quotes.
fn shell_quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '"' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests;
