//! What the copies, moves, links and patches onto `.rs` files in one shell command add up to
//! (1.x `hooks.mjs` `shellRustWrites`' lists, its `checkFile` scan and its answer). The
//! application reads the command and the files; this decides.

use super::{telemetry_via, texts};
use crate::glob::GlobError;
use crate::hooks::edit::{Counts, Findings, hit_findings, is_mod_rs};
use crate::hooks::telemetry::ShellDenyVia;
use crate::hooks::{ContextEvent, HookOutcome, LazyExemptions, PolicyDecision, TelemetryEvent};
use crate::js::truncate_utf16;
use crate::scan::{CodeKind, ScanOptions, Scanner, new_hits};

/// At most this many escape hatches are listed; the rest are counted.
const DENIES_SHOWN: usize = 10;
/// At most this many advisories go in an answer that only advises.
const ADVICE_SHOWN: usize = 8;
/// At most this many patched files are listed.
const PATCHED_SHOWN: usize = 5;
/// How much of an unread word a denial quotes, in UTF-16 units (1.x `slice(0, 120)`).
const QUOTED_UNITS: usize = 120;

/// Why temper cannot read what a command writes to a `.rs` file (1.x `unknown`'s `why`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unread {
    /// A variable, glob, stdin or a missing path (`dyn`).
    Dynamic,
    /// A relative name after a `cd` temper cannot follow (`cwd`).
    Cwd,
    /// A patch from stdin, a variable or a missing file (`patch`).
    Patch,
    /// A patch file that an earlier command in the line makes or rewrites (`later`).
    Later,
    /// A source an earlier command in the line rewrites, so the disk is not what is copied
    /// (`stale`).
    Stale,
}

/// One file a copy, move or link writes, as the scan reads it (1.x `checkFile`'s facts).
#[derive(Debug)]
pub struct CopiedFile<'a> {
    /// The target, relative to the project root, as the messages print it.
    pub rel: &'a str,
    /// The target's text before the command, or `None` when no file is there.
    pub before: Option<&'a str>,
    /// What the command puts there.
    pub after: &'a str,
    /// How the target is scanned.
    pub options: ScanOptions<'a>,
    /// For a move of a repository source that keeps how the scanner reads it, the source's
    /// file name: its own findings count as already there, and a move onto a new file adds
    /// none (a `mod.rs` moved stays one).
    pub moved_from: Option<&'a str>,
    /// The target matches a `protected` glob.
    pub protected: bool,
}

/// The findings of one command, each list in 1.x's order.
#[derive(Debug, Default)]
pub struct RustWrites {
    found: Findings,
    unread: Vec<(String, Unread)>,
    patched: Vec<String>,
    direct: bool,
}

impl RustWrites {
    /// 1.x `checkFile`: a protected target asks; a new `mod.rs` outside tests is denied; the
    /// new escape hatches, lint exceptions, removed or ignored tests and new clones of the
    /// text the command leaves are what an Edit of it would find. An exemption's globs are
    /// compiled only for a new hit of its rule, as 1.x's `exempt(h, rel)` builds them.
    ///
    /// # Errors
    ///
    /// The [`GlobError`] of an exemption glob tried that does not compile, where 1.x throws.
    pub fn check(
        &mut self,
        file: &CopiedFile<'_>,
        exempt: &LazyExemptions<'_>,
        scanner: &Scanner,
    ) -> Result<(), GlobError> {
        let CopiedFile { rel, before, .. } = *file;
        let found = &mut self.found;
        if file.protected {
            found.asks.push(format!("{rel} is verification/agent configuration. Changing it can weaken the checks (H-05, H-12). Approve only if you asked for this change."));
        }
        let kind = file.options.kind;
        let lenient = file.moved_from.is_some();
        if before.is_none()
            && is_mod_rs(rel)
            && kind != CodeKind::Test
            && file.moved_from != Some("mod.rs")
        {
            found.denies.push(format!(
                "ARC-06 {rel}: new mod.rs file.\n    fix: use `foo.rs` plus `foo/bar.rs`."
            ));
        }
        if lenient && before.is_none() {
            return Ok(());
        }
        let b = scanner.scan(before.unwrap_or(""), &file.options);
        let a = scanner.scan(file.after, &file.options);
        let (mut was, now) = (Counts::of(&b), Counts::of(&a));
        let mut was_hits = b.into_hits();
        if lenient {
            was_hits.extend_from_slice(a.hits());
            was = Counts {
                tests: was.tests.saturating_add(now.tests),
                ignores: was.ignores.saturating_add(now.ignores),
                clones: was.clones.saturating_add(now.clones),
            };
        }
        let new = new_hits(&was_hits, a.into_hits());
        let mut fresh = Vec::with_capacity(new.len());
        for hit in new {
            if !exempt.covers(&hit, rel)? {
                fresh.push(hit);
            }
        }
        hit_findings(found, &fresh, rel);
        if now.tests < was.tests {
            found.asks.push(format!("{rel}: this command removes {} test(s) (H-12). Approve only if the test is being moved or intentionally retired.", was.tests.saturating_sub(now.tests)));
        }
        let ignored = now
            .ignores
            .saturating_sub(was.ignores)
            .saturating_sub(now.tests.saturating_sub(was.tests));
        if ignored > 0 {
            found.asks.push(format!("{rel}: this command adds `#[ignore]` to {ignored} existing test(s) (H-12). Approve only if you asked for the test to stop running."));
        }
        if now.clones > was.clones {
            found.advice.push(format!("H-08 {rel}: {} new .clone() call(s). Each needs an ownership reason (callee stores it, cheap handle, or genuine duplication).", now.clones.saturating_sub(was.clones)));
        }
        Ok(())
    }

