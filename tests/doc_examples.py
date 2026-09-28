#!/usr/bin/env python3
"""Verify every code block in the rust-craft skill (the doc-example harness).

Covers skills/rust-craft/SKILL.md and skills/rust-craft/references/*.md.
Run from anywhere:

    python3 tests/doc_examples.py                          # everything
    python3 tests/doc_examples.py --file references/errors.md --keep
    python3 tests/doc_examples.py -v --no-test             # all diagnostics, skip `cargo test`

Code-block conventions (adoption spec "Code-block conventions"):

    ```rust                 Do. Compiled as one module file inside one generated
                            library crate, `cargo clippy --all-targets -- -D warnings`
                            under the template lints; tests in it are run; the
                            temper scanner must report zero hard hits.
    ```rust,ignore          Don't. Not compiled, unless it carries `expect=`.
    ```rust,ignore,expect=clippy::unwrap_used+E0502+scan:discard-ok
                            Don't with assertions. Each block is its own module in a
                            second crate; every named lint or error code must fire in
                            that block, and every `scan:<key>` must be reported by the
                            scanner for that block (any severity).
    ```rust,fragment        Illustrative Do fragment. Not compiled; the scanner must
                            report zero hard hits.
    ```toml                 Parsed with tomllib.

Any other `rust,...` info string, `expect=` without `ignore`, or `fragment` with
`ignore` is a convention failure. Other languages (text, bash, json, ...) are
counted and skipped.

Inside rust blocks:
  * A line whose trimmed text starts with `# `, or is exactly `#`, is a hidden
    context line (rustdoc rules, including `##` -> `#`): compiled and scanned,
    not part of the lesson.
  * The block is a module, so the block's `crate::` paths are rewritten to
    `crate::<module>::`, and in a Do block a `#![...]` crate-level attribute is a
    convention failure. Line numbers are preserved one-to-one.
  * Available crates: std, thiserror, serde (derive), serde_json, anyhow,
    tracing, tracing-subscriber (env-filter, json), tokio (full, test-util),
    tokio-util (rt), proptest, tempfile.
  * Doctests inside a block's own doc comments are not run (`cargo test --lib`);
    the scanner's doctest-fence rules cover them.

Generated workspace: under a temp dir (or --out). It takes the template's
`rust-toolchain.toml`, `clippy.toml`, `[workspace.package]`, profiles and
`[workspace.lints]` from templates/workspace at run time, runs fully offline
(`--offline`, registry cache only) under the pinned toolchain, and ignores
RUSTFLAGS/CARGO_TARGET_DIR from the environment. Build artifacts go to a
persistent cache (--target-dir, default ~/.cache/temper/doc-examples-target) so
dependencies compile once.

Lint relaxations. A block is a lesson, not a published API, so the generated
crates copy the template's lint table into their own `[lints]` and relax exactly
these, and nothing else:
    rust:   dead_code, missing_docs, unreachable_pub, missing_debug_implementations,
            and from the `unused` group only unused_imports, unused_variables,
            unused_assignments, unused_macros, unused_macro_rules (what a partial
            snippet legitimately leaves unused). The rest of `unused`, notably
            unused_must_use (H-10), unused_mut and unused_parens, stays on.
    clippy: missing_errors_doc, missing_panics_doc, must_use_candidate.
Everything else in the template applies, with `-D warnings`.

Compile strategy: one `cargo clippy` for all Do blocks. rustc skips lint passes
for the whole crate once any block has a hard error (no lint code), so blocks
with hard errors are reported and dropped, and the crate is re-checked until a
round is clean; lint results come from that clean round. The Don't crate uses
the same loop. Then `cargo test --lib` runs the Do blocks that contain tests.

Output: one line per failing block and check,
    references/errors.md:170: rust@161 lint: clippy::unwrap_used: used `unwrap()` ... (+2 more)
i.e. the location of the first diagnostic, the block kind and fence line, the
check, and the first diagnostic. -v prints every diagnostic. A summary follows:
blocks by kind, failures per file and check, and two lists that do not fail: Don't
blocks without expect=, and Do blocks whose `#[expect(.., reason)]` the scanner
reports as an exception request ("ask"). When `cargo test` exits non-zero (a test
aborted the process), every tested block with no reported result fails.

Exit codes: 0 all checks pass; 1 at least one block failed; 2 the harness could
not run (usage, missing toolchain or node, offline resolution failed, or a
compiler diagnostic that maps to no block).
"""

import argparse
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKILL = ROOT / 'skills' / 'rust-craft'
TEMPLATE = ROOT / 'templates' / 'workspace'
SCAN_MJS = ROOT / 'scripts' / 'lib' / 'scan.mjs'

