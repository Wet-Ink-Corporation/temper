// A realistic brownfield fixture for the report tests and the sample render:
// six crates, 198 findings, a baseline with fixes and one count increase, and
// the real ratchet() over a changed-lines map with three findings on changed
// lines. Deterministic.
import { ratchet } from '../../scripts/lib/baseline.mjs';

export const RULEMAP = {
  generated_from: 'rules.md',
  rules: {
    'H-02': { level: 'MUST', family: 'H', title: 'No `unwrap`/`expect`/`unwrap_unchecked`/`unwrap_err`/`expect_err` in production code, including doctest examples.' },
    'H-03': { level: 'MUST', family: 'H', title: 'No `panic!`/`todo!`/`unimplemented!`/`unreachable!` in production code, and no call to a std API that panics on input the code does not…' },
    'H-04': { level: 'MUST', family: 'H', title: 'No panicking index or slice on data whose bounds are not proven by construction.' },
    'H-06': { level: 'MUST', family: 'H', title: 'No `dbg!`.' },
    'H-08': { level: 'MUST', family: 'H', title: 'No clones to appease the borrow checker.' },
    'H-10': { level: 'MUST', family: 'H', title: 'No silently discarded failures (`let _ =`, `.ok();`, empty `Err` arms, masking defaults).' },
    'H-11': { level: 'MUST', family: 'H', title: 'No lossy `as` numeric casts, including enum discriminants: map a fieldless enum to a code with an exhaustive `match` in `From`.' },
    'H-15': { level: 'MUST', family: 'H', title: 'Zero warnings.' },
    'API-15': { level: 'MUST', family: 'API', title: 'Document every public item: purpose in a one-line summary, an `# Examples` doctest for primary entry points, and `# Errors` for…' },
    'API-11': { level: 'SHOULD', family: 'API', title: 'Mark values that must be used `#[must_use]`: pure constructors, builders, iterators and computed results.' },
    'OWN-13': { level: 'SHOULD', family: 'OWN', title: 'Return iterators or slices instead of allocating a collection the caller only iterates.' },
    'PERF-11': { level: 'SHOULD', family: 'PERF', title: 'Make refcount clones explicit with `Arc::clone(&x)`.' },
    'MOD-02': { level: 'SHOULD', family: 'MOD', title: 'Use `let ... else` for early exits, `if let` chains instead of nested `if let`, and `if let` guards in match arms (1.95+).' },
    'CRF-02': { level: 'SHOULD', family: 'CRF', title: 'A function does one thing at one level of abstraction.' },
    'OWN-15': { level: 'CONSIDER', family: 'OWN', title: 'Use `Box<str>`, `Box<[T]>`, `Arc<str>` or `Arc<[T]>` for immutable owned data that never grows.' },
  },
};

const LINTS = {
  'clippy::unwrap_used': { rules: ['H-02'], severity: 'must', message: 'used `unwrap()` on a `Result` value' },
  'clippy::expect_used': { rules: ['H-02'], severity: 'must', message: 'used `expect()` on an `Option` value' },
  'clippy::indexing_slicing': { rules: ['H-04'], severity: 'must', message: 'indexing may panic' },
  'clippy::as_conversions': { rules: ['H-11'], severity: 'must', message: 'using a potentially dangerous silent `as` conversion' },
  'clippy::cast_possible_truncation': { rules: ['H-11'], severity: 'must', message: 'casting `u64` to `u32` may truncate the value' },
  missing_docs: { rules: ['H-15', 'API-15'], severity: 'must', message: 'missing documentation for a public function' },
  'clippy::missing_errors_doc': { rules: ['H-15', 'API-15'], severity: 'must', message: 'docs for function returning `Result` missing `# Errors` section' },
  'clippy::panic': { rules: ['H-03'], severity: 'must', message: '`panic` should not be present in production code' },
  'clippy::print_stdout': { rules: ['H-06'], severity: 'must', message: 'use of `println!`' },
  'clippy::redundant_clone': { rules: ['H-08'], severity: 'must', message: 'redundant clone' },
  'clippy::clone_on_ref_ptr': { rules: ['H-08', 'PERF-11'], severity: 'must', message: 'using `.clone()` on a ref-counted pointer' },
  'clippy::must_use_candidate': { rules: ['API-11'], severity: 'should', message: 'this method could have a `#[must_use]` attribute' },
  'clippy::needless_collect': { rules: ['OWN-13'], severity: 'should', message: 'avoid using `collect()` when not needed' },
  'clippy::manual_let_else': { rules: ['MOD-02'], severity: 'should', message: 'this could be rewritten as `let...else`' },
  'clippy::too_many_lines': { rules: ['CRF-02'], severity: 'should', message: 'this function has too many lines (84/60)' },
  'clippy::rc_buffer': { rules: ['OWN-15'], severity: 'consider', message: 'usage of `Arc<T>` where `T` is a string or vector: `Arc<Vec<Event>>`' },
  'clippy::semicolon_if_nothing_returned': { rules: [], severity: 'unmapped', message: 'consider adding a `;` to the last statement for consistent formatting' },
  'rustdoc::broken_intra_doc_links': { rules: ['H-15'], severity: 'must', message: 'unresolved link to `Store::append<E>`', tool: 'rustdoc' },
  'scan:let-underscore': { rules: ['H-10'], severity: 'must', message: '`let _ =` discards a `Result`', tool: 'scan' },
  'scan:print': { rules: ['H-06'], severity: 'must', message: 'print macro in production code: use `tracing` or `writeln!` on a locked handle', tool: 'scan' },
};

