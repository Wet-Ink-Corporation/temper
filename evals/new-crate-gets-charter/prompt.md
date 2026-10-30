---
description: A new library crate in a workspace whose existing crate has a charter. The standard wants every crate to have its own charter, AGENTS.md, covering purpose, layer, allowed dependencies and public surface (ARC-08). The plain shape adds the crate's code and manifest only.
max_turns: 40
timeout_seconds: 1200
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

Add a new library crate, `crates/render`, that turns a `report::Report` into a Markdown table with one row per rule and its count, sorted by rule id. The nightly job will call it. Add tests.
