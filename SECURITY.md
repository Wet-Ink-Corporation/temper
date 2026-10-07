# Security

temper runs as Claude Code hooks with your user's permissions, and its CLI runs in CI. Please report a vulnerability privately, not in an issue: email ryan.britton@wet-ink.net with the temper version, a description, and the steps to reproduce. You will get a reply within a few working days.

This repository is internal to the Wet-Ink-Corporation enterprise, and temper is for Wet Ink repositories only (decisions 0.7 and wi-fd3ac9 in [`docs/history/decisions.md`](docs/history/decisions.md)). Reports come from org members, so email is the channel. GitHub's private vulnerability reporting is not set up while the repository is internal.

Supported versions: marketplace users install whatever `main` holds, so fixes land on `main` and in the next 1.x release. An adopted repository picks up a fix by refreshing its vendored copy (`temper setup --vendor-only`, see the README's Upgrading section).

In scope:
- a way for an agent to bypass the enforcement (hooks, the gate, the review binding);
- code execution or path traversal through the CLI, hooks or `setup`;
- supply-chain issues in the workflows or templates, including the vendored CLI that `setup` copies into adopted repositories (`.claude/temper/`).

Out of scope:
- `TEMPER_DISABLE=1`, a documented human escape hatch;
- the in-session layers failing open (a hook error, a timeout, the Stop loop breaker), which is by design: CI is the layer that fails closed;
- findings in the Rust code of a repository that uses temper, which belong to that repository.
