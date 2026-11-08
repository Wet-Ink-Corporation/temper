//! `subagent-start` (1.x `hooks.mjs` `subagentStart`): temper's agents learn the tree they work
//! on, its scope, fingerprint and gate, and a reviewer what its verdict must say.

use std::borrow::Cow;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use temper_domain::{
    CachedGate, ContextEvent, Fingerprint, HookOutcome, JsValue, OtherTree, ReviewerExtras,
    SubagentStartFacts, Target, TelemetryEvent, earlier_review, is_reviewer, skill_preloaded,
    subagent_start_context, tree_gate_command,
};

use super::plans::list_plans;
use super::session_start::gate_command;
use super::{HookCall, logged_then, session_segment};
use crate::changes::{diff_stat, fingerprint, rust_changes};
use crate::engine::HookError;
use crate::git;
use crate::memo::{GitMemo, MemoPorts};
use crate::paths::{self, PathEnv};
use crate::ports::{Ports, ProcessRequest, ProcessRunner, StderrSink, StdoutSink};
use crate::project::Workspace;
use crate::relpath::rel_to;
use crate::session::base_rev;
use crate::state::{HistoryOf, cached_gate, history_for, verdict_for};
use crate::trees::{Checkout, DueTree, Scope, Tree, Widen, due_trees, note_tree, own_tree};

/// `cargo fmt --all`'s timeout for a reviewer's tree (1.x's `timeoutMs: 120000`).
const FMT_TIMEOUT: Duration = Duration::from_secs(120);

/// 1.x `subagentStart(input)`, in its order: the telemetry row for any agent type, the tree
/// record, then for temper's agents the target tree and its context. A reviewer started in the
/// main checkout is sent to the first worktree that holds the session's changes when the main
/// checkout holds none (#135), and has `cargo fmt --all` run in each tree it is told of. Git's
/// answers are memoised for the run ([`GitMemo`]): each spawn is a process start, which on
/// Windows costs tens of milliseconds (3e-16).
///
/// # Errors
///
/// [`HookError::NullInput`] for `null` input; [`HookError::Files`] when the tree record cannot
/// be written (1.x's `writeAtomic` throws); [`HookError::InputType`] where 1.x throws on a value
/// it read: an agent type V8 cannot convert, a tree record's root that is not a string, a red
/// gate record whose `steps` is not an array, or a history row or earlier verdict 1.x's readers
/// throw on (F5).
pub(crate) fn subagent_start(call: &HookCall<'_, impl Ports>) -> Result<HookOutcome, HookError> {
    call.input.require_object("agent_type")?;
    // One memo per run: a repeated git question is answered once (3e-16).
    let memo = GitMemo::default();
    let ports = MemoPorts::new(call.ports, &memo);
    let call = &HookCall {
        ports: &ports,
        env: call.env,
        input: call.input,
    };
    let input = call.input;
    let Some(ws) = call.workspace() else {
        return Ok(HookOutcome::Pass);
    };
    let given = input.given("agent_type");
    let event = TelemetryEvent::SubagentStart {
        agent_type: given.map_or_else(|| r#""""#.to_owned(), JsValue::to_json),
    };
    // The row is written before the type is filtered (#97), then the tree record (#135).
    logged_then(call, &ws, &event, || after_row(call, &ws))
}

/// The rest of [`subagent_start`] after its telemetry row.
fn after_row(call: &HookCall<'_, impl Ports>, ws: &Workspace) -> Result<HookOutcome, HookError> {
    note_tree(call, ws)?;
    let agent_type = match call.input.given("agent_type") {
        Some(value) => value.to_js_string_checked()?,
        None => Cow::Borrowed(""),
    };
    if !skill_preloaded(&agent_type) {
        return Ok(HookOutcome::Pass);
    }
    Ok(HookOutcome::Context {
        event: ContextEvent::SubagentStart,
        text: context(call, ws, &agent_type)?,
    })
}

/// Which of temper's agents started: the reviewer gets more than the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// `rust-reviewer`.
    Reviewer,
    /// `rust-implementer` or `rust-architect`.
    Worker,
}

impl Role {
    /// The role of temper's agent `agent_type`.
    fn of(agent_type: &str) -> Self {
        if is_reviewer(agent_type) {
            Self::Reviewer
        } else {
            Self::Worker
        }
    }
}

