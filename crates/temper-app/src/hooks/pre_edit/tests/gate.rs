//! The skill gate, ported from `tests/handlers.test.mjs`'s three skill-gate tests (whole, with
//! their post-tool and user-prompt steps, which 2b ported on their own), on fake files without
//! git; and the edits refused before the gate.

use std::path::PathBuf;

use temper_domain::{HookEvent, HookOutcome};

use super::{Ws, decision, quoted, reason};
use crate::engine::RunEnv;
use crate::fakes::FakeFiles;
use temper_domain::PathStyle::Posix;

const ROOT: &str = "[workspace]\nresolver = \"3\"\nmembers = [\"crates/*\"]\n\n[workspace.package]\nedition = \"2024\"\nversion = \"0.1.0\"\npublish = false\n";
const MEMBER: &str = "[package]\nname = \"core\"\nedition.workspace = true\nversion.workspace = true\npublish.workspace = true\n";
const LIB_RS: &str = "//! Core.\n\n/// One.\n#[must_use]\npub fn one() -> u8 {\n    1\n}\n";
const LIB: &str = "/r/crates/core/src/lib.rs";

fn lib_rs_2() -> String {
    format!("{LIB_RS}\n/// Two.\n#[must_use]\npub fn two() -> u8 {{\n    2\n}}\n")
}

/// `handlers.test.mjs`'s `repo(t)` at `/r`, with `CLAUDE_PLUGIN_DATA` at `/data`.
fn repo(extra: &[(&str, &str)]) -> Ws {
    let files = extra.iter().fold(
        FakeFiles::new(Posix)
            .with_file("/r/Cargo.toml", ROOT)
            .with_file("/r/crates/core/Cargo.toml", MEMBER)
            .with_file(LIB, LIB_RS),
        |files, (path, text)| files.with_file(path, text),
    );
    let mut ws = Ws::new(files);
    ws.env = RunEnv {
        plugin_data: Some(PathBuf::from("/data")),
        ..RunEnv::at("/r", Posix)
    };
    ws
}

impl Ws {
    fn event(&self, event: HookEvent, json: &str) -> HookOutcome {
        self.run(event, json).expect("the handler answers")
    }

    /// `preEdit({ tool_name: 'Write', cwd, session_id, ...agent, tool_input: { file_path, content } })`,
    /// where `agent` is the agent's JSON fields, each after a comma.
    fn write_as(&self, (sid, agent): (&str, &str), file: &str, content: &str) -> HookOutcome {
        let json = format!(
            r#"{{"tool_name":"Write","cwd":"/r","session_id":"{sid}"{agent},"tool_input":{{"file_path":"{file}","content":{}}}}}"#,
            quoted(content)
        );
        self.event(HookEvent::PreEdit, &json)
    }

