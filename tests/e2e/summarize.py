"""Summarize a stream-json Claude Code transcript: temper hook decisions, tool calls, final reply,
then which of the hooks registered in hooks/hooks.json fired (`hook_started` events).

A hook that returns nothing prints nothing above, so the coverage table is the only evidence that
it ran. It is diagnostic, not a test: some hooks fire only on a path the model chose (a reviewer
handback, a subagent), so a hook marked "not fired" means that path did not happen in this run,
or that Claude Code no longer emits that event or matcher target.
"""
import collections
import json
import pathlib
import re
import sys

HOOKS = pathlib.Path(__file__).resolve().parents[2] / 'hooks' / 'hooks.json'


def short(x, n=240):
    s = x if isinstance(x, str) else json.dumps(x)
    return s.replace('\n', ' ')[:n]


def registered():
    """(event, matcher or None) for every hook registration in hooks/hooks.json."""
    spec = json.loads(HOOKS.read_text())['hooks']
    return [(event, group.get('matcher')) for event, groups in spec.items() for group in groups]


fired = collections.Counter()
for line in open(sys.argv[1]):
    if not line.strip():
        continue
    r = json.loads(line)
    t = r.get('type')
    if t == 'system' and r.get('subtype') == 'hook_started':
        fired[r.get('hook_name') or r.get('hook_event') or '?'] += 1
    elif t == 'system' and r.get('subtype') == 'hook_response' and 'temper' in (r.get('output') or ''):
        print('HOOK ', r.get('hook_name'), short(r.get('output'), 300))
    elif t == 'assistant':
        who = 'SUB ' if r.get('parent_tool_use_id') else 'MAIN'
        for c in r['message'].get('content', []):
            if c.get('type') == 'tool_use':
                print(who, 'TOOL', c['name'], short(c['input'], 160))
    elif t == 'result':
        print('FINAL', short(r.get('result', ''), 3000))
        print('turns', r.get('num_turns'), 'cost $', r.get('total_cost_usd'))

# hook_name is "Event" or "Event:target", where target is the tool, source or agent type.
print('\nhook coverage (hook_started events):')
for event, matcher in registered():
    pat = re.compile(f'(?:{matcher})') if matcher else None
    hits = {name: n for name, n in fired.items()
            if name.split(':', 1)[0] == event
            and (pat is None or pat.search(name.split(':', 1)[1] if ':' in name else ''))}
    label = f'{event}[{matcher}]' if matcher else event
    detail = ', '.join(f'{k} x{v}' for k, v in sorted(hits.items())) or 'NOT FIRED'
    print(f'  {label:<60} {detail}')
