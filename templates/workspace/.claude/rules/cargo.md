---
paths:
  - "**/Cargo.toml"
  - "Cargo.lock"
---

# Manifest rules (rust-craft)

- Declare third-party dependencies once, in the root `[workspace.dependencies]`. Members use `name = { workspace = true }` (ARC-12).
- Adding a dependency is a human decision (H-16). Justify it with maintenance status, size, license and `cargo deny check`. Enable minimal features and use `default-features = false` where it helps.
- Every member has `[lints]\nworkspace = true`. Lint levels live only in the root `[workspace.lints]` (MOD-09). Changing them is H-05 and needs human approval.
- Members inherit `edition`, `rust-version`, `license` and `version` from `[workspace.package]` (ARC-01).
- Features are additive and never change a type's meaning (API-16).
- Respect dependency direction: domain crates never depend on adapters, runtimes or frameworks (ARC-02). Check with `cargo tree -e normal -p <crate>`.
- Never hand-edit `Cargo.lock`.
