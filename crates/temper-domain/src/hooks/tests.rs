use super::{
    ContextEvent, HookEvent, HookOutcome, IRON_RULES, SKILL_NOT_LOADED, STATE_DIR_DENIAL, SkillVia,
    TelemetryEvent, TelemetryRow, lockfile_denial, skill_gate_denial, skill_load_via,
    skill_preloaded,
};
use crate::{JsValue, Timestamp};

const NAMES: [&str; 9] = [
    "session-start",
    "user-prompt",
    "pre-edit",
    "pre-bash",
    "pre-handback",
    "post-tool",
    "stop",
    "subagent-stop",
    "subagent-start",
];

#[test]
fn the_nine_1x_event_names_parse_and_print_back() {
    let names: Vec<Option<&str>> = NAMES
        .iter()
        .map(|name| name.parse::<HookEvent>().ok().map(HookEvent::name))
        .collect();

    assert_eq!(
        names,
        NAMES.map(Some),
        "each name reads as its event and prints as itself"
    );
}

#[test]
fn any_other_name_is_unknown_with_1x_s_message() {
    let messages = ["undefined", "", "Stop", "pre_edit", "toString"]
        .map(|name| name.parse::<HookEvent>().map_err(|e| e.to_string()));

    assert_eq!(
        messages,
        [
            Err("temper: unknown hook event undefined".to_owned()),
            Err("temper: unknown hook event ".to_owned()),
            Err("temper: unknown hook event Stop".to_owned()),
            Err("temper: unknown hook event pre_edit".to_owned()),
            Err("temper: unknown hook event toString".to_owned()),
        ],
        "scripts/temper.mjs hook()"
    );
}

#[test]
fn outcomes_print_as_1x_s_json() {
    let outcomes = [
        HookOutcome::Pass,
        HookOutcome::Context {
            event: ContextEvent::UserPromptSubmit,
            text: "a \"b\"\n".to_owned(),
        },
        HookOutcome::Notice {
            message: "m".to_owned(),
        },
        HookOutcome::input_unparseable(HookEvent::PreEdit),
        HookOutcome::handler_failed(HookEvent::PostTool, "cannot write /x: denied"),
    ];

    assert_eq!(
        outcomes.map(|o| o.to_json()),
        [
            None,
            Some(
                r#"{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"a \"b\"\n"}}"#
                    .to_owned()
            ),
            Some(r#"{"systemMessage":"m"}"#.to_owned()),
            Some(
                r#"{"systemMessage":"temper hook pre-edit: could not parse the hook input as JSON; nothing was checked"}"#
                    .to_owned()
            ),
            Some(
                r#"{"systemMessage":"temper hook post-tool failed: cannot write /x: denied"}"#
                    .to_owned()
            ),
        ],
        "hooks.mjs ctx and notice, and scripts/temper.mjs hook()"
    );
}

/// `hooks.mjs` `deny`, `ask` and `ctx('PreToolUse', ..)`, printed by Node 22.
#[test]
fn deny_and_ask_print_as_1x_s_json_and_omit_an_empty_context() {
    let outcomes = [
        HookOutcome::Deny {
            reason: "no \"x\"".to_owned(),
            context: None,
        },
        HookOutcome::Ask {
            reason: "a\nb".to_owned(),
            context: Some("adv".to_owned()),
        },
        HookOutcome::Ask {
            reason: "a".to_owned(),
            context: Some(String::new()),
        },
        HookOutcome::Context {
            event: ContextEvent::PreToolUse,
            text: "c".to_owned(),
        },
    ];

    assert_eq!(
        outcomes.map(|o| o.to_json()),
        [
            Some(r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"no \"x\""}}"#.to_owned()),
            Some(r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"a\nb","additionalContext":"adv"}}"#.to_owned()),
            Some(r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"a"}}"#.to_owned()),
            Some(r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"c"}}"#.to_owned()),
        ],
        "`...(extraContext ? {{ additionalContext }} : {{}})`"
    );
}

