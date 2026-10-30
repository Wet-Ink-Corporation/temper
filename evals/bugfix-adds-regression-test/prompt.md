---
description: A reported off-by-one bug with no test that covers it. The standard wants a regression test that fails before the fix (TST-01), asserting the exact value the user reported (TST-05). The plain shape fixes the arithmetic and stops.
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

A user with 25 orders and 10 orders per page only ever sees 2 pages in the order history, so their last 5 orders are unreachable. Please fix it in `crates/paging`.
