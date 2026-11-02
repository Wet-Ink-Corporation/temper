use std::borrow::Cow;

use super::{EditOp, EditStep, Replacements, apply_edit};

fn step<'a>(old: Option<&'a str>, new: &'a str, count: Replacements) -> EditStep<'a> {
    EditStep {
        old,
        new: Cow::Borrowed(new),
        count,
    }
}

fn edit<'a>(old: &'a str, new: &'a str) -> EditOp<'a> {
    EditOp::Replace(vec![step(Some(old), new, Replacements::First)])
}

fn edit_all<'a>(old: &'a str, new: &'a str) -> EditOp<'a> {
    EditOp::Replace(vec![step(Some(old), new, Replacements::All)])
}

/// 1.x `applyEdit`, printed by Node 22 for the same inputs.
#[test]
fn apply_edit_follows_1x_replace_and_split_join() {
    let cases: [(&str, EditOp<'_>); 12] = [
        ("x", EditOp::Write { content: "new" }),
        ("x", EditOp::Write { content: "" }),
        ("a b a", edit("a", "$&c")),
        ("a b a", edit_all("a", "c")),
        (
            "one two",
            EditOp::Replace(vec![
                step(Some("one"), "1", Replacements::First),
                step(Some("1 two"), "12", Replacements::First),
            ]),
        ),
        (
            "one",
            EditOp::Replace(vec![
                step(Some("one"), "1", Replacements::First),
                step(Some("one"), "x", Replacements::First),
            ]),
        ),
        ("abc", edit("", "-")),
        ("abc", edit_all("", "-")),
        ("", edit_all("", "-")),
        ("abc", edit("zz", "-")),
        (
            "abc",
            EditOp::Replace(vec![step(None, "-", Replacements::First)]),
        ),
        ("aaa", edit_all("aa", "b")),
    ];

    let got: Vec<Option<String>> = cases
        .iter()
        .map(|(before, op)| apply_edit(before, op))
        .collect();

    let want = [
        Some("new"),
        Some(""),
        Some("$&c b a"),
        Some("c b c"),
        Some("12"),
        None,
        Some("-abc"),
        Some("a-b-c"),
        Some(""),
        None,
        None,
        Some("ba"),
    ]
    .map(|text| text.map(str::to_owned));
    assert_eq!(
        got, want,
        "the replacement is literal; each step sees the one before"
    );
}

/// OC-3 (o): an empty `old_string` with `replace_all` joins per code point, where JS's
/// `split('')` would split a surrogate pair.
#[test]
fn an_empty_old_string_with_replace_all_never_splits_a_char() {
    let got = apply_edit("a😀b", &edit_all("", "-"));

    assert_eq!(got.as_deref(), Some("a-😀-b"), "D16(o)");
}

#[test]
fn an_edit_with_no_steps_leaves_the_text() {
    let got = apply_edit("abc", &EditOp::Replace(Vec::new()));

    assert_eq!(got.as_deref(), Some("abc"), "`for (const e of [])`");
}
