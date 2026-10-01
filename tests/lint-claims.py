"""Check that the skill's lint and scanner claims are true. Run from the plugin root:

    python3 tests/lint-claims.py [ROOT]

Checks:
  1. Enforced-by cells in references/rules.md that say `lint`: every lint named there
     exists on the pinned toolchain and is enabled (warn, deny or forbid) by the template's
     [workspace.lints], after groups and priorities are applied.
  2. Detect paragraphs (`**Detect.**` up to the next blank line, or the list right after a
     bare `**Detect.**`) in SKILL.md and every reference file: every lint named exists, and
     is enabled by the template unless its sentence marks it as off (convention below).
     A lint whose sentence marks it as off while the template enables it is a stale claim.
  3. Scanner keys cited in Detect paragraphs and Enforced-by cells exist in scan.mjs
     (a `key: '...'` entry, in RULES or pushed directly by scanSource).
  4. Scanner claims agree at the rule-ID level: every registry row whose Enforced-by says
     `scan` has a RULES entry carrying its ID, and every RULES entry's ID is a row whose
     Enforced-by says `scan`. A mismatch means retagging the rule or the row.
  5. scripts/lib/rulemap.json matches a fresh generation by tests/gen_rulemap.py (same
     lint resolution as checks 1-2), so the gate and reports map findings to current rules.

Conventions:
  * A lint is `clippy::name` or a bare backticked snake_case name that is a clippy or
    rustc lint on the toolchain (`todo` after `clippy::panic` is clippy's `todo`). Write
    methods with parentheses or a path (`.unwrap_or_default()`) so they are not read as
    lints. `name=level` and `cast_*`-style families are accepted.
  * A lint is marked off when its sentence contains one of: "not enabled", "not in the
    template", "does not enable", "never enable", "stay(s) off", "opt-in", "opt in",
    "opts in". Sentences end at `.`, `;`, `?` or `!` followed by a space, or at a line
    that starts a list item.
  * A scanner key is cited as `scan:<key>` (the doc-example `expect=` form), or as a
    backticked kebab-case token inside `scan (...)`/`scanner (...)`.

Exit 0 when every claim holds, 1 otherwise, 2 when the toolchain cannot be queried.
"""
import bisect
import importlib.util
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent)
SKILL = ROOT / 'skills' / 'rust-craft'
REFS = SKILL / 'references'
OFF = re.compile(r'not enabled|not in the template|does not enable|doesn.t enable|never enable|'
                 r'\bstays? off\b|\bopt-in\b|\bopts? in\b', re.I)
TICK = re.compile(r'`([^`\n]+)`')
KEBAB = re.compile(r'^[a-z0-9]+(?:-[a-z0-9]+)+$')


# ---------------------------------------------------------------- toolchain

def lint_tables():
    channel = tomllib.loads((ROOT / 'templates/workspace/rust-toolchain.toml').read_text())['toolchain']['channel']
    env = dict(os.environ, RUSTUP_TOOLCHAIN=channel)
    try:
        r = subprocess.run(['clippy-driver', '-W', 'help'], capture_output=True, text=True, env=env, timeout=120,
                           stdin=subprocess.DEVNULL)
    except (OSError, subprocess.TimeoutExpired) as e:
        print(f'lint-claims: cannot run clippy-driver: {e}', file=sys.stderr)
        sys.exit(2)
    if r.returncode != 0 or 'Lint checks' not in r.stdout:
        print(f'lint-claims: clippy-driver -W help failed for toolchain {channel}: {r.stderr.strip()[:300]}',
              file=sys.stderr)
        sys.exit(2)
    default, groups, section = {}, {}, None
    for line in r.stdout.splitlines():
        if line.startswith('Lint checks'):
            section = 'lints'
        elif line.startswith('Lint groups'):
            section = 'groups'
        m = re.match(r'\s+((?:clippy::)?[a-z0-9-]+)\s{2,}(\S.*)$', line)
        if not m or m.group(1) == 'name':
            continue
        name = m.group(1).replace('-', '_')
        if section == 'lints':
            default[name] = m.group(2).split()[0]
        elif section == 'groups':
            groups[name] = {x.strip().replace('-', '_') for x in m.group(2).split(',')}
    return channel, default, groups


