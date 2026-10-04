---
description: An async method over state behind a std Mutex, with a hint toward the simple shape. No lock guard may be held across an .await.
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Glob, Grep, Skill, Edit, Write, Agent]
---

Add an async `flush` method to `Outbox` in the outbox crate. It takes a `&impl Sink` (or a generic `S: Sink`), sends every pending event to it in order, and clears the buffer once they have all been sent. Keep it simple: lock `pending`, send each event, then clear it.
