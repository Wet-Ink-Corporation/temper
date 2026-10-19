#!/bin/bash
# temper lens checks (#110): the lens's plugin tests, and the read-only allow-list over what
# `claude plugin validate --json` says the lens hooks and calls, and a strict tsc pass over the
# lens and its tests against the plugin API types in tests/lens-types/ (wi-78d987). No model calls.
#
#   tests/lens-check.sh                       # with the `claude` and `tsc` on PATH
#   CLAUDE="npx --yes @anthropic-ai/claude-code@2.1.286" \
#   TSC="npx --yes -p typescript@7.0.2 tsc" tests/lens-check.sh                 # as CI pins them
set -euo pipefail
cd "$(dirname "$0")/.."
read -r -a claude <<< "${CLAUDE:-claude}"
read -r -a tsc <<< "${TSC:-tsc}"

python3 tests/lens_allowlist.py --self-test
report="$(mktemp)"
trap 'rm -f "$report"' EXIT
"${claude[@]}" plugin validate --json .claude-plugin/plugin.json > "$report"
python3 tests/lens_allowlist.py "$report"
"${claude[@]}" plugin test .
"${tsc[@]}" -p tests/lens.tsconfig.json
