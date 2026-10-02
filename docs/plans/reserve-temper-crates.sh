#!/usr/bin/env bash
# Reserve the temper 2.0 crate names on crates.io (D2, wi-222f58) by publishing
# a 0.0.0 placeholder of each. Run from a clone of Wet-Ink-Corporation/temper,
# logged in with `cargo login` using a token scoped to "publish-new" for
# temper-* (short expiry; revoke it afterwards).
#   bash reserve-temper-crates.sh            # dry run: package only
#   bash reserve-temper-crates.sh --publish  # publish all four
set -euo pipefail
repo="$(git rev-parse --show-toplevel)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"
printf '[workspace]\nresolver = "3"\nmembers = ["temper-cli", "temper-domain", "temper-app", "temper-host"]\n' > Cargo.toml
for c in temper-cli temper-domain temper-app temper-host; do
  mkdir -p "$c/src"
  cp "$repo/LICENSE" "$c/LICENSE-MIT"
  curl -fsSL https://www.apache.org/licenses/LICENSE-2.0.txt -o "$c/LICENSE-APACHE"
  cat > "$c/Cargo.toml" <<EOF
[package]
name = "$c"
version = "0.0.0"
edition = "2024"
rust-version = "1.96"
license = "MIT OR Apache-2.0"
description = "Part of temper, the rust-craft Rust standard enforced for coding agents (Wet Ink Corporation). Name reserved; the first release is temper 2.0."
repository = "https://github.com/Wet-Ink-Corporation/temper"
readme = "README.md"
keywords = ["rust", "lint", "code-quality", "claude-code"]
categories = ["development-tools"]
EOF
  printf '# %s\n\nPart of [temper](https://github.com/Wet-Ink-Corporation/temper), Wet Ink Corporation'"'"'s rust-craft engineering standard for Rust, enforced for coding agents.\n\nThis 0.0.0 release only reserves the name. The first functional release is temper 2.0.\n' "$c" > "$c/README.md"
  if [ "$c" = temper-cli ]; then
    printf '//! Placeholder for the temper 2.0 CLI.\n\nfn main() {}\n' > "$c/src/main.rs"
  else
    printf '//! Placeholder: reserved for temper 2.0.\n' > "$c/src/lib.rs"
  fi
done
cargo package --workspace --allow-dirty
if [ "${1:-}" = "--publish" ]; then
  for c in temper-domain temper-app temper-host temper-cli; do
    cargo publish -p "$c" --allow-dirty
  done
  echo "Published. Revoke the token at https://crates.io/settings/tokens"
else
  echo "Dry run OK. Re-run with --publish to publish."
fi
