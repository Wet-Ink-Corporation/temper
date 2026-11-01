//! 1.x tests 31 on: whole files against the whole table, and the scan properties.

use std::collections::BTreeSet;

use proptest::prelude::*;

use super::{
    ALL_KINDS, full, full_with, has, hits_of, hits_with, lints, msgs_with, options, scan, scanner,
    severity_name,
};
use crate::scan::strategies::token_soup;
use crate::scan::{CodeKind, Hit};

#[test]
fn no_new_rule_hits_on_an_idiomatic_ledger_module() {
    // Condensed from ledger-domain/src/money.rs and ledger-app/src/ledger.rs.
    let src = "\n//! Money and the ledger service.\nuse std::fmt;\nuse std::str::FromStr;\n\n/// An amount of money in cents.\n///\n/// ```\n/// # fn main() -> Result<(), ledger_domain::ParseCentsError> {\n/// let c: ledger_domain::Cents = \"12.34\".parse()?;\n/// assert_eq!(c.get(), 1234);\n/// # Ok(())\n/// # }\n/// ```\n#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]\npub struct Cents(u64);\n\nimpl Cents {\n    /// The amount in cents.\n    #[must_use]\n    pub const fn get(self) -> u64 { self.0 }\n\n    /// Adds, or `None` on overflow.\n    #[must_use]\n    pub const fn checked_add(self, other: Self) -> Option<Self> {\n        match self.0.checked_add(other.0) {\n            Some(v) => Some(Self(v)),\n            None => None,\n        }\n    }\n}\n\nimpl fmt::Display for Cents {\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n        write!(f, \"{}.{:02}\", self.0 / 100, self.0 % 100)\n    }\n}\n\nimpl FromStr for Cents {\n    type Err = ParseCentsError;\n    fn from_str(s: &str) -> Result<Self, Self::Err> {\n        let (units, fraction) = s.split_once('.').unwrap_or((s, \"0\"));\n        let units: u64 = units.parse().map_err(|_| ParseCentsError)?;\n        units.checked_mul(100).and_then(|u| u.checked_add(decimal(fraction)?)).map(Self).ok_or(ParseCentsError)\n    }\n}\n\n/// Runs commands against the store.\npub struct Ledger<S, C> {\n    store: S,\n    clock: C,\n}\n\nimpl<S: EventStore, C: Clock> Ledger<S, C> {\n    /// Builds a ledger.\n    pub fn new(store: S, clock: C) -> Self { Self { store, clock } }\n\n    /// Summarizes an account.\n    ///\n    /// # Errors\n    ///\n    /// Returns the store's error when loading fails.\n    pub fn summary(&self, account: &AccountId) -> Result<AccountSummary, S::Error> {\n        let events = self.store.load(account)?;\n        Ok(AccountSummary::from_events(&events))\n    }\n}\n";
    let new_keys = [
        "doc-unwrap",
        "empty-err-arm",
        "empty-if-let-err",
        "thread-spawn",
        "compiler-fence",
        "global-allocator",
        "map-index",
        "filter-map-ok",
        "parse-unwrap-or",
        "seqcst",
        "fetch-update",
        "crate-lint-attr",
        "pub-glob-reexport",
        "prelude-mod",
        "pub-smart-ptr",
        "struct-bounds",
        "ops-impl",
        "type-alias-primitive",
        "get-prefix",
        "doc-arg-sections",
        "mutex-unit",
        "detached-spawn",
    ];

    let report = scan(src, CodeKind::Prod);
    let hits: Vec<&Hit> = report
        .hits()
        .iter()
        .filter(|h| new_keys.contains(&h.key().as_str()))
        .collect();

    assert_eq!(hits, Vec::<&Hit>::new(), "no 1.1 rule fires");
}

#[test]
fn commented_out_code_heuristic_ignores_prose() {
    let src = "// struct variants. Closed carries no data, so it is encoded as Closed {}.\n// for each account we replay the events in order.\n// let x = compute(y);\nfn a() {}\n";

    let lines: Vec<usize> = has(src, "commented-code", CodeKind::Prod)
        .iter()
        .map(|h| h.line().get())
        .collect();

    assert_eq!(lines, [3], "only the `let` line is code");
    assert_eq!(
        full(src, CodeKind::Prod),
        ["commented-code H-06 soft@3"],
        "the oracle's whole report"
    );
}

#[test]
fn commented_code_like_1x() {
    let src = "//! let x = 1;\n// let x = f(y).\n/* fn f() {} */\n// }\n// x = y;\n// use a::b\n// match x {\n// if a < b, go on\n/* struct S; */\n//// let a = 1;\n// return;\n/*\n let y = 2;\n*/\n// impl Foo for Bar {}…\n// for x in y { z(); }";

    assert_eq!(
        hits_of(src, "commented-code", CodeKind::Prod),
        [
            "H-06:soft@3",
            "H-06:soft@5",
            "H-06:soft@7",
            "H-06:soft@8",
            "H-06:soft@9",
            "H-06:soft@10",
            "H-06:soft@11",
            "H-06:soft@12",
            "H-06:soft@16",
        ],
        "code-like starts and ends, not doc comments or prose endings"
    );
}

