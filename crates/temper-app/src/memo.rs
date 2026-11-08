//! A memo of one hook run's git answers (3e-16). The reviewer's `subagent-start` asks git the
//! same questions many times: each tree's changes and fingerprint resolve the same revisions,
//! read the same gate config at them and list the same untracked files, and on Windows each
//! answer costs a process start. 1.x asks each time; the memo answers a repeated question
//! from the first answer, where nothing can have changed that answer in between.
//!
//! Measured (PERF-01): on Windows CI the four reviewer parity cases with worktrees took 2441 to
//! 2661 ms per Rust step, spawning 59 to 63 git processes each, as 1.x does; with the memo they
//! spawn 27 to 33 (the plan's Progress has the counts per case).
//!
//! What can change an answer during a run decides how long it is kept:
//! - `rev-parse`, `symbolic-ref`, `merge-base`, `rev-list` and `show` read only refs and
//!   objects, which a hook run never writes: kept for the whole run.
//! - Any other git call (`diff`, `ls-files`) reads the work tree: kept until something could
//!   change it, which is running any other program (the reviewer's `cargo fmt --all`) or a
//!   write through [`Files`].
//!
//! Only captured git requests are kept. A streamed or null request always runs, and so does
//! one that could not run or ran past its timeout, so a failure is never replayed.

use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::ports::{
    Clock, DirEntry, ExitKind, FileMeta, Files, FsError, Ports, ProcessError, ProcessOutput,
    ProcessRequest, ProcessRunner, ReadInto, StderrSink, StdoutSink,
};

/// The git subcommands that read only refs and objects.
const LASTING: [&str; 5] = [
    "rev-parse",
    "symbolic-ref",
    "merge-base",
    "rev-list",
    "show",
];

/// The answers of one hook run. Create one per run and lend it to [`MemoPorts::new`].
#[derive(Default)]
pub(crate) struct GitMemo {
    // Interior mutability: `ProcessRunner::run` and the `Files` writes take `&self`. A `Cell`
    // is taken and put back around each lookup, so no borrow is ever held across a call.
    answers: Cell<Answers>,
}

impl fmt::Debug for GitMemo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitMemo").finish_non_exhaustive()
    }
}

/// The kept answers, by how long each holds.
#[derive(Debug, Default)]
struct Answers {
    /// Refs and objects: the whole run.
    lasting: HashMap<Key, ProcessOutput>,
    /// The work tree: until another program runs or a file is written.
    tree: HashMap<Key, ProcessOutput>,
}

/// Everything in a captured request that its answer depends on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    args: Vec<OsString>,
    cwd: PathBuf,
    stdout_cap: usize,
    stderr_cap: Option<usize>,
}

impl GitMemo {
    /// The kept answer to `key`, if any.
    fn get(&self, key: &Key) -> Option<ProcessOutput> {
        let answers = self.answers.take();
        // The memo keeps its answer; the caller gets its own copy.
        let hit = answers
            .lasting
            .get(key)
            .or_else(|| answers.tree.get(key))
            .cloned();
        self.answers.set(answers);
        hit
    }

    /// Keeps `output` as the answer to `key`.
    fn keep(&self, key: Key, output: ProcessOutput) {
        let mut answers = self.answers.take();
        let lasting = key
            .args
            .first()
            .is_some_and(|sub| LASTING.iter().any(|kept| sub == kept));
        if lasting {
            answers.lasting.insert(key, output);
        } else {
            answers.tree.insert(key, output);
        }
        self.answers.set(answers);
    }

    /// Drops every answer read from a work tree.
    fn forget_tree(&self) {
        let mut answers = self.answers.take();
        answers.tree.clear();
        self.answers.set(answers);
    }
}

/// A run's ports with git's answers memoised in a [`GitMemo`]; the clock is the run's own.
#[derive(Debug)]
pub(crate) struct MemoPorts<'a, F, R, C> {
    files: MemoFiles<'a, F>,
    process: MemoProcess<'a, R>,
    clock: &'a C,
}

