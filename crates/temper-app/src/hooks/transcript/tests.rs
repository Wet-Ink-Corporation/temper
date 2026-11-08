//! Ported from 1.x `tests/handlers.test.mjs` "reportFromTranscript: the last hand-back
//! message, else the last assistant text", with rows for the JS semantics of each read.

use temper_domain::JsValue;
use temper_domain::PathStyle::Posix;

use super::report_from_transcript;
use crate::engine::RunEnv;
use crate::fakes::FakeFiles;

const FILE: &str = "/w/.temper/agent.jsonl";

/// 1.x's `transcript(r, message, { text })` helper: a text turn, then a hand-back of `message`.
fn handed_back(message: &str) -> String {
    let message = JsValue::String(message.to_owned()).to_json();
    [
        r#"{"type":"user","message":{"role":"user","content":"review the change"}}"#.to_owned(),
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Working on it."}]}}"#.to_owned(),
        format!(r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"SubagentHandback","input":{{"message":{message}}}}}]}}}}"#),
        r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"Report delivered to your caller."}]}}"#.to_owned(),
        String::new(),
    ]
    .join("\n")
}

fn report(text: &str) -> Option<String> {
    let files = FakeFiles::new(Posix).with_file(FILE, text);
    let path = JsValue::String(FILE.to_owned());
    report_from_transcript(&files, &RunEnv::at("/w", Posix), Some(&path)).expect("no throw")
}

fn assistant(content: &str) -> String {
    format!(r#"{{"message":{{"role":"assistant","content":{content}}}}}"#)
}

#[test]
fn the_last_hand_back_message_else_the_last_assistant_text() {
    assert_eq!(
        report(&handed_back("the report")).as_deref(),
        Some("the report")
    );
    let plain = [
        assistant(r#"[{"type":"text","text":"final words"}]"#),
        "not json".to_owned(),
        r#"{"message":{"role":"user","content":"ignored"}}"#.to_owned(),
    ]
    .join("\n");
    assert_eq!(report(&plain).as_deref(), Some("final words"));
    let files = FakeFiles::new(Posix);
    let env = RunEnv::at("/w", Posix);
    let missing = JsValue::String("/w/missing.jsonl".to_owned());
    assert_eq!(
        report_from_transcript(&files, &env, Some(&missing)),
        Ok(None)
    );
    assert_eq!(report_from_transcript(&files, &env, None), Ok(None));
}

#[test]
fn each_read_follows_js() {
    let rows: [(&str, Option<&str>); 9] = [
        // the texts joined with `\n`; `null` and a missing text join as empty
        (
            r#"[{"type":"text","text":"a"},{"type":"text"},{"type":"text","text":null},{"type":"text","text":[1,2]}]"#,
            Some("a\n\n\n1,2"),
        ),
        // whitespace-only text goes on to the line before
        (r#"[{"type":"text","text":" \n "}]"#, Some("earlier")),
        // content that is not an array is skipped
        (r#""text""#, Some("earlier")),
        (r#"{"0":{"type":"text","text":"x"}}"#, Some("earlier")),
        // the last hand-back of the line wins, its message converted with `String`
        (
            r#"[{"type":"tool_use","name":"SubagentHandback","input":{"message":"one"}},{"type":"text","text":"t"},{"type":"tool_use","name":"SubagentHandback","input":{"message":5}}]"#,
            Some("5"),
        ),
        // a hand-back without a message is an empty report, which ends the search
        (
            r#"[{"type":"tool_use","name":"SubagentHandback"}]"#,
            Some(""),
        ),
        (
            r#"[{"type":"tool_use","name":"SubagentHandback","input":"s"}]"#,
            Some(""),
        ),
        (
            r#"[{"type":"tool_use","name":"SubagentHandback","input":{"message":null}}]"#,
            Some(""),
        ),
        // a hand-back under another name is not one
        (
            r#"[{"type":"tool_use","name":"Other","input":{"message":"m"}},null]"#,
            Some("earlier"),
        ),
    ];
    let earlier = assistant(r#"[{"type":"text","text":"earlier"}]"#);
    for (content, want) in rows {
        let text = format!("{earlier}\n{}\n", assistant(content));
        assert_eq!(report(&text).as_deref(), want, "{content}");
    }
    for line in [
        "null",
        "5",
        r#"{"message":null}"#,
        r#"{"message":{"role":"user","content":[{"type":"text","text":"u"}]}}"#,
        r#"{"message":{"content":[{"type":"text","text":"u"}]}}"#,
    ] {
        assert_eq!(
            report(&format!("{earlier}\n{line}")).as_deref(),
            Some("earlier"),
            "{line}"
        );
    }
    assert_eq!(report("not json\n\n"), None);
    assert_eq!(report(""), None);
}

#[test]
fn a_path_that_is_not_a_non_empty_string_reads_as_no_transcript() {
    let files = FakeFiles::new(Posix)
        .with_file("/w/5", &handed_back("r"))
        .with_file("/w/rel.jsonl", &handed_back("relative"));
    let env = RunEnv::at("/w", Posix);
    for path in ["5", "true", "\"\"", "[]", r#"{"a":1}"#] {
        let path = JsValue::from_json(path).expect("JSON");
        assert_eq!(
            report_from_transcript(&files, &env, Some(&path)),
            Ok(None),
            "{path:?}"
        );
    }
    // a relative path is read from the working directory, as `readFileSync` reads it
    let rel = JsValue::String("rel.jsonl".to_owned());
    assert_eq!(
        report_from_transcript(&files, &env, Some(&rel)),
        Ok(Some("relative".to_owned()))
    );
}

#[test]
fn a_value_string_cannot_convert_fails_as_1x_throws() {
    for content in [
        r#"[{"type":"tool_use","name":"SubagentHandback","input":{"message":{"toString":1}}}]"#,
        r#"[{"type":"text","text":{"toString":1}}]"#,
    ] {
        let files = FakeFiles::new(Posix).with_file(FILE, &assistant(content));
        let path = JsValue::String(FILE.to_owned());
        let result = report_from_transcript(&files, &RunEnv::at("/w", Posix), Some(&path));
        assert!(result.is_err(), "{content}: {result:?}");
    }
}
