// Inputs for the parity cases and the corpus. The fixture lives in JS strings because temper's own
// pre-edit hook refuses `.rs` files with hard hits.
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { HarnessError } from './error.mjs';

// A hit line of 170 ASCII characters: 1.x cuts hit text at 160 UTF-16 units.
const LONG = `    let v = x.unwrap(); // ${'a'.repeat(170 - 27)}`;

/**
 * 1d.2's oracle tree (.temper/plans/port-phase-1d2-oracle/mk.py, the tree temper-cli's
 * tests/it/scan.rs inlines as FIXTURE): two crates, `B.rs` and `a.rs` (D16(l)), CRLF, tests/,
 * build.rs, main.rs, scripts/gen.rs outside any crate, the skipped target/, .temper/ and
 * node_modules/, and notes.txt. Plus a BOM, invalid UTF-8 (lossy decoding) and a long hit line.
 */
export const SCAN_FIXTURE = {
  "Cargo.toml": "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n",
  "crates/core/Cargo.toml": "[package]\nname = \"core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
  "crates/core/src/lib.rs": "//! Core.\n\n/// Parses.\npub fn parse(v: Option<u8>) -> u8 {\n    v.unwrap()\n}\n\npub fn label(v: Option<&str>) -> &str {\n    v.expect(\"é\u0001\tx\")\n}\n",
  "crates/core/src/asks.rs": "#[expect(clippy::unwrap_used, reason = \"the caller checked it\")]\npub fn checked(v: Option<u8>) -> u8 {\n    v.unwrap()\n}\n\npub fn wait(d: std::time::Duration) {\n    std::thread::sleep(d);\n}\n",
  "crates/core/src/B.rs": "pub fn b() {\n    dbg!(1);\n}\n",
  "crates/core/src/a.rs": "pub fn a() {\n    dbg!(2);\n}\n",
  "crates/core/src/clean.rs": "pub fn add(a: u8, b: u8) -> Option<u8> {\n    a.checked_add(b)\n}\n",
  "crates/core/src/crlf.rs": "pub fn c(v: Option<u8>) -> u8 {\r\n    v.unwrap()\r\n}\r\n",
  "crates/core/tests/it.rs": "#[test]\nfn t() {\n    Some(1).unwrap();\n    dbg!(2);\n}\n",
  "crates/core/build.rs": "fn main() {\n    println!(\"cargo::rerun-if-changed=build.rs\");\n}\n",
  "crates/tool/Cargo.toml": "[package]\nname = \"tool\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
  "crates/tool/src/main.rs": "fn main() {\n    let x: Option<u8> = None;\n    x.unwrap();\n}\n",
  "crates/tool/src/cli.rs": "pub fn run() {\n    std::process::exit(2);\n}\n",
  "scripts/gen.rs": "fn f() {\n    unsafe {}\n}\n",
  "target/debug/x.rs": "fn f() { dbg!(1); }\n",
  ".temper/x.rs": "fn f() { dbg!(1); }\n",
  "node_modules/x.rs": "fn f() { dbg!(1); }\n",
  "notes.txt": "dbg!(1)\n",
  "crates/core/src/bom.rs": "﻿pub fn f() {\n    dbg!(1);\n}\n",
  "crates/core/src/latin1.rs": Buffer.concat([
    Buffer.from("// caf"), Buffer.from([0xe9, 0xff]), Buffer.from("\npub fn f(v: Option<u8>) -> u8 {\n    v.unwrap() // "),
    Buffer.from([0xe9, 0xff]), Buffer.from("\n}\n"),
  ]),
  "crates/core/src/long.rs": `pub fn f(x: Option<u8>) {\n${LONG}\n    drop(v);\n}\n`,
};

// After trim, a hit line starts with these 23 UTF-16 units.
const HEAD = 'let v = x.unwrap(); // ';

/**
 * D16(e) end to end: one hit line with an emoji well before 1.x's 160-unit cut (two units, so a
 * count of characters would keep one unit too many), and one whose emoji straddles the cut (units
 * 159 and 160), which 1.x leaves as a lone high surrogate and Rust drops.
 */
export const SURROGATE_FIXTURE = {
  'Cargo.toml': '[package]\nname = "surrogate"\nversion = "0.1.0"\nedition = "2024"\n',
  'src/lib.rs': `pub fn f(x: Option<u8>) {\n    ${HEAD}${'a'.repeat(20)}\u{1F600}${'a'.repeat(150)}\n`
    + `    ${HEAD}${'b'.repeat(159 - HEAD.length)}\u{1F600}cccc\n}\n`,
};

/**
 * The "large file" of tests/scan.test.mjs's retired-bound test, byte for byte: the 11-line block
 * until at least 4,990 lines, then a seeded function. `seedLine` is the line of its `.unwrap()`.
 */
export function largeFile() {
  const block = [
    '/// Adds two values.',
    '///',
    '/// # Errors',
    '///',
    '/// Returns `None` on overflow.',
    'pub fn add_N(a: u32, b: u32) -> Option<u32> {',
    '    let sum = a.checked_add(b)?;',
    '    if sum > 10 { return Some(sum - 1); }',
    '    Some(sum)',
    '}',
    '',
  ];
  const lines = [];
  for (let i = 0; lines.length < 4990; i += 1) lines.push(...block.map((l) => l.replace('add_N', `add_${i}`)));
  const seedLine = lines.length + 2;
  lines.push('pub fn seeded(v: Option<u8>) -> u8 {', '    v.unwrap()', '}', '');
  return { src: lines.join('\n'), seedLine };
}

/** The large file as a one-crate tree. */
export function largeCrate() {
  return {
    'Cargo.toml': '[package]\nname = "large"\nversion = "0.1.0"\nedition = "2024"\n',
    'src/lib.rs': largeFile().src,
  };
}

/** Writes `files` (relative path to string or Buffer) under `root`, bytes as given. */
export function writeTree(root, files) {
  for (const [rel, content] of Object.entries(files)) {
    const p = path.join(root, ...rel.split('/'));
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, content);
  }
}

/**
 * A copy of `src` in a new temp directory, without directories named in `skip`. A missing or
 * non-directory `src` is a HarnessError, raised before any temp directory exists.
 */
export function freshCopy(src, { skip = ['target', '.git'] } = {}) {
  if (!fs.statSync(src, { throwIfNoEntry: false })?.isDirectory()) {
    throw new HarnessError(`not a directory: ${src}`);
  }
  const dest = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-parity-'));
  fs.cpSync(src, dest, {
    recursive: true,
    filter: (p) => !(skip.includes(path.basename(p)) && fs.statSync(p).isDirectory()),
  });
  return dest;
}

/**
 * Every file and directory under `root`, as sorted `[relPath, sha256]` pairs with `/` separators;
 * a directory's pair is `[relPath + '/', 'dir']`, so an engine that creates an empty one is seen.
 */
export function snapshotTree(root) {
  const out = [];
  const rec = (dir, rel) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const r = rel ? `${rel}/${e.name}` : e.name;
      const p = path.join(dir, e.name);
      if (e.isDirectory()) {
        out.push([`${r}/`, 'dir']);
        rec(p, r);
      } else out.push([r, crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex')]);
    }
  };
  rec(root, '');
  return out.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
}