#[test]
fn forbid_override_fires_only_for_a_lint_the_crate_forbids() {
    let src = "#[allow(unsafe_code)]\npub fn f() {}\n#[expect(clippy::unwrap_used, reason = \"checked\")]\npub fn g() {}\n";
    let forbid = lints(&["unsafe_code"]);
    let other = lints(&["clippy::panic"]);
    let key = "forbid-override";

    assert_eq!(
        hits_with(src, key, &options(CodeKind::Prod, Some(&forbid))),
        ["H-05:hard@1"],
        "the forbidden lint's `#[allow]`"
    );
    assert_eq!(
        hits_with(src, key, &options(CodeKind::Test, Some(&forbid))),
        ["H-05:hard@1"],
        "hard in tests too"
    );
    // Near misses: no forbid set, or a forbid set that does not name the lint.
    assert_eq!(
        hits_with(src, key, &options(CodeKind::Prod, None)),
        Vec::<String>::new(),
        "no forbid set"
    );
    assert_eq!(
        hits_with(src, key, &options(CodeKind::Prod, Some(&other))),
        Vec::<String>::new(),
        "another lint is forbidden"
    );
    assert_eq!(
        full_with(src, &options(CodeKind::Prod, Some(&forbid))),
        [
            "allow-attr H-05 hard@1",
            "expect-attr H-05 ask@3",
            "forbid-override H-05 hard@1"
        ],
        "the oracle's whole report"
    );
}

#[test]
fn forbid_override_like_1x() {
    let forbid = lints(&["unsafe_code", "clippy::unwrap_used", "missing_docs"]);
    let src = "#![allow(unsafe_code)]\n#[cfg_attr(x, allow(clippy :: unwrap_used, dead_code))]\n#[expect(missing_docs, reason = \"r\")]\n#[allow(clippy::a::b, Unsafe_Code)]\n#[allow(dead_code, unsafe_code\n)]\n#[allow(unsafe_code, missing_docs)]";
    let msg = |line: u8, attr: &str| {
        format!(
            "{line}:`#[{attr}]` overrides a lint the workspace forbids (E0453 without --cap-lints)"
        )
    };
    let msgs = |src: &str, forbid: &BTreeSet<String>, key: &str| {
        msgs_with(src, key, &options(CodeKind::Prod, Some(forbid)))
    };

    assert_eq!(
        msgs(src, &forbid, "forbid-override"),
        [
            msg(1, "allow(unsafe_code)"),
            msg(2, "allow(clippy::unwrap_used)"),
            msg(3, "expect(missing_docs)"),
            msg(5, "allow(unsafe_code)"),
            msg(7, "allow(unsafe_code)"),
            msg(7, "allow(missing_docs)"),
        ],
        "inner, cfg_attr, spaced and multi-line forms; not a path or a capitalised name"
    );
    assert_eq!(
        msgs(src, &BTreeSet::new(), "forbid-override"),
        Vec::<String>::new(),
        "an empty set finds nothing"
    );
    let expected = "#[expect(unsafe_code, reason = \"r\")]\nfn f() {}";
    assert_eq!(
        (
            hits_with(
                expected,
                "expect-attr",
                &options(CodeKind::Prod, Some(&forbid))
            ),
            hits_with(
                expected,
                "forbid-override",
                &options(CodeKind::Prod, Some(&forbid))
            ),
        ),
        (
            vec!["H-05:ask@1".to_owned()],
            vec!["H-05:hard@1".to_owned()]
        ),
        "a reasoned `#[expect]` of a forbidden lint is still an override"
    );
}

/// 1.x's `ms < 10_000` bound is not ported: a wall-clock assertion breaks TST-10, and the bound
/// moves to Phase 1e's corpus job as a retired assertion (decision 1c-3, wi-825595).
#[test]
fn large_file_seeded_hit_near_the_end_is_reported() {
    let block = [
        "/// Adds two values.",
        "///",
        "/// # Errors",
        "///",
        "/// Returns `None` on overflow.",
        "pub fn add_N(a: u32, b: u32) -> Option<u32> {",
        "    let sum = a.checked_add(b)?;",
        "    if sum > 10 { return Some(sum - 1); }",
        "    Some(sum)",
        "}",
        "",
    ];
    let mut lines: Vec<String> = Vec::new();
    for i in 0.. {
        if lines.len() >= 4990 {
            break;
        }
        let name = format!("add_{i}");
        lines.extend(block.iter().map(|l| l.replacen("add_N", &name, 1)));
    }
    let seed = lines.len() + 2;
    lines.extend(
        [
            "pub fn seeded(v: Option<u8>) -> u8 {",
            "    v.unwrap()",
            "}",
            "",
        ]
        .map(str::to_owned),
    );
    let src = lines.join("\n");

    let hits: Vec<String> = scan(&src, CodeKind::Prod)
        .hits()
        .iter()
        .map(|h| {
            format!(
                "{}:{}:{}@{}",
                h.key(),
                h.id(),
                severity_name(h.severity()),
                h.line()
            )
        })
        .collect();

    assert_eq!(
        hits,
        [format!("unwrap:H-02:hard@{seed}")],
        "only the seeded hit, on its line"
    );
}

fn assert_lines_in_range(
    src: &str,
    forbid: Option<&BTreeSet<String>>,
) -> Result<(), TestCaseError> {
    let lines = src.split('\n').count();
    let scanner = scanner();
    for kind in ALL_KINDS {
        let report = scanner.scan(src, &options(kind, forbid));
        for h in report.hits() {
            prop_assert!(
                (1..=lines).contains(&h.line().get()),
                "line {} outside 1..={}",
                h.line(),
                lines
            );
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn scan_never_panics_and_lines_are_in_range(src in any::<String>()) {
        assert_lines_in_range(&src, None)?;
    }

    #[test]
    fn scan_never_panics_and_lines_are_in_range_on_token_soup(src in token_soup()) {
        assert_lines_in_range(&src, None)?;
    }

    #[test]
    fn scan_with_forbid_never_panics_on_token_soup(src in token_soup()) {
        let forbid = lints(&["unsafe_code", "clippy::unwrap_used"]);
        assert_lines_in_range(&src, Some(&forbid))?;
    }
}
