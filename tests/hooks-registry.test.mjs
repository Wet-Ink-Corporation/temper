// hooks/hooks.json against the CLI: every registered hook runs a handler that
// scripts/temper.mjs knows, and the PreToolUse SubagentHandback hook, which cloud
// sessions use to deliver a subagent's report, is registered (#97, amends wi-103aa0).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const registry = JSON.parse(fs.readFileSync(path.join(root, 'hooks', 'hooks.json'), 'utf8'));

const registered = () => {
  const out = [];
  for (const [event, groups] of Object.entries(registry.hooks)) {
    for (const group of groups) {
      for (const h of group.hooks) out.push({ event, matcher: group.matcher, args: h.args ?? [] });
    }
  }
  return out;
};

test('every registered hook names a handler the CLI knows', () => {
  const hooks = registered();
  assert.ok(hooks.length > 0);
  for (const { event, matcher, args } of hooks) {
    const i = args.indexOf('hook');
    assert.ok(i >= 0 && args[i + 1], `${event} ${matcher ?? ''}: no hook name in ${JSON.stringify(args)}`);
    // TEMPER_DISABLE keeps the handler from running; an unknown name is still reported first.
    const res = spawnSync(process.execPath, [path.join(root, 'scripts', 'temper.mjs'), 'hook', args[i + 1]], {
      input: '{}', encoding: 'utf8', env: { ...process.env, TEMPER_DISABLE: '1' },
    });
    assert.equal(res.status, 0, `${args[i + 1]}: exit ${res.status}`);
    assert.doesNotMatch(res.stderr, /unknown hook event/, `${args[i + 1]} is not a known hook`);
  }
});

test('the PreToolUse SubagentHandback hook is registered: cloud subagents deliver their report through it (#97)', () => {
  assert.deepEqual(registered().filter((h) => h.event === 'PreToolUse' && h.matcher === 'SubagentHandback').map((h) => h.args.at(-1)), ['pre-handback']);
});

test('SubagentStart and SubagentStop run for every agent type, so telemetry sees types the handlers skip', () => {
  assert.deepEqual(registered().filter((h) => h.event.startsWith('Subagent')).map((h) => [h.event, h.matcher]), [['SubagentStart', undefined], ['SubagentStop', undefined]]);
});
