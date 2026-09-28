# Testing and TDD

How to write tests in Rust: the red-green-refactor loop, where each kind of test lives, what makes an assertion strong, and how to use fakes, builders, property tests, snapshots, compile-fail tests and benchmarks. It also covers H-12 in depth: never weaken verification to get green. Read this before writing the first test of a change, when a test fails and you are tempted to edit it, and when reviewing a diff that touches `tests/`, `#[cfg(test)]` or snapshots.

Rules covered: TST-01, TST-02, TST-03, TST-04, TST-05, TST-06, TST-07, TST-08, TST-09, TST-10, TST-11, TST-12, TST-13, TST-14, TST-15, TST-16, TST-17, TST-18, H-12

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
```rust,ignore,expect=scan:weak-assert
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
```rust,fragment
// tests/it/main.rs (a sketch: it declares sibling files)
//! Integration tests for `shop`, compiled as one binary.

mod compile_fail;
mod connection;
#[cfg(test)] // lets clippy's allow-*-in-tests cover helpers, not only #[test] fns
mod support;
```
Helpers in `support.rs` are `pub(crate)`, because plain `pub` trips `unreachable_pub`. clippy's test allowances (`allow-expect-in-tests` and the others) cover `#[test]` functions and `#[cfg(test)]` modules, so declaring the module `#[cfg(test)] mod support;` puts its helpers under them; a helper module declared without it does not get them. Fixtures still return `Result` and are called with `?` (TST-04), because that gives better failure messages than a panic inside a helper. Keep `clippy::tests_outside_test_module` off; the workspace template leaves it off because it fires on every `#[test]` in `tests/`.

## Naming and shape

### TST-03 · Behavioral names, one behavior, Arrange-Act-Assert
**Why.** The test name is the failure message a human reads first. `rejects_withdrawal_over_balance_and_keeps_balance` says what broke; `test_withdraw_2` does not. One behavior per test means one reason to fail.

**Do.** Name `<outcome>_<condition>` in the domain's words. Separate the three phases with blank lines; comments only when the phases are not obvious.
```rust
# #[derive(Debug, Clone, Copy, PartialEq, Eq)]
# pub struct Cents(pub u64);
# #[derive(Debug, PartialEq, Eq)]
# pub enum WithdrawError { InsufficientFunds { balance: Cents, requested: Cents } }
# #[derive(Debug, PartialEq, Eq)]
# pub struct Account { balance: Cents }
# impl Account {
#     pub fn with_balance(balance: Cents) -> Self { Self { balance } }
#     pub fn withdraw(&mut self, amount: Cents) -> Result<Cents, WithdrawError> {
#         let Some(rest) = self.balance.0.checked_sub(amount.0) else {
#             return Err(WithdrawError::InsufficientFunds { balance: self.balance, requested: amount });
#         };
#         self.balance = Cents(rest);
#         Ok(self.balance)
#     }
# }
#
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
```rust,ignore,expect=scan:weak-assert
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
# #[derive(Debug, Clone, Copy, PartialEq, Eq)]
# pub struct Cents(pub u64);
# #[derive(Debug, PartialEq, Eq)]
# pub enum WithdrawError { InsufficientFunds { balance: Cents, requested: Cents } }
# #[derive(Debug, PartialEq, Eq)]
# pub struct Account { balance: Cents }
# impl Account {
#     pub fn with_balance(balance: Cents) -> Self { Self { balance } }
#     pub fn withdraw(&mut self, amount: Cents) -> Result<Cents, WithdrawError> {
#         let Some(rest) = self.balance.0.checked_sub(amount.0) else {
#             return Err(WithdrawError::InsufficientFunds { balance: self.balance, requested: amount });
#         };
#         self.balance = Cents(rest);
#         Ok(self.balance)
#     }
# }
#
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

