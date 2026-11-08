//! The git commands the use cases run, over [`ProcessRunner`] (1.x `project.mjs` `git`).

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use temper_domain::{EXCLUDED_DIRS, PathStyle, js_trim};

use crate::paths;
use crate::ports::{ByteCap, ExitKind, ProcessOutput, ProcessRequest, ProcessRunner, StdoutSink};

/// Every git call's timeout. 1.x set none; 2.0 bounds every child (D16(f), H-13).
const TIMEOUT: Duration = Duration::from_secs(120);

/// The cap on each captured stream: 1.x `run`'s `maxBuffer`.
const CAP: ByteCap = ByteCap::new(256 * 1024 * 1024);

/// 1.x `git(cwd, ['rev-parse', '--show-toplevel'])`: git's stdout as lossy UTF-8, untrimmed,
/// when it exits 0 in `cwd`; `None` otherwise.
pub(crate) fn toplevel(process: &impl ProcessRunner, cwd: &Path) -> Option<String> {
    rev_parse(process, cwd, "--show-toplevel")
}

/// 1.x `git(gitRoot, ['rev-parse', '--git-common-dir'])`: the shared `.git` directory of a
/// linked worktree (relative to `cwd`, or absolute), untrimmed; `None` when git fails.
pub(crate) fn common_dir(process: &impl ProcessRunner, cwd: &Path) -> Option<String> {
    rev_parse(process, cwd, "--git-common-dir")
}

/// The timeout of the protected-configuration checks' git calls (1.x's `timeoutMs: 5000`).
const SHORT: Duration = Duration::from_secs(5);

/// How a short git call ended, as 1.x's `run` tells the outcomes apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Short {
    /// Exit 0, with stdout as lossy UTF-8.
    Ran(String),
    /// Another exit code, or a signal (1.x's `code` is 1 then, with no `error`).
    Failed,
    /// git could not say: it did not start, ran past the timeout, or overflowed a stream
    /// (1.x's `error` is set).
    Unknown,
}

/// `git <args>` in `cwd` with the 5 s timeout.
pub(crate) fn short(process: &impl ProcessRunner, cwd: &Path, args: &[&str]) -> Short {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let request = ProcessRequest {
        program: "git",
        args: &args,
        cwd,
        stdout: StdoutSink::Capture(CAP),
        stderr_cap: CAP,
        timeout: SHORT,
    };
    match process.run(request) {
        Ok(ProcessOutput {
            status: ExitKind::Code(0),
            stdout,
            ..
        }) => Short::Ran(lossy(stdout)),
        Ok(ProcessOutput {
            status: ExitKind::Code(_) | ExitKind::Signal,
            ..
        }) => Short::Failed,
        Ok(ProcessOutput {
            status: ExitKind::TimedOut,
            ..
        })
        | Err(_) => Short::Unknown,
    }
}

/// Bytes as UTF-8, with each invalid sequence replaced, as Node's `'utf8'` decoding does.
fn lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|invalid| String::from_utf8_lossy(invalid.as_bytes()).into_owned())
}

fn rev_parse(process: &impl ProcessRunner, cwd: &Path, flag: &str) -> Option<String> {
    succeeded(process, cwd, &["rev-parse", flag])
}

/// 1.x `isLinked(proj)`: is the checkout at `git_root` a linked worktree, whose git dir is not
/// the repository's common dir? `git rev-parse --git-dir --git-common-dir`, each line trimmed
/// and resolved against the root; a failed call, or fewer than two lines, is not linked.
pub(crate) fn is_linked(process: &impl ProcessRunner, style: PathStyle, git_root: &str) -> bool {
    let args = ["rev-parse", "--git-dir", "--git-common-dir"];
    let Some(out) = succeeded(process, Path::new(git_root), &args) else {
        return false;
    };
    let mut dirs = js_trim(&out)
        .split('\n')
        .map(|line| paths::resolve(style, git_root, js_trim(line)));
    match (dirs.next(), dirs.next()) {
        (Some(git_dir), Some(common)) => git_dir != common,
        (Some(_) | None, None) | (None, Some(_)) => false,
    }
}

/// 1.x `resolveRev(proj, rev)`: `rev` as a commit sha of the repository at `git_root`, trimmed,
/// or `None` when it does not resolve or resolves to nothing.
pub(crate) fn resolve_rev(
    process: &impl ProcessRunner,
    git_root: &Path,
    rev: &str,
) -> Option<String> {
    let commit = format!("{rev}^{{commit}}");
    let sha = succeeded(
        process,
        git_root,
        &["rev-parse", "--verify", "--quiet", &commit],
    )?;
    Some(js_trim(&sha).to_owned()).filter(|sha| !sha.is_empty())
}

/// 1.x `headRev(proj)`: `git rev-parse HEAD`, trimmed (an empty answer stays empty), or
/// `None` when git fails.
pub(crate) fn head_rev(process: &impl ProcessRunner, git_root: &Path) -> Option<String> {
    succeeded(process, git_root, &["rev-parse", "HEAD"]).map(|sha| js_trim(&sha).to_owned())
}

