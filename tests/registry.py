#!/usr/bin/env python3
"""Check the rule registry (skills/rust-craft/references/rules.md) against the rest of the plugin.

    python3 tests/registry.py

Checks:
  1. rules.md rows are well formed: `| ID | MUST/SHOULD/CONSIDER | rule | enforced by | detail |`,
     and no ID appears twice.
  2. Every registry ID has exactly one section across SKILL.md and references/*.md. A
     section is a `##`-`####` heading whose text starts with the ID followed by ` ·`,
     `:`, ` and`, `,` or the end of the heading:
         ### ERR-13 · Explicit finish     ## H-02: unwrap and expect
     Only the first ID of a heading counts (`## H-07 and H-08: ...` is H-07's section,
     `### ASY-09 · H-14 · ...` is ASY-09's). `### PERF-06 (continued) · ...` is a
     continuation, not a second section. A section outside the files in the ID's Detail
     column is printed as a note, not a failure: the H-rule catalogue in
     escape-hatches.md keeps pointer sections for rules detailed elsewhere.
  3. Every rule ID cited (`[A-Z]{1,4}-NN`) in README.md, skills/**/*.md, agents/*.md,
     the templates' .claude/rules/*.md, AGENTS.md and CLAUDE.md files, Cargo.toml and
     clippy.toml files, and scripts/**/*.mjs exists in rules.md.
  4. Every file named in a Detail cell exists in references/ (or is SKILL.md).
  5. Every rule count stated in README.md or SKILL.md ("182 rules") equals the number of
     registry rows.

Output: `file:line: message` per failure, then a summary. Exit 0 when clean, 1 on any
failure, 2 when rules.md cannot be read.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKILL = ROOT / 'skills' / 'rust-craft'
REFS = SKILL / 'references'
RULES = REFS / 'rules.md'

ROW = re.compile(r'^\|\s*([A-Z]{1,4}-\d{2})\s*\|(.*)$')
HEADING = re.compile(r'^(#{2,4})\s+([A-Z]{1,4}-\d{2})(\s*(?:·|:|,|and\b)|\s*$)')
CONTINUED = re.compile(r'^#{2,4}\s+[A-Z]{1,4}-\d{2}\s*\(continued\)')
CITATION = re.compile(r'(?<![A-Za-z0-9-])([A-Z]{1,4}-\d{2})(?![0-9A-Za-z])')
COUNT = re.compile(r'\b(\d{2,4})\s+rules\b')


def rel(p):
    return str(p.relative_to(ROOT))


def load_registry(failures):
    rows = {}
    for n, line in enumerate(RULES.read_text(encoding='utf-8').splitlines(), 1):
        m = ROW.match(line)
        if not m:
            continue
        rid = m.group(1)
        cells = [c.strip() for c in re.split(r'(?<!\\)\|', line.strip().strip('|'))]
        if len(cells) != 5:
            failures.append((rel(RULES), n, f'{rid}: expected 5 cells, found {len(cells)}'))
            continue
        if cells[1] not in ('MUST', 'SHOULD', 'CONSIDER'):
            failures.append((rel(RULES), n, f'{rid}: level `{cells[1]}` is not MUST, SHOULD or CONSIDER'))
        if rid in rows:
            failures.append((rel(RULES), n, f'{rid}: duplicate registry row (first at line {rows[rid]["line"]})'))
            continue
        details = [d.strip() for d in cells[4].split(',') if d.strip()]
        rows[rid] = {'line': n, 'details': details}
    return rows


def check_sections(rows, failures, notes):
    files = [SKILL / 'SKILL.md'] + sorted(p for p in REFS.glob('*.md') if p != RULES)
    sections = {}
    for f in files:
        for n, line in enumerate(f.read_text(encoding='utf-8').splitlines(), 1):
            if CONTINUED.match(line):
                continue
            m = HEADING.match(line)
            if m:
                sections.setdefault(m.group(2), []).append((f, n))
    missing = []
    for rid, row in rows.items():
        found = sections.get(rid, [])
        if not found:
            missing.append(rid)
            failures.append((rel(RULES), row['line'], f'{rid}: no section (`### {rid} · Title`) in '
                             + ' or '.join(row['details'])))
            continue
        if len(found) > 1:
            where = ', '.join(f'{rel(f)}:{n}' for f, n in found)
            failures.append((rel(found[1][0]), found[1][1], f'{rid}: {len(found)} sections ({where}); keep one'))
        for f, n in found:
            if f.name not in row['details']:
                notes.append((rel(f), n, f'{rid}: section is in {f.name}, but its Detail is '
                              + ', '.join(row['details'])))
    for rid, found in sections.items():
        if rid not in rows:
            for f, n in found:
                failures.append((rel(f), n, f'{rid}: section heading for an ID that is not in rules.md'))
    return missing


def citation_files():
    pats = ['README.md', 'skills/**/*.md', 'agents/*.md', 'templates/**/.claude/rules/*.md',
            'templates/**/AGENTS.md', 'templates/**/CLAUDE.md', 'templates/**/Cargo.toml',
            'templates/**/clippy.toml', 'scripts/**/*.mjs']
    seen = []
    for pat in pats:
        for p in sorted(ROOT.glob(pat)):
            if p.is_file() and 'target' not in p.relative_to(ROOT).parts and p not in seen:
                seen.append(p)
    return seen


def check_citations(rows, failures):
    count = 0
    for f in citation_files():
        for n, line in enumerate(f.read_text(encoding='utf-8').splitlines(), 1):
            for m in CITATION.finditer(line):
                count += 1
                if m.group(1) not in rows:
                    failures.append((rel(f), n, f'cites {m.group(1)}, which is not in rules.md'))
    return count


def check_details(rows, failures):
    for rid, row in rows.items():
        for d in row['details']:
            if d != 'SKILL.md' and not (REFS / d).is_file():
                failures.append((rel(RULES), row['line'], f'{rid}: Detail file {d} does not exist'))


def check_counts(rows, failures):
    stated = 0
    for f in (ROOT / 'README.md', SKILL / 'SKILL.md'):
        for n, line in enumerate(f.read_text(encoding='utf-8').splitlines(), 1):
            for m in COUNT.finditer(line):
                stated += 1
                if int(m.group(1)) != len(rows):
                    failures.append((rel(f), n, f'states {m.group(1)} rules; rules.md has {len(rows)}'))
    return stated


def main():
    failures, notes = [], []
    try:
        rows = load_registry(failures)
    except OSError as e:
        print(f'registry: cannot read {RULES}: {e}', file=sys.stderr)
        return 2
    if not rows:
        print(f'registry: no rule rows found in {rel(RULES)}', file=sys.stderr)
        return 2
    missing = check_sections(rows, failures, notes)
    cited = check_citations(rows, failures)
    check_details(rows, failures)
    stated = check_counts(rows, failures)
    for f, n, msg in failures:
        print(f'{f}:{n}: {msg}')
    for f, n, msg in notes:
        print(f'{f}:{n}: note: {msg}')
    print(f'\n{len(rows)} rules; {len(rows) - len(missing)} with a section; {cited} citations checked; '
          f'{stated} stated count(s).')
    if missing:
        print('Missing sections: ' + ', '.join(missing))
    print(f'{len(failures)} failure(s)')
    return 1 if failures else 0


if __name__ == '__main__':
    sys.exit(main())