**Do.** End the example with a hidden `# Ok::<(), ErrorType>(())` line: rustdoc wraps the body in a `main` that returns the block's value, so `?` works. The type annotation is required because nothing else names the error type. A hidden `# fn main() -> Result<(), E> { .. # }` wrapper does the same job in more lines.
```rust,fragment
/// Decodes lowercase hex into bytes.
///
/// # Examples
///
/// ```
/// let bytes = hexcodec::decode("cafe")?;
/// assert_eq!(bytes, [0xca, 0xfe]);
/// # Ok::<(), hexcodec::DecodeError>(())
/// ```
///
/// # Errors
///
/// Returns [`DecodeError::OddLength`] for an odd number of digits and
/// [`DecodeError::InvalidDigit`] for anything outside `0-9a-f`.
pub fn decode(hex: &str) -> Result<Vec<u8>, DecodeError> { /* ... */ }
```
When the example needs a value a doctest cannot build (an open connection, a pool), write the body inside a hidden function that takes it as a parameter. It type-checks against the real API but never runs, so keep assertions out of it, and never add a `#[doc(hidden)] pub` constructor just for docs (API-14):
```rust,fragment
/// ```
/// # fn demo(conn: &mut shop::Connection<shop::Open>) -> Result<(), shop::SendError> {
/// conn.send(b"ping")?;
/// # Ok(())
/// # }
/// ```
```
Examples under `examples/` follow production rules too: `main` returns `Result` or `ExitCode` and uses `?`. `cargo test --examples` only builds them and runs their `#[test]` functions, and an example whose `main` fails still reports `ok`, so CI runs each one with `cargo run --locked --example <name>`.
**Detect.** `cargo test --doc`; the scanner flags `unwrap`, `expect`, `panic!` and `todo!` inside doc-comment fences (`doc-unwrap`), except fences tagged `text`, `ignore` or `compile_fail`; review fences marked `ignore` or `no_run` without a reason.

## Assertions that can fail

### TST-05 · Exact values and exact variants
**Why.** `assert!(r.is_ok())` passes for every `Ok`, including wrong ones; `assert!(r.is_err())` passes for the wrong error. Mutation testing (TST-13) finds these first. Comparing whole values also prints a structural diff on failure. A test whose expected value copies a constant or mirrors the implementation's branches passes by construction, so it can never catch a bug.

