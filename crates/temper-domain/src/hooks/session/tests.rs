use crate::hooks::{ContextEvent, HookOutcome, IRON_RULES};
use crate::js::Value;
use crate::{ConfigProblem, Fingerprint, GateRecord, PathStyle};

use super::{
    EnvExports, SessionStartFacts, SessionState, TreeState, env_file_lines, session_start_context,
    with_runtime_warnings,
};

const HEAD: &str = "1111111111111111111111111111111111111111";
const GATE: &str = r#"node "/p/scripts/temper.mjs" gate"#;

/// 1.x's eight lines, verbatim from `sessionStart`, with `loaded`, the gate command, its
/// alias, and the base line's tail.
fn text_1x(loaded: &str, gate: &str, alias: &str, tail: &str) -> String {
    [
        "This repository is a Rust workspace governed by the temper plugin and the rust-craft engineering standard.".to_owned(),
        format!("- The standard is the skill `temper:rust-craft`. It {loaded}. Edits to .rs and Cargo.toml files are refused until it has been loaded in the current agent context."),
        format!("- Iron rules (production code): {}.", IRON_RULES.join("; ")),
        "- Workflow: orient, frame, plan (.temper/plans/), red, green, refactor, self-review, gate, independent review by the temper:rust-reviewer subagent, then report with the Status block.".to_owned(),
        format!("- Gate command: {gate}{alias}. It runs cargo fmt, the escape-hatch scan, the member-manifest check, clippy -D warnings, tests and rustdoc -D warnings, cached per tree fingerprint. When .claude/temper-baseline.json exists, clippy and rustdoc findings ratchet against it: legacy debt passes; findings on changed lines and counts above the baseline fail."),
        "- A turn that changed Rust ends when the gate is green and temper:rust-reviewer has approved the current tree. To pause instead, the final message carries a line \"Status: WIP\" or \"Status: BLOCKED\" with the reason or question.".to_owned(),
        "- Edits to lint, format, toolchain, CI, hook and agent configuration, new dependencies, lint exceptions and removed tests are routed to the human for approval.".to_owned(),
        format!("- Session base revision: {tail}"),
    ]
    .join("\n")
}

const ALIAS: &str = r#" (also available as: node "$TEMPER_CLI" gate)"#;

fn facts(state: &SessionState) -> SessionStartFacts<'_> {
    SessionStartFacts {
        skill_loaded: false,
        gate_command: GATE,
        cli_exported: true,
        base: Some(HEAD),
        state,
        config_problem: None,
    }
}

fn fp() -> Fingerprint {
    "0123456789abcdef".parse().expect("hex")
}

fn no_changes() -> SessionState {
    SessionState::new(&TreeState::NoChanges).expect("no throw")
}

#[test]
fn the_context_is_1x_s_with_no_changes_and_without_git() {
    let state = no_changes();
    let clean = session_start_context(&facts(&state));
    let no_git = session_start_context(&SessionStartFacts {
        skill_loaded: true,
        base: None,
        ..facts(&state)
    });

    assert_eq!(
        clean,
        text_1x(
            "has not been loaded in this session yet",
            GATE,
            ALIAS,
            &format!("{HEAD}. Current state: no Rust changes since the session base.")
        )
    );
    assert_eq!(
        no_git,
        text_1x(
            "is loaded in this session",
            GATE,
            ALIAS,
            "none (no git). Current state: no Rust changes since the session base."
        )
    );
}

