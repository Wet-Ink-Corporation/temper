use super::{
    DependencyCheck, EditFacts, EditTarget, EditTool, LazyExemptions, ManifestEdit, PolicyDecision,
    RustEdit, edit_policy,
};
use crate::{
    Before, CodeKind, Config, FileRole, GoverningRoot, HookOutcome, JsValue, LazyGlobs, Manifest,
    ManifestEditFacts, ManifestFindings, OwnLints, RootRelation, ScanOptions, Scanner,
    TelemetryEvent, TelemetryRow, Timestamp, manifest_checks,
};

/// The config the table was printed with.
const CONFIG: &str = r#"{"scanner":{"exempt":[{"rule":"H-06","paths":["src/exempt/**"]}]},"protected":["protected.txt","src/guarded.rs"],"preapprovedDependencies":["serde"]}"#;

/// The text after `{"t":..,"agent":"main",` in a row, as the table holds it.
fn detail(event: &TelemetryEvent) -> String {
    let row = TelemetryRow {
        at: Timestamp::from_unix_millis(0),
        sid: None,
        ccr: None,
        agent: None,
        event,
    }
    .to_json();
    let head = r#"{"t":"1970-01-01T00:00:00.000Z","agent":"main","#;
    row.strip_prefix(head)
        .and_then(|rest| rest.strip_suffix('}'))
        .unwrap_or(&row)
        .to_owned()
}

/// The findings 1.x's `manifestChecks` gives the table's root manifest: its own workspace root.
fn root_findings(rel: &str, before: &str, after: &str) -> ManifestFindings {
    let Ok(after) = after.parse::<Manifest>() else {
        return ManifestFindings::default();
    };
    let parsed = before.parse::<Manifest>();
    let before = match (&parsed, before.is_empty()) {
        (_, true) => Before::Empty,
        (Ok(m), false) => Before::Parsed(m),
        (Err(_), false) => Before::Invalid,
    };
    manifest_checks(&ManifestEditFacts {
        rel,
        before,
        after: &after,
        root: Some(GoverningRoot {
            manifest: &after,
            relation: RootRelation::Itself,
        }),
        own_lints: OwnLints::NotListed,
    })
}

fn decide(case: &JsValue, scanner: &Scanner, config: &Config) -> PolicyDecision {
    let field = |key: &str| case.get(key).and_then(JsValue::as_str).unwrap_or("");
    let (rel, before, after) = (field("rel"), field("before"), field("after"));
    let protected = LazyGlobs::new(config.protected().iter().map(String::as_str));
    let exempt = LazyExemptions::new(config.exempt());
    // As 1.x's case-sensitive `rel.endsWith('.rs')`.
    let target = if rel.strip_suffix(".rs").is_some() {
        EditTarget::Rust(RustEdit {
            options: ScanOptions {
                kind: if rel.starts_with("tests/") {
                    CodeKind::Test
                } else {
                    CodeKind::Prod
                },
                file: FileRole::Source,
                forbid: None,
            },
            exempt: &exempt,
        })
    } else if rel.ends_with("Cargo.toml") {
        EditTarget::Manifest(ManifestEdit {
            findings: root_findings(rel, before, after),
            dependencies: DependencyCheck::Ask {
                preapproved: config.preapproved_dependencies(),
            },
        })
    } else {
        EditTarget::Other
    };
    edit_policy(
        EditFacts {
            rel,
            tool: EditTool::Write,
            before,
            after,
            protected: &protected,
            target,
        },
        scanner,
    )
    .expect("the globs compile")
}

/// 1.x `preEdit` from `const denies = []` to the end, on the cases of `policy_cases.json`, which
/// Node 22 printed by calling `preEdit` with a preloaded agent on a one-package workspace (the
/// generator is in the 2c Progress notes of the phase plan).
#[test]
fn edit_policy_gives_1x_s_answers_and_rows() {
    let table =
        JsValue::from_json(include_str!("../policy_cases.json")).expect("the table is JSON");
    let JsValue::Array(cases) = table else {
        panic!("the table is an array");
    };
    let scanner = Scanner::new().expect("every built-in pattern compiles");
    let config = Config::load(Some(CONFIG)).config;

    let got: Vec<(String, Option<String>, Option<String>)> = cases
        .iter()
        .map(|case| {
            let decision = decide(case, &scanner, &config);
            let name = case.get("name").and_then(JsValue::as_str).unwrap_or("");
            (
                name.to_owned(),
                decision.outcome.to_json(),
                decision.telemetry.as_ref().map(detail),
            )
        })
        .collect();
    let want: Vec<(String, Option<String>, Option<String>)> = cases
        .iter()
        .map(|case| {
            let text = |key: &str| case.get(key).and_then(JsValue::as_str).map(str::to_owned);
            (
                text("name").unwrap_or_default(),
                text("json"),
                text("telemetry"),
            )
        })
        .collect();

    assert_eq!(got.len(), 25, "every case of the table");
    for (got, want) in got.iter().zip(&want) {
        assert_eq!(got, want, "the answer and the row");
    }
}

/// A manifest deny has its own header and no `#[expect]` hint; the row names each rule.
#[test]
fn a_manifest_deny_lists_its_rules_without_the_expect_hint() {
    let scanner = Scanner::new().expect("every built-in pattern compiles");
    let protected = LazyGlobs::default();
    let deny = "MOD-09 crates/a/Cargo.toml: the package does not inherit the workspace lints.\n    fix: add it";

    let decision = edit_policy(
        EditFacts {
            rel: "crates/a/Cargo.toml",
            tool: EditTool::Write,
            before: "",
            after: "[package]\nname = \"a\"\n",
            protected: &protected,
            target: EditTarget::Manifest(ManifestEdit {
                findings: ManifestFindings {
                    denies: vec![deny.to_owned()],
                    asks: vec!["an ask".to_owned()],
                    advice: Vec::new(),
                },
                dependencies: DependencyCheck::Off,
            }),
        },
        &scanner,
    )
    .expect("no glob to compile");

    assert_eq!(
        decision.outcome,
        HookOutcome::Deny {
            reason: format!(
                "temper: this manifest edit breaks 1 workspace rule(s), so it was not applied. Fix the manifest text:\n{deny}"
            ),
            context: None,
        },
        "hooks.mjs preEdit"
    );
    assert_eq!(
        decision.telemetry,
        Some(TelemetryEvent::DenyManifest {
            file: "crates/a/Cargo.toml".to_owned(),
            rules: vec!["MOD-09".to_owned()],
        }),
        "`denies.map((d) => d.split(' ')[0])`"
    );
}
