//! Hook input of unexpected shapes, read as 1.x's `preEdit` reads it (printed by Node 22 on a
//! one-crate workspace): what passes, what is decided, and what fails as 1.x throws (the text
//! after `failed: ` is the Rust error's, OC-3 (p)).

use super::{Ws, decision, files, reason};
use crate::engine::HookError;

const LIB: &str = "/ws/crates/ledger-domain/src/lib.rs";
/// V8's text when `lintSections` or `depNamesByLine` splits manifest content that is not a
/// string (`for (const line of toml.split(..))`).
const MANIFEST_NOT_TEXT: &str = "toml.split is not a function or its return value is not iterable";

fn run(ws: &Ws, tool: &str, tool_input: &str) -> Result<&'static str, String> {
    let json = format!(
        r#"{{"tool_name":{tool},"cwd":"/ws","agent_type":"rust-implementer","tool_input":{tool_input}}}"#
    );
    ws.hook(&json)
        .map(|outcome| decision(&outcome))
        .map_err(|e| e.to_string())
}

#[test]
fn null_input_fails_as_v8_does_on_the_first_read() {
    let ws = Ws::new(files());

    let got = ws.hook("null").map_err(|e| e.to_string());

    assert_eq!(
        got,
        Err("Cannot read properties of null (reading 'tool_name')".to_owned()),
        "2-22"
    );
}

