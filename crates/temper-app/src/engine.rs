//! The engine: the ports and the run environment, wired once, and one method per command.

use std::cell::Cell;
use std::path::PathBuf;

use temper_domain::{
    GlobError, HookEvent, HookOutcome, JsTypeError, PathStyle, PolicySetupError, Scanner,
};

use crate::hooks::{self, HookCall, HookInput};
use crate::ports::{FsError, Ports};
use crate::{scan, status};

/// What a command reads from the process it runs in, captured once by the composition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEnv {
    /// The working directory, absolute, as the OS reports it.
    pub cwd: PathBuf,
    /// The platform's path rules.
    pub style: PathStyle,
    /// `CLAUDE_PLUGIN_DATA` when it is set and not empty: where the skill markers live.
    pub plugin_data: Option<PathBuf>,
    /// Node's `os.tmpdir()`, where the skill markers live without `plugin_data`.
    pub temp_dir: PathBuf,
    /// Node's `os.homedir()`: what `~` and `$HOME` name in a shell command.
    pub home: PathBuf,
    /// `CLAUDE_CODE_REMOTE_SESSION_ID` when it is set and not empty: the cloud session that
    /// telemetry rows name.
    pub remote_session: Option<String>,
    /// `TEMPER_BASE` when it is set and not empty: the base a command run outside a hook
    /// compares with, when it is a commit here.
    pub temper_base: Option<String>,
    /// This process's id: the pid probe checks that it sees this process (OC-2 A).
    pub pid: u32,
    /// `TEMPER_PLUGIN_VERSION` when it is set and not empty: the version `status --json`
    /// reports, else the engine's own (port plan 3-13).
    pub plugin_version: Option<String>,
    /// `TEMPER_DISABLE=1`: `status --json` says `"disabled": true`.
    pub disabled: bool,
}

/// The use cases, over the ports `P`.
#[derive(Debug)]
pub struct Engine<P> {
    ports: P,
    env: RunEnv,
    scanner: Scanner,
    /// Whether this process has printed the note on an unusable `TEMPER_BASE` (1.x's
    /// module-level `baseNoted`).
    base_noted: Cell<bool>,
}

impl<P: Ports> Engine<P> {
    /// An engine over `ports`, in `env`, with the compiled `scanner`. It keeps all three.
    pub fn new(ports: P, env: RunEnv, scanner: Scanner) -> Self {
        Self {
            ports,
            env,
            scanner,
            base_noted: Cell::new(false),
        }
    }

    /// `temper status`: the project, base, fingerprint, Rust changes, cached gate, recorded
    /// review, baseline and config, as 1.x's nine lines, or with `--json` as its
    /// `temper_status: 1` document, which also carries a gate running now. A `TEMPER_BASE`
    /// that is not a commit here is noted on stderr once per engine.
    ///
    /// # Errors
    ///
    /// [`CommandError::Refused`] (exit [`Exit::Usage`]) outside a Rust project, in text only
    /// (the JSON says `project: null`); [`CommandError::Unconvertible`] where 1.x converts a
    /// recorded value to a string outside any `try` and throws: the review verdict in text
    /// (wi-32d7c9), a running gate's start time in JSON. Nothing else fails, as in 1.x.
    pub fn status(&self, args: &StatusArgs) -> Result<CommandOutput, CommandError> {
        status::run(&self.ports, &self.env, &self.base_noted, args)
    }

    /// `temper scan --all` or `temper scan FILES..`: scans the targets and reports their hits
    /// as 1.x does, exiting [`Exit::Fail`] when any hit is hard.
    ///
    /// # Errors
    ///
    /// [`CommandError::Refused`] (exit [`Exit::Usage`]) outside a Rust project;
    /// [`CommandError::Files`] when a directory of the walk cannot be listed.
    pub fn scan(&self, args: &ScanArgs) -> Result<CommandOutput, CommandError> {
        scan::run(&self.ports, &self.env, &self.scanner, args)
    }

