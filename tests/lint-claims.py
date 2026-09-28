"""Check that every clippy/rustc lint cited as enforcement in rules.md is enabled
(warn or deny) by the workspace template. Run from the plugin root:
    python3 tests/lint-claims.py
"""
import re
import subprocess
import sys
import tomllib

ROOT = sys.argv[1] if len(sys.argv) > 1 else '.'
help_text = subprocess.run(['clippy-driver', '-W', 'help'], capture_output=True, text=True).stdout
groups = {}
for line in help_text.splitlines():
    m = re.match(r'\s*clippy::([a-z]+)\s{2,}(clippy::.*)$', line)
    if m:
        groups[m.group(1)] = {x.strip().replace('clippy::', '').replace('-', '_') for x in m.group(2).split(',')}
rustc_default = {}
for line in help_text.splitlines():
    m = re.match(r'\s*([a-z0-9-]+)\s+(allow|warn|deny|forbid)\s', line)
    if m and not m.group(1).startswith('clippy'):
        rustc_default[m.group(1).replace('-', '_')] = m.group(2)

tpl = tomllib.load(open(f'{ROOT}/templates/workspace/Cargo.toml', 'rb'))
clippy_cfg = tpl['workspace']['lints']['clippy']
rust_cfg = tpl['workspace']['lints']['rust']
def level(v):
    return v['level'] if isinstance(v, dict) else v
enabled = set()
for name, v in clippy_cfg.items():
    if name in groups and level(v) != 'allow':
        enabled |= groups[name]
for name, v in clippy_cfg.items():
    if name not in groups:
        (enabled.add if level(v) != 'allow' else enabled.discard)(name)

rules = open(f'{ROOT}/skills/rust-craft/references/rules.md').read()
bad = []
for row in rules.splitlines():
    cells = [c.strip() for c in row.split('|')]
    if len(cells) < 6 or not re.match(r'^[A-Z]+-\d{2}$', cells[1] if len(cells) > 1 else ''):
        continue
    rule, enforced = cells[1], cells[4]
    if 'lint' not in enforced:
        continue
    for lint in re.findall(r'`([a-z_:]+)(?:=[a-z]+)?`', enforced):
        lint = lint.split('=')[0]
        if lint.startswith('clippy::'):
            n = lint[len('clippy::'):]
            if n not in enabled:
                bad.append((rule, lint))
        elif '::' not in lint:
            # clippy lints are often written without the prefix after the first one
            if lint in {x for g in groups.values() for x in g}:
                if lint not in enabled:
                    bad.append((rule, f'clippy::{lint}'))
            elif lint in rustc_default or lint in rust_cfg:
                lvl = level(rust_cfg[lint]) if lint in rust_cfg else rustc_default[lint]
                if lvl == 'allow':
                    bad.append((rule, lint))
for rule, lint in bad:
    print(f'{rule}: {lint} is cited as enforcement but is not enabled by the template')
print(f'{len(bad)} unenforced lint claim(s)')
sys.exit(1 if bad else 0)
