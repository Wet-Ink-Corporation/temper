# Testing and TDD

How to write tests in Rust: the red-green-refactor loop, where each kind of test lives, what makes an assertion strong, and how to use fakes, builders, property tests, snapshots, compile-fail tests and benchmarks. It also covers H-12 in depth: never weaken verification to get green. Read this before writing the first test of a change, when a test fails and you are tempted to edit it, and when reviewing a diff that touches `tests/`, `#[cfg(test)]` or snapshots.

Rules covered: TST-01, TST-02, TST-03, TST-04, TST-05, TST-06, TST-07, TST-08, TST-09, TST-10, TST-11, TST-12, TST-13, TST-14, TST-15, TST-16, H-12

Test code may use `unwrap`, `expect("why")`, `assert!`, indexing and panics (the template's `clippy.toml` allows them in tests). It may not use `unsafe`, `dbg!`, `#[allow]`, sleeps, or `#[ignore]` without a reason string.

## The TDD loop in Rust

### TST-01 · Red, green, refactor
**Why.** A test that has never failed has never been shown to test anything. Watching it fail for the expected reason proves the assertion is wired to the behavior; writing the code first means the test is shaped around whatever the code happened to do.

1. **Red.** Write one test from the plan's test list. Run it: `cargo test -p <crate> <test_name>`. It must fail *for the right reason*: the assertion names the missing behavior, or the compiler reports the missing item.
2. **A compile error counts as red** only when it is about the API you are about to write: `E0425 cannot find function`, `E0599 no method named withdraw`, a missing variant or field. A type error in the test itself, a missing import, or a typo is not red; fix it and run again.
3. **Green.** Write the least code that passes. Hard-coding is fine if the next test on the list will force the general version. No escape hatches, not even temporarily: no `todo!()`, `unwrap()` or `#[allow]` "for now".
4. **Refactor.** With everything green, fix names, ownership, duplication and placement, then run `cargo clippy --all-targets`. Behavior must not change; if a test needs editing during refactor, you changed behavior.
5. **One behavior per cycle** (PROC-05). If the next step needs two new assertions about different behaviors, it is two cycles.
6. **Bugs start with a regression test.** Reproduce the bug as a failing test first, confirm it fails on the current code for the reported reason, then fix. Name it after the behavior, not the ticket (`rejects_negative_quantity`, with the issue in a comment).

**Do.** One cycle for `Account::withdraw` (code in TST-03 and TST-04 below): write `withdrawal_within_balance_returns_remaining_balance`; red is `E0599 no method named withdraw`; green is a `withdraw` that subtracts. Next cycle: `rejects_withdrawal_over_balance_and_keeps_balance`; red is the assertion failure (or the `attempt to subtract with overflow` panic); green adds `checked_sub` and the `InsufficientFunds` variant. Refactor: `let ... else` for the early return.
**Don't.**
```rust,ignore
// Implementation written first, then a test that asserts whatever it returned.
// The test cannot fail for the right reason because it never failed at all.
#[test]
fn withdraw_works() {
    assert!(Account::with_balance(Cents(10)).withdraw(Cents(3)).is_ok());
}
```
**Detect.** Plan has a test list; the diff adds tests alongside (not after) code; bug-fix diffs contain a test that fails on the base revision (`git stash` the fix and run it).

## Where tests live

### TST-02 · Test through the narrowest real seam
**Why.** Unit tests beside the code can reach private items without widening visibility; integration tests in `tests/` compile as separate crates and so can only see the public API, which keeps them honest.

| Kind | Location | Sees | Use for |
|---|---|---|---|
| Unit | `#[cfg(test)] mod tests` at the bottom of the module | private items via `use super::*` | pure logic, invariants, error paths |
| Integration | `tests/it/main.rs` plus `tests/it/<area>.rs` | public API only | workflows across modules, adapters against fakes |
| Doctest | `///` examples on public items | public API only | usage contract of entry points (TST-11) |
| Compile-fail | `tests/ui/*.rs` driven by trybuild, or `compile_fail` doctests | public API only | typestate, sealing, `Send` bounds (TST-08) |
| Benchmark | `benches/<name>.rs` with `harness = false` | public API only | performance claims (TST-16) |

Never make an item `pub` or `pub(crate)` only so a test can call it (API-14). If a private helper needs direct testing, it lives in the same module as its unit tests; if an integration test needs internals, the seam is wrong.

### TST-14 · One integration binary, shared support, builders
**Why.** Each file directly under `tests/` is linked as its own crate, so N files cost N link steps and cannot share helpers without `mod` tricks. One binary with modules links once and shares `support.rs`.
```text
crates/shop/
├── src/...
├── tests/
│   ├── it/
│   │   ├── main.rs         # declares the modules below; nothing else
│   │   ├── support.rs      # shared fixtures and builders, pub(crate)
│   │   ├── connection.rs
│   │   └── compile_fail.rs
│   └── ui/                 # trybuild inputs; not a test target
│       ├── send_on_closed.rs
│       └── send_on_closed.stderr
└── benches/encode.rs
```
**Do.**
```rust,ignore
// tests/it/main.rs (compiled; shown as a sketch because it references sibling files)
//! Integration tests for `shop`, compiled as one binary.

mod compile_fail;
mod connection;
#[cfg(test)] // lets clippy's allow-*-in-tests cover helpers, not only #[test] fns
mod support;
```
Helpers in `support.rs` are `pub(crate)`, because plain `pub` trips `unreachable_pub`. clippy's test allowances (`allow-expect-in-tests` and the others) cover only `#[test]` functions and `#[cfg(test)]` modules, not plain helper functions in `support.rs`. So make fixtures return `Result` and call them with `?` (TST-04), which also gives better failure messages. Keep `clippy::tests_outside_test_module` off; the workspace template leaves it off because it fires on every `#[test]` in `tests/`.

## Naming and shape

### TST-03 · Behavioral names, one behavior, Arrange-Act-Assert
**Why.** The test name is the failure message a human reads first. `rejects_withdrawal_over_balance_and_keeps_balance` says what broke; `test_withdraw_2` does not. One behavior per test means one reason to fail.

**Do.** Name `<outcome>_<condition>` in the domain's words. Separate the three phases with blank lines; comments only when the phases are not obvious.
```rust
#[test]
fn rejects_withdrawal_over_balance_and_keeps_balance() {
    let mut account = Account::with_balance(Cents(10));

    let result = account.withdraw(Cents(11));

    assert_eq!(
        result,
        Err(WithdrawError::InsufficientFunds {
            balance: Cents(10),
            requested: Cents(11),
        })
    );
    assert_eq!(account, Account::with_balance(Cents(10)));
}
```
Two asserts are fine when they describe one behavior (reject *and* leave state untouched). Several Act steps in one test are not.

**Don't.**
```rust,ignore
#[test]
fn test_account() {           // name says nothing; three behaviors; later ones never run if the first fails
    let mut a = Account::with_balance(Cents(10));
    assert!(a.withdraw(Cents(3)).is_ok());
    assert!(a.withdraw(Cents(100)).is_err());
    assert_eq!(a.balance(), Cents(7));
}
```

## Result-returning tests

### TST-04 · Return `Result`, use `?` for setup
**Why.** `?` in a test that returns `Result<(), E>` fails the test with the error's `Debug` output, keeps setup readable, and exercises the same `?` plumbing callers use. When a step cannot fail by construction, `expect("why this holds")` documents the reasoning; bare `unwrap()` documents nothing.

**Do.**
```rust
#[test]
fn withdrawal_within_balance_returns_remaining_balance() -> Result<(), WithdrawError> {
    // Arrange
    let mut account = Account::with_balance(Cents(100));

    // Act
    let remaining = account.withdraw(Cents(30))?;

    // Assert
    assert_eq!(remaining, Cents(70));
    Ok(())
}
```
Mixed error types: return `Result<(), Box<dyn std::error::Error>>` in tests (tests are not library signatures, so H-09 does not apply). Never use `?` on the value under test when the test is about its error: assert on the `Result` itself.

### TST-11 · Runnable doctests with `?`
**Why.** Doctests are the documentation users copy. An example with `unwrap()` teaches `unwrap()`; one that does not run can drift from the API silently.

**Do.** Hide the `main` wrapper with `#` (body elided; the full function appears in the compiled crate):
```rust,ignore
/// Decodes lowercase hex into bytes.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), hexcodec::DecodeError> {
/// let bytes = hexcodec::decode("cafe")?;
/// assert_eq!(bytes, [0xca, 0xfe]);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns [`DecodeError::OddLength`] for an odd number of digits and
/// [`DecodeError::InvalidDigit`] for anything outside `0-9a-f`.
pub fn decode(hex: &str) -> Result<Vec<u8>, DecodeError> { /* ... */ }
```
**Detect.** `cargo test --doc`; grep doc comments for `unwrap()` and for fences marked `ignore` or `no_run` without a reason.

## Assertions that can fail

### TST-05 · Exact values and exact variants
**Why.** `assert!(r.is_ok())` passes for every `Ok`, including wrong ones; `assert!(r.is_err())` passes for the wrong error. Mutation testing (TST-13) finds these first. Comparing whole values also prints a structural diff on failure.

**Do.**
- `assert_eq!(actual, expected)` on the whole value, `Result` included: `assert_eq!(decode("0g"), Err(DecodeError::InvalidDigit { index: 0 }))`. Derive `PartialEq` on error enums to allow this.
- When a variant carries a field you cannot or should not construct (an `io::Error`, a timestamp), match structurally with `assert_matches!` (stable since 1.96, `use std::assert_matches;`) or `assert!(matches!(..))` (ERR-11).
- Never compare `Display` strings of errors; messages are for humans and change (ERR-11).
```rust
#[test]
fn rejection_reports_requested_amount() {
    let mut account = Account::with_balance(Cents(0));

    let result = account.withdraw(Cents(5));

    assert_matches!(
        result,
        Err(WithdrawError::InsufficientFunds {
            requested: Cents(5),
            ..
        })
    );
}
```
**Don't.**
```rust,ignore
assert!(place_order(&mut repo, &clock, cmd).is_ok());          // any Ok passes
assert!(result.is_err());                                      // any error passes
assert_eq!(err.to_string(), "quantity must be at least one");  // couples to wording
assert!(order.quantity > 0);                                   // expected value unknown
```
**Detect.** `rg 'assert!\([^)]*\.is_(ok|err|some|none)\(\)\)' --type rust`; `to_string()` inside `assert_eq!`.

## Fakes over mocks

### TST-09 · Hand-written fakes of port traits
**Why.** A fake is a small working implementation (a `HashMap` repository, a fixed clock) that behaves like the real thing, so tests assert outcomes. Mock frameworks assert call sequences, which couples tests to implementation and breaks on every refactor. Test-only branches (`if cfg!(test)`) mean the tested code is not the shipped code.

**Do.** Production code depends on port traits (ARC-03): `OrderRepository` (`find(&self, OrderId) -> Result<Option<Order>, StorageError>`, `save(&mut self, Order) -> Result<(), StorageError>`) and `Clock` (`now(&self) -> UnixSeconds`). `place_order(&mut impl OrderRepository, &impl Clock, PlaceOrder)` never touches I/O or the system clock. Tests pass fakes:
```rust
// Inside `#[cfg(test)] mod tests` of the module that defines `place_order`.
/// In-memory fake of the repository port: real behavior, no I/O.
#[derive(Debug, Default)]
struct InMemoryOrders {
    orders: HashMap<OrderId, Order>,
}

impl OrderRepository for InMemoryOrders {
    fn find(&self, id: OrderId) -> Result<Option<Order>, StorageError> {
        Ok(self.orders.get(&id).cloned())
    }

    fn save(&mut self, order: Order) -> Result<(), StorageError> {
        self.orders.insert(order.id, order);
        Ok(())
    }
}

/// Fake that fails every call, for error-path tests.
#[derive(Debug)]
struct UnavailableOrders;

impl OrderRepository for UnavailableOrders {
    fn find(&self, _id: OrderId) -> Result<Option<Order>, StorageError> {
        Err(StorageError)
    }

    fn save(&mut self, _order: Order) -> Result<(), StorageError> {
        Err(StorageError)
    }
}

#[derive(Debug, Clone, Copy)]
struct FixedClock(UnixSeconds);

impl Clock for FixedClock {
    fn now(&self) -> UnixSeconds {
        self.0
    }
}

#[test]
fn surfaces_storage_failure() {
    let result = place_order(
        &mut UnavailableOrders,
        &FixedClock(NOW),
        a_place_order().build(),
    );

    assert_eq!(result, Err(PlaceOrderError::Storage(StorageError)));
}
```
Fakes used by several test files go in `tests/it/support.rs`, or behind a `test-support` Cargo feature when other crates need them (additive, API-16).
**Don't.**
```rust,ignore
pub fn now() -> UnixSeconds {
    if cfg!(test) { return UnixSeconds(0); }       // test-only branch: shipped code is untested
    UnixSeconds(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
mock.expect_find().times(1).returning(|_| Ok(None)); // asserts call count, not outcome
```
**Detect.** `cfg!(test)` or `#[cfg(test)]` inside function bodies of production code; `mockall` or similar outside external seams (HTTP clients, cloud SDKs).

## Test data builders

Part of TST-14. **Why.** A builder with valid defaults lets each test state only the field it is about, so adding a field to the type touches one function, not fifty literals.
```rust
/// Test data builder: valid defaults, override only what the test is about.
struct PlaceOrderBuilder(PlaceOrder);

fn a_place_order() -> PlaceOrderBuilder {
    PlaceOrderBuilder(PlaceOrder {
        id: OrderId(1),
        sku: "SKU-1".to_owned(),
        quantity: 1,
    })
}

impl PlaceOrderBuilder {
    fn id(mut self, id: u64) -> Self {
        self.0.id = OrderId(id);
        self
    }

    fn quantity(mut self, quantity: u32) -> Self {
        self.0.quantity = quantity;
        self
    }

    fn build(self) -> PlaceOrder {
        self.0
    }
}
```
Usage: `place_order(&mut repo, &FixedClock(NOW), a_place_order().quantity(0).build())`. The test reads as the one fact it checks.

## Property-based testing

### TST-06 · Properties for invariants, round-trips and laws
**Why.** Example tests check the cases you thought of. A property test generates hundreds of inputs and shrinks any failure to a minimal case, which is where parser and codec bugs live (empty input, odd lengths, boundary bytes).

**Do.** Typical properties: round-trip (`decode(encode(x)) == x`), invariants (output shape), no-panic on arbitrary input, and algebraic laws (idempotence, commutativity, `evolve` then `decide` consistency). Keep one example test per property for readability. Commit `proptest-regressions/` files: they are the regression suite for past shrunk failures.
```rust
// In `#[cfg(test)] mod tests` with `use proptest::collection::vec; use proptest::prelude::*;`
proptest! {
    #[test]
    fn decode_inverts_encode(bytes in vec(any::<u8>(), 0..256)) {
        prop_assert_eq!(decode(&encode(&bytes)), Ok(bytes));
    }

    #[test]
    fn encoding_doubles_length_and_uses_only_lowercase_hex(
        bytes in vec(any::<u8>(), 0..256),
    ) {
        let hex = encode(&bytes);
        prop_assert_eq!(hex.len(), bytes.len() * 2);
        prop_assert!(hex.bytes().all(|c| nibble(c).is_some()));
    }

    #[test]
    fn decode_never_panics_on_arbitrary_input(input in ".{0,64}") {
        // Only the absence of a panic is asserted: any Result is acceptable.
        let _outcome = decode(&input);
    }
}
```
Use `prop_assert*!` inside `proptest!` so failures shrink instead of aborting. Bound generated sizes (`0..256`) so the suite stays fast (TST-15). A property that re-implements the function under test proves nothing; state a relation, not the algorithm.

## Snapshot testing

### TST-07 · Snapshots for structured and textual output
**Why.** Hand-written expected strings for rendered text or JSON are tedious, so people assert on fragments and miss regressions. A snapshot captures the whole output and turns every change into a reviewable diff.

**Do.** `insta` with the `json` and `redactions` features (dev-dependency; adding it is H-16). Redact values that are volatile by nature (generated ids, times you do not control); inject the clock where you can instead (TST-10).
```rust
#[test]
fn renders_lines_and_total() -> fmt::Result {
    let mut text = String::new();

    sample().render(&mut text)?;

    insta::assert_snapshot!(text);
    Ok(())
}

#[test]
fn serializes_machine_receipt() {
    insta::assert_json_snapshot!(sample(), {
        ".id" => "[id]",
        ".issued_at" => "[timestamp]",
    });
}
```
Workflow: a new or changed snapshot fails the test and writes a `.snap.new`. Run `cargo insta review` (or `cargo insta test --review` to run and review in one step), read every diff, accept only changes the task intended, and commit the `.snap` files with the code. Snapshots are code: reviewers read them (TST-07). Accepting a changed snapshot is changing a test's expectation (H-12, H-16) unless the plan says the output changes.

## Compile-fail tests

### TST-08 · Prove type-level guarantees
**Why.** Typestate, sealed traits and `Send` bounds are guarantees about what does *not* compile. Without a test, a refactor can quietly make the illegal call legal.

**Do.** Pin the guarantee with `trybuild` and a committed `.stderr` (below). That is the only form that checks *why* the code fails to compile. A `compile_fail` doctest is useful documentation, but on stable rustdoc its error code (`compile_fail,E0599`) is **not checked**: a typo or an unrelated type error also "passes". Use a doctest to show the illegal call, and trybuild to prove it:
```rust
/// A connection in state `S`.
///
/// Sending requires an open connection; this does not compile:
///
/// ```compile_fail,E0599
/// let conn = shop::connection::Connection::new("db:5432");
/// conn.send(b"ping");
/// ```
#[derive(Debug)]
pub struct Connection<S> {
    addr: String,
    sent: usize,
    state: PhantomData<S>,
}
```
The proof: `trybuild` (a pre-approved dev-dependency in the template) inside the integration binary:
```rust,ignore
// tests/it/compile_fail.rs (compiled as part of the `it` binary)
#[test]
fn typestate_misuse_is_rejected_at_compile_time() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
```
Each `tests/ui/<case>.rs` has a committed `<case>.stderr`. Generate it once with `TRYBUILD=overwrite cargo test`, read it, and commit it. Review later diffs in `.stderr` like snapshots. Changing a committed `.stderr` is a change of expectation (H-12).

## Determinism

### TST-10 · No sleeps, no wall clock, no network, no shared state
**Why.** A flaky test trains everyone to re-run until green, which hides real failures. Sleeps make tests slow *and* flaky; wall-clock reads, unseeded randomness, real sockets, shared temp paths and global state make results depend on the machine and on test order.

| Source of nondeterminism | Replace with |
|---|---|
| `SystemTime::now()`, `Instant::now()` in logic | a `Clock` port and `FixedClock` fake (see Fakes) |
| `thread::sleep`, `tokio::time::sleep` to "wait for" something | tokio paused time; or synchronize on the event (channel, `JoinHandle`) |
| unseeded RNG | an injected RNG seeded per test (`StdRng::seed_from_u64(7)`); proptest manages its own seeds |
| real network or database | fakes of the port; adapter tests against a local container, tagged and isolated (TST-15) |
| fixed temp paths, env vars, globals | `tempfile::tempdir()` per test; config passed as values (ARC-11) |
| `HashMap` iteration order in output | sort, or `BTreeMap` (OUT-04) |

**Do.** `#[tokio::test(start_paused = true)]` freezes the tokio clock; when every task is idle it jumps straight to the next timer, so time-based tests run instantly and elapsed times are exact. Requires tokio's `test-util` feature in dev-dependencies.
```rust
#[tokio::test(start_paused = true)]
async fn succeeds_on_third_probe_after_two_intervals() {
    let mut probes = 0;
    let start = Instant::now();

    let result = wait_until_ready(
        || {
            probes += 1;
            probes == 3
        },
        INTERVAL,
        Duration::from_secs(5),
    )
    .await;

    assert_eq!(result, Ok(3));
    assert_eq!(start.elapsed(), INTERVAL * 2);
}
```
Here `Instant` is `tokio::time::Instant`; `std::time::Instant` ignores the paused clock. Paused time needs the current-thread runtime, which is the `#[tokio::test]` default.
**Don't.**
```rust,ignore
spawn_worker();
std::thread::sleep(Duration::from_millis(200)); // hope the worker finished: slow and flaky
assert_eq!(counter.load(Ordering::SeqCst), 1);
```
**Detect.** `rg 'sleep\(|SystemTime::now|Instant::now|thread_rng' --type rust` in test code and in logic reachable from tests; tests that pass alone but fail under `cargo nextest run` (parallel, separate processes) or in reverse order.

## Running tests fast

### TST-15 · Keep feedback fast
**Why.** The TDD loop runs tests dozens of times per change; a slow suite gets skipped.
- Run the narrowest set while iterating: `cargo test -p <crate> <filter>`, then the full gate.
- Use `cargo nextest run --workspace` when installed: one process per test, parallel, clearer failure output. **nextest does not run doctests**, so the gate also runs `cargo test --workspace --doc`.
- Unit tests take milliseconds. Tests that need containers, large fixtures or real I/O go in a separate integration module or crate, selected by a filter or a Cargo feature, and still run in CI. Do not mark them `#[ignore]` to hide slowness (H-12).
- Keep `proptest` case counts at the default or lower in the inner loop; raise them in CI with `PROPTEST_CASES`.
- Quiet output saves context (PROC-02): `cargo test -q`, `cargo nextest run --status-level fail`.

## Coverage and mutation testing

### TST-13 · Strength over coverage
**Why.** Coverage says a line ran, not that any assertion would notice if it were wrong. Mutation testing changes the code (flips `<` to `<=`, replaces a body with `Default::default()`) and checks that some test fails.
- Coverage as a signal: `cargo llvm-cov --workspace --html` (or `--lcov` for CI) to find untested branches. Never add tests only to raise a percentage.
- Mutants on changed code: `git diff origin/main > changes.diff && cargo mutants --in-diff changes.diff` (add `--package <crate>` to narrow). Results land in `mutants.out/` (`caught.txt`, `missed.txt`, `timeout.txt`, `unviable.txt`).
- **Interpreting survivors (missed).** Each one names a change no test detected. Either add the missing assertion (usually an exact value where the test had `is_ok()`, or a boundary case), or the mutated code is dead or equivalent, in which case delete or simplify it. Never exclude a mutant just to get a clean report; exclusions are H-05-style exceptions and need a reason.
- `unviable` means the mutant did not compile: fine. `timeout` usually means the mutation created an infinite loop: treated as caught, but check it is not a slow test.

## Never weaken verification

### H-12 · TST-12 · Red means something is wrong: decide what
**Why.** Tests, lints and hooks are the only evidence that the code does what the plan says. Making a check pass by changing the check destroys that evidence while reporting success.

**Forbidden moves** (each needs an explicit human decision, H-16, recorded in the plan):
- Deleting a test, a test module, a property, a snapshot or a trybuild case.
- Adding `#[ignore]`, or `#[ignore = "reason"]` to a failing test, to get green.
- Loosening an assertion: `assert_eq!` to `assert!(..is_ok())`, exact value to range, full struct to one field, `assert_matches!` with fewer fields.
- Changing an expected value to match what the code now returns.
- Catching a panic to pass (`std::panic::catch_unwind` is also a disallowed method), or adding `#[should_panic]` to a test that used to assert a value.
- `#[cfg(...)]`-ing a test out, gating it behind a feature CI does not enable, or moving it to a target that is not run.
- Accepting snapshots blindly: `cargo insta accept`, `cargo insta test --accept`, `INSTA_UPDATE=always`, or `TRYBUILD=overwrite` without reading every diff.
- Lowering lints or skipping checks: `#[allow]`, `#[expect]` without approval (H-05), editing `[lints]`, `clippy.toml`, `rustfmt.toml`, CI or hook config, `--no-verify`, `SKIP=` variables.
- Weakening the check's input: shrinking proptest ranges or case counts to dodge a failing case, deleting `proptest-regressions` entries.

**Do instead.**
1. Read the failure fully (PROC-12). Reproduce it with the narrowest command.
2. Decide explicitly, and write down which: **the code is wrong** (fix the code; the test stays) or **the test is wrong** (its expectation contradicts the agreed behavior).
3. If the test is wrong, stop and raise a HITL decision (H-16) with evidence: the requirement, the old expectation, the proposed one. Do not edit the test until the human agrees.
4. You may update an expectation (a test, a snapshot or a `.stderr`) without asking again only when the plan's Decisions table records a **human** approval of that specific behavior change. A plan an agent wrote does not count on its own. Call the change out in the Status block ("Changed expectation: `renders_lines_and_total` snapshot, per decision D2, approved by <name>"). Otherwise, return `Status: BLOCKED` with the question.
5. A flaky test is a bug: fix the nondeterminism (TST-10); do not retry or ignore it.
**Detect.** The temper hook prompts the human on test removal and on edits to protected config. Reviewers diff `tests/`, `*.snap`, `*.stderr`, `proptest-regressions/`, `[lints]` and CI files first and ask for the H-16 record for every weakened line.

## Benchmarks

### TST-16 · Every performance claim ships with a benchmark
**Why.** "Faster" without a measurement is a guess, and a later change can silently undo it. A committed benchmark makes the claim reproducible (PERF-01).

**Do.** `divan` (or `criterion`) as a dev-dependency (H-16), a `[[bench]]` target with `harness = false`, inputs at several sizes, and `black_box` on inputs. Report before and after numbers from the same machine in the change description.
```toml
[[bench]]
name = "encode"
harness = false
```
```rust
//! Benchmarks for hex encoding and decoding (TST-16).

fn main() {
    divan::main();
}

#[divan::bench(args = [16, 1024, 65_536])]
fn encode(bencher: divan::Bencher<'_, '_>, len: usize) {
    let input = vec![0xAB_u8; len];
    bencher
        .counter(divan::counter::BytesCount::new(len))
        .bench(|| hexcodec::encode(divan::black_box(&input)));
}

#[divan::bench(args = [16, 1024, 65_536])]
fn decode(bencher: divan::Bencher<'_, '_>, len: usize) {
    let input = hexcodec::encode(&vec![0xAB_u8; len]);
    bencher
        .counter(divan::counter::BytesCount::new(input.len()))
        .bench(|| hexcodec::decode(divan::black_box(&input)));
}
```
Run with `cargo bench -p hexcodec`. Build inputs outside the measured closure; return the result from the closure so it is not optimized away. Benches are not production code, but `harness = false` targets are not compiled with `cfg(test)`, so the lint config still denies `expect`/`unwrap` there: build inputs that cannot fail, or return the `Result` from the closure.

## Test smells for reviewers

| Smell | What it hides | Rule |
|---|---|---|
| `assert!(x.is_ok())` / `is_err()` / `is_some()` alone | wrong values pass | TST-05 |
| Expected value computed by the same logic as the code | the bug is in both | TST-05, TST-06 |
| Test name `test_foo`, `works`, `it_works`, ticket numbers | unclear failure meaning | TST-03 |
| Several Act steps, or asserts about unrelated behaviors | first failure masks the rest | TST-03 |
| `sleep`, `Instant::now`, `SystemTime::now`, real ports or URLs | flakiness | TST-10 |
| `pub` or `pub(crate)` added in the diff only used by tests | widened API | TST-02, API-14 |
| `cfg!(test)` in production logic | shipped code untested | TST-09 |
| Mock expectations on call counts and argument order | refactor-hostile tests | TST-09 |
| 30-line struct literals repeated across tests | brittle setup | TST-14 |
| Snapshot files changed without the plan saying output changes | blind accept | TST-07, H-12 |
| `#[ignore]`, deleted tests, loosened asserts, lowered lints | weakened verification | H-12 |
| `#[should_panic]` without `expected = "..."` | any panic passes | TST-05 |
| Test passes on the base revision for a bug-fix change | no regression test | TST-01 |
| Performance claim with no bench target in the diff | unverifiable claim | TST-16 |

## Review checklist
- [ ] Does every new behavior have a test that was red first, and does every bug fix add a regression test that fails without the fix? (TST-01)
- [ ] Are unit tests in `#[cfg(test)] mod tests` and integration tests public-API-only, with no visibility widened for tests? (TST-02)
- [ ] Do names state behavior, one behavior per test, Arrange-Act-Assert? (TST-03)
- [ ] Do tests use `?` for setup and `expect("reason")` over `unwrap()`? (TST-04)
- [ ] Are whole values and exact error variants asserted, with no bare `is_ok()`/`is_err()`? (TST-05)
- [ ] Do parsers, codecs and domain rules have property tests for round-trips and invariants? (TST-06)
- [ ] Are textual and structured outputs snapshotted, and were changed snapshots reviewed, not blind-accepted? (TST-07)
- [ ] Are typestate and sealing guarantees proven by compile-fail tests with error codes? (TST-08)
- [ ] Are doubles hand-written fakes of ports, with no test-only branches in production code? (TST-09)
- [ ] Is every test free of sleeps, wall clock, real network, shared state and order dependence? (TST-10)
- [ ] Do public entry points have runnable doctests using `?`? (TST-11)
- [ ] Was any test deleted, ignored, loosened or re-expected, any snapshot accepted, or any lint lowered, and is there an H-16 record for each? (TST-12, H-12)
- [ ] Were surviving mutants on changed code killed or explained? (TST-13)
- [ ] Is test data built with builders, and do integration tests compile as one `tests/it` binary? (TST-14)
- [ ] Does the suite stay fast, with doctests run separately when nextest is used? (TST-15)
- [ ] Does every performance claim come with a benchmark in the diff? (TST-16)