**Do.**
- `assert_eq!(actual, expected)` on the whole value, `Result` included: `assert_eq!(decode("0g"), Err(DecodeError::InvalidDigit { index: 0 }))`. Derive `PartialEq` on error enums to allow this.
- When a variant carries a field you cannot or should not construct (an `io::Error`, a timestamp), match structurally with `assert_matches!` (stable since 1.96, `use std::assert_matches;`) or `assert!(matches!(..))` (ERR-11).
- Never compare `Display` strings of errors; messages are for humans and change (ERR-11).
```rust
# use std::assert_matches;
# #[derive(Debug, Clone, Copy, PartialEq, Eq)]
# pub struct Cents(pub u64);
# #[derive(Debug, PartialEq, Eq)]
# pub enum WithdrawError { InsufficientFunds { balance: Cents, requested: Cents } }
# #[derive(Debug, PartialEq, Eq)]
# pub struct Account { balance: Cents }
# impl Account {
#     pub fn with_balance(balance: Cents) -> Self { Self { balance } }
#     pub fn withdraw(&mut self, amount: Cents) -> Result<Cents, WithdrawError> {
#         let Some(rest) = self.balance.0.checked_sub(amount.0) else {
#             return Err(WithdrawError::InsufficientFunds { balance: self.balance, requested: amount });
#         };
#         self.balance = Cents(rest);
#         Ok(self.balance)
#     }
# }
#
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
```rust,ignore,expect=scan:weak-assert
# #[test]
# fn weak_assertions() {
assert!(place_order(&mut repo, &clock, cmd).is_ok());          // any Ok passes
assert!(result.is_err());                                      // any error passes
assert_eq!(err.to_string(), "quantity must be at least one");  // couples to wording
assert!(order.quantity > 0);                                   // expected value unknown
# }
```

**Tautologies.** For a constant or a table, assert the property it must have (spacing, ordering, round-trip), not its literal value:
```rust
/// Retry delays in milliseconds: exponential backoff from 100 ms.
pub const BACKOFF_MS: [u64; 4] = [100, 200, 400, 800];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_backoff_doubles_the_previous_one() {
        for pair in BACKOFF_MS.windows(2) {
            assert_eq!(pair[1], pair[0] * 2, "delays {pair:?} do not double");
        }
    }
}
```
```rust,ignore
assert_eq!(BACKOFF_MS, [100, 200, 400, 800]); // restates the constant: passes by construction
```
When only such a test could kill a mutant, the mutant is definitional: exclude it with a reason (TST-13) instead of writing the test.
**Detect.** `clippy::assertions_on_result_states` fails the gate on `assert!(r.is_ok())` and `assert!(r.is_err())`, tests included; the scanner's `weak-assert` also flags `is_some()`/`is_none()`. `to_string()` inside `assert_eq!`; an expected value that is a copy of a `const` or of the implementation's `match`.

## Fakes over mocks

### TST-09 · Hand-written fakes of port traits
**Why.** A fake is a small working implementation (a `HashMap` repository, a fixed clock) that behaves like the real thing, so tests assert outcomes. Mock frameworks assert call sequences, which couples tests to implementation and breaks on every refactor. Test-only branches (`if cfg!(test)`) mean the tested code is not the shipped code.

**Do.** Production code depends on port traits (ARC-03): `OrderRepository` (`find(&self, OrderId) -> Result<Option<Order>, StorageError>`, `save(&mut self, Order) -> Result<(), StorageError>`) and `Clock` (`now(&self) -> UnixSeconds`). `place_order(&mut impl OrderRepository, &impl Clock, PlaceOrder)` never touches I/O or the system clock. Tests pass fakes:
```rust
# use std::collections::HashMap;
# #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
# pub struct OrderId(pub u64);
# #[derive(Debug, Clone, Copy, PartialEq, Eq)]
# pub struct UnixSeconds(pub u64);
# #[derive(Debug, Clone, PartialEq, Eq)]
# pub struct PlaceOrder { pub id: OrderId, pub sku: String, pub quantity: u32 }
# #[derive(Debug, Clone, PartialEq, Eq)]
# pub struct Order { pub id: OrderId, pub sku: String, pub placed_at: UnixSeconds }
# #[derive(Debug, Clone, Copy, PartialEq, Eq)]
# pub struct StorageError;
# #[derive(Debug, PartialEq, Eq)]
# pub enum PlaceOrderError { Storage(StorageError) }
# pub trait OrderRepository {
#     fn find(&self, id: OrderId) -> Result<Option<Order>, StorageError>;
#     fn save(&mut self, order: Order) -> Result<(), StorageError>;
# }
# pub trait Clock { fn now(&self) -> UnixSeconds; }
# pub fn place_order(
#     repo: &mut impl OrderRepository, clock: &impl Clock, cmd: PlaceOrder,
# ) -> Result<OrderId, PlaceOrderError> {
#     let order = Order { id: cmd.id, sku: cmd.sku, placed_at: clock.now() };
#     repo.save(order).map_err(PlaceOrderError::Storage)?;
#     Ok(cmd.id)
# }
#
# #[cfg(test)]
# mod tests {
# use super::*;
# const NOW: UnixSeconds = UnixSeconds(1_700_000_000);
# struct PlaceOrderBuilder(PlaceOrder);
# fn a_place_order() -> PlaceOrderBuilder {
#     PlaceOrderBuilder(PlaceOrder { id: OrderId(1), sku: "SKU-1".to_owned(), quantity: 1 })
# }
# impl PlaceOrderBuilder { fn build(self) -> PlaceOrder { self.0 } }
#
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
# }
```
Fakes used by several test files go in `tests/it/support.rs`. When other crates need them, put them behind one feature named `test-util` (tokio's name for the same thing), enabled only from other crates' `[dev-dependencies]`. The feature only adds items (fakes, builders); it never changes the behaviour of existing code, which would be a test-only branch (API-16).
```toml
# crates/ledger-app/Cargo.toml
[features]
test-util = []   # exposes `ledger_app::fakes`, for other crates' tests only

