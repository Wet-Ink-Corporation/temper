#!/usr/bin/env python3
"""temper lens stays read-only: an allow-list over what `claude plugin validate --json` says the
lens module hooks and calls (#107, Design: read-only; #110).

    claude plugin validate --json .claude-plugin/plugin.json > validate.json
    python3 tests/lens_allowlist.py validate.json
    python3 tests/lens_allowlist.py --self-test

Fails on any hooked event or `$` call outside the lists below, on any env write, on a state write
outside `temper.*`, on a gating hook without `.catch`, when the report says nothing about the
module (so a format change cannot pass silently), and when `$.process.run` is given anything but
the temper CLI of this plugin. A later change cannot quietly add `tool.check`, `$.fs.write`,
`$.http`, `$.model` or `$.prompt.submit`: widening a list here is a reviewed decision.
"""
import json
import re
import sys
from pathlib import Path

MODULE = './lens/register.tsx'
SOURCE = Path(__file__).resolve().parent.parent / 'hooks' / 'lens' / 'register.tsx'

# Events the lens may hook: it observes them and passes every result on unchanged.
HOOKS = {
    'session.start', 'session.end', 'tool.call', 'turn.complete', 'classic.Stop', 'classic.SubagentStop',
    'ui.render', 'ui.press', 'command.run',
}
# `$` calls the lens may make. `$.process.run` only ever runs the temper CLI (checked below).
CALLS = {
    '$.process.run', '$.fs.read', '$.state.get', '$.state.set', '$.store.get', '$.store.set',
    '$.command.register', '$.prompt.fill', '$.env.get', '$.session.surfaces', '$.session.id',
}
CALL_PREFIXES = ('$.ui.', '$.clock.')


def split(text):
    """A report list: commas inside a matcher (`{a=1, b=2}`) or a via note (`(via f, g)`) do not split."""
    return [x.strip() for x in re.split(r',\s*(?![^{(]*[})])', text) if x.strip()]


def problems(report, source):
    notes = []
    gating = []
    for part in [report.get('manifest', {})] + list(report.get('contents', [])):
        notes += [n for n in part.get('notes', []) if n.startswith(MODULE + ' ')]
        gating += [g for g in part.get('gatingHooks', []) if g.get('module') == MODULE]
    out = []
    if not report.get('success'):
        out.append('claude plugin validate did not pass')

    def field(name):
        hit = [n[len(MODULE) + 1 + len(name) + 1:].strip() for n in notes if n.startswith(f'{MODULE} {name}:')]
        return hit[0] if hit else None

    hooks = field('hooks')
    calls = field('calls')
    if hooks is None or calls is None:
        out.append(f'the report lists no "hooks:" or "calls:" line for {MODULE}: is the module registered in hooks/hooks.json?')
        return out
    for h in split(hooks):
        name = re.sub(r'\{.*\}$', '', h)
        if name not in HOOKS:
            out.append(f'the lens hooks {h}, which is not on the read-only allow-list')
    for c in split(calls):
        name = re.sub(r'\s*\(via .*\)$', '', c)
        if name not in CALLS and not name.startswith(CALL_PREFIXES):
            out.append(f'the lens calls {name}, which is not on the read-only allow-list')
    env_writes = field('env writes')
    if env_writes not in (None, 'nothing'):
        out.append(f'the lens writes environment variables: {env_writes}')
    state_writes = field('state writes')
    for w in [x.strip() for x in (state_writes or '').split(',') if x.strip() and x.strip() != 'nothing']:
        if not w.startswith('temper.'):
            out.append(f'the lens writes state it does not own: {w}')
    for g in gating:
        if not g.get('hasCatch'):
            out.append(f'gating hook {g.get("hook")} has no .catch: a lens failure must never decide it')
    for n in notes:
        if n.startswith(f'{MODULE} gating hook without .catch:'):
            out.append(n)
    # Every $.process.run runs `node <plugin root>/scripts/temper.mjs ...`.
    for m in re.finditer(r'\$\.process\.run\(\s*\[([^\]]*)', source):
        argv = m.group(1)
        if not re.match(r"\s*'node',\s*`\$\{\$\.plugin\.root\}/scripts/temper\.mjs`", argv):
            out.append(f'$.process.run runs something other than the temper CLI: [{argv.strip()[:80]}')
    return out


def self_test():
    good_notes = [
        f'{MODULE} hooks: session.start, tool.call, ui.render{{component=AbovePrompt}}, ui.render{{component=Pane, requestId=p}}',
        f'{MODULE} calls: $.process.run (via runStats, statusView), $.ui.resolve, $.state.get',
        f'{MODULE} env writes: nothing',
        f'{MODULE} state writes: temper.view',
    ]
    good_src = "await $.process.run(['node', `${$.plugin.root}/scripts/temper.mjs`, 'status'])"
    report = lambda notes, gating=(): {'success': True, 'contents': [{'notes': notes, 'gatingHooks': list(gating)}]}
    assert problems(report(good_notes), good_src) == [], problems(report(good_notes), good_src)
    bad = {
        'tool.check': [n.replace('tool.call', 'tool.call, tool.check') for n in good_notes],
        '$.fs.write': [n.replace('$.ui.resolve', '$.ui.resolve, $.fs.write') for n in good_notes],
        '$.prompt.submit': [n.replace('$.state.get', '$.prompt.submit') for n in good_notes],
        '$.http.fetch': [n.replace('$.state.get', '$.http.fetch (via x)') for n in good_notes],
        'environment': [n.replace('env writes: nothing', 'env writes: PATH') for n in good_notes],
        'state it does not own': [n.replace('temper.view', 'other.view') for n in good_notes],
        'no "hooks:"': good_notes[1:],
    }
    for want, notes in bad.items():
        got = problems(report(notes), good_src)
        assert any(want in p for p in got), (want, got)
    got = problems(report(good_notes, [{'module': MODULE, 'hook': 'tool.call', 'hasCatch': False}]), good_src)
    assert any('has no .catch' in p for p in got), got
    got = problems(report(good_notes), "await $.process.run(['sh', '-c', 'rm -rf /'])")
    assert any('other than the temper CLI' in p for p in got), got
    print('lens allow-list self-test: ok')


def main(argv):
    if argv[1:] == ['--self-test']:
        self_test()
        return 0
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    report = json.loads(Path(argv[1]).read_text(encoding='utf-8'))
    found = problems(report, SOURCE.read_text(encoding='utf-8'))
    for p in found:
        print(f'lens allow-list: {p}', file=sys.stderr)
    if not found:
        print('lens allow-list: the lens hooks and calls only what #107 allows')
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
