#!/bin/bash
# End-to-end smoke test: a real headless Claude Code session with this plugin
# loaded, on a throwaway Rust repository. Costs a few model calls.
#
#   tests/e2e/smoke.sh [prompt]
#
# Prints the hook decisions, tool calls and final reply, then the temper status.
set -euo pipefail
PLUGIN="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
PROMPT="${1:-Add a mul function to the calc crate that multiplies two u32 values and reports overflow instead of wrapping. Include tests.}"
cd "$WORK"
mkdir -p crates/calc/src
cp "$PLUGIN/templates/workspace/Cargo.toml" "$PLUGIN/templates/workspace/clippy.toml" "$PLUGIN/templates/workspace/rustfmt.toml" .
sed -i.bak '/^thiserror = "2"$/d' Cargo.toml && rm -f Cargo.toml.bak
cat > crates/calc/Cargo.toml <<'TOML'
[package]
name = "calc"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish = false

[lints]
workspace = true
TOML
cat > crates/calc/src/lib.rs <<'RS'
//! Tiny calculator used by the temper smoke test.

/// Adds two numbers, returning `None` on overflow.
#[must_use]
pub fn add(a: u32, b: u32) -> Option<u32> {
    a.checked_add(b)
}

#[cfg(test)]
mod tests {
    use super::add;

    #[test]
    fn adds_small_numbers() {
        assert_eq!(add(2, 3), Some(5));
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=smoke@example.com -c user.name=smoke commit -qm init
claude -p --plugin-dir "$PLUGIN" --permission-mode acceptEdits \
  --allowedTools "Bash Read Write Edit Glob Grep Skill Agent" --max-turns 60 \
  --output-format stream-json --verbose "$PROMPT" < /dev/null > "$WORK.jsonl"
# The transcript lives outside the repo: files inside it are part of the reviewed tree.
python3 "$PLUGIN/tests/e2e/summarize.py" "$WORK.jsonl"
node "$PLUGIN/scripts/temper.mjs" status
echo "workspace kept at $WORK"