    /// `temper hook <event>` with the parsed hook input: the handler's answer. So far
    /// `post-tool`, `user-prompt`, `pre-edit` and `pre-bash` are ported; `pre-edit` and
    /// `pre-bash` are not yet in the launcher's `RUST_READY`, so 1.x still answers them.
    ///
    /// # Errors
    ///
    /// [`HookError::NotPorted`] for the other events; [`HookError::NullInput`] when the input
    /// is JSON `null`; [`HookError::InputType`] for a field 1.x throws on; [`HookError::Glob`]
    /// for a config glob that does not compile; [`HookError::Files`] when a file the handler
    /// must write cannot be.
    pub fn hook(&self, event: HookEvent, input: &HookInput) -> Result<HookOutcome, HookError> {
        let call = HookCall {
            ports: &self.ports,
            env: &self.env,
            input,
        };
        hooks::run(&call, &self.scanner, event)
    }
}

/// Why a hook handler could not answer. 1.x reports each as `temper hook <event> failed: <msg>`
/// and lets the action proceed.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    /// A file the handler must write could not be.
    #[error(transparent)]
    Files(#[from] FsError),
    /// The hook input is JSON `null`, which 1.x's first property read throws on; the message
    /// is V8's (port plan 2-22).
    #[error("Cannot read properties of null (reading '{field}')")]
    NullInput {
        /// The first property the handler reads.
        field: &'static str,
    },
    /// A field of the hook input has a type 1.x's handler throws on, such as a `file_path` that
    /// is not a string. The message is V8's, without the value (port plan D16(p)).
    #[error("{0}")]
    InputType(&'static str),
    /// A glob of `.claude/temper.json` does not compile; only one past `regex`'s size limit
    /// can fail.
    #[error(transparent)]
    Glob(#[from] GlobError),
    /// A built-in hook pattern does not compile, which is a bug in temper.
    #[error(transparent)]
    Setup(#[from] PolicySetupError),
    /// The event's handler is not in this build yet (port plan 2-13).
    #[error("temper hook {} is not ported to this engine yet", event.name())]
    NotPorted {
        /// The event.
        event: HookEvent,
    },
}

/// The arguments of `temper status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusArgs {
    /// `--json`: the `temper_status: 1` document instead of the nine lines.
    pub format: OutputFormat,
    /// `--session ID`: compare with that session's base instead of `TEMPER_BASE` or HEAD.
    pub session: Option<String>,
}

/// The arguments of `temper scan`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanArgs {
    /// What to scan.
    pub targets: ScanTargets,
    /// How to write the result.
    pub format: OutputFormat,
}

/// What `temper scan` scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanTargets {
    /// `--all`: every `.rs` file below the project root.
    All,
    /// The files named, relative to the working directory, decoded lossily as Node decodes its
    /// arguments.
    Files(Vec<String>),
}

/// How a command writes its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// For people.
    Text,
    /// `--json`.
    Json,
}

/// What a command writes, and how it exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Everything for stdout.
    pub stdout: String,
    /// Everything for stderr.
    pub stderr: String,
    /// How the process exits.
    pub exit: Exit,
}

/// How a command ends; the binary maps each to its exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Exit 0.
    Ok,
    /// Exit 1: the command found something that fails it, such as a hard hit.
    Fail,
    /// Exit 2: the command cannot run here or was called wrongly.
    Usage,
}

/// Why a command could not produce its output.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// The command refused to run, with 1.x's message for stderr and its exit.
    #[error("{message}")]
    Refused {
        /// How the process exits.
        exit: Exit,
        /// The text for stderr, without a newline.
        message: String,
    },
    /// The file system failed where 1.x would fail too.
    #[error(transparent)]
    Files(#[from] FsError),
    /// The result could not be written as JSON.
    #[error("cannot write the results as JSON")]
    Json(#[source] serde_json::Error),
    /// A state file holds a value 1.x converts to a string outside any `try` and cannot, so
    /// 1.x crashes there (exit 1).
    #[error("a recorded value cannot be printed")]
    Unconvertible(#[source] JsTypeError),
}
