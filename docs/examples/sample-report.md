<!-- temper-report -->
### temper compliance: regressed · baseline raised

**REGRESSED.** 4 new findings since `origin/main` (3 on changed lines; 1 crate/lint count above the baseline on untouched lines): the ratchet fails. 195 legacy findings stay within the baseline.

> **BASELINE RAISED.** 2 counts higher in `.claude/temper-baseline.json` since `5e1f00d2c4b3`: the accepted debt grew. Raising the baseline is a human decision (H-12); check that this change was meant to.
> - `ledger-sync` `clippy::unwrap_used`: 9 → 12
> - `ledger-sync` `missing_docs`: 6 → 10

| MUST findings | H-rule findings | New | Fixed | Crates clean |
|---:|---:|---:|---:|:---|
| 144 of 198 | 144 | 4 | 5 | 2 of 6 (2 debt, 2 regressed) |

**Crates** (6; clean: no MUST findings, debt: MUST findings within the baseline, regressed: new findings)

| Crate | Status | MUST | SHOULD | Other | H | New | Fixed | Top rules |
|---|---|---:|---:|---:|---:|---:|---:|---|
| `ledger-app` | regressed | 26 | 11 | 0 | 26 | 1 | 0 | H-02 (12), API-11 (6), H-04 (6) |
| `ledger-cli` | regressed | 19 | 3 | 4 | 19 | 3 | 0 | H-02 (9), H-06 (8), MOD-02 (3) |
| `ledger-store-jsonl` | debt | 55 | 8 | 2 | 55 | 0 | 3 | H-02 (17), H-11 (12), H-15 (10) |
| `ledger-sync` | debt | 44 | 8 | 9 | 44 | 0 | 0 | H-02 (12), API-15 (10), H-15 (10) |
| `ledger-domain` | clean | 0 | 6 | 3 | 0 | 0 | 2 | API-11 (4), MOD-02 (2) |
| `ledger-bench` | clean | 0 | 0 | 0 | 0 | 0 | 0 | – |

**Top rules** (8 of 15)

| Rule | Level | Count | Rule text |
|---|---|---:|---|
| H-02 | MUST | 50 | No `unwrap`/`expect`/`unwrap_unchecked`/`unwrap_err`/`expect_err` in production code, inc… |
| H-15 | MUST | 25 | Zero warnings. |
| API-15 | MUST | 23 | Document every public item: purpose in a one-line summary, an `# Examples` doctest for pr… |
| H-04 | MUST | 23 | No panicking index or slice on data whose bounds are not proven by construction. |
| H-11 | MUST | 18 | No lossy `as` numeric casts, including enum discriminants: map a fieldless enum to a code… |
| API-11 | SHOULD | 15 | Mark values that must be used `#[must_use]`: pure constructors, builders, iterators and c… |
| H-08 | MUST | 12 | No clones to appease the borrow checker. |
| CRF-02 | SHOULD | 9 | A function does one thing at one level of abstraction. |

**New-code findings** (3, on lines changed since `origin/main`)

- `crates/ledger-cli/src/output.rs:185` `clippy::unwrap_used` (H-02): used `unwrap()` on a `Result` value
- `crates/ledger-cli/src/main.rs:63` `clippy::unwrap_used` (H-02): used `unwrap()` on a `Result` value
- `crates/ledger-cli/src/args.rs:82` `clippy::print_stdout` (H-06): use of `println!`

**Counts above the baseline on untouched lines** (1; a change can make unchanged code warn)

- `ledger-app` `clippy::indexing_slicing`: 5 → 6

<sub>temper 1.2.1 · commit 9f3c2e1a7b4d · toolchain 1.98.1 · generated 2026-10-01 16:00 UTC · details in report.html</sub>
