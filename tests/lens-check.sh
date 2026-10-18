#!/bin/bash
# temper lens checks (#110): the lens's plugin tests, and the read-only allow-list over what
# `claude plugin validate --json` says the lens hooks and calls. No model calls.
#
#   tests/lens-check.sh                       # with the `claude` on PATH
#   CLAUDE="npx --yes @anthropic-ai/claude-code@2.1.286" tests/lens-check.sh   # as CI pins it
set -euo pipefail
cd "$(dirname "$0")/.."
read -r -a claude <<< "${CLAUDE:-claude}"

python3 tests/lens_allowlist.py --self-test
report="$(mktemp)"
trap 'rm -f "$report"' EXIT
"${claude[@]}" plugin validate --json .claude-plugin/plugin.json > "$report"
python3 tests/lens_allowlist.py "$report"
"${claude[@]}" plugin test .
