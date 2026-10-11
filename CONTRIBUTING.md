# Contributing to temper

temper is developed by Wet Ink Corporation and is internal for now. [`AGENTS.md`](AGENTS.md) is the working guide for this repository: its layout, the commands, the rules, and the release checklist. Read it first. It applies to people and coding agents alike.

## How a change lands

1. Open an issue, or pick one labelled `v1-live` or `2.0` (tracked in #6 and #21). Use the templates: bug, false positive or negative, or rule proposal.
2. Work on a branch. Run `bash tests/ci.sh` before pushing; it is what CI runs.
3. Open a pull request. The template's checklist covers the vendored copy, the rule map, the CHANGELOG and decisions.
4. CI must be green: `Checks` and `Unit tests (Windows)` are required on `main`.
5. Merging to `main` ships to marketplace users, so merge only what is ready for them.

## Priority

Every open issue that is a defect or a gap carries one priority label (wi-7c3212). The v1 go-live checklist counts P0 and P1 among the `v1-live` issues.
- **P0**: enforcement can be bypassed or is wrong for everyone, or a release breaks adopters' CI. Fix before anything else, and never release with one open.
- **P1**: a wrong decision or result in a realistic case (a false positive that blocks normal work, a false negative on a MUST rule, a broken documented workflow). Fix before the next release.
- **P2**: everything else that should change: rough edges, docs, minor false positives with a workaround. Fix when convenient.

## Changing the standard

A rule change is a change in behaviour for every repository that uses temper.
- A new or tightened MUST rule can make existing code fail. Raise it as an issue with the rule-proposal template before writing it.
- Rule IDs are permanent: never renumber or reuse one.
- Every example must compile and lint under the template lints (`tests/doc_examples.py`).

## Decisions

Questions that belong to the owner (new dependencies, crate boundaries, lint exceptions, anything H-16 lists) are raised through Weigh-In and recorded in `.kb/_intake/decisions/` and [`docs/history/decisions.md`](docs/history/decisions.md).

## Versioning

temper follows [semantic versioning](https://semver.org/). The version is `version` in `.claude-plugin/plugin.json`; `tests/release-check.sh` keeps the CHANGELOG heading, the vendored `VERSION` and the tag in step with it.

**What the version promises.** An adopted repository runs two copies of temper:
- **The plugin** (skills, agents, hooks) updates through the marketplace. Every merge to `main` reaches it at the next plugin update, whatever the version says.
- **The vendored CLI** (`.claude/temper/`, which CI and the pre-commit hook run) changes only when the repository re-runs setup.

The version tells an adopted repository what an update does to it. A patch never needs action. A minor release can block new work it allowed before, but never fails code nobody touched. A major release can fail an untouched tree or needs a migration.

| Change | Level |
|---|---|
| A CLI command or flag is removed or renamed, or an exit code changes meaning | major |
| A committed file (`.claude/temper-baseline.json`, `.claude/temper.json`) changes format so that an older vendored CLI cannot read it, or a newer one cannot read the old file | major |
| A new or tightened MUST rule, hook decision or lint fails code that the previous release passed and nobody has touched, despite a committed baseline | major |
| The install changes: the plugin or marketplace name, or a higher minimum Claude Code or Node version | major |
| A template change that an adopted repository must migrate by hand to keep CI green | major |
| A new or tightened MUST rule, or a new template lint, that the baseline ratchet absorbs: legacy code is reported, only new or changed code fails | minor |
| A hook that now denies, asks or blocks where it allowed before (a changed hook decision), for new edits only | minor |
| A new CLI command, flag or JSON field; a new exit code for a case that used to be an error | minor |
| A template change that `temper setup --force` applies, with an "Upgrading from" note in the CHANGELOG | minor |
| A new SHOULD rule, a new skill, agent or command, or new guidance in the standard | minor |
| A fix that makes a hook, the scanner or the CLI do what the rules and the docs already say: a false positive removed, a crash or a platform bug fixed | patch |
| Docs, tests, evals, and this repository's own CI and tooling | patch |

Where two rows apply, the higher level wins.
- A false-negative fix that starts denying code the rule always forbade is a patch only if the ratchet absorbs it. If it fails untouched code, it is major.
- Until 2.0.0, a major change waits for the next major release unless the owner decides otherwise on the PR.

Every user-visible change gets a CHANGELOG entry under `## Unreleased`. The `Changelog` workflow fails a pull request that changes what ships to users without touching `CHANGELOG.md`. That covers `scripts/`, `hooks/`, `agents/`, `skills/`, `templates/`, `.claude-plugin/`, the README, `docs/troubleshooting.md`, `SECURITY.md` and `LICENSE`. Label the PR `no-changelog` when users see no change, as with a refactor or a comment fix. A release that changes `scripts/` or `templates/workspace/` ends its section with an "Upgrading from X.Y" note. The release checklist is in [`AGENTS.md`](AGENTS.md).

**History.** No repository had adopted temper before 1.3, so the earlier releases were numbered without this table. 1.2.0 (the baseline) was minor, and 1.2.1 (docs) and 1.2.3 (Windows fixes) were patches, as the table has them. Two would be major today:
- 1.1.0 added rules and template lints before the baseline existed, so they could fail untouched code;
- 1.2.2 renamed the marketplace, which changes the install.
