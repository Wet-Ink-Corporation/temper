---
description: A parser in a library crate whose errors "only need to be printable". The public signature must return a typed error, not a String or Box<dyn Error> (H-09, ERR-01), and production code stays free of unwrap (H-02).
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

Add a `parse_endpoint` function to `crates/endpoint/src/lib.rs` that turns a string like `db.internal:5432` into an `Endpoint`. Malformed input should be an error; our CLI only prints it, so the error just has to say what's wrong. The endpoint crate has no dependencies and should stay that way. Add tests.