# crates/ledger-cli/Cargo.toml
[dev-dependencies]
ledger-app = { workspace = true, features = ["test-util"] }
```
A shared test-support crate that depends on crate X is used only from X's `tests/`, never from X's own unit tests: as a dev-dependency of X it links a second copy of X into the unit-test build, and the compiler then rejects `X::Id` where it expects `X::Id`.
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
# #[derive(Debug, Clone, Copy, PartialEq, Eq)]
# pub struct OrderId(pub u64);
# #[derive(Debug, Clone, PartialEq, Eq)]
# pub struct PlaceOrder { pub id: OrderId, pub sku: String, pub quantity: u32 }
#
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
# #[derive(Debug, PartialEq, Eq)]
# pub enum DecodeError { OddLength, InvalidDigit { index: usize } }
# pub fn nibble(c: u8) -> Option<u8> {
#     match c { b'0'..=b'9' => Some(c - b'0'), b'a'..=b'f' => Some(c - b'a' + 10), _ => None }
# }
# pub fn encode(bytes: &[u8]) -> String {
#     let digit = |n: u8| char::from(if n < 10 { b'0' + n } else { b'a' + n - 10 });
#     bytes.iter().flat_map(|&b| [digit(b >> 4), digit(b & 0xf)]).collect()
# }
# pub fn decode(hex: &str) -> Result<Vec<u8>, DecodeError> {
#     let (pairs, []) = hex.as_bytes().as_chunks::<2>() else { return Err(DecodeError::OddLength) };
#     let digit = |c, index| nibble(c).ok_or(DecodeError::InvalidDigit { index });
#     let byte = |(i, &[hi, lo]): (usize, &[u8; 2])| {
#         let index = i.saturating_mul(2);
#         Ok((digit(hi, index)? << 4) | digit(lo, index.saturating_add(1))?)
#     };
#     pairs.iter().enumerate().map(byte).collect()
# }
#
# #[cfg(test)]
# mod tests {
# use super::*;
# use proptest::collection::vec;
# use proptest::prelude::*;
#
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
# }
```
Use `prop_assert*!` inside `proptest!` so failures shrink instead of aborting. Bound generated sizes (`0..256`) so the suite stays fast (TST-15). A property that re-implements the function under test proves nothing; state a relation, not the algorithm.

**Fuzzing.** A parser of attacker-controlled input (network protocol, file format, decoder) also gets a coverage-guided fuzz target, because under H-03 any panic reachable from input is a defect, and a bounded proptest string explores far less than a fuzzer. Use `cargo fuzz` (nightly) in a separate `fuzz/` crate outside the workspace gate. Adding it is H-16. Run it on a schedule, not per PR, and commit the corpus. Every crash becomes a failing regression test before the fix (TST-01).

## Snapshot testing

### TST-07 · Snapshots for structured and textual output
**Why.** Hand-written expected strings for rendered text or JSON are tedious, so people assert on fragments and miss regressions. A snapshot captures the whole output and turns every change into a reviewable diff.

**Do.** `insta` with the `json` and `redactions` features, as a dev-dependency (pre-approved in `.claude/temper.json`). Redact values that are volatile by nature (generated ids, times you do not control); inject the clock where you can instead (TST-10).
```rust,fragment
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
# use std::marker::PhantomData;
#
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
```rust,fragment
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
| fixture paths relative to the working directory | `Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orders.json")`, which holds however the test binary was launched |
| `HashMap` iteration order in output | sort, or `BTreeMap` (OUT-04) |

**Do.** `#[tokio::test(start_paused = true)]` freezes the tokio clock; when every task is idle it jumps straight to the next timer, so time-based tests run instantly and elapsed times are exact. Requires tokio's `test-util` feature in dev-dependencies.
```rust
# use std::time::Duration;
# use tokio::time::Instant;
# const INTERVAL: Duration = Duration::from_millis(250);
# #[derive(Debug, PartialEq, Eq)]
# pub struct NotReady;
# pub async fn wait_until_ready(
#     mut ready: impl FnMut() -> bool + Send, interval: Duration, timeout: Duration,
# ) -> Result<u32, NotReady> {
#     let deadline = Instant::now().checked_add(timeout).ok_or(NotReady)?;
#     let mut probes = 0_u32;
#     loop {
#         probes = probes.saturating_add(1);
#         if ready() { return Ok(probes); }
#         if Instant::now().checked_add(interval).is_none_or(|next| next > deadline) {
#             return Err(NotReady);
#         }
#         tokio::time::sleep(interval).await;
#     }
# }
#
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

Tests of blocking or coordination code (ASY-16) order threads with a `Barrier`, a channel or `join`, never a sleep, and assert that the wait really blocked: count wakeups or predicate checks and bound the count, so a wait that degraded into a spin fails.
**Don't.**
```rust,ignore,expect=scan:sleep
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