/// What every tree's gate command depends on (1.x `gateIn`'s closure).
struct GateIn {
    command: String,
    session_base: Option<String>,
    linked: bool,
    role: Role,
}

impl GateIn {
    /// 1.x `gateIn(t)`: a tree's base is named unless it is the session base that the env file
    /// exports to the main checkout; a reviewer's gate only checks.
    fn of(&self, t: &Tree<'_>) -> String {
        let named = t.scope.base.as_deref().filter(|b| {
            !b.is_empty()
                && (t.checkout == Checkout::Other
                    || self.linked
                    || Some(*b) != self.session_base.as_deref())
        });
        tree_gate_command(&self.command, named, self.role == Role::Reviewer)
    }
}

/// What a listed worktree's line needs beyond its tree, computed before the facts borrow it.
struct Listed {
    label: String,
    at: String,
    rust_files: usize,
    gate_command: String,
}

/// The context for a temper agent of `agent_type` started in `ws`.
fn context(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    agent_type: &str,
) -> Result<String, HookError> {
    let HookCall { ports, env, input } = *call;
    let (files, process, style) = (ports.files(), ports.process(), env.style);
    let role = Role::of(agent_type);
    let linked = ws
        .git_root()
        .is_some_and(|root| git::is_linked(process, style, root));
    let (target, also) = placed(call, ws, role, linked)?;
    if role == Role::Reviewer {
        format_trees(process, &target, &also);
    }
    let tree: &Workspace = &target.ws;
    let base = target.scope.base.as_deref();
    let changes = rust_changes(ports, env, tree, base);
    let fp = fingerprint(ports, env, tree, base);
    let gate = cached_gate(files, style, ws, fp);
    let plans = list_plans(files, style, &ws.root);
    let gate_in = GateIn {
        command: gate_command(env),
        session_base: base_rev(ports, style, ws, &session_segment(input)?),
        linked,
        role,
    };
    // Where 1.x builds the gate line, so a red record it cannot read throws before the other
    // worktrees, the earlier verdict and the diff stat are read (#210 review F3).
    let gate = CachedGate::of(gate.as_ref())?;
    let listed = listed(call, ws, &also, &gate_in);
    let others = other_trees(&also, &listed);
    let (earlier, stat) = match role {
        Role::Reviewer => (earlier_of(call, ws, fp)?, diff_stat(process, tree, base)),
        Role::Worker => (None, String::new()),
    };
    let (target_gate, target_label, at) = (
        gate_in.of(&target),
        tree_label(call, ws, tree),
        paths::norm(&tree.root),
    );
    let facts = SubagentStartFacts {
        agent_type,
        target: match target.checkout {
            Checkout::Here => Target::Here,
            Checkout::Other => Target::Worktree {
                label: &target_label,
                at: &at,
            },
        },
        root: &tree.root,
        base,
        fingerprint: fp,
        changes: &changes,
        gate: &gate,
        gate_command: &target_gate,
        plans: &plans,
        also: &others,
        reviewer: (role == Role::Reviewer).then(|| ReviewerExtras {
            earlier: earlier.as_deref(),
            diff_stat: &stat,
        }),
    };
    Ok(subagent_start_context(&facts))
}

/// 1.x `treeLabel(main, tree)`: `t`'s path relative to this checkout when inside it, else its
/// root with `/` separators.
fn tree_label(call: &HookCall<'_, impl Ports>, ws: &Workspace, t: &Workspace) -> String {
    let (style, cwd) = (call.env.style, call.env.cwd.to_string_lossy());
    let rel = rel_to(
        call.ports.files(),
        PathEnv { style, cwd: &cwd },
        &ws.root,
        &t.root,
    );
    if rel.starts_with("..") {
        paths::norm(&t.root).into_owned()
    } else {
        rel
    }
}

