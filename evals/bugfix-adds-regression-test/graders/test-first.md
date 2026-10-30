---
type: llm
focus: { source: file, path: crates/paging/src/lib.rs }
---

PASS if the change adds at least one test for a partial last page (a total that is not a multiple of the page size, such as 25 at 10 per page), and that test asserts the exact page count with `assert_eq!`, not only that a value is greater than some bound.
FAIL if the arithmetic is fixed with no new test for a partial page, if the new test asserts only an inequality or `is_some()`, or if an existing test's expected value was changed to match the new behaviour.