### H-12 · Red means something is wrong: decide what
**Why.** Tests, lints and hooks are the only evidence that the code does what the plan says. Making a check pass by changing the check destroys that evidence while reporting success.

**Forbidden moves** (each needs an explicit human decision, H-16, recorded in the plan):
- Deleting a test, a test module, a property, a snapshot or a trybuild case.
- Adding `#[ignore]` or `#[cfg_attr(.., ignore)]`, or `#[ignore = "reason"]` to a failing test, to get green.
- Loosening an assertion: `assert_eq!` to `assert!(..is_ok())`, exact value to range, full struct to one field, `assert_matches!` with fewer fields.
- Changing an expected value to match what the code now returns.
- Catching a panic to pass (`std::panic::catch_unwind` is also a disallowed method), or adding `#[should_panic]` to a test that used to assert a value.
- `#[cfg(...)]`-ing a test out (including `#![cfg(not(miri))]`), gating it behind a feature CI does not enable, moving it to a target that is not run, or changing `test`, `doctest`, `harness` or `autotests` in a manifest so that tests stop running.
- Accepting snapshots blindly: `cargo insta accept`, `cargo insta test --accept`, `INSTA_UPDATE=always`, or `TRYBUILD=overwrite` without reading every diff.
- Lowering lints or skipping checks: `#[allow]`, `#[expect]` without approval (H-05), editing `[lints]`, `clippy.toml`, `rustfmt.toml`, CI or hook config, `--no-verify`, `SKIP=` variables.
- Weakening the check's input: shrinking proptest ranges or case counts to dodge a failing case, deleting `proptest-regressions` entries.

**Do instead.**
1. Read the failure fully (PROC-12). Reproduce it with the narrowest command.
2. Decide explicitly, and write down which: **the code is wrong** (fix the code; the test stays) or **the test is wrong** (its expectation contradicts the agreed behavior).
3. If the test is wrong, stop and raise a HITL decision (H-16) with evidence: the requirement, the old expectation, the proposed one. Do not edit the test until the human agrees.
4. You may update an expectation (a test, a snapshot or a `.stderr`) without asking again only when the plan's Decisions table records a **human** approval of that specific behavior change. A plan an agent wrote does not count on its own. Call the change out in the Status block ("Changed expectation: `renders_lines_and_total` snapshot, per decision D2, approved by <name>"). Otherwise, return `Status: BLOCKED` with the question.
5. A flaky test is a bug: fix the nondeterminism (TST-10); do not retry or ignore it.
**Detect.** The temper hook prompts the human on test removal, on manifest switches that stop tests running (`test`, `doctest`, `harness`, `autotests`), and on edits to protected config. The scanner flags `#[ignore]` and `cfg_attr(.., ignore)` without a reason. Reviewers diff `tests/`, `*.snap`, `*.stderr`, `proptest-regressions/`, `[lints]` and CI files first and ask for the H-16 record for every weakened line.

### TST-12 · Never weaken tests
The testing face of H-12: removing a test or changing what it expects is a human decision (H-16), never a way to get green. The forbidden moves, the "do instead" steps and the detection are in H-12 above.

## Benchmarks

### TST-16 · Every performance claim ships with a benchmark
**Why.** "Faster" without a measurement is a guess, and a later change can silently undo it. A committed benchmark makes the claim reproducible (PERF-01).

