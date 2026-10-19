---
description: An audit of a legacy upload parser. Beside an obvious unwrap, it holds four failures on untrusted input that no default lint reports and that the standard names - a HashMap index (H-04), an integer division by an uploaded count and Duration::from_secs_f64 on an uploaded number (H-03), and a truncating `as u16` length (H-11). The audit report names them, and the audit changes no code (/temper:audit).
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Bash, Glob, Grep, Skill, Write, Agent]
---

Before we put the ingest crate on the public API, audit it. Write a prioritized list of what to fix, riskiest first, to `AUDIT.md` at the repository root: for each item, where it is, what can go wrong, and the fix. Don't change the code yet; we'll schedule the fixes from your list.
