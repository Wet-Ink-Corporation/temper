#!/usr/bin/env python3
"""Every ported 1.x engine test has a Rust home: the port plan's test map (§2.2, "every 1.x test is
ported, retired or replaced"; 1e-9, wi-a9dd30), one entry per 1.x test file under `sources`
(port plan Phase 2, 2-18). Phase 8 extends the map to every engine test.

    python3 tests/parity/check_test_map.py               # check tests/parity/test-map.json
    python3 tests/parity/check_test_map.py --self-test

For each source, the 1.x names come from `node --test --test-reporter=tap <source>`, the Rust
names from `cargo test -p <crate> --all-features --locked -- --list`. Fails, naming each problem,
when a 1.x test has no entry or two, an entry names no 1.x test, a mapped Rust path is not a test,
or an entry with no Rust path (or a `retired` note) cites no D16 item. A source whose port is not
finished carries a `pending` note naming the phases that port the rest; its unmapped tests are
counted, not failed, and every source without one is checked in full. Exit 1 on a problem, 2 when
node or cargo cannot be run: never a skip.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
MAP = Path(__file__).resolve().parent / 'test-map.json'
D16 = re.compile(r'D16\([a-z]\)')
TAP_TOP = re.compile(r'^(?:not )?ok \d+ - (.*?)(?: # (?:SKIP|TODO)\b.*)?$')
RUST_TEST = re.compile(r'^(\S+): test$')


class HarnessError(Exception):
    """node or cargo could not be run, or printed what this script cannot read."""


def tap_unescape(s):
    return re.sub(r'\\(.)', lambda m: {'n': '\n', 'r': '\r', 't': '\t'}.get(m.group(1), m.group(1)), s)


def node_names(tap):
    """The top-level test names in node's TAP output, in file order."""
    return [tap_unescape(m.group(1)) for m in map(TAP_TOP.match, tap.splitlines()) if m]


def rust_names(listing):
    """The test paths in `cargo test -- --list` output."""
    return {m.group(1) for m in map(RUST_TEST.match, listing.splitlines()) if m}


def check(entries, node, rust, pending=None):
    """The problems with `entries` against the 1.x names `node` and the Rust paths `rust`.

    With a `pending` note, a 1.x test that has no entry yet is left to a later phase."""
    problems = []
    counts = {}
    for e in entries:
        counts[e.get('node')] = counts.get(e.get('node'), 0) + 1
    if pending is not None and not (isinstance(pending, str) and pending.strip()):
        problems.append('a pending note must name the phases that port the rest')
    for name in node:
        if counts.get(name, 0) == 0:
            if pending is None:
                problems.append(f'unmapped 1.x test: {name!r}')
        elif counts[name] > 1:
            problems.append(f'1.x test mapped twice: {name!r}')
    known = set(node)
    for e in entries:
        name = e.get('node')
        if name not in known:
            problems.append(f'entry names no 1.x test: {name!r}')
        paths = e.get('rust', [])
        for p in paths:
            if p not in rust:
                problems.append(f'not a Rust test: {p!r} (for {name!r})')
        retired = e.get('retired')
        if (not paths or retired is not None) and not (isinstance(retired, str) and D16.search(retired)):
            problems.append(f'retired entry cites no D16 item: {name!r}')
    return problems


def run(cmd):
    try:
        r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, encoding='utf-8', timeout=1800,
                           stdin=subprocess.DEVNULL)
    except (OSError, subprocess.TimeoutExpired) as e:
        raise HarnessError(f'cannot run {" ".join(cmd)}: {e}') from e
    if r.returncode != 0:
        raise HarnessError(f'{" ".join(cmd)} exited {r.returncode}: {r.stderr.strip()[-2000:]}')
    return r.stdout


def eq(got, want):
    if got != want:
        raise AssertionError(f'self-test: got {got!r}, want {want!r}')


def self_test():
    node = ['a', 'b # c', 'large']
    rust = {'x::a', 'x::b', 'x::large'}
    good = [{'node': 'a', 'rust': ['x::a']}, {'node': 'b # c', 'rust': ['x::b']},
            {'node': 'large', 'rust': ['x::large'], 'retired': 'its timing bound, D16(k)'}]
    eq(check(good, node, rust), [])
    eq(check(good[:2], node, rust), ["unmapped 1.x test: 'large'"])
    eq(check(good + [good[0]], node, rust), ["1.x test mapped twice: 'a'"])
    eq(check(good + [{'node': 'gone', 'rust': ['x::a']}], node, rust), ["entry names no 1.x test: 'gone'"])
    missing = [{'node': 'a', 'rust': ['x::nope']}, *good[1:]]
    eq(check(missing, node, rust), ["not a Rust test: 'x::nope' (for 'a')"])
    for retired in [{'node': 'large', 'rust': []}, {'node': 'large', 'rust': [], 'retired': 'moved to CI'},
                    {'node': 'large', 'rust': ['x::large'], 'retired': 'moved to CI'}]:
        eq(check(good[:2] + [retired], node, rust), ["retired entry cites no D16 item: 'large'"])
    eq(check(good[:2] + [{'node': 'large', 'retired': 'D16(k)'}], node, rust), [])
    eq(check(good[:2], node, rust, pending='port phase 2c'), [])
    eq(check(good[:2], node, rust, pending=' '), ['a pending note must name the phases that port the rest'])
    eq(check(good + [{'node': 'gone', 'rust': ['x::a']}], node, rust, pending='2c'), ["entry names no 1.x test: 'gone'"])
    tap = 'TAP version 13\n# Subtest: a\n    ok 1 - inner\nok 1 - a\nnot ok 2 - b \\# c\nok 3 - large # SKIP slow\n1..3\n'
    eq(node_names(tap), node)
    eq(rust_names('x::a: test\nx::b: test\n\n2 tests, 0 benchmarks\nsrc/lib.rs - f (line 3): test\n'), {'x::a', 'x::b'})
    print('test map self-test: ok')


def main(argv):
    if argv == ['--self-test']:
        self_test()
        return 0
    if argv:
        print('usage: check_test_map.py [--self-test]', file=sys.stderr)
        return 2
    doc = json.loads(MAP.read_text(encoding='utf-8'))
    rust_by_crate = {}
    failed = False
    for src in doc['sources']:
        node = node_names(run(['node', '--test', '--test-reporter=tap', src['source']]))
        if not node:
            raise HarnessError(f'no test names in the TAP output of {src["source"]}')
        crate = src['crate']
        if crate not in rust_by_crate:
            listing = run(['cargo', 'test', '-p', crate, '--all-features', '--locked', '-q', '--', '--list'])
            rust_by_crate[crate] = rust_names(listing)
        problems = check(src['tests'], node, rust_by_crate[crate], src.get('pending'))
        for p in problems:
            print(f'check_test_map: {src["source"]}: {p}', file=sys.stderr)
        failed = failed or bool(problems)
        mapped = len({e.get('node') for e in src['tests']} & set(node))
        rest = f'; {len(node) - mapped} pending ({src["pending"]})' if src.get('pending') else ''
        print(f'check_test_map: {mapped} of {len(node)} 1.x tests in {src["source"]} mapped to {crate} tests{rest}')
    return 1 if failed else 0


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except HarnessError as e:
        print(f'check_test_map: {e}', file=sys.stderr)
        sys.exit(2)