const CRATES = [
  { name: 'ledger-domain', files: ['src/account.rs', 'src/money.rs', 'src/lib.rs'], inherited: true, charter: true,
    lints: { 'clippy::must_use_candidate': 4, 'clippy::manual_let_else': 2, 'clippy::semicolon_if_nothing_returned': 3 } },
  { name: 'ledger-app', files: ['src/ledger.rs', 'src/event_store.rs', 'src/clock.rs', 'src/projection/balances.rs'], inherited: true, charter: true,
    lints: { 'clippy::unwrap_used': 8, 'clippy::expect_used': 4, 'clippy::indexing_slicing': 6, 'clippy::missing_errors_doc': 5, 'clippy::must_use_candidate': 6, 'clippy::needless_collect': 3, 'clippy::too_many_lines': 2, 'clippy::clone_on_ref_ptr': 3 } },
  { name: 'ledger-store-jsonl', files: ['src/store.rs', 'src/record.rs', 'src/record/v1.rs', 'src/lock.rs', 'src/lib.rs'], inherited: true, charter: true,
    lints: { 'clippy::unwrap_used': 17, 'clippy::indexing_slicing': 9, 'clippy::as_conversions': 7, 'clippy::cast_possible_truncation': 5, missing_docs: 8, 'clippy::redundant_clone': 4, 'scan:let-underscore': 3, 'clippy::too_many_lines': 3, 'clippy::must_use_candidate': 5, 'rustdoc::broken_intra_doc_links': 2, 'clippy::rc_buffer': 2 } },
  { name: 'ledger-cli', files: ['src/main.rs', 'src/args.rs', 'src/output.rs'], inherited: true, charter: false,
    lints: { 'clippy::unwrap_used': 6, 'clippy::print_stdout': 5, 'clippy::panic': 2, 'scan:print': 3, 'clippy::semicolon_if_nothing_returned': 4, 'clippy::manual_let_else': 3, 'clippy::expect_used': 3 } },
  { name: 'ledger-sync', files: ['src/lib.rs', 'src/replicator.rs', 'src/wire.rs', 'src/backoff.rs', 'src/peer/handshake_protocol_negotiation.rs'], inherited: false, charter: false,
    lints: { 'clippy::unwrap_used': 12, 'clippy::indexing_slicing': 8, 'clippy::as_conversions': 6, missing_docs: 10, 'clippy::panic': 3, 'clippy::redundant_clone': 5, 'clippy::needless_collect': 4, 'clippy::too_many_lines': 4, 'clippy::semicolon_if_nothing_returned': 6, 'clippy::rc_buffer': 3 } },
  { name: 'ledger-bench', files: ['benches/append.rs'], inherited: true, charter: true, lints: {} },
];

function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The collection, the baseline it is compared with, and the ratchet result for base `origin/main`. */
export function sample() {
  const rand = rng(42);
  const pick = (xs) => xs[Math.floor(rand() * xs.length)];
  const findings = [];
  for (const c of CRATES) {
    for (const [lint, n] of Object.entries(c.lints)) {
      const spec = LINTS[lint];
      for (let i = 0; i < n; i++) {
        const line = 8 + Math.floor(rand() * 380);
        findings.push({
          tool: spec.tool ?? (lint.startsWith('rustdoc::') ? 'rustdoc' : 'clippy'), lint, crate: c.name,
          file: `crates/${c.name}/${pick(c.files)}`, line, end_line: line, message: spec.message, rules: spec.rules, severity: spec.severity,
        });
      }
    }
  }
  const collection = {
    temper_findings: 1, at: '2026-09-28T14:02:11.000Z', commit: '9f3c2e1a7b4d5c6e8f9a0b1c2d3e4f5a6b7c8d9e', toolchain: '1.98.1',
    crates: CRATES.map((c) => ({ name: c.name, dir: `crates/${c.name}`, lints_inherited: c.inherited, charter: c.charter })),
    findings, errors: [],
  };
  // Three findings on changed lines in ledger-cli: two unwraps and a println.
  const cli = findings.filter((f) => f.crate === 'ledger-cli');
  const newCode = [cli.find((f) => f.lint === 'clippy::unwrap_used'), cli.filter((f) => f.lint === 'clippy::unwrap_used')[1], cli.find((f) => f.lint === 'clippy::print_stdout')];
  const totals = {};
  for (const f of findings) {
    totals[f.crate] ??= {};
    totals[f.crate][f.lint] = (totals[f.crate][f.lint] ?? 0) + 1;
  }
  for (const f of newCode) totals[f.crate][f.lint] -= 1;
  totals['ledger-app']['clippy::indexing_slicing'] -= 1; // a signature change made an untouched caller warn
  totals['ledger-store-jsonl']['clippy::unwrap_used'] += 3; // three fixed since the baseline
  totals['ledger-domain'].missing_docs = 2; // documented since
  const baseline = {
    temper_baseline: 1, created: '2026-08-14T09:12:00.000Z', updated: '2026-09-01T16:40:00.000Z',
    commit: '1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b', toolchain: '1.98.1', totals, files: {},
  };
  const changed = new Map();
  for (const f of newCode) {
    if (!changed.has(f.file)) changed.set(f.file, new Set());
    changed.get(f.file).add(f.line);
  }
  changed.base = '5e1f00d2c4b3a2918273645546372819a0b1c2d3';
  return { collection, baseline, ratchet: ratchet(collection, baseline, { changed }), changed };
}

/** The same scenario where the change also raised two baseline counts (an approved `--update`). */
export function sampleRaised() {
  const s = sample();
  s.ratchet.raised = [
    { crate: 'ledger-sync', lint: 'clippy::unwrap_used', from: 9, to: 12 },
    { crate: 'ledger-sync', lint: 'missing_docs', from: 6, to: 10 },
  ];
  return s;
}
