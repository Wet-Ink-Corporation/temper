#!/bin/bash
# End-to-end brownfield test for the lint ratchet (no model calls):
#
#   tests/e2e/brownfield.sh            # KEEP=1 keeps the scratch repository
#
# A scratch git repository gets the template lints and a crate with legacy debt
# (unwrap, indexing, a missing doc, an `as` cast, a broken doc link, a local
# macro). Without a baseline `--check` is strict; `--init` refuses unformatted
# code and raises inside Claude Code. With a committed baseline the gate is
# green on that debt and red on new code: a new unwrap, new indexing, a swap
# that keeps the counts, a new macro invocation. That holds under hostile git
# config (diff.mnemonicPrefix, diff.interHunkContext); an unstaged crate move,
# a crate rename and a rename of both directory and name stay green. Output
# paths through a symlink, and --tighten on another host, are refused. A fix is green and `baseline --tighten`
# locks it in. Finishes with `temper report`. Needs git, node and the pinned
# toolchain (templates/workspace/rust-toolchain.toml).
set -euo pipefail
PLUGIN="$(cd "$(dirname "$0")/../.." && pwd)"
TEMPER=(node "$PLUGIN/scripts/temper.mjs")
WORK="$(mktemp -d "${TMPDIR:-/tmp}/temper-brownfield.XXXXXX")"
export CARGO_TARGET_DIR="$WORK/target" CARGO_TERM_COLOR=never
export GIT_AUTHOR_NAME=brownfield GIT_AUTHOR_EMAIL=brownfield@example.com GIT_COMMITTER_NAME=brownfield GIT_COMMITTER_EMAIL=brownfield@example.com
# The ratchet's base is HEAD, as in a pre-commit hook. The script plays the human at a
# terminal, so it is not a Claude Code session (CLAUDECODE is checked separately below).
unset TEMPER_BASE TEMPER_DISABLE CLAUDECODE TEMPER_ALLOW_RAISE
cleanup() { if [ "${KEEP:-0}" = 1 ]; then echo "kept $WORK"; else rm -rf "$WORK"; fi; }
trap cleanup EXIT
START=$SECONDS
pass() { printf 'ok   %s\n' "$1"; }
fail() { printf 'FAIL %s\n%s\n' "$1" "${2:-}" >&2; exit 1; }
# expect NAME EXPECTED-EXIT REGEX... -- CMD...: run CMD, check its exit code and that its output matches every REGEX.
expect() {
  local name=$1 want=$2; shift 2
  local res=()
  while [ "$1" != -- ]; do res+=("$1"); shift; done
  shift
  local out code=0
  out="$("$@" 2>&1)" || code=$?
  [ "$code" = "$want" ] || fail "$name: exit $code, expected $want" "$out"
  for re in "${res[@]}"; do grep -Eq -- "$re" <<<"$out" || fail "$name: output does not match /$re/" "$out"; done
  pass "$name"
  LAST="$out"
}
line_of() { grep -n -- "$1" "$2" | tail -1 | cut -d: -f1; }

cd "$WORK"
T="$PLUGIN/templates/workspace"
cp "$T/Cargo.toml" "$T/clippy.toml" "$T/rustfmt.toml" "$T/rust-toolchain.toml" .
mkdir -p .cargo .claude crates/ledger/src
cp "$T/.cargo/config.toml" .cargo/
cp "$T/.claude/temper.json" .claude/
sed -i.bak '/^thiserror = /d' Cargo.toml && rm -f Cargo.toml.bak
cat > crates/ledger/Cargo.toml <<'TOML'
[package]
name = "ledger"
description = "Legacy ledger code from before temper"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/ledger/src/lib.rs <<'RS'
//! Legacy ledger code from before temper, see [`Journal`].

/// Parses an amount.
#[must_use]
pub fn parse_amount(s: &str) -> u64 {
    s.trim().parse().unwrap()
}

/// Parses a count.
#[must_use]
pub fn parse_count(s: &str) -> u32 {
    s.trim().parse().unwrap()
}

/// The first entry.
#[must_use]
pub fn first(entries: &[u64]) -> u64 {
    entries[0]
}

#[must_use]
pub fn to_cents(amount: u64) -> u32 {
    amount as u32
}

macro_rules! getter {
    ($name:ident, $i:expr) => {
        /// Reads one entry as cents.
        #[must_use]
        pub fn $name(v: &[u64]) -> u32 {
            v[$i] as u32
        }
    };
}

getter!(g0, 0);

#[cfg(test)]
mod tests {
    use super::{first, parse_amount};