# The crates a ```rust block may use. Fixed here, not read from the template (whose
# [workspace.dependencies] is a stub): features cover what the examples need, e.g.
# `EnvFilter`, `TaskTracker`, and paused time in tests (`start_paused`).
DEPS = {
    'thiserror': '2',
    'serde': {'version': '1', 'features': ['derive']},
    'serde_json': '1',
    'anyhow': '1',
    'tracing': '0.1',
    'tracing-subscriber': {'version': '0.3', 'features': ['env-filter', 'json']},
    'tokio': {'version': '1', 'features': ['full', 'test-util']},
    'tokio-util': {'version': '0.7', 'features': ['rt']},
    'proptest': '1',
    'tempfile': '3',
}
RELAXED = {
    'rust': ['dead_code', 'missing_docs', 'unreachable_pub', 'missing_debug_implementations',
             'unused_imports', 'unused_variables', 'unused_assignments', 'unused_macros',
             'unused_macro_rules'],
    'clippy': ['missing_errors_doc', 'missing_panics_doc', 'must_use_candidate'],
}
CHECK_ORDER = ['convention', 'compile', 'lint', 'scan', 'test', 'expect', 'toml']
SUMMARY_MSG = re.compile(r'^(aborting due to|could not compile|\d+ warnings? emitted|'
                         r'build failed|for more information about this error)')
HAS_TEST = re.compile(r'#\s*\[\s*(?:tokio\s*::\s*)?test\b|\bproptest\s*!')
LIB_HEADER = '//! Generated by temper `tests/doc_examples.py` from the rust-craft skill. Do not edit.\n'
LIB_HEADER_LINES = 2
HAS_MAIN = re.compile(r'^\s*(?:pub\s+)?(?:async\s+)?fn\s+main\s*\(', re.M)


class HarnessError(Exception):
    """The harness itself could not run: exit code 2."""


@dataclass
class Diag:
    line: int          # line in the markdown file
    code: str          # lint name, error code, scan key, or check-specific tag
    text: str
    rendered: str = ''
    level: str = 'error'

    def short(self):
        return f'{self.code}: {self.text}' if self.code else self.text


@dataclass
class Block:
    file: str          # path relative to skills/rust-craft
    line: int          # fence line (1-based)
    info: str
    body: list         # raw lines between the fences
    kind: str = 'other'   # do | dont | fragment | toml | other
    expects: list = field(default_factory=list)
    src: list = field(default_factory=list)  # processed lines (rust kinds)
    mod: str = ''
    module: str = ''   # generated module source
    asks: list = field(default_factory=list)  # md lines of scanner "ask" hits (`#[expect]` exceptions)
    failures: dict = field(default_factory=dict)  # check -> [Diag]

    @property
    def label(self):
        return {'do': 'rust', 'dont': 'rust,ignore', 'fragment': 'rust,fragment'}.get(self.kind, self.kind)

    def md_line(self, src_line):
        """Markdown line for a 1-based line of the block's source."""
        return self.line + src_line

    def fail(self, check, diag):
        self.failures.setdefault(check, []).append(diag)


# ---------------------------------------------------------------- extraction

FENCE_OPEN = re.compile(r'^(\s*)(`{3,}|~{3,})\s*([^\s`]*)')


def parse_file(path, rel):
    lines = path.read_text(encoding='utf-8').split('\n')
    blocks = []
    i = 0
    while i < len(lines):
        m = FENCE_OPEN.match(lines[i])
        if not m:
            i += 1
            continue
        indent, fence, info = len(m.group(1)), m.group(2), m.group(3)
        close = re.compile(r'^\s*' + re.escape(fence[0]) + '{' + str(len(fence)) + r',}\s*$')
        j = i + 1
        while j < len(lines) and not close.match(lines[j]):
            j += 1
        body = [strip_indent(line, indent) for line in lines[i + 1:j]]
        b = Block(rel, i + 1, info, body)
        classify(b)
        if j >= len(lines):
            b.fail('convention', Diag(b.line, 'fence', 'code fence is never closed'))
        blocks.append(b)
        i = j + 1
    return blocks


def strip_indent(line, n):
    k = 0
    while k < n and k < len(line) and line[k] == ' ':
        k += 1
    return line[k:]


def classify(b):
    tokens = [t for t in b.info.split(',') if t]
    lang = tokens[0].lower() if tokens else ''
    if lang == 'toml':
        b.kind = 'toml'
        return
    if lang != 'rust':
        b.kind = 'other'
        return
    flags = set()
    for t in tokens[1:]:
        if t in ('ignore', 'fragment'):
            flags.add(t)
        elif t.startswith('expect='):
            b.expects += [e for e in t[len('expect='):].split('+') if e]
        else:
            b.fail('convention', Diag(b.line, 'info-string',
                   f'unknown info-string token `{t}` (allowed: rust, rust,ignore[,expect=..], rust,fragment)'))
    if 'ignore' in flags and 'fragment' in flags:
        b.fail('convention', Diag(b.line, 'info-string', '`ignore` and `fragment` cannot be combined'))
    if b.expects and 'ignore' not in flags:
        b.fail('convention', Diag(b.line, 'info-string', '`expect=` is only valid on `rust,ignore` blocks'))
    b.kind = 'dont' if 'ignore' in flags else 'fragment' if 'fragment' in flags else 'do'
    b.src = [unhide(line) for line in b.body]


