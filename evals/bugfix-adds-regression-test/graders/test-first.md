---
type: llm
# TST-01: the order matters, so the judge reads the trace, not the final file.
focus: trace
---

PASS if, before the line in `page_count` that does the division was changed, the agent (or a subagent it started) added a test for a partial last page (a total that is not a multiple of the page size, such as 25 at 10 per page), and the trace shows that test run and failing before the fix, then passing after it. When a subagent made the change and its own tool calls are not in the trace, its result counts if it reports adding that test, seeing it fail before the fix and pass after it.
FAIL if the division was changed before any such test existed, if the new test was never run before the fix, or if an existing test's expected value was changed to match the new behaviour.
