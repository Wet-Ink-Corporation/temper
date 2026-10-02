#!/bin/bash
# SessionStart hook for Claude Code cloud sessions working on this repository
# (plan step 0.6). Installs what tests/ci.sh needs that the cloud image lacks:
# the Rust toolchain pinned in templates/workspace/rust-toolchain.toml, with
# clippy and rustfmt, and the example workspace's crates. Node 22+ and
# Python 3.11+ come with the image; this only checks them.
# Local sessions are left alone. Idempotent; output goes to stderr so that
# nothing is added to the session context.
set -euo pipefail

[ "${CLAUDE_CODE_REMOTE:-}" = "true" ] || exit 0
cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}"

{
  toolchain=$(sed -n 's/^channel = "\(.*\)"$/\1/p' templates/workspace/rust-toolchain.toml)
  rustup toolchain install "$toolchain" --profile minimal --component clippy,rustfmt --no-self-update

  # tests/doc_examples.py and tests/e2e/brownfield.sh build against the
  # example workspace's dependency set.
  (cd templates/example-workspace && cargo fetch --locked)

  node -e 'const [maj] = process.versions.node.split("."); if (+maj < 22) { console.error(`temper: Node ${process.version} found; tests/ci.sh needs 22+`); }'
  python3 -c 'import sys; sys.version_info >= (3, 11) or print("temper: Python 3.11+ needed by tests/", file=sys.stderr)'
} >&2