/// 1.x `noteTree`'s `git symbolic-ref --short -q HEAD`: the checkout's branch, trimmed, or
/// `None` when HEAD is detached, git fails or the name is empty (`stdout.trim() || null`).
pub(crate) fn symbolic_ref_short(process: &impl ProcessRunner, git_root: &Path) -> Option<String> {
    let args = ["symbolic-ref", "--short", "-q", "HEAD"];
    let branch = succeeded(process, git_root, &args)?;
    Some(js_trim(&branch).to_owned()).filter(|branch| !branch.is_empty())
}

/// 1.x `isAncestor(proj, a, b)`: `git merge-base --is-ancestor a b` exits 0, so commit `a` is
/// an ancestor of commit `b` (or is `b`). Any other outcome is not.
pub(crate) fn is_ancestor(process: &impl ProcessRunner, git_root: &Path, a: &str, b: &str) -> bool {
    succeeded(process, git_root, &["merge-base", "--is-ancestor", a, b]).is_some()
}

/// How a streamed git call ended. Either way, what reached the sink counts, as 1.x's
/// `fingerprint` hashes `git diff`'s stdout without looking at the exit code, and `spawnSync`
/// returns the output read before an error; the caller names both outcomes (#207 review F2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "name both outcomes, even when each keeps what streamed"]
pub(crate) enum Streamed {
    /// git ran to an exit (any code), a signal or its timeout.
    Ran,
    /// git did not start, or its output could not be read to the end.
    Lost,
}

/// `git <args>` at `git_root`, with its stdout streamed into `sink`.
pub(crate) fn stream_into(
    process: &impl ProcessRunner,
    git_root: &Path,
    args: &[&str],
    sink: &mut (dyn Write + Send),
) -> Streamed {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let request = ProcessRequest {
        program: "git",
        args: &args,
        cwd: git_root,
        stdout: StdoutSink::Stream(sink),
        stderr_cap: CAP,
        timeout: TIMEOUT,
    };
    match process.run(request) {
        Ok(_) => Streamed::Ran,
        // 1.x's `r.stdout ?? ''`: a git that did not start wrote nothing to hash.
        Err(_) => Streamed::Lost,
    }
}

/// 1.x `untracked(proj)`: `git ls-files --others --exclude-standard -z`, split on NUL, without
/// empty names and without anything in the excluded directories; none when git fails.
pub(crate) fn untracked(process: &impl ProcessRunner, git_root: &Path) -> Vec<String> {
    let args = ["ls-files", "--others", "--exclude-standard", "-z"];
    succeeded(process, git_root, &args).map_or_else(Vec::new, |out| {
        out.split('\0')
            .filter(|name| !name.is_empty() && !excluded(name))
            .map(str::to_owned)
            .collect()
    })
}

/// The tracked half of 1.x `changedFiles`: `git diff --name-only -z [base] -- . :(exclude)..`,
/// split on NUL without empty names; none when git fails.
pub(crate) fn diff_names(
    process: &impl ProcessRunner,
    git_root: &Path,
    base: Option<&str>,
) -> Vec<String> {
    let excludes = EXCLUDED_DIRS.map(|dir| format!(":(exclude){dir}"));
    let mut args = vec!["diff", "--name-only", "-z"];
    args.extend(base);
    args.extend(["--", "."]);
    args.extend(excludes.iter().map(String::as_str));
    succeeded(process, git_root, &args).map_or_else(Vec::new, |out| {
        out.split('\0')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

/// 1.x `showAt(proj, rev, rel)`: the file `rel` (with `\` read as `/`) at `rev`, or `None`
/// when git fails.
pub(crate) fn show_at(
    process: &impl ProcessRunner,
    git_root: &Path,
    rev: &str,
    rel: &str,
) -> Option<String> {
    let spec = format!("{rev}:{}", paths::norm(rel));
    succeeded(process, git_root, &["show", &spec])
}

/// A name in one of the directories git's change lists leave out: 1.x's
/// `EXCLUDE.some((x) => f === x || f.startsWith(`${x}/`))`.
fn excluded(name: &str) -> bool {
    EXCLUDED_DIRS.iter().any(|dir| {
        name.strip_prefix(dir)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// `git <args>` in `cwd`: stdout as lossy UTF-8 when it exits 0, else `None`.
fn succeeded(process: &impl ProcessRunner, cwd: &Path, args: &[&str]) -> Option<String> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let request = ProcessRequest {
        program: "git",
        args: &args,
        cwd,
        stdout: StdoutSink::Capture(CAP),
        stderr_cap: CAP,
        timeout: TIMEOUT,
    };
    match process.run(request) {
        Ok(ProcessOutput {
            status: ExitKind::Code(0),
            stdout,
            ..
        }) => Some(lossy(stdout)),
        // 1.x reads every other outcome as a failed git run: another exit code, a signal, a
        // timeout, git missing (its code 127) and `maxBuffer`'s `ENOBUFS` alike.
        Ok(_) | Err(_) => None,
    }
}
