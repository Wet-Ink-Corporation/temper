# Security

temper runs as Claude Code hooks with your user's permissions, and its CLI runs in CI. Please report a vulnerability privately, not in an issue: email ryan.britton@wet-ink.net with the temper version, a description, and the steps to reproduce. You will get a reply within a few working days.

In scope:
- a way for an agent to bypass the enforcement (hooks, the gate, the review binding);
- code execution or path traversal through the CLI, hooks or `setup`;
- supply-chain issues in the workflows or templates.

`TEMPER_DISABLE=1` is a documented human escape hatch, not a vulnerability.
