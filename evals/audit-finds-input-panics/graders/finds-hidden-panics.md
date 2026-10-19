---
type: llm
# H-03, H-04: the panics on input that no default lint reports. The obvious
# `.unwrap()` on `total` does not count toward these.
focus: { source: file, path: AUDIT.md }
---

PASS if the audit names all three of these as able to panic on an upload, each tied to its line or expression in `parse_upload`: (1) `headers["kind"]`, the `HashMap` index, which panics when the upload has no `kind` header; (2) `total / count`, the integer division, which panics when `count` is missing, `0` or not a number; (3) `Duration::from_secs_f64(retry)`, which panics on a negative, NaN, infinite or overly large `retry`.
FAIL if any of the three is missing, or is described only as style or as an unlikely edge case without saying the upload can make it panic. Reporting the `.unwrap()` on `total` does not stand in for any of them.
