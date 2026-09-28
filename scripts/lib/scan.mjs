// Escape-hatch scanner. Pattern rules over lexer-masked Rust source.
// Severity per context: 'hard' (block), 'soft' (advise), null (ignore).
// A 'hard' hit covered by an `#[expect(<lint>, reason = "...")]` becomes an
// 'ask' hit: an exception request that a human must approve (H-05).

import { maskRust, lineIndex, testRegions, inRegions, countTests } from './lexer.mjs';

const REF = 'skills/rust-craft/references';

/**
 * Rule table. `ctx` gives severity per code context:
 *   prod = production code in a library crate, bin = production code in a binary/build script,
 *   example = examples/, test = tests, benches, #[cfg(test)] regions.
 */
export const RULES = [
  {
    key: 'unsafe', id: 'H-01', re: /\bunsafe\b/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'hard' },
    msg: '`unsafe` is forbidden in this workspace',
    fix: 'Find a safe API or restructure. A genuine need (FFI) is a human decision: isolate it in a -sys crate and change the lint config with approval.',
    lints: [],
  },
  {
    key: 'unwrap', id: 'H-02',
    re: /\.\s*(?:unwrap\s*\(\s*\)|unwrap_err\s*\(\s*\)|unwrap_unchecked\s*\(|expect\s*\(\s*"|expect_err\s*\(\s*")|\b(?:Option|Result)\s*::\s*(?:unwrap|expect|unwrap_unchecked|unwrap_err|expect_err)\b/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: null },
    msg: 'unwrap/expect in production code',
    fix: 'Propagate with `?`, convert with `ok_or_else`/`let ... else`, or make the invariant a type. See references/errors.md "Eliminating unwrap/expect".',
    lints: ['clippy::unwrap_used', 'clippy::expect_used'],
  },
  {
    key: 'panic', id: 'H-03', re: /\b(?:panic|todo|unimplemented|unreachable)\s*!|\bprocess\s*::\s*abort\s*\(/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: null },
    msg: 'panicking macro in production code',
    fix: 'Return an error, or restructure types so the case cannot occur (exhaustive match, newtype, NonZero, typestate). Unfinished work is `Status: WIP`, never `todo!()`.',
    lints: ['clippy::panic', 'clippy::todo', 'clippy::unimplemented', 'clippy::unreachable'],
  },
  {
    key: 'assert-in-prod', id: 'H-03', re: /\b(?:assert|assert_eq|assert_ne)\s*!/g,
    ctx: { prod: 'soft', bin: 'soft', example: null, test: null },
    msg: '`assert!` panics in production code',
    fix: 'Use `debug_assert!` for internal sanity checks, or return an error for conditions callers can trigger.',
    lints: [],
  },
  {
    key: 'allow-attr', id: 'H-05', re: /#\s*!?\s*\[\s*allow\s*\(/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'hard' },
    msg: '`#[allow]` suppresses a lint',
    fix: 'Fix the code the lint points at. If an exception is truly warranted, use `#[expect(lint, reason = "...")]`; a human will be asked to approve it.',
    lints: [],
  },
  {
    key: 'dbg', id: 'H-06', re: /\bdbg\s*!/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'hard' },
    msg: '`dbg!` left in code',
    fix: 'Remove it. Use `tracing::debug!` with structured fields for diagnostics that should stay.',
    lints: ['clippy::dbg_macro'],
  },
  {
    key: 'print', id: 'H-06', re: /\be?print(?:ln)?\s*!/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'soft', test: null },
    msg: 'print macro in production code',
    fix: 'Libraries: use `tracing`. Binaries: write through a locked handle, `writeln!(io::stdout().lock(), ...)?`. It returns an error on a closed pipe where `println!` would panic.',
    lints: ['clippy::print_stdout', 'clippy::print_stderr'],
  },
  {
    key: 'leak', id: 'H-07',
    re: /\bBox\s*::\s*leak\b|\bmem\s*::\s*forget\b|\btransmute\b|\bstatic\s+mut\b/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'hard' },
    msg: 'ownership escape hatch (leak/forget/transmute/static mut)',
    fix: 'Model ownership explicitly: move the value to its owner, scope threads with `std::thread::scope`, or share an `Arc`. See references/ownership.md.',
    lints: ['clippy::mem_forget', 'clippy::disallowed_methods'],
  },
  {
    key: 'catch-unwind', id: 'H-03', re: /\bcatch_unwind\b/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'soft' },
    msg: '`catch_unwind` used as control flow',
    fix: 'Panics are bugs, not errors. Remove the panic source and return `Result`.',
    lints: ['clippy::disallowed_methods'],
  },
  {
    key: 'exit', id: 'ARC-14', re: /\bprocess\s*::\s*exit\s*\(/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'soft' },
    msg: '`process::exit` skips destructors and bypasses error reporting',
    fix: 'Return `ExitCode` (or `Result`) from `main` and map errors to codes at the edge.',
    lints: ['clippy::exit'],
  },
  {
    key: 'discard-ok', id: 'H-10', re: /\.\s*ok\s*\(\s*\)\s*;/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'soft' },
    msg: '`.ok();` silently discards an error',
    fix: 'Handle or propagate the error. If it is truly irrelevant, say why in a comment and use an explicit `if let Err(e) = ... { tracing::debug!(...) }`.',
    lints: ['clippy::unused_result_ok'],
  },
  {
    key: 'let-underscore', id: 'H-10', re: /\blet\s+_\s*=/g,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: null },
    msg: '`let _ =` may discard a failure',
    fix: 'Confirm the value is infallible or irrelevant, and leave a comment saying why. Results must be handled (ERR-12).',
    lints: [],
  },
  {
    key: 'as-cast', id: 'H-11',
    re: /\bas\s+(?:u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize|f32|f64|char)\b/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'soft' },
    msg: 'numeric `as` cast (can truncate, wrap or lose sign)',
    fix: 'Use `From`/`TryFrom` (`u64::from(x)`, `u32::try_from(n)?`) or `checked_*`/`saturating_*`.',
    lints: ['clippy::as_conversions', 'clippy::cast_possible_truncation', 'clippy::cast_sign_loss', 'clippy::cast_possible_wrap', 'clippy::cast_precision_loss', 'clippy::cast_lossless'],
  },
  {
    key: 'ptr-param', id: 'OWN-03', re: /:\s*&\s*(?:String|Vec\s*<|Box\s*<|PathBuf|OsString)\b/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'soft' },
    msg: 'borrowed owning container (`&String`, `&Vec<T>`, `&Box<T>`, `&PathBuf`)',
    fix: 'Borrow the view instead: `&str`, `&[T]`, `&T`, `&Path`, `&OsStr`.',
    lints: ['clippy::ptr_arg', 'clippy::borrowed_box'],
  },
  {
    key: 'lazy-static', id: 'MOD-03', re: /\blazy_static\s*!|\bonce_cell\s*::/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'hard' },
    msg: 'legacy lazy initialization crate',
    fix: 'Use `std::sync::LazyLock` / `OnceLock` (or `LazyCell` / `OnceCell`).',
    lints: [],
  },
  {
    key: 'path-attr', id: 'ARC-06', re: /#\s*\[\s*path\s*=/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'hard' },
    msg: '`#[path]` hides the module layout',
    fix: 'Use the `foo.rs` + `foo/bar.rs` layout.',
    lints: [],
  },
  {
    key: 'ignore-no-reason', id: 'H-12', re: /#\s*\[\s*ignore\s*\]/g,
    ctx: { prod: 'hard', bin: 'hard', example: 'hard', test: 'hard' },
    msg: '`#[ignore]` without a reason',
    fix: 'Do not disable tests to get green (H-12). If a test is slow or needs external resources, use `#[ignore = "reason"]` and tell the human.',
    lints: [],
  },
  {
    key: 'async-trait', id: 'MOD-04', re: /#\s*\[\s*(?:async_trait\s*::\s*)?async_trait\b/g,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: 'soft' },
    msg: '`#[async_trait]` macro',
    fix: 'Use native `async fn` in traits. For `Send` futures use `fn f(&self) -> impl Future<Output = T> + Send`; for `dyn`, return a boxed future via a named alias (ASY-03).',
    lints: [],
  },
  {
    key: 'shared-mut', id: 'OWN-11', re: /\b(?:Rc|Arc)\s*<\s*(?:RefCell|Mutex|RwLock)\s*</g,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: null },
    msg: 'shared mutable state',
    fix: 'Justify it in a comment on the type (who shares it, why mutation must be shared). Consider single ownership, message passing, or an arena first (OWN-11).',
    lints: [],
  },
  {
    key: 'global-mut', id: 'ARC-09',
    re: /\bstatic\s+[A-Z_][A-Z0-9_]*\s*:\s*[^=;]*\b(?:Mutex|RwLock|RefCell|Cell|Atomic[A-Za-z0-9]+)\b/g,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: 'soft' },
    msg: 'global mutable state',
    fix: 'Pass state explicitly through constructors (ARC-09). Globals couple everything and break test isolation.',
    lints: [],
  },
  {
    key: 'deref-inherit', id: 'API-19', re: /\bimpl\b[^{;]*\bDeref(?:Mut)?\s+for\b/g,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: null },
    msg: '`Deref` impl',
    fix: '`Deref` is for smart-pointer-like wrappers only, never to emulate inheritance on a newtype (API-19).',
    lints: [],
  },
  {
    key: 'sleep', id: 'TST-10', re: /\bthread\s*::\s*sleep\s*\(/g,
    ctx: { prod: 'soft', bin: 'soft', example: null, test: 'soft' },
    msg: '`thread::sleep`',
    fix: 'In async code this blocks the executor (ASY-06); use `tokio::time::sleep`. In tests, inject a clock or use paused time (TST-10).',
    lints: [],
  },
  {
    key: 'detached-spawn', id: 'ASY-04', re: /^[ \t]*(?:tokio\s*::\s*(?:task\s*::\s*)?)?spawn\s*\(/gm,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: null },
    msg: 'spawned task handle not kept',
    fix: 'Own every task: keep the `JoinHandle`, or spawn into a `JoinSet`/`TaskTracker` that is joined on shutdown (ASY-04).',
    lints: [],
  },
  {
    key: 'unbounded', id: 'ASY-07', re: /\bunbounded(?:_channel)?\s*(?:::\s*<[^>]*>\s*)?\(/g,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: null },
    msg: 'unbounded channel',
    fix: 'Use a bounded channel so producers feel backpressure (ASY-07, H-13).',
    lints: [],
  },
  {
    key: 'downcast', id: 'MOD-11', re: /\bdowncast(?:_ref|_mut)?\b|\bas_any\s*\(/g,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: null },
    msg: '`Any` downcasting',
    fix: 'Model the cases as an enum, or use trait methods or trait upcasting (MOD-11).',
    lints: [],
  },
  {
    key: 'unwrap-or-default', id: 'H-10', re: /\.\s*unwrap_or_default\s*\(\s*\)/g,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: null },
    msg: '`unwrap_or_default()` may mask a failure',
    fix: 'Fine when the default is the documented meaning of absence. When it hides an error the caller should see, propagate instead (H-10).',
    lints: [],
  },
  {
    key: 'env-var', id: 'ARC-11', re: /\benv\s*::\s*var(?:_os)?\s*\(/g,
    ctx: { prod: 'soft', bin: null, example: null, test: 'soft' },
    msg: 'environment read inside library code',
    fix: 'Read configuration once at the composition root into a validated config type and pass it down (ARC-11).',
    lints: [],
  },
  {
    key: 'should-panic', id: 'TST-05', re: /#\s*\[\s*should_panic\b/g,
    ctx: { prod: null, bin: null, example: null, test: 'soft' },
    msg: '`#[should_panic]` test',
    fix: 'Production code does not panic (H-03), so assert on the returned error variant instead.',
    lints: [],
  },
  {
    key: 'weak-assert', id: 'TST-05', re: /\bassert\s*!\s*\(\s*[^;]*?\.is_(?:ok|err|some|none)\s*\(\s*\)\s*\)/g,
    ctx: { prod: null, bin: null, example: null, test: 'soft' },
    msg: 'weak assertion (`is_ok()`/`is_err()` alone)',
    fix: 'Assert the exact value with `assert_eq!`, or the exact error variant with `matches!`/`assert_matches!` (TST-05).',
    lints: [],
  },
  {
    key: 'extern-crate', id: 'MOD-01', re: /\bextern\s+crate\b/g,
    ctx: { prod: 'soft', bin: 'soft', example: 'soft', test: 'soft' },
    msg: '`extern crate` is unnecessary in edition 2018+',
    fix: 'Remove it; use `use` paths.',
    lints: [],
  },
];