#[test]
fn a_missing_or_falsy_file_path_and_a_file_outside_the_project_pass() {
    let ws = Ws::new(files());

    let got = [
        "{}",
        r#"{"file_path":""}"#,
        r#"{"file_path":0}"#,
        r#"{"file_path":false}"#,
        r#"{"file_path":null}"#,
        r#""/ws/crates/ledger-domain/src/lib.rs""#,
        "[1]",
        r#"{"file_path":"/elsewhere/x.rs","content":"fn f() { dbg!(1); }"}"#,
    ]
    .map(|ti| run(&ws, r#""Write""#, ti));

    let want: [Result<&str, String>; 8] = std::array::from_fn(|_| Ok("none"));
    assert_eq!(got, want, "`if (!file)`, and `rel.startsWith('..')`");
}

#[test]
fn a_relative_file_path_is_resolved_against_the_working_directory() {
    let ws = Ws::new(files());

    let got = run(
        &ws,
        r#""Write""#,
        r#"{"file_path":"crates/ledger-domain/src/lib.rs","content":"fn f() { dbg!(1); }"}"#,
    );

    assert_eq!(
        got,
        Ok("deny"),
        "`path.resolve(file)` from the process's cwd"
    );
}

#[test]
fn a_file_path_that_is_not_a_string_fails() {
    let ws = Ws::new(files());

    let got = [
        r#"{"file_path":5}"#,
        r#"{"file_path":{}}"#,
        r#"{"file_path":true}"#,
    ]
    .map(|ti| run(&ws, r#""Write""#, ti));

    // V8 goes on with `. Received type number (5)`; the value is left out (D16(p)).
    let want: [Result<&str, String>; 3] =
        std::array::from_fn(
            |_| Err(r#"The "paths[0]" argument must be of type string"#.to_owned()),
        );
    assert_eq!(got, want, "`path.resolve` throws a TypeError");
}

/// 1.x applies the edit, then throws where the text is first read as a string: the manifest
/// checks or the scan. Other files never read it.
#[test]
fn written_content_that_is_not_a_string_fails_only_where_1x_reads_it() {
    let ws = Ws::new(files());
    let content = |file: &str, content: &str| {
        run(
            &ws,
            r#""Write""#,
            &format!(r#"{{"file_path":"{file}","content":{content}}}"#),
        )
    };

    let got = [
        content(LIB, "5"),
        content(LIB, "{}"),
        content("/ws/crates/ledger-domain/Cargo.toml", "5"),
        content("/ws/notes.md", "5"),
        content(LIB, "null"),
    ];

    let scan = || Err("src.split is not a function".to_owned());
    assert_eq!(
        got,
        [
            scan(),
            scan(),
            Err(MANIFEST_NOT_TEXT.to_owned()),
            Ok("none"),
            Ok("ask"),
        ],
        "`ti.content ?? ''`; an empty lib.rs removes its test"
    );
}

/// 1.x's `parseToml` reads `String(text ?? '')`, so written content that is not a string is
/// checked as its JS string when that, and a non-empty file before it, parse; otherwise the
/// line-based fallback throws on the value.
#[test]
fn manifest_content_that_is_not_a_string_is_checked_as_its_js_string() {
    let ws = Ws::new(files().with_file("/ws/crates/bad/Cargo.toml", "[package\n"));
    let content = |file: &str, content: &str| {
        let json = format!(
            r#"{{"tool_name":"Write","cwd":"/ws","agent_type":"rust-implementer","tool_input":{{"file_path":"{file}","content":{content}}}}}"#
        );
        ws.hook(&json).map_err(|e| e.to_string())
    };
    let new = "/ws/crates/newc/Cargo.toml";
    let member = "/ws/crates/ledger-domain/Cargo.toml";

    let decided = [
        content(new, "[]"),
        content(new, r#"["a = 1"]"#),
        content(new, r#"["[package]\nname='x'\n[lints]\nworkspace = true"]"#),
        content(member, "[]"),
    ]
    .map(|got| got.map(|outcome| (decision(&outcome), reason(&outcome))));
    let failed = [
        content(new, r#"[["[package]"],"x=1"]"#),
        content(new, "true"),
        content(member, "{}"),
        content("/ws/crates/bad/Cargo.toml", r#"["a = 1"]"#),
    ];

    let changes = |key: &str| {
        format!(
            "- crates/ledger-domain/Cargo.toml: `package.{key}` changes ({{\"workspace\":true}} -> unset); MSRV, edition and resolver changes are human decisions (H-16)."
        )
    };
    assert_eq!(
        decided,
        [
            Ok(("none", "\n".to_owned())),
            Ok(("none", "\n".to_owned())),
            Ok((
                "ask",
                "temper needs your approval:\n- crates/newc/Cargo.toml: new crate `x` in the workspace. Crate boundaries are a human decision (H-16, ARC-07), and the gate fails until the crate has its charter (`AGENTS.md`, ARC-08).\n".to_owned()
            )),
            Ok((
                "ask",
                format!(
                    "temper needs your approval:\n{}\n{}\n",
                    changes("rust-version"),
                    changes("edition")
                )
            )),
        ],
        "printed by 1.x's preEdit on the same tree"
    );
    let failure = || Err(MANIFEST_NOT_TEXT.to_owned());
    assert_eq!(
        failed,
        [failure(), failure(), failure(), failure()],
        "`String([['[package]'], 'x=1'])` is `[package],x=1`; `lintSections` splits the value"
    );
}

#[test]
fn edits_follow_js_iteration() {
    let ws = Ws::new(files());
    let multi = |edits: &str| {
        run(
            &ws,
            r#""MultiEdit""#,
            &format!(r#"{{"file_path":"{LIB}","edits":{edits}}}"#),
        )
    };

    // An error as whether it says `text`.
    let says = |edits: &str, text: &str| multi(edits).map_err(|e| e.contains(text));
    let null_step = "reading 'old_string'";

    let got = [
        says("{}", "not iterable"),
        says("5", "not iterable"),
        says(r#""ab""#, ""),
        says(r#""""#, ""),
        says("[null]", null_step),
        says(r#"[{"old_string":"zz"},null]"#, ""),
        says(
            r#"[{"old_string":"//! Domain.","new_string":"//! D."},null]"#,
            null_step,
        ),
        says(r#"[5, "x"]"#, ""),
        says("null", ""),
    ];

    assert_eq!(
        got,
        [
            Err(true),
            Err(true),
            Ok("none"),
            Ok("none"),
            Err(true),
            Ok("none"),
            Err(true),
            Ok("none"),
            Ok("none"),
        ],
        "`for (const e of edits)`: a string iterates its chars; a failed step ends the loop before a null"
    );
}

#[test]
fn new_string_is_coerced_and_any_other_tool_name_is_an_edit() {
    let ws = Ws::new(files());
    let edit = |tool: &str, new: &str| {
        run(
            &ws,
            tool,
            &format!(r#"{{"file_path":"{LIB}","old_string":"/// One.","new_string":{new}}}"#),
        )
    };

    let got = [
        edit(r#""Edit""#, "5"),
        edit(r#""Edit""#, r#""/// One.\nfn f() { dbg!(1); }""#),
        edit(r#""Foo""#, r#""/// One.\nfn f() { dbg!(1); }""#),
        edit("null", r#""/// One.\nfn f() { dbg!(1); }""#),
        edit(r#""Edit""#, "null"),
    ];

    assert_eq!(
        got,
        [Ok("none"), Ok("deny"), Ok("deny"), Ok("deny"), Ok("none")],
        "`String(e.new_string ?? '')`; `tool === 'Write'` and `tool === 'MultiEdit'` only"
    );
}

#[test]
fn a_glob_that_does_not_compile_is_a_handler_error() {
    // `regex`'s default size limit is about 10 MB; a glob of `?`s far past it does not compile.
    let protected = "?".repeat(1 << 20);
    let config = format!(r#"{{"protected":["{protected}"]}}"#);
    let ws = Ws::new(files().with_file("/ws/.claude/temper.json", &config));

    let got = ws.hook(r#"{"tool_name":"Write","cwd":"/ws","agent_type":"rust-implementer","tool_input":{"file_path":"/ws/notes.md","content":"x"}}"#);

    assert!(matches!(got, Err(HookError::Glob(_))), "{got:?}");
}
