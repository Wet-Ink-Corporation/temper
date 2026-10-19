---
type: llm
# PROC-03, TST-01: the order matters, so the judge reads the trace, not the
# final message. A plan written by a subagent counts when the trace shows it
# (its Write call, or the plan path in the subagent's result) before code.
focus: trace
---

PASS if, before the first Write or Edit of any `.rs` file (by the agent or by any subagent it started), a plan was written to a file under `.temper/plans/`, and that plan lists the tests to write (a test list of behaviours, such as the 61st request in a window being rejected, keys being independent, or a rejected request not counting). When a subagent wrote the plan and its own tool calls are not in the trace, its result counts if it was returned before the first `.rs` edit and reports a plan file under `.temper/plans/` with its tests (or the plan file's contents, read later in the trace, list them).
FAIL if no plan file under `.temper/plans/` was written, if the first plan file was written only after a `.rs` file had been edited, or if the plan has no list of tests.
