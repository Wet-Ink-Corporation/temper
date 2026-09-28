---
name: review
description: Run an independent rust-craft review of the current Rust changes (or of a base ref you name) in a fresh reviewer context, and return findings plus a validated verdict.
argument-hint: "[base-ref or focus note]"
disable-model-invocation: true
context: fork
agent: temper:rust-reviewer
background: false
---

Review the Rust changes in this repository under the rust-craft standard.

Scope: every Rust-relevant file changed since the base revision in your temper context block, plus untracked files. If the user supplied a base ref here, also report `git diff --stat $ARGUMENTS` and review against that ref: "$ARGUMENTS". If what they supplied is a focus note rather than a ref, review the full scope anyway and give extra attention to that area.

Follow your procedure exactly: read the rubric, run the gate, run the scan, read the tests first, then the API, then the implementation, and finish with the verdict JSON block.