    fn post_tool(&self, sid: &str, rest: &str) -> HookOutcome {
        self.event(
            HookEvent::PostTool,
            &format!(r#"{{"cwd":"/r","session_id":"{sid}"{rest}}}"#),
        )
    }

    fn user_prompt(&self, sid: &str) -> HookOutcome {
        self.event(
            HookEvent::UserPrompt,
            &format!(r#"{{"cwd":"/r","session_id":"{sid}"}}"#),
        )
    }
}

const SID: &str = "s-skill";
const GP: &str = r#","agent_type":"general-purpose","agent_id":"gp1""#;

/// The first half of `handlers.test.mjs`'s skill-gate test: before the skill is loaded.
#[test]
fn skill_gate_a_rust_edit_is_refused_until_rust_craft_is_loaded_through_the_skill_tool() {
    let r = repo(&[]);
    let sid = SID;
    let gp = GP;
    let edit = |content: &str, agent: &str| r.write_as((sid, agent), LIB, content);

    let nudge = r.user_prompt(sid);
    assert!(
        matches!(&nudge, HookOutcome::Context { text, .. } if text.contains("rust-craft standard (skill `temper:rust-craft`) is not loaded in this session yet")),
        "{nudge:?}"
    );
    for agent in ["", gp] {
        let res = edit(&lib_rs_2(), agent);
        assert_eq!(decision(&res), "deny", "{agent}");
        assert!(reason(&res).starts_with("temper: the rust-craft standard is not loaded in this agent context, so this Rust edit was not applied."));
        assert!(reason(&res).contains("Invoke the Skill tool with skill \"temper:rust-craft\""));
    }
    let manifest = r.event(
        HookEvent::PreEdit,
        &format!(
            r#"{{"tool_name":"Edit","cwd":"/r","session_id":"{sid}","tool_input":{{"file_path":"/r/crates/core/Cargo.toml","old_string":"publish.workspace = true\n","new_string":"publish.workspace = true\ndescription = \"Core\"\n"}}}}"#
        ),
    );
    assert_eq!(decision(&manifest), "deny");
    assert_eq!(
        decision(&r.write_as((sid, ""), "/r/NOTES.md", "x")),
        "none",
        "a non-Rust file is not gated"
    );
    assert_eq!(
        decision(&edit(
            &lib_rs_2(),
            r#","agent_type":"temper:rust-implementer","agent_id":"impl1""#
        )),
        "none",
        "the preloaded subagents carry the skill"
    );
}

/// The second half of the same test: other skills do not count, and the marker is per agent
/// context and per session.
#[test]
fn skill_gate_only_rust_craft_loads_it_for_one_agent_context_in_one_session() {
    let r = repo(&[]);
    let sid = SID;
    let gp = GP;
    let edit = |content: &str, agent: &str| r.write_as((sid, agent), LIB, content);
    let multi = |second: &str| {
        let json = format!(
            r#"{{"tool_name":"MultiEdit","cwd":"/r","session_id":"{sid}","tool_input":{{"file_path":"{LIB}","edits":[{{"old_string":"/// One.","new_string":"/// The number one."}},{{"old_string":"    1\n","new_string":{}}}]}}}}"#,
            quoted(second)
        );
        r.event(HookEvent::PreEdit, &json)
    };

    // Other skills and other files do not count.
    r.post_tool(
        sid,
        r#","tool_name":"Skill","tool_input":{"skill":"temper:review"}"#,
    );
    r.post_tool(
        sid,
        r#","tool_name":"Read","tool_input":{"file_path":"/plugins/temper/skills/rust-craft/references/rules.md"}"#,
    );
    assert_eq!(decision(&edit(&lib_rs_2(), "")), "deny");

    let loaded = r.post_tool(
        sid,
        r#","tool_name":"Skill","tool_input":{"skill":"temper:rust-craft"}"#,
    );
    assert_eq!(loaded, HookOutcome::Pass);
    assert_eq!(decision(&edit(&lib_rs_2(), "")), "none");
    assert_eq!(
        decision(&edit(
            &lib_rs_2().replace("    2\n", "    \"2\".parse().unwrap()\n"),
            ""
        )),
        "deny",
        "the escape-hatch scan still applies"
    );
    assert_eq!(
        [
            decision(&multi("    1_u8\n")),
            decision(&multi("    \"1\".parse().unwrap()\n"))
        ],
        ["none", "deny"],
        "MultiEdit applies every edit before the scan"
    );
    assert_eq!(r.user_prompt(sid), HookOutcome::Pass);
    assert_eq!(
        decision(&edit(&lib_rs_2(), gp)),
        "deny",
        "the marker is per agent context"
    );
    assert_eq!(
        decision(&r.write_as(("s-other", ""), LIB, &lib_rs_2())),
        "deny",
        "and per session"
    );
}

#[test]
fn skill_gate_reading_the_rust_craft_skill_md_also_loads_it_with_either_path_separator() {
    let r = repo(&[]);

    r.post_tool(
        "s-read",
        r#","tool_name":"Read","tool_input":{"file_path":"/home/u/.claude/plugins/temper/skills/rust-craft/SKILL.md"}"#,
    );
    let read = r.write_as(("s-read", ""), LIB, &lib_rs_2());
    r.post_tool(
        "s-read-win",
        r#","agent_id":"w1","tool_name":"Read","tool_input":{"file_path":"C:\\Users\\u\\.claude\\plugins\\temper\\skills\\rust-craft\\SKILL.md"}"#,
    );
    let windows = r.write_as(
        (
            "s-read-win",
            r#","agent_type":"general-purpose","agent_id":"w1""#,
        ),
        LIB,
        &lib_rs_2(),
    );
    r.post_tool(
        "s-name",
        r#","tool_name":"Skill","tool_input":{"name":"rust-craft"}"#,
    );
    let name = r.write_as(("s-name", ""), LIB, &lib_rs_2());

    assert_eq!(
        [decision(&read), decision(&windows), decision(&name)],
        ["none", "none", "none"],
        "a Read with either separator, and the Skill tool's `name`"
    );
}

#[test]
fn skill_gate_skill_gate_false_in_temper_json_turns_it_off() {
    let r = repo(&[("/r/.claude/temper.json", r#"{"skillGate":false}"#)]);

    let res = r.write_as(("s-nogate", ""), LIB, &lib_rs_2());

    assert_eq!(decision(&res), "none");
    assert_eq!(r.user_prompt("s-nogate"), HookOutcome::Pass);
}

/// The skill gate's row, and its text with every Iron rule (the domain pins the text).
#[test]
fn a_gated_edit_logs_deny_skill_gate_with_the_file() {
    let r = repo(&[]);

    let res = r.write_as(("s1", ""), LIB, &lib_rs_2());

    assert_eq!(decision(&res), "deny");
    assert_eq!(
        r.ports
            .files
            .text("/r/target/temper/telemetry.jsonl")
            .as_deref(),
        Some(
            "{\"t\":\"2024-10-27T03:33:20.123Z\",\"sid\":\"s1\",\"agent\":\"main\",\"event\":\"deny-skill-gate\",\"file\":\"crates/core/src/lib.rs\"}\n"
        ),
        "telemetry before the denial"
    );
}

/// `.git/` and the state dir are refused before anything else, the lockfile with a row.
#[test]
fn edits_under_git_or_the_state_dir_are_denied_and_the_lockfile_logs_a_row() {
    let r = repo(&[("/r/Cargo.lock", "version = 4\n")]);
    let state = r.write_as(("s1", ""), "/r/target/temper/gate/x.json", "{}");
    let git = r.write_as(("s1", ""), "/r/crates/core/.git/config", "x");
    let near = r.write_as(("s1", ""), "/r/target/temperx/a.json", "{}");

    assert_eq!(
        [decision(&state), decision(&git), decision(&near)],
        ["deny", "deny", "deny"],
        "`path.resolve(file).startsWith(absState)` is a text prefix"
    );
    assert!(reason(&state).starts_with("temper: files under .git"));
    assert!(
        r.ports
            .files
            .text("/r/target/temper/telemetry.jsonl")
            .is_none(),
        "no row"
    );

    let lock = r.write_as(("s1", ""), "/r/Cargo.lock", "version = 3\n");
    assert_eq!(decision(&lock), "deny");
    assert_eq!(
        r.ports
            .files
            .text("/r/target/temper/telemetry.jsonl")
            .as_deref(),
        Some(
            "{\"t\":\"2024-10-27T03:33:20.123Z\",\"sid\":\"s1\",\"agent\":\"main\",\"event\":\"deny-lockfile\",\"file\":\"Cargo.lock\"}\n"
        )
    );
}