#[test]
fn the_skill_is_loaded_through_the_skill_tool_or_a_read_of_its_skill_md() {
    use SkillVia::{Read, Skill};
    let cases = [
        (Some("Skill"), "temper:rust-craft", ""),
        (Some("Skill"), "rust-craft", ""),
        (Some("Skill"), "x:y:rust-craft", ""),
        (Some("Skill"), "temper:review", ""),
        (Some("Skill"), "rust-craft-extra", ""),
        (Some("Skill"), "", "/p/rust-craft/SKILL.md"),
        (Some("Read"), "", "/p/skills/rust-craft/SKILL.md"),
        (Some("Read"), "", r"C:\p\skills\rust-craft\SKILL.md"),
        (Some("Read"), "", "/p/skills/rust-craft/references/rules.md"),
        (Some("Read"), "", "rust-craft/SKILL.md"),
        (Some("Read"), "", "/p/rust-craft/SKILL.md\n"),
        (Some("Read"), "", "/p/rust-craft/skill.md"),
        (Some("Read"), "rust-craft", ""),
        (Some("Write"), "rust-craft", "/p/rust-craft/SKILL.md"),
        (None, "rust-craft", "/p/rust-craft/SKILL.md"),
    ];

    let via: Vec<Option<SkillVia>> = cases
        .iter()
        .map(|&(tool, skill, file)| skill_load_via(tool, skill, file))
        .collect();

    assert_eq!(
        via,
        [
            Some(Skill),
            Some(Skill),
            Some(Skill),
            None,
            None,
            None,
            Some(Read),
            Some(Read),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
        "hooks.mjs postTool: SKILL and the SKILL.md path"
    );
}

#[test]
fn the_skill_nudge_is_1x_s_text() {
    assert_eq!(
        SKILL_NOT_LOADED,
        "temper: this is a Rust workspace and the rust-craft standard (skill `temper:rust-craft`) is not loaded in this session yet; Rust edits are refused until it is. Planning Rust work benefits from loading it first.",
        "hooks.mjs userPromptSubmit"
    );
}

fn json(text: &str) -> JsValue {
    JsValue::from_json(text).expect("valid JSON")
}

#[test]
fn telemetry_rows_keep_1x_s_key_order_and_values() {
    let event = TelemetryEvent::SkillLoaded {
        via: SkillVia::Read,
    };
    let at = Timestamp::from_unix_millis(1_730_000_000_123);
    let (sid, agent, odd) = (
        json(r#""s1""#),
        json(r#""rust-reviewer""#),
        json(r#"{"b":1e21,"1":null}"#),
    );
    let row = |sid, ccr, agent| {
        TelemetryRow {
            at,
            sid,
            ccr,
            agent,
            event: &event,
        }
        .to_json()
    };

    assert_eq!(
        [
            row(Some(&sid), None, None),
            row(None, Some("cse_1"), Some(&agent)),
            row(Some(&odd), None, Some(&odd)),
        ],
        [
            r#"{"t":"2024-10-27T03:33:20.123Z","sid":"s1","agent":"main","event":"skill-loaded","via":"Read"}"#,
            r#"{"t":"2024-10-27T03:33:20.123Z","ccr":"cse_1","agent":"rust-reviewer","event":"skill-loaded","via":"Read"}"#,
            r#"{"t":"2024-10-27T03:33:20.123Z","sid":{"1":null,"b":1e+21},"agent":{"1":null,"b":1e+21},"event":"skill-loaded","via":"Read"}"#,
        ],
        "project.mjs telemetry(): t, sid?, ccr?, agent, event, detail"
    );
}

/// The rows `preEdit` writes, with their detail in 1.x's order.
#[test]
fn pre_edit_rows_carry_their_detail_in_1x_order() {
    let at = Timestamp::from_unix_millis(1_730_000_000_123);
    let file = || "crates/a/src/lib.rs".to_owned();
    let events = [
        TelemetryEvent::DenyLockfile {
            file: "Cargo.lock".to_owned(),
        },
        TelemetryEvent::DenySkillGate { file: file() },
        TelemetryEvent::DenyManifest {
            file: "Cargo.toml".to_owned(),
            rules: vec!["MOD-09".to_owned()],
        },
        TelemetryEvent::DenyScan {
            file: file(),
            rules: vec!["H-02".to_owned(), "H-06".to_owned()],
        },
        TelemetryEvent::Ask {
            file: file(),
            count: 2,
        },
    ];

    let rows: Vec<String> = events
        .iter()
        .map(|event| {
            TelemetryRow {
                at,
                sid: None,
                ccr: None,
                agent: None,
                event,
            }
            .to_json()
        })
        .collect();

    let head = r#"{"t":"2024-10-27T03:33:20.123Z","agent":"main","event":"#;
    assert_eq!(
        rows,
        [
            format!(r#"{head}"deny-lockfile","file":"Cargo.lock"}}"#),
            format!(r#"{head}"deny-skill-gate","file":"crates/a/src/lib.rs"}}"#),
            format!(r#"{head}"deny-manifest","file":"Cargo.toml","rules":["MOD-09"]}}"#),
            format!(r#"{head}"deny-scan","file":"crates/a/src/lib.rs","rules":["H-02","H-06"]}}"#),
            format!(r#"{head}"ask","file":"crates/a/src/lib.rs","count":2}}"#),
        ],
        "hooks.mjs preEdit's telemetry() calls"
    );
}

/// Printed by Node 22 from 1.x's `preEdit` text and `IRON_RULES`.
#[test]
fn the_skill_gate_denial_is_1x_s_text_with_every_iron_rule() {
    let want = "temper: the rust-craft standard is not loaded in this agent context, so this Rust edit was not applied.\nInvoke the Skill tool with skill \"temper:rust-craft\" (or read the rust-craft SKILL.md), then retry the same edit.\nNon-negotiables this edit will be checked against: H-01 no unsafe; H-02 no unwrap/expect in production code or doctests; H-03 no panic!/todo!/unimplemented!/unreachable!, and no std call that panics on input you do not control (integer / and %, thread::spawn, env::args, Duration::from_secs_f64, ...); H-04 no panicking index/slice on unproven bounds, including map[&k] and deque[i]; H-05 no #[allow]; #[expect(lint, reason)] needs human approval; never weaken lint/CI/hook config; H-06 no dbg!, no print macros in production code (binaries: writeln! on a locked handle), no commented-out code, no untracked TODOs; H-07 no Box::leak/mem::forget/transmute/static mut/'static-to-silence-lifetimes; shared mutable state is justified; H-08 no clones to appease the borrow checker; H-09 no String/Box<dyn Error>/anyhow errors in public library signatures; H-10 no silently discarded failures (let _ =, .ok();, empty Err arms, masking defaults); H-11 no lossy `as` numeric casts; H-12 never weaken tests or checks to get green: no deleted or #[ignore]d tests, --no-verify, --ignore-rust-version or manifest switches that stop tests (test, doctest, harness, autotests); H-13 bounded channels/buffers, timeouts, owned tasks and threads; H-14 no blocking in async; no lock guard, RefCell borrow, entered span or watch::Ref across .await; Drop never blocks; H-15 zero warnings; public items documented; H-16 new deps, major bumps, [patch], rust-version raises, API breaks, exceptions, crate boundaries, migrations, test rewrites are human decisions.";

    assert_eq!(skill_gate_denial(), want, "hooks.mjs preEdit");
    assert_eq!(IRON_RULES.len(), 16, "H-01 to H-16");
}

/// 1.x `PRELOADED`, `/(^|:)rust-(implementer|reviewer|architect)$/`, printed by Node 22.
#[test]
fn the_three_temper_agents_carry_the_skill_preloaded() {
    let types = [
        "rust-implementer",
        "temper:rust-reviewer",
        "x:y:rust-architect",
        "rust-architectx",
        "general-purpose",
        "",
        "rust-implementer\n",
        "Xrust-reviewer",
        "a:rust-craft",
    ];

    let got = types.map(skill_preloaded);

    assert_eq!(
        got,
        [true, true, true, false, false, false, false, false, false],
        "`$` without `m` is the end of the text"
    );
}

#[test]
fn the_lockfile_and_state_dir_denials_are_1x_s_texts() {
    assert_eq!(
        lockfile_denial("crates/a/Cargo.lock"),
        "temper: crates/a/Cargo.lock is maintained by Cargo and is never edited by hand (PROC-13). Change it with cargo: `cargo update -p <crate> --precise <version>` to pin or roll back one dependency, `cargo update --workspace` after a manifest edit, `cargo update --dry-run` to preview. For a merge conflict, check out one side's Cargo.lock and re-run a cargo command.",
        "hooks.mjs LOCKFILE_DENY"
    );
    assert_eq!(
        STATE_DIR_DENIAL,
        "temper: files under .git (including temper gate state and review verdicts) are not edited directly. Verdicts come from the temper:rust-reviewer subagent; gate results come from the gate command.",
        "hooks.mjs preEdit"
    );
}
