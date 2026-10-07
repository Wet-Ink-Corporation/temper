#!/bin/bash
# The reference workspace passes its own gate (no model calls):
#
#   tests/e2e/example-gate.sh          # KEEP=1 keeps the scratch copy
#
# templates/example-workspace is nested in this repository, so its gate cannot
# run in place: project discovery would take the temper repository as the git
# root. This copies it to a scratch directory, makes that a git repository with
# one commit, and runs the workspace's vendored CLI there:
# `node .claude/temper/temper.mjs gate --check --no-cache` (--check verifies the
# formatting instead of applying it; --no-cache collects afresh). The gate exits
# 0 only on GREEN; RED and UNAVAILABLE (no usable cargo) both exit 1, and the
# script also requires the GREEN line. Needs git, node and the toolchain pinned
# in templates/example-workspace/rust-toolchain.toml.
set -euo pipefail
PLUGIN="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/temper-example-gate.XXXXXX")"
WS="$WORK/ws"
# The target directory sits outside the copy, so the build leaves the work tree clean.
export CARGO_TARGET_DIR="$WORK/target" CARGO_TERM_COLOR=never
export GIT_AUTHOR_NAME=example-gate GIT_AUTHOR_EMAIL=example-gate@example.com GIT_COMMITTER_NAME=example-gate GIT_COMMITTER_EMAIL=example-gate@example.com
# The gate's base is HEAD, as at a terminal outside a Claude Code session.
unset TEMPER_BASE TEMPER_DISABLE CLAUDECODE TEMPER_ALLOW_RAISE
cleanup() { if [ "${KEEP:-0}" = 1 ]; then echo "kept $WORK"; else rm -rf "$WORK"; fi; }
trap cleanup EXIT

mkdir "$WS"
# Copies the working tree, minus the gitignored target/ and .temper/ a local
# cargo or gate run may have left in templates/example-workspace.
tar -C "$PLUGIN/templates/example-workspace" --exclude=./target --exclude=./.temper -cf - . | tar -C "$WS" -xf -
git -C "$WS" init -q
git -C "$WS" add -A
git -C "$WS" commit -qm "example workspace"

set +e
out="$(cd "$WS" && node .claude/temper/temper.mjs gate --check --no-cache 2>&1)"
code=$?
set -e
printf '%s\n' "$out"
if [ "$code" -ne 0 ] || ! printf '%s\n' "$out" | grep -q '^temper gate: GREEN'; then
  printf 'FAIL example workspace gate (exit %s): not GREEN\n' "$code" >&2
  exit 1
fi
printf 'ok   example workspace passes its own gate\n'