**Do.** `divan` (or `criterion`) as a dev-dependency (both pre-approved in `.claude/temper.json`), a `[[bench]]` target with `harness = false`, inputs at several sizes, and `black_box` on inputs. Report before and after numbers from the same machine in the change description.
```toml
[[bench]]
name = "encode"
harness = false
```
```rust,fragment
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
Run with `cargo bench -p hexcodec`. Build inputs outside the measured closure; return the result from the closure so it is not optimized away. Benches follow production rules: `harness = false` targets are not compiled with `cfg(test)`, so the lint config still denies `expect`/`unwrap` there. Build inputs that cannot fail, or return the `Result` from the closure.

## Concurrency under Miri and on aarch64

### TST-17 · Concurrency code runs under Miri and on an ARM runner
**Why.** x86-64 orders memory so strongly that a wrong `Relaxed` almost never misbehaves there, while the same test fails on ARM. Miri emulates weak memory and explores a different thread schedule per seed, and it reports deadlocks, leaks and threads left running, all in safe code. A test that publishes data behind a `Relaxed` flag fails under Miri at the default seed; the `Release`/`Acquire` version passes. Passing on x86-64 alone is no evidence that the orderings are right.

**Scope.** Required for a crate with any non-`Relaxed` atomic ordering, a `Condvar`, or hand-written thread coordination (ASY-13, ASY-16). Worth it for any crate that spawns threads, holds locks across calls, or builds `Rc` graphs.

**Do.**
1. Keep the template's `miri` job: a pinned nightly with the `miri` component, run weekly and on demand. Each seed replays the tests under a different schedule. It turns isolation off so adapter tests can use the file system; the ARC-02 purity check is a separate run over the pure crates with isolation on. The `env-set` flags cut proptest to 8 cases (one 256-case property takes about 16 minutes under Miri) and stop it writing failure files:
```yaml
- run: cargo +nightly-2026-09-28 miri test --locked --workspace --no-fail-fast
  env:
    MIRIFLAGS: >-
      -Zmiri-many-seeds=0..8
      -Zmiri-disable-isolation
      -Zmiri-env-set=PROPTEST_CASES=8
      -Zmiri-env-set=PROPTEST_DISABLE_FAILURE_PERSISTENCE=1
```
2. Add an ARM job for the crates in scope:
```yaml
arm:
  runs-on: ubuntu-24.04-arm # GitHub-hosted arm64, public and private repositories
  steps:
    # checkout, toolchain "1.98.1" and cache: the same pinned steps as `gate`
    - run: cargo test --locked -p <crate>
```
3. A test Miri cannot run (process spawning, network, FFI) carries `#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]`, so it stays visible in the ignored count. Shrink expensive tests from the job's `MIRIFLAGS`, as above; when a loop must shrink, use a `cfg!(miri)` constant inside test code:
```rust
//! Test-side Miri idioms: fewer rounds under the interpreter, in test code only.

use std::sync::{Mutex, PoisonError};

/// A running total shared by worker threads.
#[derive(Debug, Default)]
pub struct Tally {
    total: Mutex<u64>,
}

impl Tally {
    /// Adds `amount`, saturating at `u64::MAX`.
    pub fn add(&self, amount: u64) {
        // One assignment cannot leave the total half-written, so a poisoned lock is still valid.
        let mut total = self.total.lock().unwrap_or_else(PoisonError::into_inner);
        *total = total.saturating_add(amount);
    }

    /// The current total.
    #[must_use]
    pub fn total(&self) -> u64 {
        *self.total.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::Tally;

    /// Miri interprets every step, so it runs fewer rounds. Never in production code.
    const ROUNDS: u64 = if cfg!(miri) { 4 } else { 1_000 };

    #[test]
    fn concurrent_adds_are_all_counted() {
        let tally = Tally::default();

        thread::scope(|scope| {
            let mut adders = Vec::new();
            for i in 0..2 {
                let adder = thread::Builder::new()
                    .name(format!("adder-{i}"))
                    .spawn_scoped(scope, || (0..ROUNDS).for_each(|_| tally.add(1)))
                    .expect("spawn adder");
                adders.push(adder);
            }
            for adder in adders {
                adder.join().expect("adder panicked");
            }
        });

        assert_eq!(tally.total(), 2 * ROUNDS);
    }
}
```
**Don't.**
```rust,ignore,expect=scan:ignore-no-reason
#![cfg(not(miri))]                                   // the whole file vanishes from Miri's count
#[cfg_attr(miri, ignore)]                            // no reason (H-12)
const RETRIES: u32 = if cfg!(miri) { 1 } else { 5 }; // production code now differs under test
```
Never add `-Zmiri-ignore-leaks`, `-Zmiri-fixed-schedule`, `-Zmiri-deterministic-concurrency`, or any `-Zmiri-disable-*` flag other than `-Zmiri-disable-isolation`, to get green: they stop Miri finding exactly these bugs, and several are marked unsound (H-12). A Miri report of a leak, a deadlock or "main thread terminated without waiting" is a blocker (ASY-04, H-13, OWN-11).
**Detect.** A crate in scope with no ARM job or no Miri coverage; `cfg!(miri)` or `cfg(miri)` outside `#[cfg(test)]` code; the flags above in `MIRIFLAGS`; the scanner flags `cfg_attr(.., ignore)` without a reason.
**Limits.** Miri runs one execution per seed, so a pass is evidence, not proof. It reports a thread locking a `Mutex` it already holds, but found no two-thread lock-order inversion in 64 seeds, so lock order stays a review item (ASY-14). It cannot spawn processes, use the network or call FFI, and it runs 3 to 4 orders of magnitude slower than native tests.

