//! The read-only reviewer applies no patch, Rust or not (#140), and for it alone a command
//! temper cannot rule out applies one is denied too (#151): 1.x `preBash`'s first branch.
//! Other agents' dynamic commands are left to the Stop gate and CI, which check the Rust they
//! leave.

use temper_domain::{BashPolicy, InterpreterCode, PolicyDecision};

use crate::hooks::HookCall;
use crate::hooks::shell::{
    Guard, ParseOpts, PatchCommand, PathCx, opaque_patch, parse_commands, patch_command,
};
use crate::paths::PathEnv;
use crate::ports::Ports;
use crate::project::Workspace;

/// 1.x's reviewer branch: a patch the reviewer applies, or the first command temper cannot
/// read exactly, read with no value from temper's model of variables (#151 review 7). Where
/// 1.x's reader throws, that reading finds nothing.
pub(super) fn reviewer_patch(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    cmd: &str,
    code: &InterpreterCode,
) -> Option<PolicyDecision> {
    let start = super::start_dir(call, ws)?;
    let env = call.env;
    let cwd = env.cwd.to_string_lossy();
    let temp = env.temp_dir.to_string_lossy();
    let home = env.home.to_string_lossy();
    let parse = |untracked: bool| {
        // Each reading has its own budget, as each is its own rule in 1.x.
        let guard = Guard::default();
        let cx = PathCx {
            at: PathEnv {
                style: env.style,
                cwd: &cwd,
            },
            os_tmp: &temp,
            guard: Some(&guard),
        };
        let opts = ParseOpts {
            paths: cx,
            cwd: Some(&start),
            home: &home,
            top: Some(&ws.root),
            git_dir: ".git",
            tmp: Some(&temp),
            untracked,
        };
        // 1.x `readerRule(() => parseCommands(...), [])`: a line it cannot read has no command.
        let commands = parse_commands(cmd, opts).unwrap_or_default();
        let applies = commands
            .iter()
            .any(|c| matches!(patch_command(c, cx), Some(PatchCommand::Applies(_))));
        let opaque = untracked
            .then(|| {
                // 1.x `readerRule(() => opaquePatch(c), null)`: a command it throws on is not
                // opaque.
                commands
                    .iter()
                    .find_map(|c| opaque_patch(c, cx, code).ok().flatten())
            })
            .flatten();
        // A word read past the line's budget reads empty and stops the guard (#202 re-review
        // M2): the reading then gives the fallback of a line it cannot read.
        if guard.overflowed() {
            return (false, None);
        }
        (applies, opaque)
    };
    if parse(false).0 {
        return Some(BashPolicy::reviewer_patch());
    }
    let (_, opaque) = parse(true);
    opaque.map(|o| BashPolicy::reviewer_opaque(o.kind, &o.why, o.hint))
}
