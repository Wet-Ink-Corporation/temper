use super::toml;
use crate::manifest::{
    DepKind, DepName, dep_names, dep_requirements, dependency_names, major_of,
    non_caret_requirement,
};

fn names(list: &[DepName]) -> Vec<String> {
    list.iter().map(DepName::to_string).collect()
}

#[test]
fn non_caret_requirements() {
    for ok in ["1", "1.4", "^1.4.2", "0.3", "1.4.2"] {
        assert_eq!(non_caret_requirement(ok), None, "{ok}");
    }
    // The texts of 1.x's `nonCaretRequirement`.
    let cases = [
        ("=1.4.2", "an exact `=` pin"),
        ("~1.4", "a tilde `~` requirement"),
        ("*", "a wildcard `*`"),
        ("1.*", "a wildcard `*`"),
        (">=1.2, <1.5", "an upper bound `<`"),
        (">=1.2", "an open-ended `>=`"),
    ];
    for (req, why) in cases {
        assert_eq!(non_caret_requirement(req), Some(why), "{req}");
    }
    let reqs = dep_requirements(&toml(
        "[workspace.dependencies]\na = \"1\"\nb = { version = \"=2\" }\nc = { path = \"../c\" }\n[target.x.dev-dependencies]\nd = \"~3\"\n",
    ));
    let pairs: Vec<_> = reqs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(
        pairs,
        [
            ("workspace.dependencies:a", "1"),
            ("workspace.dependencies:b", "=2"),
            ("target.x.dev-dependencies:d", "~3")
        ]
    );
}

#[test]
fn non_caret_requirement_ignores_empty_parts() {
    assert_eq!(non_caret_requirement(""), None, "no parts");
    assert_eq!(non_caret_requirement(" , "), None, "only blank parts");
    assert_eq!(
        non_caret_requirement(">=1, 2"),
        None,
        "a caret part beside a floor"
    );
}

#[test]
fn major_versions() {
    let majors: Vec<_> = ["1", "^1.4", "=1.0.3", "2", "0.8", "0.9.1", "~0.0.3", "*"]
        .into_iter()
        .map(major_of)
        .collect();
    let want = [
        Some("1"),
        Some("1"),
        Some("1"),
        Some("2"),
        Some("0.8"),
        Some("0.9"),
        Some("0.0.3"),
        None,
    ];
    assert_eq!(majors, want.map(|m| m.map(str::to_owned)));
    assert_eq!(major_of("0"), Some("0".to_owned()), "a bare 0");
    assert_eq!(
        major_of("0.0"),
        Some("0.0".to_owned()),
        "0.0 without a patch"
    );
    assert_eq!(
        major_of("v12.x"),
        Some("12".to_owned()),
        "the first run of digits"
    );
}

#[test]
fn dependency_names_in_document_order() {
    let m = toml(
        "[dependencies]\nnix.workspace = true\n[dev-dependencies]\nproptest = \"1\"\n[[bench]]\nname = \"b\"\nharness = false\n[target.'cfg(unix)'.build-dependencies]\ncc = \"1\"\n[workspace.dependencies]\nserde = \"1\"\n",
    );
    let mut sorted = names(&dependency_names(&m));
    sorted.sort();
    assert_eq!(
        sorted,
        [
            "build-dependencies:cc",
            "dependencies:nix",
            "dependencies:serde",
            "dev-dependencies:proptest"
        ]
    );
    // 1.x's Set order: the three tables, then workspace.dependencies, then each target.
    assert_eq!(
        names(&dependency_names(&m)),
        [
            "dependencies:nix",
            "dev-dependencies:proptest",
            "dependencies:serde",
            "build-dependencies:cc"
        ]
    );
    let twice = toml("[dependencies]\na = \"1\"\n[workspace.dependencies]\na = \"1\"\nb = \"1\"\n");
    assert_eq!(
        names(&dependency_names(&twice)),
        ["dependencies:a", "dependencies:b"],
        "a set: each name once"
    );
}

#[test]
fn dependency_names_read_arrays_as_index_keyed_objects() {
    // 1.x: Object.keys of an array gives its indices, and Object.values(target) its items.
    let m = toml("dependencies = [\"x\"]\ntarget = [{ dependencies = { y = \"1\" } }]\n");
    assert_eq!(
        names(&dependency_names(&m)),
        ["dependencies:0", "dependencies:y"]
    );
    assert_eq!(
        dependency_names(&toml("dependencies = \"x\"\n")),
        [],
        "a string is not a table"
    );
}

#[test]
fn a_proto_dependency_is_not_seen_as_in_1x() {
    // 1.x's `obj["__proto__"] = v` sets the prototype, so the key never becomes a dependency.
    let m = toml("[dependencies]\n__proto__ = \"1\"\nserde = \"1\"\n");
    assert_eq!(names(&dependency_names(&m)), ["dependencies:serde"]);
    let reqs: Vec<_> = dep_requirements(&m).into_iter().map(|(k, _)| k).collect();
    assert_eq!(reqs, ["dependencies:serde"]);
}

#[test]
fn dep_names_falls_back_to_lines_for_invalid_toml() {
    let text = "[package]\nname = \"a\n[dependencies]\nserde = \"1\"\n  tokio = { version = \"1\" }\n[dev-dependencies.insta]\nversion = \"1\"\n[target.'cfg(unix)'.build-dependencies]\ncc = \"1\"\n[features]\nx = []\n";
    assert_eq!(
        names(&dep_names(text)),
        [
            "dependencies:serde",
            "dependencies:tokio",
            "dev-dependencies:insta",
            "build-dependencies:cc"
        ]
    );
    let valid = dep_names("[dependencies]\nserde = \"1\"\n");
    assert_eq!(
        valid,
        [DepName {
            kind: DepKind::Normal,
            name: "serde".to_owned()
        }]
    );
}

#[test]
fn the_line_fallback_reads_headers_as_1x_does() {
    // `[[bin]]` is no header to 1.x's `^\s*\[([^\]]+)\]\s*$`, so the table stays open; a
    // quoted name is not `[\w-]+`; CRLF and JS whitespace are trimmed.
    // Each expectation was printed by 1.x's `depNames` on the same text.
    let text = "[dependencies\n[dependencies]\r\na = 1\n\u{A0}d\u{3000}= 4\n[[bin]]\nb = 2\n[dependencies.\"q\"]\nc = 3\n";
    assert_eq!(
        names(&dep_names(text)),
        ["dependencies:a", "dependencies:d", "dependencies:b"]
    );
}
