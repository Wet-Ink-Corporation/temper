//! Which processes are alive (1.x `project.mjs` `pidAlive`), read once per call (OC-2 A,
//! wi-91c5c1; port plan D16(u)).
//!
//! 1.x asks the kernel per pid with `process.kill(pid, 0)`. With no `unsafe` and no FFI
//! (H-01), 2.0 reads the live pids once instead: the numeric names in `/proc` where `/proc`
//! shows this process and `/proc/self` resolves to it, else one `ps -A -o pid=` (Unix) or
//! `tasklist /FO CSV /NH` (Windows). A `/proc` with this pid's directory whose `self` is
//! another pid belongs to another pid namespace, and `ps` would read it too, so nothing
//! answers. A probe that cannot answer reads every pid as alive, so nothing is deleted on a
//! guess.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use temper_domain::{JsValue, PathStyle};

use crate::engine::RunEnv;
use crate::ports::{
    ByteCap, EntryKind, ExitKind, Files, Ports, ProcessOutput, ProcessRequest, ProcessRunner,
    StdoutSink,
};

/// The probe's timeout (Invariant 6).
const TIMEOUT: Duration = Duration::from_secs(5);

/// The cap on the probe's output: about 200,000 `tasklist` rows.
const CAP: ByteCap = ByteCap::new(16 * 1024 * 1024);

/// The live pids, as one probe saw them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LivePids {
    /// The probe listed these, this process among them.
    Known(HashSet<u32>),
    /// No probe could answer: every pid reads as alive.
    Unknown,
}

impl LivePids {
    /// 1.x `pidAlive(pid)` for a JSON value: an integer from 1 to 2^31 - 1 (outside int32,
    /// `process.kill` throws, which 1.x reads as dead) that the probe listed, or any such
    /// integer when the probe could not answer.
    pub(crate) fn alive(&self, pid: &JsValue) -> bool {
        let Some(pid) = int32_pid(pid) else {
            return false;
        };
        match self {
            Self::Known(pids) => pids.contains(&pid),
            Self::Unknown => true,
        }
    }
}

/// `pid` as a process id, when it is a positive int32: `Number.isInteger(pid) && pid > 0`
/// and inside the range `process.kill` takes.
fn int32_pid(pid: &JsValue) -> Option<u32> {
    let JsValue::Number(n) = *pid else {
        return None;
    };
    if n.fract() != 0.0 || !(1.0..=2_147_483_647.0).contains(&n) {
        return None;
    }
    // A whole number in range prints as its digits, which parse exactly.
    pid_digits(&n.to_string())
}

/// The live pids, from `/proc` or one probe process; [`LivePids::Unknown`] when neither
/// lists this process (a `/proc` of another pid namespace, a `ps` without `-o`, a failure).
pub(crate) fn live_pids(ports: &impl Ports, env: &RunEnv) -> LivePids {
    let listed = match env.style {
        PathStyle::Posix => match proc_pids(ports.files(), env.pid) {
            ProcView::Listed(pids) => Some(pids),
            ProcView::Absent => probe(ports.process(), env, &PS),
            // `ps` reads the same `/proc`, so it cannot answer either.
            ProcView::Foreign => None,
        },
        PathStyle::Windows => probe(ports.process(), env, &TASKLIST),
    };
    match listed {
        Some(pids) if pids.contains(&env.pid) => LivePids::Known(pids),
        // The probe does not see this process, so it says nothing about the others.
        Some(_) | None => LivePids::Unknown,
    }
}

/// What `/proc` says about the live pids.
#[derive(Debug)]
enum ProcView {
    /// No `/proc` that lists this process: the probe process answers instead.
    Absent,
    /// A `/proc` of another pid namespace that also has a directory of this pid, such as a
    /// host `/proc` bind-mounted into a pid-unshared sandbox: `/proc/self` is not this pid,
    /// so the pid's directory is an unrelated process's (wi-e29ccf F1).
    Foreign,
    /// The numeric names in `/proc`.
    Listed(HashSet<u32>),
}

/// The numeric names in `/proc`, when `/proc/<own pid>` is a directory there and
/// `/proc/self` resolves to it.
fn proc_pids(files: &impl Files, own: u32) -> ProcView {
    let own_dir = format!("/proc/{own}");
    match files.stat(Path::new(&own_dir)) {
        Ok(Some(meta)) if meta.kind == EntryKind::Dir => {}
        // No `/proc` here, or not this process's: the probe process answers instead.
        Ok(_) | Err(_) => return ProcView::Absent,
    }
    let Ok(entries) = files.read_dir(Path::new("/proc")) else {
        // An unlistable `/proc` is no answer: the probe process answers instead.
        return ProcView::Absent;
    };
    let is_own = |real: &Path| real == Path::new(&own_dir);
    match files.real_path(Path::new("/proc/self")) {
        Ok(Some(real)) if is_own(&real) => ProcView::Listed(
            entries
                .iter()
                .filter_map(|entry| entry.name.to_str().and_then(pid_digits))
                .collect(),
        ),
        // `/proc/self` is another pid, missing or unreadable: this `/proc` cannot be trusted.
        Ok(_) | Err(_) => ProcView::Foreign,
    }
}

/// A program that lists the live pids, and how its stdout is read.
struct Probe {
    program: &'static str,
    args: &'static [&'static str],
    parse: fn(&str) -> HashSet<u32>,
}

/// Unix without `/proc` (macOS, the BSDs).
const PS: Probe = Probe {
    program: "ps",
    args: &["-A", "-o", "pid="],
    parse: ps_pids,
};

/// Windows.
const TASKLIST: Probe = Probe {
    program: "tasklist",
    args: &["/FO", "CSV", "/NH"],
    parse: tasklist_pids,
};

/// Runs the probe in the working directory and reads its stdout, when it exits 0; `None` when
/// it does not start, fails, times out or overflows.
fn probe(process: &impl ProcessRunner, env: &RunEnv, probe: &Probe) -> Option<HashSet<u32>> {
    let args: Vec<OsString> = probe.args.iter().map(OsString::from).collect();
    let request = ProcessRequest {
        program: probe.program,
        args: &args,
        cwd: &env.cwd,
        stdout: StdoutSink::Capture(CAP),
        stderr_cap: CAP,
        timeout: TIMEOUT,
    };
    match process.run(request) {
        Ok(ProcessOutput {
            status: ExitKind::Code(0),
            stdout,
            ..
        }) => Some((probe.parse)(&String::from_utf8_lossy(&stdout))),
        // A probe that cannot answer is `LivePids::Unknown` (OC-2 A).
        Ok(_) | Err(_) => None,
    }
}

/// `ps -A -o pid=`: one pid per line, padded with spaces; other lines are skipped.
fn ps_pids(text: &str) -> HashSet<u32> {
    text.lines()
        .filter_map(|line| pid_digits(line.trim()))
        .collect()
}

/// `tasklist /FO CSV /NH`: `"image","pid","session","#","memory"` per line, every field
/// quoted (an image name holds no `"`); lines that are not such a row, such as a localised
/// `INFO:` message, are skipped.
fn tasklist_pids(text: &str) -> HashSet<u32> {
    text.lines()
        .filter_map(|line| line.strip_prefix('"')?.split("\",\"").nth(1))
        .filter_map(pid_digits)
        .collect()
}

/// `text` as a pid when it is only ASCII digits that fit a `u32`.
fn pid_digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests;
