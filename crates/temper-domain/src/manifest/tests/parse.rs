use super::{at, text, toml};
use crate::js::{Object, Value, js_json};
use crate::manifest::Manifest;

/// The manifest of 1.x's "parses the Cargo manifest shapes temper reads".
const SHAPES: &str = r#"# comment
[package]
name = "ledger-cli" # trailing comment
edition.workspace = true
"quoted.key" = 'lit#eral'

[lints]
workspace = true

[[bin]]
name = "ledger"
path = "src/main.rs"

[[bin]]
name = "tool"
test = false

[target.'cfg(unix)'.dependencies]
nix = { version = "0.29", default-features = false, features = [
  "fs",   # files
  "user",
] }

[profile.release.package."*"]
opt-level = 3

[features]
default = []
desc = """
two \
  lines"""
raw = '''a
b'''
n = 0x1F
f = 1_000.5
d = 1979-05-27 07:32:00Z
"#;

#[test]
fn parses_the_cargo_manifest_shapes_temper_reads() {
    let m = toml(SHAPES);
    let inherit: Object = [("workspace".to_owned(), Value::Bool(true))]
        .into_iter()
        .collect();
    let Some(Value::Array(bins)) = m.get("bin") else {
        panic!("[[bin]] is an array")
    };
    let names: Vec<_> = bins
        .iter()
        .map(|b| b.get("name").and_then(Value::as_str))
        .collect();
    assert_eq!(names, [Some("ledger"), Some("tool")]);
    assert_eq!(
        bins.get(1).and_then(|b| b.get("test")),
        Some(&Value::Bool(false))
    );
    let cases: [(&[&str], Value); 12] = [
        (&["package", "name"], text("ledger-cli")),
        (&["package", "edition"], Value::Object(inherit)),
        (&["package", "quoted.key"], text("lit#eral")),
        (&["lints", "workspace"], Value::Bool(true)),
        (
            &["target", "cfg(unix)", "dependencies", "nix", "features"],
            Value::Array(vec![text("fs"), text("user")]),
        ),
        (
            &["profile", "release", "package", "*", "opt-level"],
            Value::Number(3.0),
        ),
        (&["features", "default"], Value::Array(vec![])),
        (&["features", "desc"], text("two lines")),
        (&["features", "raw"], text("a\nb")),
        (&["features", "n"], Value::Number(31.0)),
        (&["features", "f"], Value::Number(1000.5)),
        // `toml.mjs` keeps a date or time as its source text; so does the port.
        (&["features", "d"], text("1979-05-27 07:32:00Z")),
    ];
    for (path, want) in &cases {
        assert_eq!(at(&m, path), Some(want), "{path:?}");
    }
    assert_eq!(at(&m, &["features", "missing"]), None, "an absent key");
}

#[test]
fn crlf_escapes_and_invalid_input() {
    let m = toml("a = \"x\\ty\\u00e9\"\r\n[b]\r\nc = [1,\r\n 2]\r\n");
    assert_eq!(
        js_json(&Value::Object(m.0)),
        r#"{"a":"x\tyé","b":{"c":[1,2]}}"#
    );
    for invalid in ["a = ", "[x\nb = 1", "a = \"unterminated\n", "a = 1 b = 2"] {
        assert!(
            invalid.parse::<Manifest>().is_err(),
            "{invalid:?} is not valid TOML"
        );
    }
}

#[test]
fn the_template_and_example_manifests_parse() {
    let manifests = [
        (
            "templates/workspace",
            include_str!("../../../../../templates/workspace/Cargo.toml"),
        ),
        (
            "templates/example-workspace",
            include_str!("../../../../../templates/example-workspace/Cargo.toml"),
        ),
    ];
    for (rel, text) in manifests {
        let m = toml(text);
        assert!(
            at(&m, &["workspace", "lints"]).is_some(),
            "{rel} has [workspace.lints]"
        );
    }
}

#[test]
fn values_keep_the_js_model_of_toml_mjs() {
    // Each value as `JSON.stringify(parseToml(..))` prints it in Node 22.
    let cases = [
        ("a = 9007199254740993", r#"{"a":9007199254740992}"#),
        ("a = 0o17\nb = 0b101\nc = -0", r#"{"a":15,"b":5,"c":0}"#),
        (
            "a = inf\nb = -inf\nc = nan\nd = -0.0",
            r#"{"a":null,"b":null,"c":null,"d":0}"#,
        ),
        ("a = 1e400", r#"{"a":null}"#),
        (
            "a = 1979-05-27T07:32:00.5-07:00\nb = 07:32:00\nc = 1979-05-27",
            r#"{"a":"1979-05-27T07:32:00.5-07:00","b":"07:32:00","c":"1979-05-27"}"#,
        ),
        (
            "[t]\nb = 1\n\"2\" = 2\na = 3\n\"1\" = 4",
            r#"{"t":{"1":4,"2":2,"b":1,"a":3}}"#,
        ),
        ("\u{FEFF}a = 1", r#"{"a":1}"#),
        // `toml.mjs` assigns `obj["__proto__"] = v`, which never makes an own key.
        (
            "[package]\nbuild = { __proto__ = 1 }",
            r#"{"package":{"build":{}}}"#,
        ),
        (
            "[package.build]\n__proto__ = { x = 1 }",
            r#"{"package":{"build":{}}}"#,
        ),
    ];
    for (source, json) in cases {
        assert_eq!(js_json(&Value::Object(toml(source).0)), json, "{source:?}");
    }
}

#[test]
fn a_parse_error_says_the_manifest_is_not_valid_toml() {
    let err = "a = ".parse::<Manifest>().expect_err("`a = ` has no value");
    assert_eq!(err.to_string(), "the manifest is not valid TOML");
    assert!(
        std::error::Error::source(&err).is_some(),
        "the toml error is the source"
    );
}
