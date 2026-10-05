---
description: An ordinary Rust feature request. The standard must load and the result must be free of escape hatches, with a test.
max_turns: 40
timeout_seconds: 900
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

In this repository, add a `mul` function to the calc crate that multiplies two u32 values and reports overflow instead of wrapping. Add tests for it.
