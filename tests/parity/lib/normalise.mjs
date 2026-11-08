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

/**
 * Port 3e: a tree record's `at` is `Date.now()` when the hook ran, so each engine's own clock; a
 * whole record ending in `"at":<n>}` with `n` not 0 has it become `"at":"<MS>"`. The session
 * record's `"at":0` (the session base's, port 3d) and every other key are compared.
 */
export function maskTreeAt(text) {
  return text.replace(/^(\{.*"at":)[1-9]\d*\}$/s, '$1"<MS>"}');
}

/**
 * Port 3e, narrowed by the #210 review (F1): a git index holds each file's stat data (ctime,
 * mtime, dev, ino, uid, gid and size), which differ between the two copies when a case's git call
 * rewrites the index, and whose size git zeroes for a racily clean entry. `buf` (an index's raw
 * bytes, `DIRC` first, version 2, 3 or 4, SHA-1 oids) becomes `<GIT INDEX v<n>>` and one line per
 * entry: its mode, oid, flags (stage, name length and the v3 bits) and path, so everything but the
 * stat data is compared. Extensions and the trailing checksum are left out: the checksum covers
 * the stat data, and the untracked cache holds stat data too. A buffer that is not an index or
 * does not decode is returned as text, unmasked, so it is compared whole.
 */
export function maskGitIndex(buf) {
  const entries = indexEntries(buf);
  return entries ? [`<GIT INDEX v${buf.readUInt32BE(4)}>`, ...entries].join('\n') : buf.toString('utf8');
}

/** The entries of a git index as `mode oid flags path` lines, or null when `buf` does not decode as one. */
function indexEntries(buf) {
  if (buf.length < 12 || buf.toString('latin1', 0, 4) !== 'DIRC') return null;
  const version = buf.readUInt32BE(4);
  if (![2, 3, 4].includes(version)) return null;
  const count = buf.readUInt32BE(8);
  const lines = [];
  let at = 12;
  let previous = Buffer.alloc(0);
  for (let i = 0; i < count; i++) {
    // ctime, mtime, dev and ino (24 bytes), mode, uid, gid and size (16), the oid (20), flags (2).
    if (at + 62 > buf.length) return null;
    const mode = buf.readUInt32BE(at + 24).toString(8);
    const oid = buf.toString('hex', at + 40, at + 60);
    let flags = buf.readUInt16BE(at + 60).toString(16).padStart(4, '0');
    let p = at + 62;
    if (version >= 3 && buf.readUInt16BE(at + 60) & 0x4000) {
      if (p + 2 > buf.length) return null;
      flags += buf.readUInt16BE(p).toString(16).padStart(4, '0');
      p += 2;
    }
    let name;
    if (version === 4) {
      // The name is the previous one less N trailing bytes (a varint), then a NUL-terminated suffix.
      let strip = 0;
      let byte;
      do {
        if (p >= buf.length) return null;
        byte = buf[p++];
        strip = (strip << 7) | (byte & 0x7f);
        if (byte & 0x80) strip += 1;
      } while (byte & 0x80);
      const end = buf.indexOf(0, p);
      if (end < 0 || strip > previous.length) return null;
      name = Buffer.concat([previous.subarray(0, previous.length - strip), buf.subarray(p, end)]);
      at = end + 1;
    } else {
      const end = buf.indexOf(0, p);
      if (end < 0) return null;
      name = buf.subarray(p, end);
      // Each entry is NUL-padded to a multiple of 8 bytes, with at least one NUL.
      at += Math.ceil((end + 1 - at) / 8) * 8;
    }
    previous = name;
    lines.push(`${mode} ${oid} ${flags} ${name.toString('utf8')}`);
  }
  return at <= buf.length ? lines : null;
}

const ISO = String.raw`\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z`;

/**
 * Port 3f (§2.2 lists it): a verdict record's `recorded_at` is `new Date().toISOString()` when the
 * hook ran, so each engine's own clock. As a pretty or compact JSON key (`verdicts/<fp>.json`, a
 * history row, `status --json`'s `review`) and in the earlier-review line a reviewer's context
 * quotes (`recorded: <verdict> at <time>`), an ISO time becomes `<T>`. Every other key is compared.
 */
export function maskRecordedAt(text) {
  return text
    .replace(new RegExp(String.raw`("recorded_at": ?")${ISO}"`, 'g'), '$1<T>"')
    .replace(new RegExp(String.raw`(recorded: (?:approved|changes-requested) at )${ISO}`, 'g'), '$1<T>');
}

/**
 * D16(a), port 3f: the text after `verdict JSON does not parse: ` in a hook's JSON answer is
 * `serde_json`'s, not V8's (whose text quotes the block, newlines included), so it becomes `<MSG>`
 * up to the denial's or the block's last line, which follow the only problem 1.x then reports. Counted, like `maskFailureText`, so a case can pin how many.
 */
export function maskVerdictParse(text) {
  let count = 0;
  const out = text.replace(/(verdict JSON does not parse: )(?:[^"\\]|\\.)*?(?=\\n(?:Fix the verdict block|End your final report))/g, (_, head) => {
    count += 1;
    return `${head}<MSG>`;
  });
  return { text: out, count };
}
