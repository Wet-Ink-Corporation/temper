//! The Iron rules in the compact form the hooks quote (1.x `hooks.mjs` `IRON_RULES`).

/// The compact form of the Iron rules table in `skills/rust-craft/SKILL.md`, H-01 to H-16. The
/// two are kept in step; `temper-cli`'s tests check every rule of the table is here.
pub const IRON_RULES: [&str; 16] = [
    "H-01 no unsafe",
    "H-02 no unwrap/expect in production code or doctests",
    "H-03 no panic!/todo!/unimplemented!/unreachable!, and no std call that panics on input you do not control (integer / and %, thread::spawn, env::args, Duration::from_secs_f64, ...)",
    "H-04 no panicking index/slice on unproven bounds, including map[&k] and deque[i]",
    "H-05 no #[allow]; #[expect(lint, reason)] needs human approval; never weaken lint/CI/hook config",
    "H-06 no dbg!, no print macros in production code (binaries: writeln! on a locked handle), no commented-out code, no untracked TODOs",
    "H-07 no Box::leak/mem::forget/transmute/static mut/'static-to-silence-lifetimes; shared mutable state is justified",
    "H-08 no clones to appease the borrow checker",
    "H-09 no String/Box<dyn Error>/anyhow errors in public library signatures",
    "H-10 no silently discarded failures (let _ =, .ok();, empty Err arms, masking defaults)",
    "H-11 no lossy `as` numeric casts",
    "H-12 never weaken tests or checks to get green: no deleted or #[ignore]d tests, --no-verify, --ignore-rust-version or manifest switches that stop tests (test, doctest, harness, autotests)",
    "H-13 bounded channels/buffers, timeouts, owned tasks and threads",
    "H-14 no blocking in async; no lock guard, RefCell borrow, entered span or watch::Ref across .await; Drop never blocks",
    "H-15 zero warnings; public items documented",
    "H-16 new deps, major bumps, [patch], rust-version raises, API breaks, exceptions, crate boundaries, migrations, test rewrites are human decisions",
];

/// `pre-edit`'s refusal of a Rust edit while the rust-craft skill is not loaded in the agent
/// context, listing every Iron rule.
#[must_use]
pub fn skill_gate_denial() -> String {
    format!(
        "temper: the rust-craft standard is not loaded in this agent context, so this Rust edit was not applied.\nInvoke the Skill tool with skill \"temper:rust-craft\" (or read the rust-craft SKILL.md), then retry the same edit.\nNon-negotiables this edit will be checked against: {}.",
        IRON_RULES.join("; ")
    )
}