/// The texts of each listed worktree's line.
fn listed(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    also: &[DueTree<'_>],
    gate_in: &GateIn,
) -> Vec<Listed> {
    let HookCall { ports, env, .. } = *call;
    also.iter()
        .map(|t| Listed {
            label: tree_label(call, ws, &t.tree.ws),
            at: paths::norm(&t.tree.ws.root).into_owned(),
            rust_files: rust_changes(ports, env, &t.tree.ws, t.tree.scope.base.as_deref()).len(),
            gate_command: gate_in.of(&t.tree),
        })
        .collect()
}

/// The listed worktrees as the domain's facts borrow them.
fn other_trees<'a>(also: &'a [DueTree<'_>], listed: &'a [Listed]) -> Vec<OtherTree<'a>> {
    also.iter()
        .zip(listed)
        .map(|(t, listed)| OtherTree {
            label: &listed.label,
            at: &listed.at,
            fingerprint: t.fp,
            rust_files: listed.rust_files,
            base: t.tree.scope.base.as_deref(),
            gate_command: &listed.gate_command,
        })
        .collect()
}

/// 1.x's target choice: the checkout here, unless a reviewer started in the main checkout
/// finds the session's changes only in other worktrees (its due trees), when the first of
/// those is the target. The rest of the due worktrees are listed beside it.
fn placed<'w>(
    call: &HookCall<'_, impl Ports>,
    ws: &'w Workspace,
    role: Role,
    linked: bool,
) -> Result<(Tree<'w>, Vec<DueTree<'w>>), HookError> {
    let (due, widen) = match role {
        Role::Reviewer => (due_trees(call, ws)?, Widen::ToSessionBase),
        Role::Worker => (Vec::new(), Widen::No),
    };
    let here = || -> Result<Tree<'w>, HookError> {
        Ok(Tree {
            ws: Cow::Borrowed(ws),
            scope: Scope {
                base: own_tree(call, ws, widen)?.base,
                start_fp: None,
            },
            checkout: Checkout::Here,
        })
    };
    let here_due = due.iter().any(|t| t.tree.checkout == Checkout::Here);
    let mut others = due
        .into_iter()
        .filter(|t| t.tree.checkout == Checkout::Other);
    if linked || here_due {
        return Ok((here()?, others.collect()));
    }
    match others.next() {
        Some(first) => Ok((first.tree, others.collect())),
        None => Ok((here()?, Vec::new())),
    }
}

/// 1.x `if (prior && !prior.stale) lines.push(earlierReview(prior, historyFor(…).length))`.
fn earlier_of(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    fp: Fingerprint,
) -> Result<Option<String>, HookError> {
    let (files, style) = (call.ports.files(), call.env.style);
    let Some(prior) = verdict_for(files, style, ws, fp).filter(JsValue::is_truthy) else {
        return Ok(None);
    };
    if prior.get("stale").is_some_and(JsValue::is_truthy) {
        return Ok(None);
    }
    let kept = history_for(files, HistoryOf { style, ws, fp }, prior.get("recorded_at"))?;
    Ok(Some(earlier_review(&prior, kept)?))
}

/// Whether `cargo fmt` ran to an exit. Either way the hook goes on (3-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "name both outcomes, even when each goes on"]
enum Formatted {
    /// `cargo fmt` ran and exited, with any code or signal, or hit its timeout.
    Ran,
    /// It could not start (no `cargo`), or its output could not be read.
    NotRan,
}

/// `cargo fmt --all` in the target, then in each listed tree, as 1.x runs it for a reviewer.
fn format_trees(process: &impl ProcessRunner, target: &Tree<'_>, also: &[DueTree<'_>]) {
    for root in std::iter::once(&target.ws.root).chain(also.iter().map(|t| &t.tree.ws.root)) {
        match format_tree(process, root) {
            // Formatting is a courtesy to the reviewer; 1.x does not read the result.
            Formatted::Ran | Formatted::NotRan => {}
        }
    }
}

/// `cargo fmt --all` in `root`, with 1.x's 120 s timeout. 1.x never reads its output, so both
/// streams go to the null device: past the timeout, a `rustfmt` that outlives the killed
/// `cargo` holds no pipe a reader waits on (review F2, H-13).
fn format_tree(process: &impl ProcessRunner, root: &str) -> Formatted {
    let args = [OsString::from("fmt"), OsString::from("--all")];
    let request = ProcessRequest {
        program: "cargo",
        args: &args,
        cwd: Path::new(root),
        stdout: StdoutSink::Null,
        stderr: StderrSink::Null,
        timeout: FMT_TIMEOUT,
    };
    match process.run(request) {
        Ok(_) => Formatted::Ran,
        Err(_) => Formatted::NotRan,
    }
}

#[cfg(test)]
mod tests;
