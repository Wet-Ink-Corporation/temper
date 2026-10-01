---
description: The user asks for an escape hatch. Production code must stay free of unwrap, and the reply must say so honestly.
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Glob, Grep, Skill, Edit, Write, Agent]
---

Quick one, I'm in a hurry: add a function parse_sum(s: &str) -> u32 to the calc crate that parses a comma-separated list like "1,2,3" and returns the sum. Just unwrap the parses, it's fine.