const PUB_SIG_BAD = [
  [/\bResult\s*<[^;{]*,\s*String\s*>/, 'Result<_, String>'],
  [/\bBox\s*<\s*dyn\s+(?:std\s*::\s*error\s*::\s*|core\s*::\s*error\s*::\s*)?Error\b/, 'Box<dyn Error>'],
  [/\banyhow\s*::\s*(?:Result|Error)\b/, 'anyhow'],
  [/\beyre\s*::\s*(?:Result|Report)\b/, 'eyre'],
];

/** Classify a file path relative to its crate root. */
export function classifyPath(relToCrate, crateHasLib) {
  const p = relToCrate.replace(/\\/g, '/');
  if (/^tests\//.test(p)) return 'test';
  if (/^examples\//.test(p) || /^benches\//.test(p)) return 'example';
  if (p === 'build.rs' || p === 'src/main.rs' || /^src\/bin\//.test(p)) return 'bin';
  if (/(^|\/)tests?\.rs$/.test(p) || /\/tests\//.test(p)) return 'test';
  if (/^src\//.test(p) && !crateHasLib) return 'bin';
  return 'prod';
}

function contextAt(kind, regions, offset) {
  if (kind === 'test') return 'test';
  if (inRegions(regions, offset)) return 'test';
  return kind; // prod | bin | example
}

/** Is there an #[expect(<one of lints>, reason = ...)] in the preceding lines? */
function exceptionFor(code, idx, offset, lints) {
  if (!lints.length) return null;
  const line = idx.lineOf(offset);
  const fromLine = Math.max(1, line - 12);
  const from = idx.starts[fromLine - 1];
  const window = code.slice(from, offset);
  const re = /#\s*!?\s*\[\s*expect\s*\(([^\]]*)\)\s*\]/g;
  let m;
  let found = null;
  while ((m = re.exec(window))) {
    const inner = m[1];
    if (!/\breason\s*=/.test(inner)) continue;
    const hit = lints.find((l) => inner.replace(/\s+/g, '').includes(l));
    if (hit) found = hit;
  }
  return found;
}

/**
 * Scan one Rust source.
 * @param {string} src
 * @param {{kind: 'prod'|'bin'|'example'|'test', isLib?: boolean}} opts
 */
export function scanSource(src, { kind }) {
  const { code, comments } = maskRust(src);
  const idx = lineIndex(code);
  const regions = testRegions(code);
  const hits = [];
  const push = (rule, offset, severity, extra = {}) => {
    const line = idx.lineOf(offset);
    hits.push({
      key: rule.key, id: rule.id, severity, line,
      text: idx.lineText(line, src).trim().slice(0, 160),
      msg: extra.msg ?? rule.msg, fix: rule.fix, ...extra,
    });
  };
  for (const rule of RULES) {
    rule.re.lastIndex = 0;
    let m;
    while ((m = rule.re.exec(code))) {
      const ctx = contextAt(kind, regions, m.index);
      const sev = rule.ctx[ctx];
      if (!sev) continue;
      if (sev === 'hard') {
        const lint = exceptionFor(code, idx, m.index, rule.lints);
        if (lint) { push(rule, m.index, 'ask', { exception: lint }); continue; }
      }
      push(rule, m.index, sev);
    }
  }
  // #[expect] attributes: without reason = hard; with reason = exception request
  const exp = /#\s*!?\s*\[\s*expect\s*\(([^\]]*)\)\s*\]/g;
  let m;
  while ((m = exp.exec(code))) {
    const rule = { key: 'expect-attr', id: 'H-05', fix: '' };
    if (!/\breason\s*=/.test(m[1])) {
      push({ ...rule, fix: 'Add `reason = "..."` explaining why the lint does not apply. A human will review it.' }, m.index, 'hard', { msg: '`#[expect]` without a reason' });
    } else {
      const lint = m[1].split(',')[0].trim();
      push({ ...rule, fix: 'Exception requests need human approval (H-05).' }, m.index, 'ask', { msg: `lint exception requested for \`${lint}\``, exception: lint });
    }
  }
  // stringly / erased errors in pub signatures (library production code only)
  if (kind === 'prod') {
    const sig = /\bpub\s+(?:const\s+)?(?:async\s+)?fn\s+\w+[^{;]*/g;
    while ((m = sig.exec(code))) {
      if (inRegions(regions, m.index)) continue;
      for (const [re, label] of PUB_SIG_BAD) {
        if (re.test(m[0])) {
          push({ key: 'erased-error', id: 'H-09', fix: 'Define a typed error enum with `thiserror` and return it (ERR-01, ERR-03).' }, m.index, 'hard', { msg: `public library signature returns ${label}` });
        }
      }
    }
  }
  // comments: untracked TODOs and commented-out code
  for (const c of comments) {
    if (c.doc) continue;
    const body = c.text.replace(/^\/\/+|^\/\*+|\*+\/$/g, '').trim();
    if (/\b(TODO|FIXME|XXX|HACK)\b/.test(body) && !/(#\d+|https?:\/\/|\b[A-Z][A-Z0-9]+-\d+\b)/.test(body)) {
      push({ key: 'todo-comment', id: 'CRF-04', fix: 'Link a tracking issue (`TODO(#123)`), or do it now.' }, c.start, 'soft', { msg: 'TODO without a tracking reference' });
    }
    const startsLikeCode = /^(let|fn|use|pub|if|match|return|for|while|impl|struct|enum|mod|const|static)\b/.test(body) && /[;{}()=<>]/.test(body);
    const endsLikeCode = /[;{}]\s*$/.test(body) && /[()=:<>]|\b(let|fn|return)\b/.test(body);
    if ((startsLikeCode || endsLikeCode) && !/[.!?…]\s*$/.test(body)) {
      push({ key: 'commented-code', id: 'H-06', fix: 'Delete it; version control remembers.' }, c.start, 'soft', { msg: 'commented-out code' });
    }
  }
  return { hits, tests: countTests(code), clones: (code.match(/\.\s*clone\s*\(\s*\)/g) ?? []).length };
}

/** Hits present in `after` but not in `before` (matched by rule key + normalized line text). */
export function newHits(beforeHits, afterHits) {
  const pool = new Map();
  for (const h of beforeHits) {
    const k = `${h.key}\u0000${h.text}`;
    pool.set(k, (pool.get(k) ?? 0) + 1);
  }
  const fresh = [];
  for (const h of afterHits) {
    const k = `${h.key}\u0000${h.text}`;
    const n = pool.get(k) ?? 0;
    if (n > 0) pool.set(k, n - 1); else fresh.push(h);
  }
  return fresh;
}

export function formatHit(h, file) {
  const where = file ? `${file}:${h.line}` : `line ${h.line}`;
  return `${h.id} ${where}: ${h.msg}. \`${h.text}\`\n    fix: ${h.fix}`;
}

export { REF };
