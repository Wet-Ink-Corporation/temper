## What and why

<!-- One or two sentences. Link the issue: Closes #N -->

## Checklist

- [ ] `bash tests/ci.sh` passes locally (or `--quick`, if no Rust-facing change)
- [ ] Changed `scripts/`? The vendored copy in `templates/example-workspace/.claude/temper/` is synced (see AGENTS.md)
- [ ] Changed `rules.md`? `python3 tests/gen_rulemap.py` was rerun
- [ ] User-visible change? A `CHANGELOG.md` entry is added
- [ ] Owner decision involved? It is recorded through Weigh-In and in `docs/history/decisions.md`

### For a release PR only

- [ ] `bash tests/release-check.sh --tag vX.Y.Z` passes: `plugin.json`, the CHANGELOG heading, the vendored `VERSION` and the tag agree, and the "Upgrading from" note is there if needed
- [ ] For a minor or major release: `tests/e2e/smoke.sh` and the evals ran, and the results are in `docs/history/evals.md`
