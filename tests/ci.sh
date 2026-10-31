#!/bin/bash
# Every check that runs without a model call, in the order CI runs them.
#
#   tests/ci.sh            # all checks
#   tests/ci.sh --quick    # skip the steps that compile Rust (the workspace's clippy, tests,
#                          # docs and licence check, doc examples, brownfield e2e, example
#                          # workspace gate)
#
# Needs node 22+, python 3.11+, git, and the toolchain pinned in
# templates/workspace/rust-toolchain.toml with clippy and rustfmt (the example
# workspace pins the same one in its own rust-toolchain.toml). The plugin
# manifest check (`claude plugin validate`) and the temper lens checks
# (tests/lens-check.sh) need the Claude Code CLI and run as their own CI steps.
set -euo pipefail
cd "$(dirname "$0")/.."

quick=0
[ "${1:-}" = "--quick" ] && quick=1

step() { printf '\n== %s\n' "$*"; }

step "workspace config matches templates/workspace/ (.cargo/config.toml only appends the xtask alias)"
for f in clippy.toml rustfmt.toml deny.toml rust-toolchain.toml; do cmp "templates/workspace/$f" "$f"; done
t=templates/workspace/.cargo/config.toml
cmp "$t" <(head -c "$(wc -c < "$t")" .cargo/config.toml)

step "workspace format (the 2.0 port's crates)"
cargo fmt --all -- --check

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

step "temper-lens's vendored CLI and rule registry match scripts/ and the standard's"
l=plugins/temper-lens
diff -q scripts/temper.mjs "$l/scripts/temper.mjs"
diff -rq scripts/lib "$l/scripts/lib"
diff -q skills/rust-craft/references/rules.md "$l/references/rules.md"

step "the temper plugin ships no mod (a mod turns the whole plugin off in desktop chats; the lens is its own plugin)"
node -e '
  const h = require("./hooks/hooks.json"), p = require("./.claude-plugin/plugin.json");
  const bad = [h.modules && "hooks/hooks.json lists modules", p.types && "plugin.json names types"].filter(Boolean);
  if (bad.length) { console.error(`the temper plugin must not ship a mod: ${bad.join(", ")}; put it in plugins/temper-lens/`); process.exit(1); }'

step "temper lens allow-list checker"
python3 tests/lens_allowlist.py --self-test

step "release check (both plugin.json files, CHANGELOG heading and vendored VERSION agree)"
bash tests/release-check.sh

if [ "$quick" = 1 ]; then
  step "skipped: the workspace's clippy, tests, docs, licence and version checks, doc examples, the brownfield e2e and the example workspace gate (--quick)"
  exit 0
fi

step "workspace clippy"
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

step "workspace tests"
cargo test --workspace --all-features --locked

step "workspace docs"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked

step "each crate carries both licence texts, equal to the root's"
cargo xtask check-licenses

step "the workspace's version sites agree"
cargo xtask versions --check

step "doc examples (every code block in the skill)"
python3 tests/doc_examples.py --fetch

step "brownfield e2e (lint baseline on a legacy fixture)"
bash tests/e2e/brownfield.sh

step "example workspace passes its own gate (vendored CLI, on a scratch copy)"
bash tests/e2e/example-gate.sh

step "all checks passed"