## Dependency behaviour

### TST-18 · Pin the dependency behaviour you rely on
**Why.** A dependency's signature says what compiles, not how it behaves. When correctness depends on sort stability, rounding, parser leniency or output order, an upgrade or a feature enabled elsewhere in the build can change the answer while everything still compiles. A small test in the consuming crate turns that into a red build.
**Do.** Name the assumption in the test's doc comment and assert exactly that behaviour:
```rust
#[cfg(test)]
mod tests {
    /// Report output relies on `serde_json::Value` objects serializing in key order
    /// (OUT-04). The `preserve_order` feature of `serde_json`, enabled anywhere in the build
    /// graph (ARC-16), would switch them to insertion order.
    #[test]
    fn json_objects_serialize_in_key_order() {
        let value = serde_json::json!({ "b": 1, "a": 2 });

        assert_eq!(value.to_string(), r#"{"a":2,"b":1}"#);
    }
}
```
The template's weekly `latest-deps` job (`cargo update`, then clippy and the tests, `continue-on-error`) is where these tests fire first: it runs against releases the lockfile has not taken yet. A failure there is a heads-up for the next bump (ARC-12), not a broken main.
**Don't.** Rely on the behaviour only through snapshot or end-to-end tests, where a change shows up as a large diff with no named cause, or not at all.
**Detect.** Code whose correctness rests on a dependency's ordering, rounding or leniency, with no test that names the assumption; a dependency bump PR whose diff contains no such test.

## Test smells for reviewers

| Smell | What it hides | Rule |
|---|---|---|
| `assert!(x.is_ok())` / `is_err()` / `is_some()` alone | wrong values pass | TST-05 |
| Expected value computed by the same logic as the code | the bug is in both | TST-05, TST-06 |
| Expected value copied from a `const` or table under test | passes by construction | TST-05 |
| `cfg!(miri)` in production code, `#![cfg(not(miri))]` on a test file | tested code differs from shipped code; tests hidden | TST-17, H-12 |
| Correctness resting on a dependency's ordering or rounding, with no test naming it | an upgrade changes answers silently | TST-18 |
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
- [ ] Are whole values and exact error variants asserted, with no bare `is_ok()`/`is_err()` and no expected value copied from the code under test? (TST-05)
- [ ] Do parsers, codecs and domain rules have property tests for round-trips and invariants, and do parsers of attacker-controlled input have a fuzz target? (TST-06)
- [ ] Are textual and structured outputs snapshotted, and were changed snapshots reviewed, not blind-accepted? (TST-07)
- [ ] Are typestate and sealing guarantees proven by compile-fail tests with error codes? (TST-08)
- [ ] Are doubles hand-written fakes of ports, with no test-only branches in production code, and shared fakes behind a `test-util` feature? (TST-09)
- [ ] Is every test free of sleeps, wall clock, real network, shared state and order dependence, with fixtures found through `CARGO_MANIFEST_DIR`? (TST-10)
- [ ] Do public entry points have runnable doctests using `?`? (TST-11)
- [ ] Was any test deleted, ignored, loosened or re-expected, any snapshot accepted, or any lint lowered, and is there an H-16 record for each? (TST-12, H-12)
- [ ] Were surviving mutants on changed code killed or explained? (TST-13)
- [ ] Is test data built with builders, and do integration tests compile as one `tests/it` binary? (TST-14)
- [ ] Does the suite stay fast, with doctests run separately when nextest is used? (TST-15)
- [ ] Does every performance claim come with a benchmark in the diff? (TST-16)
- [ ] Is a crate with non-`Relaxed` atomics, a `Condvar` or thread coordination covered by the Miri job and an ARM runner, with every Miri-ignored test giving a reason? (TST-17)
- [ ] Is every dependency behaviour the code relies on, but the signature does not express, pinned by a test? (TST-18)
