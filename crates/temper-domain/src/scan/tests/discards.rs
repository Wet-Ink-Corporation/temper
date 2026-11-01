//! 1.x tests 16 to 18: silently discarded errors and the ignore count.

use super::{IGNORE_SRC, scan};
use crate::scan::CodeKind;

#[test]
fn scan_reports_ignore_count() {
    assert_eq!(
        scan(IGNORE_SRC, CodeKind::Test).ignores(),
        3,
        "cfg_attr(miri, ignore) is not counted"
    );
}
