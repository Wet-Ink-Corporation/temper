#!/bin/bash
# Every check that runs without a model call, in the order CI runs them.
#
#   tests/ci.sh            # all checks
#   tests/ci.sh --quick    # skip the two that compile Rust (doc examples, brownfield e2e)
#
# Needs node 22+, python 3.11+, git, and the toolchain pinned in
# templates/workspace/rust-toolchain.toml with clippy and rustfmt. The plugin
# manifest check (`claude plugin validate`) needs the Claude Code CLI and runs
# as its own CI step.
set -euo pipefail
cd "$(dirname "$0")/.."

quick=0
[ "${1:-}" = "--quick" ] && quick=1

step() { printf '\n== %s\n' "$*"; }

step "unit tests (scanner, hooks, manifests, baseline, report, vendor, release check)"
node --test tests/*.test.mjs

step "rule registry"
python3 tests/registry.py

step "lint and scanner claims"
python3 tests/lint-claims.py

step "rule map is current"
python3 tests/gen_rulemap.py --check

step "example workspace's vendored CLI matches scripts/"
v=templates/example-workspace/.claude/temper
diff -q scripts/temper.mjs "$v/temper.mjs"
for f in scripts/lib/*; do diff -q "$f" "$v/lib/$(basename "$f")"; done

step "release check (plugin.json, CHANGELOG heading and vendored VERSION agree)"
bash tests/release-check.sh

if [ "$quick" = 1 ]; then
  step "skipped: doc examples and the brownfield e2e (--quick)"
  exit 0
fi

step "doc examples (every code block in the skill)"
python3 tests/doc_examples.py --fetch

step "brownfield e2e (lint baseline on a legacy fixture)"
bash tests/e2e/brownfield.sh

step "all checks passed"