def unhide(line):
    """rustdoc hidden-line rules (`map_line` in rustdoc)."""
    t = line.strip()
    if t.startswith('##'):
        return line.replace('##', '#', 1)
    if t.startswith('# '):
        return t[2:]
    if t == '#':
        return ''
    return line


def build_module(b):
    """The block as a module file: same line numbers, `crate::` re-rooted at the module."""
    rewrite = re.compile(r'(?<![\w$])crate::')
    out = []
    for n, line in enumerate(b.src, 1):
        if b.kind == 'do' and re.match(r'\s*#!\[', line):
            b.fail('convention', Diag(b.md_line(n), 'crate-attr',
                   'crate-level `#![..]` attribute: a block compiles as a module; use a fragment or drop it'))
        out.append(rewrite.sub(f'crate::{b.mod}::', line))
    b.module = '\n'.join(out) + '\n'


def module_name(rel, line, taken):
    stem = re.sub(r'[^a-z0-9]+', '_', Path(rel).stem.lower()).strip('_') or 'block'
    name = f'{stem}_l{line}'
    while name in taken:
        name += '_'
    taken.add(name)
    return name


# ---------------------------------------------------------------- toolchain

def clean_env(channel):
    env = {k: v for k, v in os.environ.items()
           if k not in ('RUSTFLAGS', 'RUSTDOCFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_TARGET_DIR',
                        'CARGO_BUILD_TARGET_DIR', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_BUILD_WARNINGS')}
    # CARGO_CACHE_RUSTC_INFO=0: cargo caches failed rustc probes in the shared target dir
    # and would replay a bad one forever.
    env.update({'CARGO_NET_OFFLINE': 'true', 'CARGO_TERM_COLOR': 'never', 'RUST_BACKTRACE': '0',
                'CARGO_CACHE_RUSTC_INFO': '0'})
    if channel:
        env['RUSTUP_TOOLCHAIN'] = channel
    return env


def devnull_problem():
    """cargo gives rustc's target probe /dev/null as stdin; if /dev/null has been replaced
    by a regular file, the probe compiles its contents and fails at random."""
    try:
        st = os.stat(os.devnull)
    except OSError as e:
        return f'{os.devnull} is missing ({e})'
    if os.name == 'posix' and not stat.S_ISCHR(st.st_mode):
        return (f'{os.devnull} is not a character device (a {st.st_size}-byte regular file), so cargo\'s '
                f'rustc probe reads junk from it; an administrator must restore the device node')
    return None


def toolchain_channel():
    f = TEMPLATE / 'rust-toolchain.toml'
    if not f.exists():
        return None
    return tomllib.loads(f.read_text()).get('toolchain', {}).get('channel')


def known_lints(env):
    """All lint names the toolchain knows: {'clippy::x', 'rustc_lint', ...} plus group names."""
    try:
        out = subprocess.run(['clippy-driver', '-W', 'help'], capture_output=True, text=True,
                             env=env, timeout=120, stdin=subprocess.DEVNULL).stdout
    except (OSError, subprocess.TimeoutExpired) as e:
        raise HarnessError(f'cannot run clippy-driver: {e}') from e
    names = set()
    for line in out.splitlines():
        m = re.match(r'\s+((?:clippy::)?[a-z0-9-]+)\s{2,}', line)
        if m and m.group(1) not in ('name',):
            names.add(m.group(1).replace('-', '_'))
    if not names:
        raise HarnessError('clippy-driver -W help listed no lints')
    return names


# ---------------------------------------------------------------- workspace

def toml_key(k):
    return k if re.match(r'^[A-Za-z0-9_-]+$', k) else json.dumps(k)


def toml_value(v):
    if isinstance(v, bool):
        return 'true' if v else 'false'
    if isinstance(v, (int, float)):
        return str(v)
    if isinstance(v, str):
        return json.dumps(v)
    if isinstance(v, list):
        return '[' + ', '.join(toml_value(x) for x in v) + ']'
    if isinstance(v, dict):
        return '{ ' + ', '.join(f'{toml_key(k)} = {toml_value(x)}' for k, x in v.items()) + ' }'
    raise HarnessError(f'cannot write TOML value {v!r}')


def toml_table(name, d, out, inline=False):
    """Emit `[name]`. Dict values become inline tables when `inline`, else sub-tables."""
    subtables = {} if inline else {k: v for k, v in d.items() if isinstance(v, dict)}
    scalars = {k: v for k, v in d.items() if k not in subtables}
    if scalars or not subtables:
        out.append(f'[{name}]')
        out += [f'{toml_key(k)} = {toml_value(v)}' for k, v in scalars.items()]
        out.append('')
    for k, v in subtables.items():
        toml_table(f'{name}.{toml_key(k)}', v, out)


def crate_lints(template):
    lints = {tool: dict(table) for tool, table in template['workspace'].get('lints', {}).items()}
    for tool, names in RELAXED.items():
        lints.setdefault(tool, {})
        for n in names:
            lints[tool][n] = 'allow'
    return lints


def write_workspace(ws, do_blocks, dont_blocks):
    template = tomllib.loads((TEMPLATE / 'Cargo.toml').read_text())
    tw = template.get('workspace', {})
    out = ['# Generated by temper tests/doc_examples.py. Do not edit.', '']
    toml_table('workspace', {'resolver': tw.get('resolver', '3'),
                             'members': ['crates/doc-examples', 'crates/dont-examples']}, out)
    pkg = {k: v for k, v in tw.get('package', {}).items() if k in ('edition', 'rust-version', 'version')}
    toml_table('workspace.package', pkg, out)
    toml_table('workspace.dependencies', DEPS, out, inline=True)
    for name, prof in template.get('profile', {}).items():
        toml_table(f'profile.{toml_key(name)}', prof, out)
    ws.mkdir(parents=True, exist_ok=True)
    (ws / 'Cargo.toml').write_text('\n'.join(out))
    for f in ('rust-toolchain.toml', 'clippy.toml'):
        if (TEMPLATE / f).exists():
            shutil.copy(TEMPLATE / f, ws / f)
    lints = crate_lints(template)
    for crate in ('doc-examples', 'dont-examples'):
        d = ws / 'crates' / crate
        (d / 'src').mkdir(parents=True, exist_ok=True)
        m = []
        toml_table('package', {'name': crate, 'publish': False}, m)
        m[1:1] = ['version.workspace = true', 'edition.workspace = true', 'rust-version.workspace = true']
        toml_table('lib', {'path': 'src/lib.rs', 'doctest': False}, m)
        m.append('[dependencies]')
        m += [f'{d_}.workspace = true' for d_ in DEPS]
        m.append('')
        for tool, table in lints.items():
            toml_table(f'lints.{tool}', table, m, inline=True)
        (d / 'Cargo.toml').write_text('\n'.join(m))
    write_crate(ws / 'crates' / 'doc-examples', do_blocks)
    write_crate(ws / 'crates' / 'dont-examples', dont_blocks)


def write_crate(crate_dir, blocks):
    src = crate_dir / 'src'
    for f in src.glob('*.rs'):
        f.unlink()
    lib = [LIB_HEADER]
    for b in blocks:
        (src / f'{b.mod}.rs').write_text(b.module, encoding='utf-8')
        lib.append(f'pub mod {b.mod};')
    (src / 'lib.rs').write_text('\n'.join(lib) + '\n')


# ---------------------------------------------------------------- compile

def run_cargo(args, ws, env, target_dir, timeout):
    cmd = ['cargo', *args[:1], '--offline', '--target-dir', str(target_dir), *args[1:]]
    try:
        return subprocess.run(cmd, cwd=ws, env=env, capture_output=True, text=True, timeout=timeout,
                              stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired as e:
        raise HarnessError(f'`{" ".join(cmd)}` timed out after {timeout}s') from e
    except OSError as e:
        raise HarnessError(f'cannot run cargo: {e}') from e


def block_files(ws, crate, blocks):
    """Map generated file paths to (block, line offset). lib.rs lines map to fence lines."""
    src = (ws / 'crates' / crate / 'src').resolve()
    files = {str(src / f'{b.mod}.rs'): b for b in blocks}
    lib = {LIB_HEADER_LINES + i + 1: b for i, b in enumerate(blocks)}
    return files, str(src / 'lib.rs'), lib


def locate(msg, ws, where):
    files, lib_path, lib = where
    spans = msg.get('spans', [])
    ordered = [s for s in spans if s.get('is_primary')] + [s for s in spans if not s.get('is_primary')]
    for child in msg.get('children', []):
        ordered += child.get('spans', [])
    for span in ordered:
        while span:  # follow macro expansions back to a block line
            path = os.path.normpath(os.path.join(ws, span['file_name']))
            if path in files:
                return files[path], span['line_start']
            if path == lib_path and span['line_start'] in lib:
                return lib[span['line_start']], 0
            span = (span.get('expansion') or {}).get('span')
    return None, None


def compile_diags(stdout, ws, crate, blocks):
    """Parse cargo JSON output into {block mod: [Diag]} plus unmapped rendered messages."""
    where = block_files(ws, crate, blocks)
    crate_src = str((ws / 'crates' / crate).resolve())
    per_block, unmapped, seen = {}, [], set()
    for raw in stdout.splitlines():
        if not raw.startswith('{'):
            continue
        try:
            rec = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if rec.get('reason') != 'compiler-message':
            continue
        if not rec.get('target', {}).get('src_path', '').startswith(crate_src):
            continue
        msg = rec['message']
        code = (msg.get('code') or {}).get('code') or ''
        if msg.get('level') not in ('error', 'warning') or (not code and SUMMARY_MSG.match(msg['message'])):
            continue
        blk, line = locate(msg, ws, where)
        if not blk:
            unmapped.append(msg.get('rendered') or msg['message'])
            continue
        d = Diag(blk.md_line(line), code, msg['message'], msg.get('rendered') or '', msg['level'])
        key = (blk.mod, d.line, d.code, d.text)
        if key not in seen:
            seen.add(key)
            per_block.setdefault(blk.mod, []).append(d)
    return per_block, unmapped


def is_hard(d):
    """A compile error that is not a lint. rustc skips all lint passes once one exists."""
    return d.level == 'error' and (not d.code or re.fullmatch(r'E\d{4}', d.code) is not None)


def compile_rounds(ws, env, target_dir, crate, blocks, timeout, log):
    """Check `crate` until a round has no hard errors.

    Returns (final, dropped): final maps block mod -> diags from the clean round;
    dropped maps block mod -> diags from the round in which it had hard errors."""
    active = list(blocks)
    dropped = {}
    for rnd in range(1, 50):
        write_crate(ws / 'crates' / crate, active)
        if not active:
            return {}, dropped
        t0 = time.monotonic()
        r = run_cargo(['clippy', '-p', crate, '--all-targets', '--message-format=json', '--', '-D', 'warnings'],
                      ws, env, target_dir, timeout)
        per_block, unmapped = compile_diags(r.stdout, ws, crate, active)
        log(f'  {crate} round {rnd}: {len(active)} blocks, {time.monotonic() - t0:.0f}s')
        if unmapped:
            raise HarnessError(f'{crate}: compiler output maps to no block:\n' + '\n'.join(unmapped[:5]))
        if r.returncode != 0 and not per_block:
            hint = devnull_problem()
            raise HarnessError(f'cargo clippy -p {crate} failed without block diagnostics:\n{r.stderr[-3000:]}'
                               + (f'\nLikely cause: {hint}' if hint else ''))
        hard = {m for m, ds in per_block.items() if any(is_hard(d) for d in ds)}
        if not hard:
            return per_block, dropped
        for m in hard:
            dropped[m] = per_block[m]
        active = [b for b in active if b.mod not in hard]
    raise HarnessError(f'{crate}: too many compile rounds')


def check_do(blocks, final, dropped):
    for b in blocks:
        if b.mod in dropped:
            hard = [d for d in dropped[b.mod] if is_hard(d)]
            for d in hard:
                b.fail('compile', d)
        for d in final.get(b.mod, []):
            b.fail('lint', d)


def check_dont(blocks, final, dropped, lints):
    relaxed = set(RELAXED['rust']) | {f'clippy::{n}' for n in RELAXED['clippy']}
    for b in blocks:
        diags = dropped.get(b.mod, final.get(b.mod, []))
        fired = {d.code for d in diags}
        hard = [d for d in diags if is_hard(d)]
        for e in b.expects:
            if e.startswith('scan:'):
                continue
            if e in fired:
                continue
            if e in relaxed:
                why = 'the harness relaxes this lint, so it cannot fire here'
            elif not re.fullmatch(r'E\d{4}', e) and e not in lints:
                why = 'no such lint on the pinned toolchain'
            elif hard and not re.fullmatch(r'E\d{4}', e):
                why = f'lints do not run on code with compile errors; first: {hard[0].short()}'
            elif diags:
                why = f'got {", ".join(sorted({d.code or "error" for d in diags}))}; first: {diags[0].short()}'
            else:
                why = 'the block compiles cleanly'
            b.fail('expect', Diag(b.line, e, f'expected `{e}` did not fire ({why})'))


# ---------------------------------------------------------------- tests

TEST_LINE = re.compile(r'^test (\S+) \.\.\. (ok|FAILED|ignored)')
PANIC_AT = re.compile(r"panicked at (\S+?):(\d+):\d+:?")


def run_tests(ws, env, target_dir, blocks, timeout, log):
    """`cargo test --lib` over the blocks with tests; build errors drop a block and retry."""
    by_mod = {b.mod: b for b in blocks}
    active = list(blocks)
    for _ in range(20):
        write_crate(ws / 'crates' / 'doc-examples', active)
        if not active:
            return
        t0 = time.monotonic()
        r = run_cargo(['test', '-p', 'doc-examples', '--lib', '--no-fail-fast', '--message-format=json'],
                      ws, env, target_dir, timeout)
        log(f'  doc-examples tests: {len(active)} blocks, {time.monotonic() - t0:.0f}s')
        if any(TEST_LINE.match(x) for x in r.stdout.splitlines()):
            failed = parse_test_output(r.stdout, by_mod)
            if r.returncode != 0:
                check_test_process(r, active, failed)
            return
        per_block, unmapped = compile_diags(r.stdout, ws, 'doc-examples', active)
        errs = {m: [d for d in ds if d.level == 'error'] for m, ds in per_block.items()}
        errs = {m: ds for m, ds in errs.items() if ds}
        if r.returncode == 0:
            return  # built, no tests ran
        if not errs:
            detail = '\n'.join(unmapped[:5]) or r.stderr[-3000:]
            hint = devnull_problem()
            raise HarnessError(f'cargo test failed without block diagnostics:\n{detail}'
                               + (f'\nLikely cause: {hint}' if hint else ''))
        for m, ds in errs.items():
            by_mod[m].fail('test', Diag(ds[0].line, ds[0].code, f'test build: {ds[0].text}', ds[0].rendered))
        active = [b for b in active if b.mod not in errs]


def check_test_process(r, blocks, failed):
    """cargo test exited non-zero: a test may have killed the process (stack overflow,
    abort), leaving later tests unrun. Fail every block with no reported test result."""
    reported = {m.group(1).split('::')[0] for x in r.stdout.splitlines() if (m := TEST_LINE.match(x))}
    output = r.stdout + '\n' + r.stderr
    died = re.search(r"\(signal: [^)]*\)|exit (?:status|code): ?\d+", r.stderr)
    how = died.group(0) if died else f'cargo exit {r.returncode}'
    culprits = {}
    for m in re.finditer(r"thread '([^']+)'(?: \(\d+\))? (has overflowed its stack|panicked while [^\n]*)", output):
        culprits.setdefault(m.group(1).split('::')[0], (m.group(1), m.group(2)))
    attributed = 0
    for b in blocks:
        if b.mod in culprits:
            name, what = culprits[b.mod]
            b.fail('test', Diag(b.line, name.split('::', 1)[-1], f'test process died: {what} {how}'))
            attributed += 1
        elif b.mod not in reported:
            b.fail('test', Diag(b.line, 'no-result', f'no test result: the test process died first {how}'))
            attributed += 1
    if not attributed and not failed:
        tail = '\n'.join(r.stderr.strip().splitlines()[-8:])
        raise HarnessError(f'cargo test exited {r.returncode} but no block can be blamed:\n{tail}')


def parse_test_output(stdout, by_mod):
    lines = [x for x in stdout.splitlines() if not x.startswith('{')]
    failed = [m.group(1) for x in lines if (m := TEST_LINE.match(x)) and m.group(2) == 'FAILED']
    details = {}
    cur = None
    for x in lines:
        m = re.match(r'^---- (\S+) stdout ----$', x)
        if m:
            cur = m.group(1)
            details[cur] = []
        elif cur and (x.startswith('failures:') or x.startswith('---- ')):
            cur = None
        elif cur:
            details[cur].append(x)
    for name in failed:
        b = by_mod.get(name.split('::')[0])
        if not b:
            continue
        text = details.get(name, [])
        line = b.line
        msg = 'test failed'
        for i, x in enumerate(text):
            p = PANIC_AT.search(x)
            if p:
                if Path(p.group(1)).name == f'{b.mod}.rs':
                    line = b.md_line(int(p.group(2)))
                rest = [y.strip() for y in text[i + 1:] if y.strip() and not y.startswith('note:')]
                msg = 'panicked: ' + ' '.join(rest[:3]) if rest else 'panicked'
                break
        b.fail('test', Diag(line, name.split('::', 1)[-1], msg[:300], '\n'.join(text)))
    return failed


# ---------------------------------------------------------------- scanner

SCAN_JS = r"""
import { scanSource, RULES } from %s;
import fs from 'node:fs';
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const results = {};
for (const b of input) results[b.id] = scanSource(b.src, { kind: b.kind }).hits;
process.stdout.write(JSON.stringify({ keys: RULES.map((r) => r.key), results }));
"""


def scan_blocks(blocks, scan_mjs):
    """Run scan.mjs over each block's compiled source. Returns (keys, {mod: hits})."""
    if not shutil.which('node'):
        raise HarnessError('node is not on PATH (needed for the scanner pass)')
    payload = []
    for b in blocks:
        src = '\n'.join(b.src) + '\n'
        payload.append({'id': b.mod, 'src': src, 'kind': 'bin' if HAS_MAIN.search(src) else 'prod'})
    js = SCAN_JS % json.dumps(scan_mjs.as_uri())
    r = subprocess.run(['node', '--input-type=module', '-e', js], input=json.dumps(payload),
                       capture_output=True, text=True, timeout=300)
    if r.returncode != 0:
        raise HarnessError(f'scanner failed:\n{r.stderr[-2000:]}')
    out = json.loads(r.stdout)
    extra = set(re.findall(r"\bkey:\s*'([a-z0-9-]+)'", scan_mjs.read_text()))
    return set(out['keys']) | extra, out['results']


def check_scan(blocks, keys, results):
    for b in blocks:
        hits = results.get(b.mod, [])
        if b.kind in ('do', 'fragment'):
            b.asks = [b.md_line(h['line']) for h in hits if h['severity'] == 'ask']
            for h in hits:
                if h['severity'] == 'hard':
                    b.fail('scan', Diag(b.md_line(h['line']), f'scan:{h["key"]}', f'{h["id"]} {h["msg"]}'))
        else:
            fired = {h['key'] for h in hits}
            for e in b.expects:
                if not e.startswith('scan:'):
                    continue
                key = e[len('scan:'):]
                if key not in keys:
                    b.fail('expect', Diag(b.line, e, f'expected `{e}`: no such scanner key in scan.mjs'))
                elif key not in fired:
                    got = ', '.join(sorted(fired)) or 'no hits'
                    b.fail('expect', Diag(b.line, e, f'expected `{e}` did not fire (scanner reported: {got})'))


# ---------------------------------------------------------------- toml

def check_toml(b):
    try:
        tomllib.loads('\n'.join(b.body))
    except tomllib.TOMLDecodeError as e:
        m = re.search(r'at line (\d+)', str(e))
        line = b.md_line(int(m.group(1))) if m else b.line
        b.fail('toml', Diag(line, 'toml', str(e)))


# ---------------------------------------------------------------- main

def select_files(skill, filters):
    files = [p for p in [skill / 'SKILL.md'] + sorted((skill / 'references').glob('*.md')) if p.exists()]
    if not filters:
        return files
    chosen = []
    for f in filters:
        norm = f.replace('\\', '/')
        hits = [p for p in files if str(p).endswith('/' + norm.lstrip('./')) or str(p) == os.path.abspath(f)
                or p.name == norm]
        if not hits:
            raise HarnessError(f'--file {f}: no such skill file (expected SKILL.md or references/<name>.md)')
        chosen += [h for h in hits if h not in chosen]
    return chosen


def report(blocks, verbose, elapsed):
    failing = [b for b in blocks if b.failures]
    for b in failing:
        for check in CHECK_ORDER:
            ds = b.failures.get(check)
            if not ds:
                continue
            ds = sorted(ds, key=lambda d: d.line)
            more = f' (+{len(ds) - 1} more)' if len(ds) > 1 else ''
            print(f'{b.file}:{ds[0].line}: {b.label}@{b.line} {check}: {ds[0].short()}{more}')
            if verbose:
                for d in ds:
                    print(f'    {b.file}:{d.line}: {d.short()}')
                    if d.rendered and check in ('compile', 'lint', 'test'):
                        print('      ' + d.rendered.rstrip().replace('\n', '\n      '))
    kinds = {}
    for b in blocks:
        k = b.label
        if b.kind == 'dont':
            k += ' (expect=)' if b.expects else ' (no expect=)'
        kinds[k] = kinds.get(k, 0) + 1
    print()
    print('Blocks: ' + ', '.join(f'{k} {n}' for k, n in sorted(kinds.items())))
    per_file = {}
    for b in blocks:
        s = per_file.setdefault(b.file, {'blocks': 0, 'failing': 0, 'checks': {}})
        s['blocks'] += 1
        if b.failures:
            s['failing'] += 1
            for c in b.failures:
                s['checks'][c] = s['checks'].get(c, 0) + 1
    print('Failing blocks per file:')
    for f, s in sorted(per_file.items()):
        detail = ', '.join(f'{c} {s["checks"][c]}' for c in CHECK_ORDER if c in s['checks'])
        print(f'  {f}: {s["failing"]}/{s["blocks"]}' + (f' ({detail})' if detail else ''))
    no_expect = {}
    for b in blocks:
        if b.kind == 'dont' and not b.expects:
            no_expect.setdefault(b.file, []).append(str(b.line))
    if no_expect:
        print(f'Don\'t blocks without expect= (reported, not failing): {sum(map(len, no_expect.values()))}')
        for f, ls in sorted(no_expect.items()):
            print(f'  {f}: ' + ', '.join(ls))
    asks = {}
    for b in blocks:
        if b.asks:
            asks.setdefault(b.file, []).append(f'{b.line} (line {", ".join(map(str, sorted(set(b.asks))))})')
    if asks:
        print(f'Do blocks with `#[expect]` exception requests (scanner "ask" hits; reported, not failing): '
              f'{sum(map(len, asks.values()))}')
        for f, ls in sorted(asks.items()):
            print(f'  {f}: ' + ', '.join(ls))
    print(f'{len(failing)} failing block(s) of {len(blocks)} in {elapsed:.0f}s')
    return 1 if failing else 0


def main(argv):
    ap = argparse.ArgumentParser(description='Verify the code blocks in the rust-craft skill.')
    ap.add_argument('--file', action='append', default=[],
                    help='only this skill file (repeatable), e.g. references/errors.md or SKILL.md')
    ap.add_argument('--keep', action='store_true', help='keep the generated workspace and print its path')
    ap.add_argument('--out', help='generate the workspace in this directory (implies --keep)')
    cache = os.environ.get('XDG_CACHE_HOME') or os.path.expanduser('~/.cache')
    ap.add_argument('--target-dir', default=os.path.join(cache, 'temper', 'doc-examples-target'),
                    help='cargo target dir, reused across runs (default: %(default)s)')
    ap.add_argument('--no-test', action='store_true', help='skip `cargo test`')
    ap.add_argument('--scanner', default=str(SCAN_MJS), help='scan.mjs to use (default: %(default)s)')
    ap.add_argument('--timeout', type=int, default=1800, help='seconds per cargo command (default: %(default)s)')
    ap.add_argument('--skill-dir', default=str(SKILL), help='skill to check (default: %(default)s)')
    ap.add_argument('-v', '--verbose', action='store_true', help='print every diagnostic')
    args = ap.parse_args(argv)
    skill = Path(args.skill_dir).resolve()
    t0 = time.monotonic()

    def log(line):
        print(line, file=sys.stderr, flush=True)

    blocks, taken = [], set()
    for f in select_files(skill, args.file):
        for b in parse_file(f, str(f.relative_to(skill))):
            if b.kind in ('do', 'dont', 'fragment'):
                b.mod = module_name(b.file, b.line, taken)
                build_module(b)
            blocks.append(b)

    for b in blocks:
        if b.kind == 'toml':
            check_toml(b)

    scanned = [b for b in blocks if b.kind in ('do', 'fragment') or (b.kind == 'dont' and b.expects)]
    if scanned:
        keys, results = scan_blocks(scanned, Path(args.scanner).resolve())
        check_scan(scanned, keys, results)

    do_blocks = [b for b in blocks if b.kind == 'do']
    dont_blocks = [b for b in blocks if b.kind == 'dont' and any(not e.startswith('scan:') for e in b.expects)]
    if do_blocks or dont_blocks:
        channel = toolchain_channel()
        env = clean_env(channel)
        try:
            v = subprocess.run(['cargo', '--version'], capture_output=True, text=True, env=env, timeout=120,
                               stdin=subprocess.DEVNULL)
        except (OSError, subprocess.TimeoutExpired) as e:
            raise HarnessError(f'cannot run cargo: {e}') from e
        if v.returncode != 0:
            raise HarnessError(f'toolchain {channel} is not usable: {v.stderr.strip()}')
        log(f'doc_examples: {v.stdout.strip()}, {len(do_blocks)} Do and {len(dont_blocks)} Don\'t blocks to compile')
        if devnull_problem():
            log(f'doc_examples: warning: {devnull_problem()}')
        lints = known_lints(env)
        ws = Path(args.out).resolve() if args.out else Path(tempfile.mkdtemp(prefix='temper-doc-examples-'))
        target_dir = Path(args.target_dir).resolve()
        target_dir.mkdir(parents=True, exist_ok=True)
        try:
            write_workspace(ws, do_blocks, dont_blocks)
            final, dropped = compile_rounds(ws, env, target_dir, 'doc-examples', do_blocks, args.timeout, log)
            check_do(do_blocks, final, dropped)
            final_d, dropped_d = compile_rounds(ws, env, target_dir, 'dont-examples', dont_blocks, args.timeout, log)
            check_dont(dont_blocks, final_d, dropped_d, lints)
            if not args.no_test:
                testable = [b for b in do_blocks if b.mod not in dropped and HAS_TEST.search(b.module)]
                run_tests(ws, env, target_dir, testable, args.timeout, log)
            write_crate(ws / 'crates' / 'doc-examples', do_blocks)  # leave the full crate for --keep
            write_crate(ws / 'crates' / 'dont-examples', dont_blocks)
        finally:
            if args.keep or args.out:
                log(f'doc_examples: generated workspace kept at {ws}')
            else:
                shutil.rmtree(ws, ignore_errors=True)

    return report(blocks, args.verbose, time.monotonic() - t0)


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except HarnessError as e:
        print(f'doc_examples: {e}', file=sys.stderr)
        sys.exit(2)
