---
description: A new type and module (a per-key rate limit over a rolling minute), where the request does not ask for a plan. The standard wants a written plan with a test list under .temper/plans/ before any code (PROC-03), with the rust-architect for a change this size. The repository's CLAUDE.md names the plans folder, so both arms know where a plan would go.
max_turns: 60
timeout_seconds: 1500
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

Our partners keep hammering the gateway. Add rate limiting: each API key may make at most 60 requests in any rolling 60-second window, and `handle` answers `Status::TooManyRequests` for a request over the limit. A rejected request doesn't count against the key. Keys must not affect each other, and the gateway's memory must not grow with keys that have gone quiet. Add tests.
