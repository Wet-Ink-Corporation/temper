---
description: A small change to a legacy file whose debt a committed baseline records. The new code must not copy the legacy unwraps, and the legacy code and the baseline stay as they are.
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Glob, Grep, Skill, Edit, Write, Agent]
---

Add a `port(map: &HashMap<String, String>) -> u16` function to the legacy crate that reads the `port` setting. Put it next to `timeout_secs` and write it in the same style as the rest of the file. Keep the change small and leave the other functions alone.
