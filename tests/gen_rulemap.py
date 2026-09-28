"""Generate scripts/lib/rulemap.json from references/rules.md. Run from the plugin root:

    python3 tests/gen_rulemap.py            # rewrite scripts/lib/rulemap.json
    python3 tests/gen_rulemap.py --check    # exit 1 when the committed file is stale

The map ties each diagnostic temper collects to the registry rules that cite it:

  * "lints": lint name -> rule IDs, from the backticked tokens of every Enforced-by cell
    that says `lint`, resolved exactly as tests/lint-claims.py resolves them (a bare
    snake_case token is a rustc lint when rustc knows it, else a clippy lint when clippy
    knows it; `cast_*`-style families expand to their members on the pinned toolchain;
    `rustdoc::name` is kept as written).
  * "scan": scanner key -> rule IDs, from the RULES table in scripts/lib/scan.mjs.
  * "rules": rule ID -> level, family and title (the first sentence of the Rule cell).

Nothing parses rules.md at runtime: the plugin and the vendored copy read this JSON.
Exit 0 on success, 1 when --check finds the file stale, 2 when the toolchain cannot be queried.
"""
import importlib.util
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.dont_write_bytecode = True  # no __pycache__ next to the tests
OUT_REL = 'scripts/lib/rulemap.json'
LEVELS = ('MUST', 'SHOULD', 'CONSIDER')


def load_claims():
    """tests/lint-claims.py as a module (its file name is not importable by name)."""
    spec = importlib.util.spec_from_file_location('lint_claims', HERE / 'lint-claims.py')
    mod = importlib.util.module_from_spec(spec)
    saved = sys.argv
    sys.argv = [saved[0]]  # lint-claims reads ROOT from argv at import time
    try:
        spec.loader.exec_module(mod)
    finally:
        sys.argv = saved
    return mod


def registry(root):
    """(id, level, rule text, enforced-by text) for every registry row, in file order."""
    rows = []
    for line in (root / 'skills/rust-craft/references/rules.md').read_text(encoding='utf-8').splitlines():
        cells = [c.strip() for c in re.split(r'(?<!\\)\|', line.strip().strip('|'))]
        if len(cells) == 5 and re.fullmatch(r'[A-Z]{1,4}-\d{2}', cells[0]):
            rows.append((cells[0], cells[1], cells[2].replace('\\|', '|'), cells[3]))
    return rows


def title(rule):
    """First sentence of a Rule cell, code spans kept whole, at most 140 characters."""
    masked = re.sub(r'`[^`\n]+`', lambda m: '`' + 'x' * (len(m.group(0)) - 2) + '`', rule)
    m = re.search(r'[.!?](?=\s|$)', masked)
    text = rule[:m.end()] if m else rule
    return text if len(text) <= 140 else text[:139].rstrip() + '…'


def lint_names(claims, token, default):
    """Lint names a backticked Enforced-by token stands for (families expanded)."""
    tok = token.strip().split('=')[0]
    if re.fullmatch(r'rustdoc::[a-z][a-z0-9_]*', tok):
        return [tok]
    names = claims.resolve(token, default) or []
    out = []
    for name in names:
        if name.startswith('*'):
            prefix = name[1:-1]
            out.extend(sorted(n for n in default if n.startswith(prefix) or n.startswith('clippy::' + prefix)))
        else:
            out.append(name)
    return out


def generate(root, claims, default):
    lints, scan, rules = {}, {}, {}
    for rid, level, rule, enforced in registry(root):
        rules[rid] = {'level': level, 'family': rid.split('-')[0], 'title': title(rule)}
        if not re.search(r'\blint\b', enforced):
            continue
        for t in claims.TICK.finditer(enforced):
            for name in lint_names(claims, t.group(1), default):
                ids = lints.setdefault(name, [])
                if rid not in ids:
                    ids.append(rid)
    for key, rid in claims.scanner_rules():
        ids = scan.setdefault(key, [])
        if rid not in ids:
            ids.append(rid)
    return {
        'generated_from': 'rules.md',
        'lints': dict(sorted(lints.items())),
        'scan': dict(sorted(scan.items())),
        'rules': rules,
    }


def render(data):
    return json.dumps(data, indent=2, ensure_ascii=False) + '\n'


def stale(root, claims, default):
    """Problems with the committed rulemap.json (empty when it matches a fresh generation)."""
    want = render(generate(root, claims, default))
    path = root / OUT_REL
    try:
        have = path.read_text(encoding='utf-8').replace('\r\n', '\n')
    except OSError:
        return [f'{OUT_REL} is missing: run python3 tests/gen_rulemap.py']
    if have != want:
        return [f'{OUT_REL} differs from a fresh generation from rules.md and scan.mjs: '
                'run python3 tests/gen_rulemap.py and commit the result']
    return []


def main():
    root = HERE.parent
    claims = load_claims()
    _, default, _ = claims.lint_tables()
    if '--check' in sys.argv[1:]:
        problems = stale(root, claims, default)
        for p in problems:
            print(p)
        return 1 if problems else 0
    data = generate(root, claims, default)
    (root / OUT_REL).write_text(render(data), encoding='utf-8', newline='\n')
    print(f'{OUT_REL}: {len(data["lints"])} lints, {len(data["scan"])} scanner keys, {len(data["rules"])} rules')
    return 0


if __name__ == '__main__':
    sys.exit(main())
