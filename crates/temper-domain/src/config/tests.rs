use super::*;

fn loaded(text: &str) -> ConfigLoad {
    Config::load(Some(text))
}

fn config(text: &str) -> Config {
    let load = loaded(text);
    assert_eq!(load.problem, None, "{text} loads cleanly");
    load.config
}

fn wrong(key: &'static str, expected: &'static str) -> ConfigProblem {
    ConfigProblem::WrongType { key, expected }
}

#[test]
fn no_file_and_an_empty_file_give_the_defaults() {
    for load in [Config::load(None), Config::load(Some(""))] {
        assert_eq!(load.problem, None, "1.x's `if (!text)`");
        assert_eq!(load.config, Config::defaults());
    }
}

#[test]
fn the_defaults_are_1x_default_config() {
    let d = Config::defaults();
    assert!(d.skill_gate(), "skillGate");
    assert_eq!(d.review(), ReviewMode::Required);
    assert!(d.ask_on_new_dependency(), "askOnNewDependency");
    assert_eq!(
        d.preapproved_dependencies(),
        [
            "thiserror",
            "anyhow",
            "serde",
            "serde_json",
            "tracing",
            "tracing-subscriber",
            "proptest",
            "insta",
            "trybuild",
            "divan",
            "criterion",
            "tempfile"
        ]
    );
    let gate = d.gate();
    assert_eq!(gate.features(), CargoFeatures::All);
    assert_eq!((gate.doc(), gate.scan()), (Switch::On, Switch::On));
    assert!(
        (gate.step_timeout_sec() - 900.0).abs() < f64::EPSILON,
        "stepTimeoutSec"
    );
    assert_eq!(
        d.protected(),
        [
            "**/clippy.toml",
            "**/.clippy.toml",
            "**/rustfmt.toml",
            "**/.rustfmt.toml",
            "**/deny.toml",
            "rust-toolchain",
            "rust-toolchain.toml",
            ".cargo/**",
            ".claude/**",
            ".github/**",
            ".githooks/**",
            "CLAUDE.md",
            "AGENTS.md",
        ]
    );
    assert_eq!(d.exempt(), []);
    assert_eq!(d.own_lints(), [""; 0]);
    assert_eq!(d.telemetry_branch(), None);
}

#[test]
fn top_level_keys_replace_and_nested_ones_merge_one_level() {
    let c = config(
        r#"{"skillGate": false, "review": "advisory", "gate": {"doc": false}, "protected": ["x/**"], "unknown": 1}"#,
    );
    assert!(!c.skill_gate(), "replaced");
    assert_eq!(c.review(), ReviewMode::Advisory);
    assert_eq!(c.gate().doc(), Switch::Off, "merged in");
    assert_eq!(
        c.gate().scan(),
        Switch::On,
        "the other gate keys keep their defaults"
    );
    assert_eq!(
        c.gate().features(),
        CargoFeatures::All,
        "the other gate keys keep their defaults"
    );
    assert_eq!(c.protected(), ["x/**"], "protected is replaced, not merged");
    assert!(c.ask_on_new_dependency(), "untouched");
}

#[test]
fn scanner_manifests_and_telemetry_merge_too() {
    let c = config(
        r#"{"scanner": {"exempt": [{"rule": "H-01", "paths": ["crates/ffi-sys/**"]}, {"rule": "H-02"}]}, "manifests": {"ownLints": ["crates/ffi"]}, "telemetry": {"branch": "temper-telemetry"}, "review": "off", "preapprovedDependencies": ["rand"]}"#,
    );
    let rules: Vec<_> = c
        .exempt()
        .iter()
        .map(|e| (e.rule(), e.paths().to_vec()))
        .collect();
    assert_eq!(
        rules,
        [
            ("H-01", vec!["crates/ffi-sys/**".to_owned()]),
            ("H-02", vec![])
        ]
    );
    assert_eq!(c.own_lints(), ["crates/ffi"]);
    assert_eq!(c.telemetry_branch(), Some("temper-telemetry"));
    assert_eq!(c.review(), ReviewMode::Off);
    assert_eq!(c.preapproved_dependencies(), ["rand"]);
}

