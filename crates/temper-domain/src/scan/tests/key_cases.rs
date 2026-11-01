//! The 1.x `KEY_CASES` table (`tests/scan.test.mjs`): per key, positive sources with the hits 1.x
//! reports, and near misses that must not fire, in 1.x order over three modules. A failure here
//! is a change in scanner output.

mod escape_hatches;
mod state_and_time;
mod tests_and_finders;

use super::hits_of;
use crate::scan::CodeKind;

/// Each source's hits for `key` equal the listed `id:severity@line`s.
fn assert_hits(key: &str, cases: &[(CodeKind, &str, &[&str])]) {
    for &(kind, src, want) in cases {
        assert_eq!(hits_of(src, key, kind), want, "{kind:?}: {src}");
    }
}

/// No source has a hit for `key`.
fn assert_misses(key: &str, cases: &[(CodeKind, &str)]) {
    for &(kind, src) in cases {
        assert_eq!(
            hits_of(src, key, kind),
            Vec::<String>::new(),
            "{kind:?}: {src}"
        );
    }
}