def effective_levels(default, groups):
    """Level of every lint after the template's [workspace.lints] (groups first, by priority)."""
    tpl = tomllib.load(open(ROOT / 'templates/workspace/Cargo.toml', 'rb'))['workspace']['lints']
    entries = []
    for tool, table in tpl.items():
        for name, v in table.items():
            level = v['level'] if isinstance(v, dict) else v
            prio = v.get('priority', 0) if isinstance(v, dict) else 0
            full = name if tool == 'rust' else f'{tool}::{name}'
            entries.append((prio, full not in groups, full, level))
    levels = dict(default)
    for _, _, name, level in sorted(entries):
        for member in groups.get(name, {name}):
            levels[member] = level
    return levels


def resolve(token, default):
    """Map a backticked token to lint names, or None if it is not a lint."""
    token = token.strip().split('=')[0].replace('-', '_')
    m = re.fullmatch(r'((?:clippy::)?[a-z][a-z0-9_]*)\*', token)
    if m:  # a family such as `cast_*`, checked as a whole; `validate_*` is just a name pattern
        prefix = m.group(1)
        clippy = prefix.startswith('clippy::')
        if any(n.startswith(prefix) or (not clippy and n.startswith('clippy::' + prefix)) for n in default):
            return ['*' + token]
        return ['*' + token] if clippy else None
    if token.endswith('*'):
        return None
    if token.startswith('clippy::'):
        return [token]
    if re.fullmatch(r'[a-z][a-z0-9_]*', token):
        if token in default:
            return [token]
        if 'clippy::' + token in default:
            return ['clippy::' + token]
    return None


# ---------------------------------------------------------------- scanner

def scanner_rules():
    """(key, id) of every RULES entry: imported through node, or read as text when a
    half-edited scan.mjs does not load."""
    js = ("import(process.argv[1]).then((m) => process.stdout.write(JSON.stringify("
          "m.RULES.map((r) => [r.key, r.id]))))")
    try:
        r = subprocess.run(['node', '-e', js, (ROOT / 'scripts/lib/scan.mjs').as_uri()], capture_output=True,
                           text=True, timeout=60, stdin=subprocess.DEVNULL)
        if r.returncode == 0:
            return [tuple(x) for x in json.loads(r.stdout)]
    except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError):
        pass
    text = (ROOT / 'scripts/lib/scan.mjs').read_text()
    return re.findall(r"\bkey:\s*'([a-z0-9-]+)'\s*,\s*id:\s*'([A-Z]{1,4}-\d{2})'", text)


def scanner_keys():
    """Scanner keys, read as text so a half-edited scan.mjs still yields its keys."""
    return set(re.findall(r"\bkey:\s*'([a-z0-9-]+)'", (ROOT / 'scripts/lib/scan.mjs').read_text()))


def scan_citations(text):
    """(key, offset) for every scanner key cited in text."""
    out = [(m.group(1), m.start()) for m in re.finditer(r'\bscan:([a-z0-9][a-z0-9-]*)', text)]
    for m in re.finditer(r'\bscan(?:ner)?\s*\(([^)]*)\)', text):
        for t in TICK.finditer(m.group(1)):
            if KEBAB.match(t.group(1)):
                out.append((t.group(1), m.start(1) + t.start()))
    return out


# ---------------------------------------------------------------- sources

def registry_rows():
    rows = []
    for n, line in enumerate((REFS / 'rules.md').read_text().splitlines(), 1):
        cells = [c.strip() for c in re.split(r'(?<!\\)\|', line.strip().strip('|'))]
        if len(cells) == 5 and re.fullmatch(r'[A-Z]{1,4}-\d{2}', cells[0]):
            rows.append((n, cells[0], cells[3]))
    return rows


def detect_paragraphs(path):
    """Yield (first line number, text, line starts) for each Detect paragraph."""
    lines = path.read_text().splitlines()
    i = 0
    while i < len(lines):
        m = re.match(r'\s*(?:[-*]\s+)?\*\*Detect[.:]?\*\*[.:]?\s*(.*)$', lines[i])
        if not m:
            i += 1
            continue
        start, parts, j = i + 1, [m.group(1)], i + 1
        if not m.group(1).strip():
            while j < len(lines) and not lines[j].strip():
                j += 1
            start = j + 1
            parts = []
        while j < len(lines) and lines[j].strip() and not re.match(r'\s*(#|```|~~~|\*\*[A-Z])', lines[j]):
            parts.append(lines[j])
            j += 1
        text = '\n'.join(parts)
        starts = [0]
        for p in parts[:-1]:
            starts.append(starts[-1] + len(p) + 1)
        yield start, text, starts
        i = max(j, i + 1)


def sentence_bounds(text):
    """Start offsets of sentences, computed with code spans masked."""
    masked = TICK.sub(lambda m: '`' + 'x' * (len(m.group(0)) - 2) + '`', text)
    bounds = [0]
    for m in re.finditer(r'[.;!?]\s+(?=\S)|\n(?=\s*[-*] )', masked):
        bounds.append(m.end())
    return bounds


