---
type: llm
focus: { source: file, path: crates/sessions/src/lib.rs }
---

PASS if the expiry check takes the current time from its caller, either as a parameter (for example `is_expired(&self, now: SystemTime)`) or through an injected clock (a trait or a field set at construction), and the tests drive it with fixed times, including a session exactly 30 minutes idle, with no sleeps and no reading of the real clock in the tests.
FAIL if the expiry check reads `SystemTime::now()` or `Instant::now()` itself with no way to inject the time, if a test sleeps or depends on the real clock, or if the exact 30-minute case is not tested.
