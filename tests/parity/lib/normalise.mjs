// The only rewrites applied to engine output before a byte comparison. Each one cites the decision
// that allows it; anything else that differs is a parity failure.
import fs from 'node:fs';

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

const HIT_LINE = /^\[(hard|ask|soft)\] /;

/**
 * D16(l) (wi-ecb9dd): 1.x orders text-form hits with `localeCompare`, Rust by byte order, so paths
 * that differ only in case (`B.rs`, `a.rs`) come out in another order. Sorts the hits inside each
 * run of one severity, each hit with its indented continuation lines (`    fix: ..`); the blank line
 * and the summary stay where they are. Applied only to text-form cases whose fixture has such a pair.
 */
export function sortHitLines(text) {
  const out = [];
  let run = [];
  let severity = null;
  const flush = () => {
    out.push(...run.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0)).flat());
    run = [];
    severity = null;
  };
  for (const line of text.split('\n')) {
    const m = HIT_LINE.exec(line);
    if (!m && severity && /^\s/.test(line)) {
      run.at(-1).push(line);
      continue;
    }
    if (!m || m[1] !== severity) flush();
    if (m) {
      severity = m[1];
      run.push([line]);
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
 * at the end of a line). Applied to 1.x's output only; `count` goes in the report.
 */
export function dropLoneSurrogateTail(json) {
  let count = 0;
  const text = json.replace(/\\ud[89ab][0-9a-f]{2}"(?=,?\n)/gi, () => {
    count += 1;
    return '"';
  });
  return { text, count };
}
