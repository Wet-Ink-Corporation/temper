"""Summarize a stream-json Claude Code transcript: temper hook decisions, tool calls, final reply."""
import json
import sys

def short(x, n=240):
    s = x if isinstance(x, str) else json.dumps(x)
    return s.replace('\n', ' ')[:n]

for line in open(sys.argv[1]):
    if not line.strip():
        continue
    r = json.loads(line)
    t = r.get('type')
    if t == 'system' and r.get('subtype') == 'hook_response' and 'temper' in (r.get('output') or ''):
        print('HOOK ', r.get('hook_name'), short(r.get('output'), 300))
    elif t == 'assistant':
        who = 'SUB ' if r.get('parent_tool_use_id') else 'MAIN'
        for c in r['message'].get('content', []):
            if c.get('type') == 'tool_use':
                print(who, 'TOOL', c['name'], short(c['input'], 160))
    elif t == 'result':
        print('FINAL', short(r.get('result', ''), 3000))
        print('turns', r.get('num_turns'), 'cost $', r.get('total_cost_usd'))
