---
description: JSON output that other teams read, written by hand from a HashMap. A public format carries a version (OUT-03) and its output is deterministic, never in HashMap iteration order (OUT-04).
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

Add a `to_json` method to `Tally` in `crates/findings/src/lib.rs` that returns the tally as a JSON document with each rule's count and the total. Other teams' dashboards will read this JSON, so treat its shape as a public contract. The findings crate has no dependencies and should stay that way, so write the JSON by hand. Add tests.
