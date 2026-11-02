//! Commands that change the dependency graph or accept snapshots (1.x `hooks.mjs`
//! `cargoDependencyCommands`, `CARGO_VALUE_FLAGS` and the `cargo insta` check).

use regex::Regex;

use crate::hooks::setup::{PolicySetupError, compile};
use crate::js::{JsFlags, is_js_space, js_trim};

/// `cargo add` and `cargo remove` options that take a separate value (1.x
/// `CARGO_VALUE_FLAGS`).
const VALUE_FLAGS: [&str; 17] = [
    "-p",
    "--package",
    "-F",
    "--features",
    "--rename",
    "--registry",
    "--path",
    "--git",
    "--branch",
    "--tag",
    "--rev",
    "--manifest-path",
    "--target",
    "--lockfile-path",
    "--config",
    "-Z",
    "--color",
];

/// What a dependency command does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DepOp {
    /// `cargo add`.
    Add,
    /// `cargo remove` or `cargo rm`.
    Remove,
}

impl DepOp {
    /// The subcommand as 1.x names it.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
        }
    }
}

/// One `cargo add` or `cargo remove` and the crates it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DepCommand<'a> {
    pub(super) op: DepOp,
    /// Each crate argument, without a `@version`.
    pub(super) crates: Vec<&'a str>,
}

/// The compiled patterns of the dependency checks.
#[derive(Debug)]
pub(super) struct DepPatterns {
    command: Regex,
    insta: Regex,
}

impl DepPatterns {
    /// Compiles the patterns.
    pub(super) fn new() -> Result<Self, PolicySetupError> {
        Ok(Self {
            command: compile(
                "deps:cargo",
                r"(?:^|\s)cargo\s+(?:\+\S+\s+)?(add|remove|rm)(?:\s+(.*))?$",
                JsFlags::Plain,
            )?,
            insta: compile(
                "deps:insta",
                r"\bcargo\s+insta\s+(?:accept|test\b[^|;&]*--accept)",
                JsFlags::Plain,
            )?,
        })
    }

    /// 1.x `cargoDependencyCommands(cmd)` over its `segments(cmd)`.
    pub(super) fn commands<'s>(
        &self,
        segments: impl IntoIterator<Item = &'s str>,
    ) -> Vec<DepCommand<'s>> {
        segments
            .into_iter()
            .filter_map(|seg| {
                let m = self.command.captures(js_trim(seg))?;
                let op = match m.get(1)?.as_str() {
                    "add" => DepOp::Add,
                    _ => DepOp::Remove,
                };
                let args = m.get(2).map_or("", |args| args.as_str());
                let mut toks = js_trim(args).split(is_js_space).filter(|t| !t.is_empty());
                let mut crates = Vec::new();
                while let Some(tok) = toks.next() {
                    if tok.starts_with('-') {
                        if VALUE_FLAGS.contains(&tok) {
                            // The flag's value.
                            toks.next();
                        }
                        continue;
                    }
                    crates.push(tok.split('@').next().unwrap_or(tok));
                }
                Some(DepCommand { op, crates })
            })
            .collect()
    }

    /// 1.x `/\bcargo\s+insta\s+(?:accept|test\b[^|;&]*--accept)/.test(cmd)`.
    pub(super) fn accepts_snapshots(&self, cmd: &str) -> bool {
        self.insta.is_match(cmd)
    }
}