#[test]
fn changes_report_the_gate_and_the_review_as_1x_does() {
    let green = GateRecord::from_json(r#"{"ok":true}"#).expect("JSON");
    let red = GateRecord::from_json(r#"{"ok":0}"#).expect("JSON");
    let falsy = GateRecord::from_json("0").expect("JSON");
    let approved = Value::from_json(r#"{"verdict":"approved","stale":true}"#).expect("JSON");
    let odd = Value::from_json(r#"{"verdict":[1,null,2]}"#).expect("JSON");
    let none = Value::from_json(r#"{"x":1}"#).expect("JSON");
    let states = [
        (Some(&green), Some(&approved), "gate green; review approved"),
        (Some(&red), Some(&odd), "gate red; review 1,,2"),
        (Some(&falsy), Some(&none), "gate not run; review undefined"),
        (None, Some(&Value::Number(0.0)), "gate not run; review none"),
    ];
    for (gate, review, said) in states {
        let state = TreeState::Changed {
            files: 2,
            gate,
            review,
            fingerprint: fp(),
        };

        let state = SessionState::new(&state).expect("no throw");

        let text = session_start_context(&facts(&state));

        let tail = format!(
            "{HEAD}. Current state: 2 Rust file(s) changed; {said} (fingerprint 0123456789abcdef)."
        );
        assert_eq!(
            text,
            text_1x(
                "has not been loaded in this session yet",
                GATE,
                ALIAS,
                &tail
            ),
            "{said}"
        );
    }
}

#[test]
fn a_review_verdict_with_a_to_string_key_throws_as_in_1x() {
    let review = Value::from_json(r#"{"verdict":{"toString":1}}"#).expect("JSON");
    let state = TreeState::Changed {
        files: 1,
        gate: None,
        review: Some(&review),
        fingerprint: fp(),
    };

    let error = SessionState::new(&state).expect_err("1.x throws");
    assert!(
        error.to_string().contains("toString"),
        "Cannot convert object to primitive value: {error}"
    );
}

#[test]
fn without_a_plugin_root_the_gate_command_is_temper_gate_with_no_alias() {
    let state = no_changes();
    let text = session_start_context(&SessionStartFacts {
        gate_command: "temper gate",
        cli_exported: false,
        ..facts(&state)
    });

    assert!(
        text.contains("\n- Gate command: temper gate. It runs cargo fmt,"),
        "OC-6 A: no $TEMPER_CLI is exported"
    );
}

#[test]
fn a_config_problem_adds_oc5_s_line_after_the_base_line() {
    let problem = ConfigProblem::NotAnObject;
    let state = no_changes();
    let text = session_start_context(&SessionStartFacts {
        config_problem: Some(&problem),
        ..facts(&state)
    });

    let tail = format!(
        "{HEAD}. Current state: no Rust changes since the session base.\n- Config: unreadable ({problem}): defaults in use."
    );
    assert_eq!(
        text,
        text_1x(
            "has not been loaded in this session yet",
            GATE,
            ALIAS,
            &tail
        )
    );
}

/// Ported from `handlers.test.mjs` "`SessionStart`: an old runtime adds the warning for the
/// human and for the model, and nothing else".
#[test]
fn an_old_runtime_adds_the_warning_for_the_human_and_the_model() {
    let warning = "temper: Node 17.9.1 is older than the documented minimum, Node 18.0.0.";
    let out = HookOutcome::Context {
        event: ContextEvent::SessionStart,
        text: "line one".to_owned(),
    };

    let warned = with_runtime_warnings(out, &[warning.to_owned()]);

    assert_eq!(
        warned.to_json().as_deref(),
        Some(
            r#"{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"line one\n- temper: Node 17.9.1 is older than the documented minimum, Node 18.0.0."},"systemMessage":"temper: Node 17.9.1 is older than the documented minimum, Node 18.0.0."}"#
        ),
        "1.x's spread keeps hookSpecificOutput first"
    );
}

/// Ported from `handlers.test.mjs` "`SessionStart` on a current Node adds no warning".
#[test]
fn on_a_current_node_no_warning_is_added() {
    let out = HookOutcome::Context {
        event: ContextEvent::SessionStart,
        text: "line one".to_owned(),
    };

    assert_eq!(with_runtime_warnings(out.clone(), &[]), out);
}

const NONE: EnvExports<'static> = EnvExports {
    cli: None,
    base: None,
    bin: None,
    bin_dir: None,
    style: PathStyle::Posix,
};

#[test]
fn the_1x_lines_come_first_byte_for_byte_and_d17_s_after() {
    let lines = env_file_lines(&EnvExports {
        cli: Some("/p/scripts/temper.mjs"),
        base: Some("1111"),
        bin: Some("/opt/t/temper"),
        bin_dir: Some("/opt/t"),
        ..NONE
    });

    assert_eq!(
        lines.split_inclusive('\n').collect::<Vec<_>>(),
        [
            "export TEMPER_CLI=\"/p/scripts/temper.mjs\"\n",
            "export TEMPER_BASE=\"1111\"\n",
            "export TEMPER_BIN=\"/opt/t/temper\"\n",
            "export PATH=\"/opt/t:$PATH\"\n",
        ],
        "1.x's two lines, then D17's (OC-6 A)"
    );
}

#[test]
fn a_missing_or_empty_base_and_a_missing_plugin_root_write_no_line() {
    assert_eq!(
        [
            env_file_lines(&EnvExports {
                cli: Some("/p/scripts/temper.mjs"),
                base: Some(""),
                ..NONE
            }),
            env_file_lines(&EnvExports {
                base: Some("abc"),
                ..NONE
            }),
            env_file_lines(&NONE),
        ],
        [
            "export TEMPER_CLI=\"/p/scripts/temper.mjs\"\n".to_owned(),
            "export TEMPER_BASE=\"abc\"\n".to_owned(),
            String::new(),
        ],
        "1.x's `baseRev(..) ? … : ''`; no TEMPER_CLI without a plugin root"
    );
}

#[test]
fn windows_paths_take_slashes_and_the_directory_its_msys_form() {
    let windows = |bin: &'static str, dir: &'static str| {
        env_file_lines(&EnvExports {
            bin: Some(bin),
            bin_dir: Some(dir),
            style: PathStyle::Windows,
            ..NONE
        })
    };

    assert_eq!(
        [
            windows(r"C:\Users\me\temper.exe", r"C:\Users\me"),
            windows(r"d:\temper.exe", r"d:\"),
            windows(r"\\srv\share\temper.exe", r"\\srv\share"),
        ],
        [
            "export TEMPER_BIN=\"C:/Users/me/temper.exe\"\nexport PATH=\"/c/Users/me:$PATH\"\n",
            "export TEMPER_BIN=\"d:/temper.exe\"\nexport PATH=\"/d/:$PATH\"\n",
            "export TEMPER_BIN=\"//srv/share/temper.exe\"\nexport PATH=\"//srv/share:$PATH\"\n",
        ],
        "Git Bash sources the file and splits PATH on `:`"
    );
}

#[test]
fn d17_values_are_escaped_for_double_quotes() {
    let lines = env_file_lines(&EnvExports {
        bin: Some("/a $b/\"c`/t"),
        bin_dir: Some("/a $b/\"c`"),
        ..NONE
    });

    assert_eq!(
        lines, "export TEMPER_BIN=\"/a \\$b/\\\"c\\`/t\"\nexport PATH=\"/a \\$b/\\\"c\\`:$PATH\"\n",
        "the shell reads the path back unchanged"
    );
}