    /// What the command writes to a `.rs` file, named by `text` as written, temper cannot read.
    pub fn unread(&mut self, text: String, why: Unread) {
        self.unread.push((text, why));
    }

    /// A patch changes these Rust sources.
    pub fn patched(&mut self, files: impl IntoIterator<Item = String>) {
        self.patched.extend(files);
    }

    /// The command writes straight into a `.rs` file (`> x.r\s`, `curl -o x.rs`).
    pub fn direct(&mut self) {
        self.direct = true;
    }

    /// 1.x's answer, in its order: a direct write, an unread patch, an unread copy, a patch of
    /// Rust sources, escape hatches; then the asks with the advisories as context, else the
    /// advisories alone. `None` when the command writes no Rust source.
    pub(super) fn decide(self, redacted: impl Fn() -> String) -> Option<PolicyDecision> {
        let Self {
            found,
            unread,
            patched,
            direct,
        } = self;
        if direct {
            let row = telemetry_via(ShellDenyVia::ShellReader, Some(redacted()));
            return Some(deny(texts::RUST_SOURCE.to_owned(), row));
        }
        let patch = unread
            .iter()
            .find(|(_, why)| matches!(why, Unread::Patch | Unread::Later));
        if let Some((text, why)) = patch {
            let row = telemetry_via(ShellDenyVia::PatchUnread, Some(redacted()));
            return Some(deny(unread_patch(text, *why), row));
        }
        if let Some((text, why)) = unread.first() {
            let row = telemetry_via(ShellDenyVia::CopyUnread, Some(redacted()));
            return Some(deny(unread_copy(text, *why), row));
        }
        if !patched.is_empty() {
            let via = ShellDenyVia::Patch {
                files: patched.len(),
            };
            return Some(deny(
                patched_text(&patched),
                telemetry_via(via, Some(redacted())),
            ));
        }
        decide_found(found, redacted)
    }
}

/// The escape hatches, asks and advisories of the copies.
fn decide_found(found: Findings, redacted: impl Fn() -> String) -> Option<PolicyDecision> {
    let Findings {
        denies,
        asks,
        advice,
    } = found;
    if !denies.is_empty() {
        let rules = denies
            .iter()
            .map(|d| d.split(' ').next().unwrap_or(d).to_owned())
            .collect();
        let row = TelemetryEvent::DenyShellCopyScan {
            rules,
            cmd: redacted(),
        };
        return Some(deny(escape_hatches(denies), row));
    }
    if !asks.is_empty() {
        let context = (!advice.is_empty())
            .then(|| format!("temper advisories for this command:\n{}", advice.join("\n")));
        return Some(PolicyDecision {
            outcome: HookOutcome::Ask {
                reason: format!("temper needs your approval:\n- {}", asks.join("\n- ")),
                context,
            },
            telemetry: Some(TelemetryEvent::ShellCopyAsk { count: asks.len() }),
        });
    }
    if advice.is_empty() {
        return None;
    }
    let shown = advice.get(..ADVICE_SHOWN).unwrap_or(&advice);
    Some(PolicyDecision {
        outcome: HookOutcome::Context {
            event: ContextEvent::PreToolUse,
            text: format!(
                "temper advisories for this command (not blocking, but a reviewer will ask):\n{}",
                shown.join("\n")
            ),
        },
        telemetry: None,
    })
}

