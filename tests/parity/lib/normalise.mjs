// The only rewrites applied to engine output before a byte comparison. Each one cites the decision
// that allows it; anything else that differs is a parity failure.
import fs from 'node:fs';

import { HarnessError } from './error.mjs';

/**
 * Replaces each temp root with `<TMP>`: as given, as its real path (macOS /var is /private/var),
 * and on win32 in both separator spellings and JSON-escaped. Longest first, so an inner root is
 * not half-masked by an outer one. Roots differ per run (§2.2's "fresh copy"), so they are masked
 * on both sides.
 */
export function maskRoots(text, roots, platform = process.platform) {
  const forms = new Set();
  for (const root of roots) {
    const spellings = [root];
    if (fs.existsSync(root)) spellings.push(fs.realpathSync.native(root));
    for (const s of spellings) {
      forms.add(s);
      if (platform === 'win32') {
        const back = s.replaceAll('/', '\\');
        forms.add(back);
        forms.add(s.replaceAll('\\', '/'));
        forms.add(back.replaceAll('\\', '\\\\'));
      }
    }
  }
  let out = text;
  for (const form of [...forms].sort((a, b) => b.length - a.length)) out = out.replaceAll(form, '<TMP>');
  return out;
}

const HIT_LINE = /^\[(hard|ask|soft)\] \S+ (.+?):(\d+): /;

const utf8Order = (a, b) => Buffer.compare(Buffer.from(a, 'utf8'), Buffer.from(b, 'utf8'));

/**
 * D16(l) (wi-ecb9dd): 1.x orders text-form hits by file with `localeCompare`, Rust by byte order,
 * so paths that differ only in case (`B.rs`, `a.rs`) come out in another order. Re-sorts 1.x's
 * output the way Rust sorts: inside each run of one severity, a stable sort of the hits by file in
 * UTF-8 byte order, then by line, each hit with its indented continuation lines (`    fix: ..`).
 * The blank line and the summary stay where they are. Applied to 1.x's side only, so any other
 * order difference (a line-order swap, a file order that is not D16(l)'s) still fails.
 */
export function byteOrderHits(text) {
  const out = [];
  let run = [];
  let severity = null;
  const flush = () => {
    run.sort((a, b) => utf8Order(a.file, b.file) || a.line - b.line);
    out.push(...run.flatMap((h) => h.lines));
    run = [];
    severity = null;
  };
  for (const line of text.split('\n')) {
    const m = HIT_LINE.exec(line);
    if (!m && severity && /^\s/.test(line)) {
      run.at(-1).lines.push(line);
      continue;
    }
    if (!m && /^\[(hard|ask|soft)\] /.test(line)) throw new HarnessError(`not a hit line: ${JSON.stringify(line)}`);
    if (!m || m[1] !== severity) flush();
    if (m) {
      severity = m[1];
      run.push({ file: m[2], line: Number(m[3]), lines: [line] });
    } else {
      out.push(line);
    }
  }
  flush();
  return out.join('\n');
}

/**
 * D16(e): 1.x cuts a hit's `text` at 160 UTF-16 units and can leave half of an astral character,
 * which JSON.stringify writes as a lone `\udXXX` high surrogate; Rust drops the straddling
 * character. Removes such an escape where it ends a string value (before `"` and an optional comma
 * at the end of a line), and only after an even number of backslashes, so an escaped backslash
 * followed by the text `ud800` is left alone. Applied to 1.x's output only; `count` goes in the report.
 */
export function dropLoneSurrogateTail(json) {
  let count = 0;
  const text = json.replace(/(?<!\\)((?:\\\\)*)\\ud[89ab][0-9a-f]{2}"(?=,?\n)/gi, (_, pairs) => {
    count += 1;
    return `${pairs}"`;
  });
  return { text, count };
}

/**
 * §2.2 ("`at` … fields are masked"): each telemetry row's leading `{"t":"<ISO time>"` becomes
 * `{"t":"<T>"`. Only the first key of a line, so a row whose `t` is not first, or is not an ISO
 * time, still fails; the rest of the row is compared as text, key order included (2-14).
 */
export function maskTelemetryTimes(text) {
  return text.replace(/^\{"t":"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z"/gm, '{"t":"<T>"');
}

/** §2.2: a skill marker's content, an ISO time and nothing else, becomes `<T>`. */
export function maskMarkerTime(text) {
  return /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(text) ? '<T>' : text;
}

/**
 * OC-3 (p): the text after `temper hook <event> failed: ` is the Rust error chain, not Node's
 * errno text, so it becomes `<MSG>` up to the closing `"}`. Applied to both sides, only in the
 * cases that declare it; `count` goes in the report, like D16(e)'s.
 */
export function maskFailureText(stdout) {
  let count = 0;
  const text = stdout.replace(/^(\{"systemMessage":"temper hook [a-z-]+ failed: )(?:[^"\\]|\\.)*("\})$/, (_, head, tail) => {
    count += 1;
    return `${head}<MSG>${tail}`;
  });
  return { text, count };
}

/** Port 3d: a session's `started` file, the epoch ms when its base was set, becomes `<MS>`. */
export function maskStartedTime(text) {
  return /^\d+$/.test(text) ? '<MS>' : text;
}

/**
 * Port 3d: a `gate-lost` row's `foundMs` is the time the hook found the gate less the gate's start,
 * so each engine's own clock; a number there becomes `"<MS>"`. Every other key is compared.
 */
export function maskFoundMs(text) {
  return text.replace(/^(.*"event":"gate-lost".*"foundMs":)-?\d+(?:\.\d+)?(?:e[+-]?\d+)?/gm, '$1"<MS>"');
}

/**
 * D16(w), OC-6 A (wi-168ff8): 2.0's `CLAUDE_ENV_FILE` gains `export TEMPER_BIN="…"` and
 * `export PATH="…:$PATH"` after 1.x's lines (D17). Those lines, and only whole lines of that shape,
 * are dropped and counted, so the caller can assert two per env-file write on Rust's side and none
 * on 1.x's, as `maskFailureText`'s count is.
 */
export function maskD17Lines(text) {
  let count = 0;
  const drop = () => {
    count += 1;
    return '';
  };
  const out = text.replace(/^export TEMPER_BIN="(?:[^"\\]|\\.)*"\n/gm, drop).replace(/^export PATH="(?:[^"\\]|\\.)*:\$PATH"\n/gm, drop);
  return { text: out, count };
}

/**
 * OC-5 A (wi-018023): 2.0's `SessionStart` context ends with `- Config: unreadable (<why>): defaults
 * in use.` when `.claude/temper.json` cannot be read, where 1.x says nothing. Inside the hook's JSON
 * the line follows an escaped newline; that newline and line are dropped.
 */
export function maskConfigLine(text) {
  return text.replace(/\\n- Config: unreadable \((?:[^"\\]|\\.)*?\): defaults in use\./g, '');
}

/**
 * Port 3d: a tree record is named by its checkout's id, a hash of the checkout's path, which differs
 * between the two engines' copies; `tree-<12 hex>-` in a written file's name becomes `tree-<ID>-`.
 */
export function maskTreeIds(files) {
  return Object.fromEntries(Object.entries(files).map(([rel, text]) => [rel.replace(/\/tree-[0-9a-f]{12}-([^/]*)$/, '/tree-<ID>-$1'), text]));
}
