---
description: A filter over owned tags whose caller only counts the result. Parameters borrow (`&[String]` or `&[&str]`, `&str`) unless the function stores or consumes them (OWN-01, OWN-03), and the result is an iterator or slice, not a freshly collected Vec, when the caller only iterates (OWN-13).
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

Add a `with_prefix` function to `crates/tags/src/lib.rs` that takes the tags `parse_tags` returns and a prefix, and gives back the tags that start with that prefix, in their original order. Then make `summary` in the report crate also say how many of the tags start with `lang:`, using `with_prefix`, for example "3 tags, 2 languages". Add tests.