/// A denial with its row.
fn deny(reason: String, row: TelemetryEvent) -> PolicyDecision {
    PolicyDecision {
        outcome: HookOutcome::Deny {
            reason,
            context: None,
        },
        telemetry: Some(row),
    }
}

/// 1.x `PATCH_ROUTE`: the sanctioned way to move Rust between worktrees (#140).
const PATCH_ROUTE: &str = "To bring a change over from another worktree, commit it in the worktree it was made in and run `git cherry-pick <commit>` (or `git merge <branch>`) here: the Stop gate and CI check what it brings in.";

fn unread_patch(text: &str, why: Unread) -> String {
    let why = if why == Unread::Later {
        "its patch file does not exist yet, or is rewritten, earlier in this command"
    } else {
        "stdin, a variable or a missing file"
    };
    format!(
        "temper: temper cannot read the patch that `{}` applies ({why}), so it cannot tell whether it changes Rust sources, and the command was not run (H-12). Save the patch to a file in one command and check it with `git apply --stat <file>`; make changes to .rs files with the Edit or Write tools so the standard's checks run on them. {PATCH_ROUTE} Retrying this patch the same way is denied again.",
        truncate_utf16(text, QUOTED_UNITS)
    )
}

fn unread_copy(text: &str, why: Unread) -> String {
    let t = truncate_utf16(text, QUOTED_UNITS);
    let why = match why {
        Unread::Cwd => format!(
            "temper cannot tell which directory `{t}` is relative to (an earlier `cd` it cannot follow)"
        ),
        Unread::Stale => format!(
            "temper cannot read what this command would write to a .rs file (`{t}` is rewritten earlier in this command, so the file on disk is not what gets copied)"
        ),
        Unread::Dynamic | Unread::Patch | Unread::Later => format!(
            "temper cannot read what this command would write to a .rs file (`{t}` is a variable, glob, stdin or a missing path)"
        ),
    };
    format!(
        "temper: write Rust sources with the Edit or Write tools, not the shell, so the standard's checks run on them (H-12). {why}, so it was not run. Copy from a literal path that exists, or use the Edit tool."
    )
}

fn patched_text(patched: &[String]) -> String {
    let mut list = patched.get(..PATCHED_SHOWN).unwrap_or(patched).join(", ");
    if let Some(more) = patched.len().checked_sub(PATCHED_SHOWN).filter(|&n| n > 0) {
        list.push_str(", and ");
        list.push_str(&more.to_string());
        list.push_str(" more");
    }
    format!(
        "temper: this patch changes Rust sources ({list}), and applying it from the shell skips the standard's checks (H-12), so it was not run. Make the change with the Edit or Write tools, so the scanner checks each hunk. {PATCH_ROUTE} Retrying this patch, from a file, a pipe or with git am, is denied again. `git apply --check`, `git apply --stat` and `patch --dry-run` still run for inspecting it."
    )
}

fn escape_hatches(denies: Vec<String>) -> String {
    let n = denies.len();
    let mut lines = vec![format!(
        "temper: this command writes {n} escape hatch(es) into Rust sources, so it was not run. Rewrite the source without them, or make the change with the Edit or Write tools:"
    )];
    lines.extend(denies.into_iter().take(DENIES_SHOWN));
    if let Some(more) = n.checked_sub(DENIES_SHOWN).filter(|&more| more > 0) {
        lines.push(format!("... and {more} more."));
    }
    lines.push("If one of these is a genuine exception, put `#[expect(<lint>, reason = \"...\")]` on the item; the human will be asked to approve it. See rust-craft references/escape-hatches.md.".to_owned());
    lines.join("\n")
}
