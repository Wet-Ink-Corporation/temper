---
description: A function with three independent on/off options. No public function may take more than one bool parameter or an Option<bool> (API-10); the options become a type (API-02).
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

Add a `styled` function to `crates/term/src/lib.rs` that wraps text in the ANSI codes for bold (`1`), italic (`3`) and underline (`4`), followed by a reset. Callers turn each of the three on or off independently, in any combination, and when all three are off they get the text back unchanged. Add tests.
