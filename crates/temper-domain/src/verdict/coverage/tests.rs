use proptest::prelude::*;

use crate::js::Value;

use super::{SuffixIndex, is_abs, norm_path, unreviewed};

/// 1.x's `coveredBy(read, changed)` as written, the oracle for the index.
fn covered_by_1x<'c>(read: &str, changed: &'c [String]) -> Option<&'c str> {
    if let Some(same) = changed.iter().find(|f| *f == read) {
        return Some(same);
    }
    if !is_abs(read) {
        return None;
    }
    let mut tails: Vec<&String> = changed
        .iter()
        .filter(|f| !f.is_empty() && read.ends_with(&format!("/{f}")))
        .collect();
    // A stable sort, longest first, as 1.x's `sort((a, b) => b.length - a.length)`.
    tails.sort_by_key(|f| std::cmp::Reverse(f.len()));
    tails.first().map(|f| f.as_str())
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn the_index_answers_as_1x_on_the_ported_rows() {
    let changed = strings(&["crates/core/src/lib.rs", "src/lib.rs", "build.rs", "a/b.rs"]);
    let index = SuffixIndex::new(&changed);
    let rows: [(&str, Option<&str>); 10] = [
        ("crates/core/src/lib.rs", Some("crates/core/src/lib.rs")),
        ("src/lib.rs", Some("src/lib.rs")),
        ("lib.rs", None),
        ("core/src/lib.rs", None),
        (
            "/repo/crates/core/src/lib.rs",
            Some("crates/core/src/lib.rs"),
        ),
        ("/repo/src/lib.rs", Some("src/lib.rs")),
        ("C:/repo/crates/core/build.rs", Some("build.rs")),
        ("/xa/b.rs", None),
        ("/a/b.rs", Some("a/b.rs")),
        ("//a/b.rs", Some("a/b.rs")),
    ];
    for (read, expected) in rows {
        assert_eq!(index.covers(read), expected, "{read:?}");
        assert_eq!(
            covered_by_1x(read, &changed),
            expected,
            "oracle on {read:?}"
        );
    }
}

#[test]
fn an_empty_changed_path_covers_only_an_empty_entry() {
    let changed = strings(&["", "x.rs"]);
    let index = SuffixIndex::new(&changed);
    assert_eq!(index.covers(""), Some(""));
    assert_eq!(index.covers("/"), None);
    assert_eq!(index.covers("/x.rs"), Some("x.rs"));
}

/// A path drawn from separators, dots, a drive letter and two names, so suffixes collide.
fn path() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("a"),
        Just("b.rs"),
        Just("/"),
        Just("\\"),
        Just("./"),
        Just("C:"),
        Just(" "),
        Just("//"),
    ];
    prop::collection::vec(piece, 0..7).prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn the_index_answers_as_1x_coveredby_on_random_path_sets(
        changed in prop::collection::vec(path(), 0..8),
        reads in prop::collection::vec(path(), 0..8),
    ) {
        let normalised: Vec<String> = changed.iter().map(|f| norm_path(f)).collect();
        let index = SuffixIndex::new(&normalised);
        for read in &reads {
            let read = norm_path(read);
            prop_assert_eq!(index.covers(&read), covered_by_1x(&read, &normalised), "{:?}", read);
        }
    }
}

#[test]
fn many_long_entries_against_many_changed_files_stay_linear() {
    // P1: 2^16 entries against 2^12 changed paths, and 16 entries of 10^5 separators.
    let changed: Vec<String> = (0..1 << 12)
        .map(|i| format!("crates/c{i}/src/lib.rs"))
        .collect();
    let mut reads: Vec<Value> = (0..1 << 16)
        .map(|i| Value::String(format!("/home/dev/p{i}/crates/c{}/src/lib.rs", i % 4000)))
        .collect();
    let deep = format!("/{}crates/c1/src/lib.rs", "/".repeat(100_000));
    reads.extend((0..16).map(|_| Value::String(deep.clone())));
    let missed = unreviewed(&changed, &reads).expect("plain string entries");
    let expected: Vec<String> = (4000..1 << 12)
        .map(|i| format!("crates/c{i}/src/lib.rs"))
        .collect();
    assert_eq!(missed, expected);
}