# ---------------------------------------------------------------- main

def main():
    channel, default, groups = lint_tables()
    levels = effective_levels(default, groups)
    scan = scanner_keys()
    bad = []
    counts = {'lints': 0, 'keys': 0, 'paragraphs': 0}

    def check_lint(where, name, off_marked):
        counts['lints'] += 1
        if name.startswith('*'):  # family: some member exists and (unless marked off) is enabled
            prefix = name[1:-1]
            members = [n for n in default if n.startswith(prefix) or n.startswith('clippy::' + prefix)]
            if not members:
                bad.append((where, f'`{name[1:]}` matches no lint on {channel}'))
            elif not off_marked and all(levels.get(n, 'allow') == 'allow' for n in members):
                bad.append((where, f'`{name[1:]}` is cited as detection but no member is enabled by the template'))
            return
        if name not in default:
            bad.append((where, f'`{name.lstrip("?")}` is not a lint on {channel}'))
        elif off_marked:
            if levels.get(name, 'allow') != 'allow':
                bad.append((where, f'`{name}` is described as not enabled or opt-in, but the template enables it'))
        elif levels.get(name, 'allow') == 'allow':
            bad.append((where, f'`{name}` is cited as detection but is not enabled by the template'))

    # 1 + 3: registry Enforced-by cells
    for n, rid, enforced in registry_rows():
        where = f'references/rules.md:{n} {rid}'
        if re.search(r'\blint\b', enforced):
            for t in TICK.finditer(enforced):
                for name in resolve(t.group(1), default) or []:
                    check_lint(where, name, False)
        for key, _ in scan_citations(enforced):
            counts['keys'] += 1
            if key not in scan:
                bad.append((where, f'cites scanner key `{key}`, which scan.mjs does not define'))

    # 4: scanner claims at the rule-ID level, both directions
    rows = registry_rows()
    claims_scan = {rid: bool(re.search(r'\bscan\b', enforced)) for _, rid, enforced in rows}
    rule_ids = {}
    for key, rid in scanner_rules():
        rule_ids.setdefault(rid, []).append(key)
    for n, rid, enforced in rows:
        if claims_scan[rid] and rid not in rule_ids:
            bad.append((f'references/rules.md:{n} {rid}', 'Enforced-by says scan, but no scan.mjs RULES entry '
                        'carries this ID'))
    for rid, keys in sorted(rule_ids.items()):
        where = 'scripts/lib/scan.mjs ' + ', '.join(f'`{k}`' for k in keys)
        if rid not in claims_scan:
            bad.append((where, f'tagged {rid}, which is not in rules.md'))
        elif not claims_scan[rid]:
            bad.append((where, f'tagged {rid}, but that rule\'s Enforced-by does not mention scan'))

    # 2 + 3: Detect paragraphs
    for path in [SKILL / 'SKILL.md'] + sorted(p for p in REFS.glob('*.md') if p.name != 'rules.md'):
        rel = path.relative_to(SKILL)
        for start, text, starts in detect_paragraphs(path):
            counts['paragraphs'] += 1
            bounds = sentence_bounds(text)

            def line_of(off):
                return start + bisect.bisect_right(starts, off) - 1

            def sentence(off):
                k = bisect.bisect_right(bounds, off) - 1
                return text[bounds[k]:bounds[k + 1] if k + 1 < len(bounds) else len(text)]

            for t in TICK.finditer(text):
                names = resolve(t.group(1), default)
                if names is None:
                    continue
                off_marked = bool(OFF.search(sentence(t.start())))
                for name in names:
                    check_lint(f'{rel}:{line_of(t.start())}', name, off_marked)
            for key, off in scan_citations(text):
                counts['keys'] += 1
                if key not in scan:
                    bad.append((f'{rel}:{line_of(off)}', f'cites scanner key `{key}`, which scan.mjs does not define'))

    # 5: the generated rule map is fresh
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location('gen_rulemap', Path(__file__).resolve().parent / 'gen_rulemap.py')
    gen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gen)
    for msg in gen.stale(ROOT, sys.modules[__name__], default):
        bad.append(('scripts/lib/rulemap.json', msg))

    for where, msg in bad:
        print(f'{where}: {msg}')
    print(f'{counts["lints"]} lint and {counts["keys"]} scanner-key citations checked '
          f'({counts["paragraphs"]} Detect paragraphs + rules.md); {len(bad)} false claim(s)')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
