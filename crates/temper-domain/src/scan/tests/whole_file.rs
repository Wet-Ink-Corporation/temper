//! 1.x tests 31 on: whole files against the whole table, and the scan properties.

use proptest::prelude::*;

use super::{ALL_KINDS, scan, scanner};
use crate::scan::strategies::token_soup;
use crate::scan::{CodeKind, FileRole, Hit, ScanOptions};

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

fn assert_lines_in_range(src: &str) -> Result<(), TestCaseError> {
    let lines = src.split('\n').count();
    let scanner = scanner();
    for kind in ALL_KINDS {
        let report = scanner.scan(
            src,
            &ScanOptions {
                kind,
                file: FileRole::Source,
            },
        );
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
        assert_lines_in_range(&src)?;
    }

    #[test]
    fn scan_never_panics_and_lines_are_in_range_on_token_soup(src in token_soup()) {
        assert_lines_in_range(&src)?;
    }
}