    #[test]
    fn parses_and_reads_the_first_entry() {
        assert_eq!(parse_amount(" 5 "), 5, "trimmed input parses");
        assert_eq!(first(&[3, 4]), 3, "first entry");
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
cargo fmt --all
git init -q && git add -A && git commit -qm 'legacy ledger'
LIB=crates/ledger/src/lib.rs

# 1. no baseline: the gate is red and --check is strict (zero findings required)
expect 'plain gate (no baseline) is red on legacy debt' 1 'clippy FAILED' -- "${TEMPER[@]}" gate --no-cache
mkdir -p target/temper
expect 'baseline --check without a baseline requires zero findings' 1 'FAIL' 'no baseline: zero findings required \([0-9]+ found\)' "$LIB:[0-9]+ clippy::unwrap_used" -- "${TEMPER[@]}" baseline --check --findings-out target/temper/strict.json
node -e 'const c = require(process.argv[1]); if (c.ratchet?.mode !== "strict" || !c.ratchet.new_code.length || c.ratchet.ok) process.exit(1)' "$WORK/target/temper/strict.json" \
  || fail 'strict --findings-out carries the ratchet result' "$(head -c 400 target/temper/strict.json)"
pass 'strict --findings-out carries the ratchet result (mode strict)'
expect '--findings-out outside target/ or .temper/ is refused' 2 'refused' -- "${TEMPER[@]}" baseline --check --findings-out "$LIB"
ln -s ../crates/ledger/src target/src
expect '--findings-out through a symlinked directory is refused' 2 'refused' -- "${TEMPER[@]}" baseline --check --findings-out target/src/lib.rs
git diff --quiet -- "$LIB" || fail 'the symlinked target was not written' "$(head -c 200 "$LIB")"
rm target/src

# 2. --init refuses unformatted code, and a raise inside Claude Code
sed -i.bak 's/^    entries\[0\]$/        entries[0]/' "$LIB" && rm -f "$LIB.bak"
expect 'baseline --init refuses unformatted code' 1 'cargo fmt --all -- --check. fails' 'commit the formatting on its own first' -- "${TEMPER[@]}" baseline --init
git checkout -q -- .
expect 'baseline --init refuses inside Claude Code' 2 'refused inside a Claude Code session' -- env CLAUDECODE=1 "${TEMPER[@]}" baseline --init
[ ! -e .claude/temper-baseline.json ] || fail 'nothing was recorded by the refused runs'

# 3. baseline --init records the debt; commit it
expect 'baseline --init records the legacy findings' 0 'wrote \.claude/temper-baseline\.json: [0-9]+ finding' 'host [a-z0-9_]+-' -- "${TEMPER[@]}" baseline --init
for lint in clippy::unwrap_used clippy::indexing_slicing missing_docs clippy::as_conversions scan:unwrap scan:as-cast rustdoc::broken_intra_doc_links; do
  grep -q "\"$lint\"" .claude/temper-baseline.json || fail "baseline records $lint" "$(cat .claude/temper-baseline.json)"
done
grep -q '"ledger": "crates/ledger"' .claude/temper-baseline.json || fail 'baseline records the crate dir' "$(cat .claude/temper-baseline.json)"
pass 'baseline has unwrap, indexing, missing doc, as cast (clippy and scan), the rustdoc link and the crate dir'
expect 'baseline --init refuses to overwrite' 2 'already exists' -- "${TEMPER[@]}" baseline --init
git add -A && git commit -qm 'temper baseline'
# From here on, git config that used to break the new-code check.
git config diff.mnemonicPrefix true
git config diff.interHunkContext 8
git config diff.srcPrefix x/
git config diff.dstPrefix y/

# 4. green on legacy debt, and says how much there is
expect 'gate is green on legacy debt' 0 'GREEN' 'clippy: no new findings' 'doc: no new findings' 'legacy: [0-9]+ finding\(s\) \(baseline [0-9]+\)' -- "${TEMPER[@]}" gate --no-cache
echo "     $(grep -Eo 'legacy: [0-9]+ finding\(s\) \(baseline [0-9]+\)' <<<"$LAST")"

# 5. a new unwrap on a new line: red, named (the scan step sees it first; the ratchet names the clippy lint)
cat >> "$LIB" <<'RS'

/// Parses a fee.
#[must_use]
pub fn parse_fee(s: &str) -> u64 {
    s.trim().parse::<u64>().unwrap()
}
RS
NEW=$(line_of 'parse::<u64>().unwrap()' "$LIB")
expect 'gate is red on a new unwrap and names it' 1 'RED' "H-02 $LIB:$NEW" -- "${TEMPER[@]}" gate --no-cache
expect 'baseline --check names the new clippy finding (mnemonicPrefix set)' 1 "$LIB:$NEW clippy::unwrap_used" -- "${TEMPER[@]}" baseline --check --base HEAD
git checkout -q -- .

# 5b. new indexing (no scanner rule): the gate's clippy ratchet is what fails
cat >> "$LIB" <<'RS'

/// The second entry.
#[must_use]
pub fn second(entries: &[u64]) -> u64 {
    entries[1]
}
RS
NEW=$(line_of 'entries\[1\]' "$LIB")
expect 'gate ratchet is red on new indexing and names it' 1 'scan ok' 'clippy FAILED' "$LIB:$NEW clippy::indexing_slicing" -- "${TEMPER[@]}" gate --no-cache
git checkout -q -- .

# 5c. a new invocation of the legacy macro: the findings move to the call site, so it is new code
sed -i.bak 's/^getter!(g0, 0);$/getter!(g0, 0);\ngetter!(g1, 1);/' "$LIB" && rm -f "$LIB.bak"
NEW=$(line_of '^getter!(g1, 1);' "$LIB")
expect 'gate is red on a new macro invocation' 1 'clippy FAILED' "$LIB:$NEW clippy::indexing_slicing" "$LIB:$NEW clippy::as_conversions" -- "${TEMPER[@]}" gate --no-cache
git checkout -q -- .

# 6. a swap: fix one legacy unwrap, add another (identical text) in the same file: counts unchanged, still red
sed -i.bak '/pub fn parse_count/,/^}/ s/parse()\.unwrap()/parse().unwrap_or(0)/' "$LIB" && rm -f "$LIB.bak"
cat >> "$LIB" <<'RS'

/// Parses a fee.
#[must_use]
pub fn parse_fee(s: &str) -> u64 {
    s.trim().parse().unwrap()
}
RS
[ "$(grep -c 'parse().unwrap()' "$LIB")" = 2 ] || fail 'swap setup' "$(cat "$LIB")"
NEW=$(line_of 'parse().unwrap()' "$LIB")
expect 'gate is red on a swap within one crate' 1 'RED' "$LIB:$NEW clippy::unwrap_used" -- "${TEMPER[@]}" gate --no-cache
expect 'baseline --check is red on the swap' 1 "$LIB:$NEW clippy::unwrap_used" -- "${TEMPER[@]}" baseline --check --base HEAD
git checkout -q -- .

# 7. edits that must stay green
# 7a. doc lines just above and below a legacy unwrap (diff.interHunkContext=8 would merge the hunks)
sed -i.bak 's|^/// Parses a count\.$|/// Parses a count of entries.|; s|^/// The first entry\.$|/// The first entry of the list.|' "$LIB" && rm -f "$LIB.bak"
expect 'doc edits around a legacy finding stay green (interHunkContext set)' 0 'GREEN' -- "${TEMPER[@]}" gate --no-cache
git checkout -q -- .
# 7b. an unstaged move of the whole crate (plain mv, nothing staged)
mv crates/ledger crates/ledger2
expect 'an unstaged crate move stays green' 0 'GREEN' -- "${TEMPER[@]}" gate --no-cache
expect 'baseline --check passes the unstaged move' 0 'PASS' -- "${TEMPER[@]}" baseline --check --base HEAD
mv crates/ledger2 crates/ledger
# 7c. renaming the crate (same directory) keeps its counts
sed -i.bak 's/^name = "ledger"$/name = "ledger-core"/' crates/ledger/Cargo.toml && rm -f crates/ledger/Cargo.toml.bak
expect 'a crate rename stays green and says so' 0 'PASS' 'crate `ledger` is now `ledger-core` \(same directory\)' -- "${TEMPER[@]}" baseline --check --base HEAD
git checkout -q -- .
# 7d. the usual crate rename: directory and name together, unstaged
mv crates/ledger crates/ledger-core
sed -i.bak 's/^name = "ledger"$/name = "ledger-core"/' crates/ledger-core/Cargo.toml && rm -f crates/ledger-core/Cargo.toml.bak
expect 'a crate renamed with its directory stays green' 0 'GREEN' -- "${TEMPER[@]}" gate --no-cache
expect 'baseline --check follows the moved crate' 0 'PASS' 'crate `ledger` is now `ledger-core` \(moved\)' -- "${TEMPER[@]}" baseline --check --base HEAD
rm -rf crates/ledger-core && git checkout -q -- .

# 8. fix one legacy finding: green, reports it; --tighten lowers the count; --check passes
sed -i.bak 's/    entries\[0\]/    entries.first().copied().unwrap_or_default()/' "$LIB" && rm -f "$LIB.bak"
expect 'gate is green after fixing a legacy finding and reports it' 0 'GREEN' '1 fixed since the baseline: run temper baseline --tighten' -- "${TEMPER[@]}" gate --no-cache
expect 'baseline status says it is not tight' 0 'tight: no; 1 fixed' -- "${TEMPER[@]}" baseline
cp .claude/temper-baseline.json target/baseline.json
sed -i.bak 's/"host": "[^"]*"/"host": "aarch64-other-host"/' .claude/temper-baseline.json && rm -f .claude/temper-baseline.json.bak
expect 'baseline --tighten refuses on another host' 2 'refused: the baseline was recorded on aarch64-other-host' -- "${TEMPER[@]}" baseline --tighten
cp target/baseline.json .claude/temper-baseline.json
expect 'baseline --tighten lowers the count' 0 'ledger clippy::indexing_slicing: 2 -> 1' -- "${TEMPER[@]}" baseline --tighten
grep -q '"clippy::indexing_slicing": 1' .claude/temper-baseline.json || fail 'tighten wrote the lower count' "$(cat .claude/temper-baseline.json)"
expect 'baseline --tighten is idempotent' 0 'already tight' -- "${TEMPER[@]}" baseline --tighten
expect 'baseline --check passes after tightening' 0 'PASS' 'legacy: [0-9]+ finding\(s\)' -- "${TEMPER[@]}" baseline --check --base HEAD --findings-out target/temper/findings.json
[ -s target/temper/findings.json ] || fail '--findings-out wrote the collection'
git add -A && git commit -qm 'fix first(); tighten the baseline'
expect 'gate is green on the tightened baseline' 0 'GREEN' -- "${TEMPER[@]}" gate --no-cache

# 9. a raised baseline is flagged by the gate and by --check against the old base, but fails neither
sed -i.bak 's/"clippy::indexing_slicing": 1/"clippy::indexing_slicing": 5/' .claude/temper-baseline.json && rm -f .claude/temper-baseline.json.bak
expect 'the gate flags a raised baseline' 0 'GREEN' 'BASELINE RAISED since [0-9a-f]{12}: 1 count' 'ledger clippy::indexing_slicing: 1 -> 5' -- "${TEMPER[@]}" gate --no-cache
git add -A && git commit -qm 'raise the baseline (a human decision)'
expect 'baseline --check flags a raised baseline' 0 'PASS' 'BASELINE RAISED since [0-9a-f]{12}: 1 count' 'ledger clippy::indexing_slicing: 1 -> 5' -- "${TEMPER[@]}" baseline --check --base HEAD~1
git reset -q --hard HEAD~1

# 10. the report: three files whose numbers agree with the collection
if [ ! -f "$PLUGIN/scripts/lib/report.mjs" ]; then
  echo "skip 10. temper report: scripts/lib/report.mjs is not in this plugin copy yet"
else
  expect 'temper report writes json, md and html' 0 'report\.json' -- "${TEMPER[@]}" report --findings target/temper/findings.json
  R=target/temper-report
  for f in report.json report.md report.html; do [ -s "$R/$f" ] || fail "report wrote $f"; done
  node -e '
    const fs = require("fs");
    const [cPath, dir] = process.argv.slice(1);
    const c = JSON.parse(fs.readFileSync(cPath, "utf8"));
    const r = JSON.parse(fs.readFileSync(`${dir}/report.json`, "utf8"));
    const md = fs.readFileSync(`${dir}/report.md`, "utf8");
    const html = fs.readFileSync(`${dir}/report.html`, "utf8");
    const n = c.findings.filter((f) => !f.exception).length;
    const must = c.findings.filter((f) => !f.exception && f.severity === "must").length;
    const bad = [];
    if (r.totals.findings !== n) bad.push(`report.json totals.findings ${r.totals.findings} != ${n} findings collected`);
    if (r.totals.must !== must) bad.push(`report.json totals.must ${r.totals.must} != ${must}`);
    for (const [name, text] of [["report.md", md], ["report.html", html]]) {
      if (!new RegExp(`\\b${n}\\b`).test(text)) bad.push(`${name} does not show the ${n} findings`);
      if (!new RegExp(`\\b${must}\\b`).test(text)) bad.push(`${name} does not show the ${must} MUST findings`);
    }
    if (md.split("\n").length > 60) bad.push(`report.md has ${md.split("\n").length} lines (limit 60)`);
    if (bad.length) { console.error(bad.join("\n")); process.exit(1); }
    console.log(`     ${n} findings, ${must} MUST in all three files`);
  ' target/temper/findings.json "$R" || fail 'report numbers match the collection'
  pass 'report numbers match the collection'
fi

echo "brownfield e2e passed in $((SECONDS - START))s"
