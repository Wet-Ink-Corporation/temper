---
name: gate
description: Run the temper Rust gate (cargo fmt, clippy -D warnings, tests, rustdoc -D warnings, escape-hatch scan) and summarize what fails and what to do about it.
disable-model-invocation: true
allowed-tools: Bash(node *)
---

Gate result for the current tree:

```!
node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" gate || true
```

Review status:

```!
node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" status || true
```

Summarize the result for the user in 2 to 6 lines. If the gate is RED, name each failing step and its first error with `file:line`, and state the fix each one needs under the rust-craft standard (cite the rule ID). Do not start fixing unless the user asks. If the gate is GREEN but there is no approving review for this fingerprint, say so and offer `/temper:review`.
