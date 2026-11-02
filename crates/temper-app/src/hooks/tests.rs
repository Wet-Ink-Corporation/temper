use std::path::PathBuf;

use temper_domain::PathStyle::{Posix, Windows};
use temper_domain::{ContextEvent, HookEvent, HookOutcome, SKILL_NOT_LOADED, Scanner};

use super::HookInput;
use super::input::{agent_key, session_segment};
use crate::engine::{HookError, RunEnv};
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::ports::{ExitKind, FsOp};

const WORKSPACE: &str = "[workspace]\nmembers = [\"crates/*\"]\n";
const ISO_NOW: &str = "2024-10-27T03:33:20.123Z";

fn input(json: &str) -> HookInput {
    HookInput::parse(json).expect("the test's input is JSON")
}

/// 1.x `agentKey(input)` and `sessionDir`'s last segment, printed by Node 22.22.0.
#[test]
fn agent_key_and_session_segment_follow_1x() {
    let cases = [
        ("{}", "main", "unknown"),
        (
            r#"{"agent_id":"a/b","session_id":"s 1"}"#,
            "agent-a_b",
            "s_1",
        ),
        (r#"{"agent_id":5,"session_id":0}"#, "agent-5", "unknown"),
        (
            r#"{"agent_id":"é😀","session_id":"x😀"}"#,
            "agent-___",
            "x__",
        ),
        (r#"{"agent_id":"","session_id":""}"#, "main", "unknown"),
        (
            r#"{"agent_id":{},"session_id":[1,2]}"#,
            "agent-_object_Object_",
            "1_2",
        ),
        (r#"{"agent_id":[],"session_id":[]}"#, "agent-", ""),
        (r#"{"agent_id":null,"session_id":null}"#, "main", "unknown"),
        (
            r#"{"agent_id":true,"session_id":false}"#,
            "agent-true",
            "unknown",
        ),
        (
            r#"{"agent_id":"A-z_0.9","session_id":".."}"#,
            "agent-A-z_0.9",
            "..",
        ),
        ("[1]", "main", "unknown"),
        ("\"text\"", "main", "unknown"),
    ];

    let got: Vec<(String, String)> = cases
        .iter()
        .map(|(json, _, _)| {
            let input = input(json);
            (agent_key(&input), session_segment(&input))
        })
        .collect();
    let want: Vec<(String, String)> = cases
        .iter()
        .map(|(_, key, seg)| ((*key).to_owned(), (*seg).to_owned()))
        .collect();

    assert_eq!(
        got, want,
        "per UTF-16 unit, as `.replace(/[^\\w.-]/g, '_')`"
    );
}

#[test]
fn empty_input_reads_as_an_empty_object_and_bad_json_is_an_error() {
    let parsed = ["", "{}", " ", "{"].map(|text| HookInput::parse(text).is_ok());

    assert_eq!(
        parsed,
        [true, true, false, false],
        "`JSON.parse(raw || '{{}}')`"
    );
}

/// A Posix workspace at `/ws` without git, with `extra` files.
fn workspace(extra: &[(&str, &str)]) -> FakeFiles {
    extra.iter().fold(
        FakeFiles::new(Posix).with_file("/ws/Cargo.toml", WORKSPACE),
        |files, (path, text)| files.with_file(path, text),
    )
}

fn env_with_data(data: Option<&str>) -> RunEnv {
    RunEnv {
        plugin_data: data.map(PathBuf::from),
        ..RunEnv::at("/ws", Posix)
    }
}

/// `hooks::run` with a freshly compiled scanner, which only `pre-edit` reads.
fn run(
    ports: &FakePorts,
    env: &RunEnv,
    event: HookEvent,
    input: &HookInput,
) -> Result<HookOutcome, HookError> {
    let call = super::HookCall { ports, env, input };
    super::run(&call, &scanner(), event)
}

fn scanner() -> Scanner {
    Scanner::new().expect("every built-in pattern compiles")
}

fn post_tool(ports: &FakePorts, env: &RunEnv, json: &str) -> Result<HookOutcome, HookError> {
    run(ports, env, HookEvent::PostTool, &input(json))
}

#[test]
fn post_tool_marks_the_skill_for_the_skill_tool_and_a_read_of_skill_md() {
    let calls = [
        r#"{"tool_name":"Skill","session_id":"s1","tool_input":{"skill":"temper:rust-craft"}}"#,
        r#"{"tool_name":"Skill","session_id":"s2","agent_id":"a1","tool_input":{"name":"rust-craft"}}"#,
        r#"{"tool_name":"Skill","session_id":"s3","tool_input":{"skill":null,"name":"rust-craft"}}"#,
        r#"{"tool_name":"Read","session_id":"s4","tool_input":{"file_path":"/p/skills/rust-craft/SKILL.md"}}"#,
        r#"{"tool_name":"Read","session_id":"s5","agent_id":"w1","tool_input":{"file_path":"C:\\u\\skills\\rust-craft\\SKILL.md"}}"#,
        r#"{"tool_name":"Skill","session_id":"x1","tool_input":{"skill":"temper:review"}}"#,
        r#"{"tool_name":"Read","session_id":"x2","tool_input":{"file_path":"/p/rust-craft/references/rules.md"}}"#,
        r#"{"tool_name":"Read","session_id":"x3","tool_input":"/p/rust-craft/SKILL.md"}"#,
    ];
    let ports = FakePorts::new(FakeFiles::new(Posix));
    let env = env_with_data(Some("/data"));

    let answers: Vec<Option<String>> = calls
        .iter()
        .map(|json| post_tool(&ports, &env, json).ok().and_then(|o| o.to_json()))
        .collect();

    assert_eq!(answers, vec![None; 8], "post-tool never answers");
    assert_eq!(
        ports.files.paths(),
        [
            "/data/sessions/s1/skill-main",
            "/data/sessions/s2/skill-agent-a1",
            "/data/sessions/s3/skill-main",
            "/data/sessions/s4/skill-main",
            "/data/sessions/s5/skill-agent-w1",
        ],
        "one marker per session and agent context, and none for other tools or files"
    );
    assert_eq!(
        ports.files.text("/data/sessions/s1/skill-main").as_deref(),
        Some(ISO_NOW),
        "the marker holds `new Date().toISOString()`"
    );
}

#[test]
fn markers_fall_back_to_the_temp_dir_and_resolve_their_path() {
    let skill = r#"{"tool_name":"Skill","session_id":"..","tool_input":{"skill":"rust-craft"}}"#;
    let cases = [
        (env_with_data(None), "/tmp/temper-plugin-data/skill-main"),
        (env_with_data(Some("data/")), "/ws/data/skill-main"),
        (
            RunEnv::at(r"C:\ws", Windows),
            r"C:\Temp\temper-plugin-data\skill-main",
        ),
    ];

    let written: Vec<Vec<String>> = cases
        .iter()
        .map(|(env, _)| {
            let ports = FakePorts::new(FakeFiles::new(env.style));
            post_tool(&ports, env, skill)
                .map(|_| ports.files.paths())
                .unwrap_or_default()
        })
        .collect();
    let want: Vec<Vec<String>> = cases
        .iter()
        .map(|(_, path)| vec![(*path).to_owned()])
        .collect();

    assert_eq!(
        written, want,
        "`path.join(CLAUDE_PLUGIN_DATA || os.tmpdir()/temper-plugin-data, 'sessions', sid)`"
    );
}

#[test]
fn post_tool_logs_skill_loaded_only_inside_a_project() {
    let skill = r#"{"tool_name":"Skill","session_id":"s1","agent_type":"rust-reviewer","tool_input":{"skill":"rust-craft"}}"#;
    let read = r#"{"tool_name":"Read","tool_input":{"file_path":"/p/rust-craft/SKILL.md"}}"#;
    let ports = FakePorts::new(workspace(&[]));
    let outside = FakePorts::new(FakeFiles::new(Posix));
    let env = env_with_data(Some("/data"));

    let ran = [
        post_tool(&ports, &env, skill).is_ok(),
        post_tool(&ports, &env, read).is_ok(),
        post_tool(&outside, &env, skill).is_ok(),
    ];

    assert_eq!(ran, [true; 3], "each call succeeds");
    assert_eq!(
        ports
            .files
            .text("/ws/target/temper/telemetry.jsonl")
            .as_deref(),
        Some(concat!(
            r#"{"t":"2024-10-27T03:33:20.123Z","sid":"s1","agent":"rust-reviewer","event":"skill-loaded","via":"Skill"}"#,
            "\n",
            r#"{"t":"2024-10-27T03:33:20.123Z","agent":"main","event":"skill-loaded","via":"Read"}"#,
            "\n",
        )),
        "one row per load, in the state dir of the project the input's cwd is in"
    );
    assert_eq!(
        outside.files.paths(),
        ["/data/sessions/s1/skill-main"],
        "outside a project, only the marker"
    );
}

#[test]
fn user_prompt_nudges_only_in_a_project_with_the_gate_on_and_no_marker() {
    let prompt = r#"{"session_id":"s1","cwd":"/ws"}"#;
    let nudge = HookOutcome::Context {
        event: ContextEvent::UserPromptSubmit,
        text: SKILL_NOT_LOADED.to_owned(),
    };
    let env = env_with_data(Some("/data"));
    let cases = [
        (workspace(&[]), Some(nudge)),
        (
            workspace(&[("/data/sessions/s1/skill-main", ISO_NOW)]),
            Some(HookOutcome::Pass),
        ),
        (
            workspace(&[("/ws/.claude/temper.json", r#"{"skillGate":false}"#)]),
            Some(HookOutcome::Pass),
        ),
        (FakeFiles::new(Posix), Some(HookOutcome::Pass)),
    ];

    let answers: Vec<Option<HookOutcome>> = cases
        .into_iter()
        .map(|(files, _)| {
            run(
                &FakePorts::new(files),
                &env,
                HookEvent::UserPrompt,
                &input(prompt),
            )
            .ok()
        })
        .collect();

    assert_eq!(
        answers,
        [
            Some(HookOutcome::Context {
                event: ContextEvent::UserPromptSubmit,
                text: SKILL_NOT_LOADED.to_owned(),
            }),
            Some(HookOutcome::Pass),
            Some(HookOutcome::Pass),
            Some(HookOutcome::Pass),
        ],
        "hooks.mjs userPromptSubmit"
    );
}

#[test]
fn the_input_cwd_chooses_the_project_and_a_missing_one_falls_back() {
    let files = || {
        FakeFiles::new(Posix)
            .with_file("/a/Cargo.toml", WORKSPACE)
            .with_file("/b/Cargo.toml", WORKSPACE)
    };
    let env = RunEnv::at("/a", Posix);
    let ask = |cwd: &str| {
        let ports = FakePorts::new(files());
        let json =
            format!(r#"{{"tool_name":"Skill","cwd":{cwd},"tool_input":{{"skill":"rust-craft"}}}}"#);
        post_tool(&ports, &env, &json)
            .map(|_| {
                ports
                    .files
                    .paths()
                    .into_iter()
                    .filter(|p| p.ends_with("telemetry.jsonl"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    assert_eq!(
        [ask(r#""/b""#), ask(r#""/nowhere""#), ask("5"), ask(r#""""#)],
        [
            vec!["/b/target/temper/telemetry.jsonl".to_owned()],
            vec!["/a/target/temper/telemetry.jsonl".to_owned()],
            vec!["/a/target/temper/telemetry.jsonl".to_owned()],
            vec!["/a/target/temper/telemetry.jsonl".to_owned()],
        ],
        "`cwd && fs.existsSync(cwd) ? cwd : process.cwd()`"
    );
}

#[test]
fn a_marker_that_cannot_be_written_is_a_handler_error() {
    let ports = FakePorts::new(FakeFiles::new(Posix).with_read_only("/data"));
    let env = env_with_data(Some("/data"));

    let failed = post_tool(
        &ports,
        &env,
        r#"{"tool_name":"Skill","tool_input":{"skill":"rust-craft"}}"#,
    );

    assert!(
        matches!(&failed, Err(HookError::Files(e)) if e.op() == FsOp::Write),
        "{failed:?}"
    );
}

#[test]
fn null_input_fails_as_v8_does_on_the_first_read() {
    let ports = FakePorts::new(FakeFiles::new(Posix));
    let env = RunEnv::at("/ws", Posix);

    let messages = [HookEvent::PostTool, HookEvent::UserPrompt]
        .map(|event| run(&ports, &env, event, &input("null")).map_err(|e| e.to_string()));

    assert_eq!(
        messages,
        [
            Err("Cannot read properties of null (reading 'tool_input')".to_owned()),
            Err("Cannot read properties of null (reading 'session_id')".to_owned()),
        ],
        "1.x's handler error text (port plan 2-22)"
    );
}

#[test]
fn unported_events_say_so() {
    let ports = FakePorts::new(FakeFiles::new(Posix));
    let env = RunEnv::at("/ws", Posix);

    let answer = run(&ports, &env, HookEvent::Stop, &input("{}"));

    assert!(
        matches!(
            answer,
            Err(HookError::NotPorted {
                event: HookEvent::Stop
            })
        ),
        "{answer:?}"
    );
}

#[test]
fn telemetry_rows_carry_ccr_and_a_failed_append_is_silent() {
    let skill = r#"{"tool_name":"Skill","session_id":null,"tool_input":{"skill":"rust-craft"}}"#;
    let ports = FakePorts::new(workspace(&[]));
    let locked = FakePorts::new(workspace(&[]).with_read_only("/ws/target"));
    let env = RunEnv {
        remote_session: Some("cse_9".to_owned()),
        ..env_with_data(Some("/data"))
    };

    let ran = [
        post_tool(&ports, &env, skill).is_ok(),
        post_tool(&locked, &env, skill).is_ok(),
    ];

    assert_eq!(ran, [true, true], "telemetry never fails the hook");
    assert_eq!(
        ports
            .files
            .text("/ws/target/temper/telemetry.jsonl")
            .as_deref(),
        Some(concat!(
            r#"{"t":"2024-10-27T03:33:20.123Z","sid":null,"ccr":"cse_9","agent":"main","event":"skill-loaded","via":"Skill"}"#,
            "\n"
        )),
        "`sid` as given, `ccr` from the cloud session"
    );
    assert_eq!(
        locked.files.paths(),
        ["/data/sessions/unknown/skill-main", "/ws/Cargo.toml"],
        "no row where the state dir cannot be written"
    );
}

#[test]
fn the_state_dir_is_git_s_common_dir() {
    let ok = |stdout: &str| Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    };
    let process = FakeProcess::new()
        .answering(
            &["git", "rev-parse", "--show-toplevel"],
            "/wt/sub",
            ok("/wt\n"),
        )
        .answering(
            &["git", "rev-parse", "--git-common-dir"],
            "/wt",
            ok("../main/.git\n"),
        );
    let ports = FakePorts {
        process,
        ..FakePorts::new(
            FakeFiles::new(Posix)
                .with_file("/wt/Cargo.toml", WORKSPACE)
                .with_file("/wt/sub/notes.md", ""),
        )
    };
    let env = env_with_data(Some("/data"));

    let ran = post_tool(
        &ports,
        &env,
        r#"{"tool_name":"Skill","cwd":"/wt/sub","tool_input":{"skill":"rust-craft"}}"#,
    );

    assert!(ran.is_ok(), "{ran:?}");
    assert!(
        ports
            .files
            .paths()
            .contains(&"/main/.git/temper/telemetry.jsonl".to_owned()),
        "{:?}",
        ports.files.paths()
    );
}
