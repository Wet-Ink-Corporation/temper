use super::{
    ContextEvent, HookEvent, HookOutcome, SKILL_NOT_LOADED, SkillVia, TelemetryEvent, TelemetryRow,
    skill_load_via,
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
