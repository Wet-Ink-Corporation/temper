#!/bin/bash
# The release checklist's version checks, as a script (plan step 1.5).
#
#   tests/release-check.sh                  # in tests/ci.sh: the versions agree
#   tests/release-check.sh --tag v1.3.0     # in the release workflow: and the tag
#
# Asserts that these agree:
#   - `version` in .claude-plugin/plugin.json;
#   - the top `## X.Y.Z (YYYY-MM-DD)` heading in CHANGELOG.md;
#   - templates/example-workspace/.claude/temper/VERSION;
#   - with --tag, the tag (vX.Y.Z). If the tag exists, it must point at HEAD.
#
# Also asserts that the release's CHANGELOG section ends with an "Upgrading
# from" note when the release changes scripts/ or templates/workspace/. The
# release's changes are those since the previous version's tag (the next
# `## X.Y.Z` heading down), or since --base REF. That needs the tags: without
# them the note check is skipped, except with --tag, where it fails. Without
# --tag, a release whose tag already exists is not checked again.
#
# With --tag, an `## Unreleased` section above the release heading also fails:
# a release ships its whole CHANGELOG section.
#
# --notes FILE writes the release's CHANGELOG section, without its heading, to
# FILE: the GitHub release notes. It is written only when every check passes.
#
# --root DIR checks another checkout (the tests use it); the default is this one.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
tag=""
base=""
notes=""
while [ $# -gt 0 ]; do
  case "$1" in
    --tag) tag="${2:?--tag needs a value}"; shift 2 ;;
    --base) base="${2:?--base needs a value}"; shift 2 ;;
    --notes) notes="${2:?--notes needs a value}"; case "$notes" in /*) ;; *) notes="$PWD/$notes" ;; esac; shift 2 ;;
    --root) root="${2:?--root needs a value}"; shift 2 ;;
    *) echo "release-check: unknown argument: $1" >&2; exit 2 ;;
  esac
done
cd "$root"

fail=0
err() { echo "release-check: $*" >&2; fail=1; }
ok() { echo "release-check: $*"; }

# 1. The three versions in the tree.
plugin=$(node -p 'require("./.claude-plugin/plugin.json").version')
vendored=$(tr -d '\r\n' < templates/example-workspace/.claude/temper/VERSION)
heading_re='^## ([0-9]+\.[0-9]+\.[0-9]+) \(([0-9]{4}-[0-9]{2}-[0-9]{2})\)[[:space:]]*$'
headings=$(grep -E "$heading_re" CHANGELOG.md | sed -E "s/$heading_re/\\1/" || true)
changelog=$(printf '%s\n' "$headings" | sed -n 1p)
previous=$(printf '%s\n' "$headings" | sed -n 2p)

[ -n "$changelog" ] || err "CHANGELOG.md has no '## X.Y.Z (YYYY-MM-DD)' heading"
[ "$plugin" = "$changelog" ] || err "plugin.json is $plugin, the top CHANGELOG heading is ${changelog:-missing}"
[ "$plugin" = "$vendored" ] || err "plugin.json is $plugin, templates/example-workspace/.claude/temper/VERSION is $vendored"

# 2. The tag.
if [ -n "$tag" ]; then
  [ "$tag" = "v$plugin" ] || err "the tag is $tag, plugin.json is $plugin (want v$plugin)"
  if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    [ "$(git rev-parse "$tag^{commit}")" = "$(git rev-parse HEAD)" ] || err "tag $tag exists and does not point at HEAD"
  fi
  if awk -v v="$changelog" '/^## /{ if ($2 == v) exit; if ($0 ~ /^## Unreleased/) { found=1; exit } } END { exit !found }' CHANGELOG.md; then
    err "CHANGELOG.md has an '## Unreleased' section above $changelog; fold it into the release"
  fi
fi

# 3. The "Upgrading from" note.
note_check() {
  local from="$base"
  if [ -z "$from" ]; then
    if [ -z "$tag" ] && git rev-parse -q --verify "refs/tags/v$plugin" >/dev/null; then
      ok "v$plugin is already tagged; its Upgrading note is not checked again"
      return
    fi
    [ -n "$previous" ] || { ok "no earlier release in CHANGELOG.md; no Upgrading note needed"; return; }
    from="v$previous"
  fi
  if ! git rev-parse -q --verify "$from^{commit}" >/dev/null; then
    if [ -n "$tag" ]; then
      err "cannot resolve $from to find what $plugin changes; fetch the tags (fetch-depth: 0)"
    else
      ok "cannot resolve $from (no tags fetched?); Upgrading note not checked"
    fi
    return
  fi
  local changed
  changed=$(git diff --name-only "$from" HEAD -- scripts templates/workspace)
  if [ -z "$changed" ]; then
    ok "$plugin changes nothing in scripts/ or templates/workspace/ since $from; no Upgrading note needed"
    return
  fi
  # The section's last paragraph: the lines after the last blank line before
  # the next heading.
  local last
  last=$(awk -v v="$plugin" '
    /^## / { if (inside) exit; if ($2 == v) { inside=1; next } }
    inside && /^[[:space:]]*$/ { gap=1; next }
    inside { if (gap) { para=""; gap=0 } para = para $0 "\n" }
    END { printf "%s", para }' CHANGELOG.md)
  if printf '%s' "$last" | head -n 1 | grep -qE '^Upgrading from '; then
    ok "the $plugin section ends with an Upgrading note"
  else
    err "$plugin changes scripts/ or templates/workspace/ since $from, but its CHANGELOG section does not end with an 'Upgrading from X.Y' note. Changed:"
    printf '%s\n' "$changed" | sed 's/^/  /' >&2
  fi
}
note_check

if [ "$fail" = 0 ]; then
  ok "versions agree: $plugin${tag:+ ($tag)}"
  if [ -n "$notes" ]; then
    # The section between its heading and the next one, without the blank
    # lines at either end.
    awk -v v="$plugin" '
      /^## / { if (inside) exit; if ($2 == v) { inside=1; next } }
      inside { lines[++n] = $0 }
      END {
        first = 1; while (first <= n && lines[first] ~ /^[[:space:]]*$/) first++
        last = n; while (last >= first && lines[last] ~ /^[[:space:]]*$/) last--
        for (i = first; i <= last; i++) print lines[i]
      }' CHANGELOG.md > "$notes"
    ok "wrote the $plugin release notes to $notes"
  fi
fi
exit "$fail"