impl<'a, F, R, C> MemoPorts<'a, F, R, C> {
    /// `ports`, with their git answers kept in `memo`.
    pub(crate) fn new<P>(ports: &'a P, memo: &'a GitMemo) -> Self
    where
        P: Ports<Files = F, Process = R, Clock = C>,
    {
        Self {
            files: MemoFiles {
                inner: ports.files(),
                memo,
            },
            process: MemoProcess {
                inner: ports.process(),
                memo,
            },
            clock: ports.clock(),
        }
    }
}

impl<'a, F: Files, R: ProcessRunner, C: Clock> Ports for MemoPorts<'a, F, R, C> {
    type Files = MemoFiles<'a, F>;
    type Process = MemoProcess<'a, R>;
    type Clock = C;

    fn files(&self) -> &Self::Files {
        &self.files
    }

    fn process(&self) -> &Self::Process {
        &self.process
    }

    fn clock(&self) -> &C {
        self.clock
    }
}

/// The process port of [`MemoPorts`].
#[derive(Debug)]
pub(crate) struct MemoProcess<'a, R> {
    inner: &'a R,
    memo: &'a GitMemo,
}

impl<R: ProcessRunner> ProcessRunner for MemoProcess<'_, R> {
    fn run(&self, request: ProcessRequest<'_>) -> Result<ProcessOutput, ProcessError> {
        if request.program != "git" {
            let output = self.inner.run(request);
            // Another program may have changed a work tree, as `cargo fmt` does.
            self.memo.forget_tree();
            return output;
        }
        let StdoutSink::Capture(stdout_cap) = request.stdout else {
            return self.inner.run(request);
        };
        let key = Key {
            args: request.args.to_vec(),
            cwd: request.cwd.to_path_buf(),
            stdout_cap: stdout_cap.get(),
            stderr_cap: match request.stderr {
                StderrSink::Capture(cap) => Some(cap.get()),
                StderrSink::Null => None,
            },
        };
        if let Some(output) = self.memo.get(&key) {
            return Ok(output);
        }
        let output = self.inner.run(request)?;
        if output.status != ExitKind::TimedOut {
            // The memo keeps a copy; the caller owns the answer it was given.
            self.memo.keep(key, output.clone());
        }
        Ok(output)
    }
}

/// The file port of [`MemoPorts`]: reads pass through, and each write also drops the answers
/// read from a work tree.
#[derive(Debug)]
pub(crate) struct MemoFiles<'a, F> {
    inner: &'a F,
    memo: &'a GitMemo,
}

impl<F> MemoFiles<'_, F> {
    /// `result`, after dropping the work-tree answers: a write may have changed a tree.
    fn wrote<T>(&self, result: Result<T, FsError>) -> Result<T, FsError> {
        self.memo.forget_tree();
        result
    }
}

impl<F: Files> Files for MemoFiles<'_, F> {
    fn read_text(&self, path: &Path) -> Result<Option<String>, FsError> {
        self.inner.read_text(path)
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, FsError> {
        self.inner.read_dir(path)
    }

    fn real_path(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        self.inner.real_path(path)
    }

    fn stat(&self, path: &Path) -> Result<Option<FileMeta>, FsError> {
        self.inner.stat(path)
    }

    fn read_into(
        &self,
        path: &Path,
        limit: u64,
        sink: &mut dyn io::Write,
    ) -> Result<ReadInto, FsError> {
        self.inner.read_into(path, limit, sink)
    }

    fn write_atomic(&self, path: &Path, text: &str) -> Result<(), FsError> {
        self.wrote(self.inner.write_atomic(path, text))
    }

    fn append_line(&self, path: &Path, line: &str) -> Result<(), FsError> {
        self.wrote(self.inner.append_line(path, line))
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        self.wrote(self.inner.create_dir_all(path))
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        self.wrote(self.inner.rename(from, to))
    }

    fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        self.wrote(self.inner.remove_file(path))
    }

    fn remove_all(&self, path: &Path) -> Result<(), FsError> {
        self.wrote(self.inner.remove_all(path))
    }

    fn create_new(&self, path: &Path) -> Result<(), FsError> {
        self.wrote(self.inner.create_new(path))
    }
}

#[cfg(test)]
mod tests;
