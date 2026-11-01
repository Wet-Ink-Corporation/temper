// How a mismatch is explained (1e-6, wi-9817a6). Bytes decide parity; these only describe a
// difference: where the bytes first part, and which hits each engine has that the other lacks.
import { HarnessError } from './error.mjs';

const EXCERPT = 40; // bytes on each side of the first difference: an 80-byte window
const CAP = 50; // hits listed per side; the totals count them all

/** `null` when equal, else `{ offset, line, a, b }`: the first differing byte, its 1-based line, and excerpts. */
export function firstDifference(a, b) {
  const n = Math.min(a.length, b.length);
  let offset = 0;
  while (offset < n && a[offset] === b[offset]) offset += 1;
  if (offset === n && a.length === b.length) return null;
  let line = 1;
  for (let i = 0; i < offset; i += 1) if (a[i] === 0x0a) line += 1;
  const excerpt = (buf) => buf.subarray(Math.max(0, offset - EXCERPT), offset + EXCERPT).toString('utf8');
  return { offset, line, a: excerpt(a), b: excerpt(b) };
}

/** `value` with every object's keys sorted, so equal hits stringify equally. */
export function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.keys(value).sort().map((k) => [k, canonical(value[k])]));
  }
  return value;
}

function parseHits(text, side) {
  let hits;
  try {
    hits = JSON.parse(String(text));
  } catch (e) {
    throw new HarnessError(`${side} output is not JSON: ${e.message}`, { cause: e });
  }
  if (!Array.isArray(hits)) throw new HarnessError(`${side} output is not a JSON array of hits`);
  return hits;
}

/** Hits as multisets keyed on their canonical JSON: what only 1.x has, what only Rust has, and whether only the order differs. */
export function hitDiff(nodeJson, rustJson) {
  const node = parseHits(nodeJson, '1.x');
  const rust = parseHits(rustJson, 'Rust');
  const counts = new Map();
  const seen = new Map();
  for (const [hits, d] of [[node, 1], [rust, -1]]) {
    for (const h of hits) {
      const k = JSON.stringify(canonical(h));
      counts.set(k, (counts.get(k) ?? 0) + d);
      if (!seen.has(k)) seen.set(k, h);
    }
  }
  const onlyNode = [];
  const onlyRust = [];
  for (const [k, c] of counts) {
    const list = c > 0 ? onlyNode : onlyRust;
    for (let i = 0; i < Math.abs(c); i += 1) list.push(seen.get(k));
  }
  const orderOnly = onlyNode.length === 0 && onlyRust.length === 0 && String(nodeJson) !== String(rustJson);
  return {
    onlyNode: onlyNode.slice(0, CAP),
    onlyRust: onlyRust.slice(0, CAP),
    onlyNodeTotal: onlyNode.length,
    onlyRustTotal: onlyRust.length,
    orderOnly,
  };
}