#[test]
fn null_reads_as_1x_reads_it() {
    // 1.x: `user.gate ?? {}`, `user.protected ?? DEFAULT`, and the readers' `?? []`.
    let c = config(
        r#"{"gate": null, "protected": null, "scanner": {"exempt": null}, "manifests": {"ownLints": null}, "preapprovedDependencies": null, "telemetry": {"branch": null}}"#,
    );
    assert_eq!(c.gate(), Config::defaults().gate());
    assert_eq!(c.protected(), Config::defaults().protected());
    assert_eq!(c.exempt(), []);
    assert_eq!(c.own_lints(), [""; 0]);
    assert_eq!(
        c.preapproved_dependencies(),
        [""; 0],
        "1.x's `new Set(x ?? [])`"
    );
    assert_eq!(c.telemetry_branch(), None);
    assert_eq!(
        config(r#"{"telemetry": {"branch": ""}}"#).telemetry_branch(),
        None,
        "1.x's `!branch`"
    );
}

#[test]
fn invalid_json_gives_the_defaults_and_says_why() {
    for text in ["{", " ", "{\"a\": 1,}", "\u{FEFF}{}"] {
        let load = loaded(text);
        assert_eq!(load.config, Config::defaults(), "{text:?}");
        let Some(ConfigProblem::NotJson { detail }) = &load.problem else {
            panic!("{text:?}: {:?}", load.problem)
        };
        assert!(!detail.is_empty(), "serde_json's detail (D16(a))");
    }
}

#[test]
fn a_document_that_is_not_an_object_gives_the_defaults() {
    for text in ["null", "5", "\"x\"", "[1]", "true"] {
        let load = loaded(text);
        assert_eq!(load.config, Config::defaults(), "{text}");
        assert_eq!(load.problem, Some(ConfigProblem::NotAnObject), "{text}");
    }
}

#[test]
fn a_wrongly_typed_key_takes_its_default_and_is_reported() {
    // OC-2 B (wi-470923), extending D16(d): 1.x read these by JS truthiness or crashed.
    let cases = [
        (r#"{"skillGate": 0}"#, wrong("skillGate", "a boolean")),
        (r#"{"skillGate": null}"#, wrong("skillGate", "a boolean")),
        (
            r#"{"review": "sometimes"}"#,
            wrong("review", "\"required\", \"advisory\" or \"off\""),
        ),
        (
            r#"{"askOnNewDependency": "no"}"#,
            wrong("askOnNewDependency", "a boolean"),
        ),
        (
            r#"{"preapprovedDependencies": "serde"}"#,
            wrong("preapprovedDependencies", "an array of strings"),
        ),
        (
            r#"{"preapprovedDependencies": ["a", 1]}"#,
            wrong("preapprovedDependencies", "an array of strings"),
        ),
        (r#"{"gate": 1}"#, wrong("gate", "an object")),
        (r#"{"gate": {"doc": "no"}}"#, wrong("gate.doc", "a boolean")),
        (
            r#"{"gate": {"stepTimeoutSec": "9"}}"#,
            wrong("gate.stepTimeoutSec", "a number"),
        ),
        (
            r#"{"protected": "x/**"}"#,
            wrong("protected", "an array of strings"),
        ),
        (r#"{"scanner": []}"#, wrong("scanner", "an object")),
        (
            r#"{"scanner": {"exempt": {}}}"#,
            wrong("scanner.exempt", "an array of { rule, paths } objects"),
        ),
        (
            r#"{"scanner": {"exempt": [{"rule": 1}]}}"#,
            wrong("scanner.exempt", "an array of { rule, paths } objects"),
        ),
        (
            r#"{"scanner": {"exempt": [{"rule": "H-01", "paths": "x"}]}}"#,
            wrong("scanner.exempt", "an array of { rule, paths } objects"),
        ),
        (
            r#"{"manifests": {"ownLints": [true]}}"#,
            wrong("manifests.ownLints", "an array of strings"),
        ),
        (
            r#"{"telemetry": {"branch": 5}}"#,
            wrong("telemetry.branch", "a string or null"),
        ),
    ];
    for (text, problem) in cases {
        let load = loaded(text);
        assert_eq!(
            load.config,
            Config::defaults(),
            "{text}: the key takes its default"
        );
        assert_eq!(load.problem, Some(problem), "{text}");
    }
}

#[test]
fn only_the_wrongly_typed_key_falls_back() {
    let load = loaded(r#"{"skillGate": false, "gate": {"doc": 1, "scan": false}, "review": 3}"#);
    assert!(!load.config.skill_gate(), "a good key is kept");
    assert_eq!(
        load.config.gate().doc(),
        Switch::On,
        "the bad key takes its default"
    );
    assert_eq!(
        load.config.gate().scan(),
        Switch::Off,
        "its good sibling is kept"
    );
    assert_eq!(load.config.review(), ReviewMode::Required);
    assert_eq!(
        load.problem,
        Some(wrong("review", "\"required\", \"advisory\" or \"off\"")),
        "the first, in 1.x's key order"
    );
}

#[test]
fn problems_say_why_in_the_d16d_line() {
    // V8 reads `1e400` as Infinity; serde_json rejects it, so the file is unreadable but not
    // invalid JSON, and the text must not say it is.
    for text in ["{", r#"{"gate":{"stepTimeoutSec":1e400}}"#] {
        let problem = loaded(text).problem.expect("unreadable");
        assert!(
            problem.to_string().starts_with("unreadable JSON: "),
            "{text}: {problem}"
        );
    }
    assert_eq!(ConfigProblem::NotAnObject.to_string(), "not a JSON object");
    assert_eq!(
        wrong("gate.doc", "a boolean").to_string(),
        "`gate.doc` is not a boolean"
    );
}
