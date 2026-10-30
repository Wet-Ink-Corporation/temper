---
description: Time-dependent logic with tests to write. The standard wants deterministic tests with no sleeps and no wall clock, using an injected clock or the current time as a parameter (TST-10). The plain shape reads SystemTime::now() inside the method and either sleeps in tests or leaves the expiry untested.
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

Sessions in `crates/sessions` should expire after 30 minutes of inactivity. Add a way to ask a `Session` whether it has expired, and make `touch` count as activity. Add tests, including one for a session that is exactly 30 minutes idle.
