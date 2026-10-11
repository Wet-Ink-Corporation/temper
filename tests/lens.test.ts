// temper lens (#110): the band above the prompt, under `claude plugin test .`. The test's own
// hooks answer `$.process.run` (the temper CLI), `$.session.surfaces` and `$.env.get`, so no
// process runs: each case hands the lens a `status --json` document and reads the band it draws.
import { expect, mock, test } from 'claude-code/testing'
import type { On, RenderSurface } from 'claude-code'

const SURFACES = ['terminal', 'desktop'] as const

const BAND = {
  plugin: 'temper',
  component: 'AbovePrompt',
  props: { hasSurvey: false, isWorking: false, maxRows: 10, bodyColumns: 120, scroll: { offset: 0, bodyRows: 10 }, view: {} },
} as const

const doc = (over: Record<string, unknown> = {}) => JSON.stringify({
  temper_status: 1,
  version: '1.3.0',
  root: '/w',
  stateDir: '/w/.git/temper',
  base: 'abc',
  fingerprint: '0123456789abcdef',
  changed: ['src/a.rs', 'src/b.rs', 'src/c.rs'],
  gate: null,
  review: null,
  baseline: null,
  config: { review: 'required', skillGate: true },
  ...over,
})

const GREEN = { ok: true, unavailable: false, at: 't', steps: [{ name: 'fmt', ok: true, ms: 1, summary: '' }], running: null }
const RED = { ok: false, unavailable: false, at: 't', steps: [{ name: 'fmt', ok: true, ms: 1, summary: '' }, { name: 'clippy', ok: false, ms: 9, summary: 'x' }], running: null }
const finding = (severity: string) => ({ severity, rule: 'ERR-02', file: 'src/a.rs', line: 3, fix: 'type the error' })

/** The world beneath the lens: the CLI's answer, the surfaces, the environment; records each CLI run. */
function world(on: On, opts: { stdout?: string; exitCode?: number; surfaces?: RenderSurface[]; env?: Record<string, string> } = {}) {
  const runs: string[][] = []
  on('process.run', async (_$, e) => {
    runs.push([...e.argv])
    return { value: { exitCode: opts.exitCode ?? 0, stdout: opts.stdout ?? doc(), stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.surfaces', async () => ({ value: opts.surfaces ?? ['terminal'] }) as any)
  bottoms(on)
  mock.env(on, opts.env ?? {})
  return runs
}

/** The engine's own ends of the events the lens passes on: nothing beneath them in a test. */
function bottoms(on: On) {
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('turn.complete', async () => ({ text: '' }))
  on('session.attach', async (_$, e) => ({ clientId: e.clientId }) as any)
  on('session.id', async () => ({ value: 'test-session' }) as any)
  // The engine draws nothing in the band: an empty Box stands for that here.
  on('ui.render', { component: 'AbovePrompt' }, async () => h('Box', {}) as any)
}

/** A turn ends; the lens refreshes once its hook has returned, so the clock moves on by 0 ms. */
async function turnEnd($: any, clock: { advance: (ms: number) => Promise<void> }) {
  await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't1', reason: 'answer' })
  await clock.advance(0)
}

const start = ($: any, surface: RenderSurface | null = 'terminal') => $.session.start({ cwd: '/w', surface, isInteractive: true })

async function bandText($: any, surface: RenderSurface, bodyColumns = 120): Promise<string> {
  const ui = await $.ui.mount({ ...BAND, surface, props: { ...BAND.props, bodyColumns } })
  const texts = await ui.findAll({ type: 'Text' })
  await ui.unmount()
  return texts.map((t: { text: string }) => t.text).join('')
}

for (const surface of SURFACES) {
  test(`band on ${surface}: a red gate and a review asking for changes`, async ($, on) => {
    const runs = world(on, {
      surfaces: [surface],
      stdout: doc({ gate: RED, review: { verdict: 'changes-requested', stale: false, fingerprint: 'f', recorded_at: 't', scores: {}, findings: [finding('major'), finding('major'), finding('nit')] }, baseline: { path: 'b', total: 41, toolchain: '1' } }),
    })
    await start($, surface)
    expect(await bandText($, surface)).toBe('temper  3 .rs changed · gate red (clippy) · review: changes-requested, 2 major · baseline 41')
    expect(runs).toHaveLength(1)
    const [run = []] = runs
    expect(run.slice(0, 1)).toEqual(['node'])
    expect(run[1]).toMatch(/\/scripts\/temper\.mjs$/)
    expect(run.slice(2, 5)).toEqual(['status', '--json', '--session'])
  })

  test(`band on ${surface}: a green gate and an approved review`, async ($, on) => {
    world(on, { surfaces: [surface], stdout: doc({ gate: GREEN, review: { verdict: 'approved', stale: false, fingerprint: 'f', recorded_at: 't', scores: {}, findings: [] } }) })
    await start($, surface)
    expect(await bandText($, surface)).toBe('temper  3 .rs changed · gate green · review: approved')
  })

  test(`band on ${surface}: a stale review, and no review for a changed tree`, async ($, on) => {
    world(on, { surfaces: [surface], stdout: doc({ gate: GREEN, review: { verdict: 'approved', stale: true, fingerprint: 'f', recorded_at: 't', scores: {}, findings: [] } }) })
    await start($, surface)
    expect(await bandText($, surface)).toBe('temper  3 .rs changed · gate green · review: approved (stale)')
  })

  test(`band on ${surface}: no gate and no review yet`, async ($, on) => {
    world(on, { surfaces: [surface] })
    await start($, surface)
    expect(await bandText($, surface)).toBe('temper  3 .rs changed · gate not run · review: none')
  })

  test(`band on ${surface}: a gate running names its step`, async ($, on) => {
    world(on, { surfaces: [surface], stdout: doc({ gate: { ok: null, unavailable: false, at: null, steps: [], running: { startedAt: 't', steps: [], current: { name: 'clippy', startedAt: 't' } } } }) })
    await start($, surface)
    expect(await bandText($, surface)).toContain('gate running: 1 clippy')
  })

  test(`band on ${surface}: narrow, it drops the baseline, the review's counts, the gate's step, then whole segments`, async ($, on) => {
    world(on, {
      surfaces: [surface],
      stdout: doc({ gate: RED, review: { verdict: 'changes-requested', stale: false, fingerprint: 'f', recorded_at: 't', scores: {}, findings: [finding('blocker')] }, baseline: { path: 'b', total: 41, toolchain: '1' } }),
    })
    await start($, surface)
    expect(await bandText($, surface, 90)).toBe('temper  3 .rs changed · gate red (clippy) · review: changes-requested, 1 blocker')
    expect(await bandText($, surface, 70)).toBe('temper  3 .rs changed · gate red (clippy) · review: changes-requested')
    expect(await bandText($, surface, 60)).toBe('temper  3 .rs changed · gate red · review: changes-requested')
    expect(await bandText($, surface, 35)).toBe('temper  3 .rs changed · gate red')
  })

  test(`band on ${surface}: a failing CLI draws the fallback row`, async ($, on) => {
    world(on, { surfaces: [surface], exitCode: 1, stdout: '' })
    await start($, surface)
    expect(await bandText($, surface)).toBe('temper: status unavailable')
  })

  test(`band on ${surface}: output that does not parse draws the fallback row`, async ($, on) => {
    world(on, { surfaces: [surface], stdout: '{ not json' })
    await start($, surface)
    expect(await bandText($, surface)).toBe('temper: status unavailable')
  })
}

const OFF_ENVS: Record<string, string>[] = [{ TEMPER_DISABLE: '1' }, { TEMPER_LENS: '0' }]
const envName = (env: Record<string, string>) => Object.entries(env).map(kv => kv.join('=')).join(' ')

for (const env of OFF_ENVS) {
  test(`${envName(env)}: the lens runs nothing and draws nothing`, async ($, on) => {
    const clock = mock.clock(on)
    const runs = world(on, { env })
    await start($)
    await turnEnd($, clock)
    expect(runs).toHaveLength(0)
    expect(await bandText($, 'terminal')).toBe('')
  })
}

test('outside a Rust project: one CLI run at start, then nothing', async ($, on) => {
  const clock = mock.clock(on)
  const runs = world(on, { stdout: JSON.stringify({ temper_status: 1, version: '1.3.0', project: null }) })
  await start($)
  await turnEnd($, clock)
  expect(runs).toHaveLength(1)
  expect(await bandText($, 'terminal')).toBe('')
})

test('where no terminal or desktop draws, the lens runs nothing', async ($, on) => {
  const clock = mock.clock(on)
  const runs = world(on, { surfaces: [] })
  await start($, null)
  await turnEnd($, clock)
  expect(runs).toHaveLength(0)
})

test('a desktop client attaching after the start draws the band at once', async ($, on) => {
  const clock = mock.clock(on)
  let surfaces: RenderSurface[] = []
  const runs: string[][] = []
  on('process.run', async (_$, e) => {
    runs.push([...e.argv])
    return { value: { exitCode: 0, stdout: doc({ gate: GREEN }), stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.surfaces', async () => ({ value: surfaces }) as any)
  bottoms(on)
  mock.env(on, {})
  await start($, null)
  expect(runs).toHaveLength(0)
  surfaces = ['desktop']
  await $.session.attach({ surface: 'desktop', clientId: 'desktop:default' })
  await clock.advance(0)
  expect(runs).toHaveLength(1)
  expect(await bandText($, 'desktop')).toContain('gate green')
})

test('outside a Rust project, found only at a later refresh: the lens switches off', async ($, on) => {
  const clock = mock.clock(on)
  let surfaces: RenderSurface[] = []
  const runs: string[][] = []
  on('process.run', async (_$, e) => {
    runs.push([...e.argv])
    return { value: { exitCode: 0, stdout: JSON.stringify({ temper_status: 1, version: '1.3.0', project: null }), stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.surfaces', async () => ({ value: surfaces }) as any)
  bottoms(on)
  mock.env(on, {})
  await start($, null)
  surfaces = ['terminal']
  await turnEnd($, clock)
  expect(runs).toHaveLength(1)
  await turnEnd($, clock)
  expect(runs).toHaveLength(1)
})

test('a turn ending refreshes the band', async ($, on) => {
  let stdout = doc()
  const runs: string[][] = []
  on('process.run', async (_$, e) => {
    runs.push([...e.argv])
    return { value: { exitCode: 0, stdout, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.surfaces', async () => ({ value: ['terminal'] }) as any)
  bottoms(on)
  mock.env(on, {})
  const clock = mock.clock(on)
  await start($)
  expect(await bandText($, 'terminal')).toContain('gate not run')
  stdout = doc({ gate: GREEN })
  await turnEnd($, clock)
  expect(runs).toHaveLength(2)
  expect(await bandText($, 'terminal')).toContain('gate green')
})

test('edits refresh the band once, 1.5 s after the first edit; a Read does not', async ($, on) => {
  const clock = mock.clock(on)
  const runs = world(on)
  on('tool.call', async () => ({ result: { text: 'ok' } }) as any)
  await start($)
  expect(runs).toHaveLength(1)
  await $.tool.call({ tool: 'Edit', tool_use_id: 'u1', file_path: '/w/src/a.rs', old_string: 'a', new_string: 'b' } as any)
  await $.tool.call({ tool: 'Bash', tool_use_id: 'u2', command: 'cargo fmt' } as any)
  await $.tool.call({ tool: 'Read', tool_use_id: 'u3', file_path: '/w/src/a.rs' } as any)
  expect(runs).toHaveLength(1)
  await clock.advance(1500)
  expect(runs).toHaveLength(2)
  await $.tool.call({ tool: 'Read', tool_use_id: 'u4', file_path: '/w/src/a.rs' } as any)
  await clock.advance(5000)
  expect(runs).toHaveLength(2)
})

// What the layer beneath answers, the lens returns as is: a result, a rewritten result, a denial.
const BENEATH = {
  'a result': { result: { text: 'the result', isError: true } },
  'a rewritten result': { result: { text: 'rewritten by a hook beneath' }, context: ['temper: note'] },
  'a denial': { deny: 'temper: blocked by the scan' },
}
const CALLS = {
  Edit: { tool: 'Edit', tool_use_id: 'u1', file_path: '/w/src/a.rs', old_string: 'a', new_string: 'b' },
  Bash: { tool: 'Bash', tool_use_id: 'u2', command: 'cargo build' },
}
for (const [what, answer] of Object.entries(BENEATH)) {
  for (const [tool, call] of Object.entries(CALLS)) {
    test(`the lens passes ${tool} down as it came, and ${what} back unchanged`, async ($, on) => {
      mock.clock(on)
      world(on)
      const seen: unknown[] = []
      on('tool.call', async (_$, e) => {
        seen.push(e)
        return answer as any
      })
      await start($)
      expect(await $.tool.call(call as any)).toEqual(answer)
      expect(seen).toHaveLength(1)
      expect(seen[0]).toEqual(call)
    })
  }
}

// The /temper-lens pane (#112): the reviewer's verdict, findings by severity, the rule's text on a
// press, and a Draft fix button that fills the prompt box and never submits.
const PANE = {
  plugin: 'temper',
  component: 'Pane',
  requestId: 'temper-lens',
  props: { title: 'temper', isFocused: true, bodyColumns: 100, placement: 'dock', scroll: { offset: 0, bodyRows: 30 }, view: {} },
} as const

const RULES = [
  '| ID | Level | Rule | Enforced by | Detail |',
  '|---|---|---|---|---|',
  '| ERR-02 | SHOULD | Scope error types to an operation or module. | review | errors.md |',
  '| H-02 | MUST | No `unwrap` in production code. | lint `clippy::unwrap_used`, scan | escape-hatches.md |',
].join('\n')

const SCORES = {
  correctness: 4, escape_hatches: 2, ownership: 3, errors: 2, api: 3, architecture: 'n/a',
  tests: 3, performance: 'n/a', concurrency: 'n/a', observability: 'n/a', craft: 3, docs: 3,
}
const VERDICT = {
  verdict: 'changes-requested', stale: false, fingerprint: '0123456789abcdef', recorded_at: 't', scores: SCORES,
  findings: [
    { severity: 'nit', rule: 'ERR-02', file: 'src/b.rs', line: null, fix: 'name the error after the operation' },
    { severity: 'blocker', rule: 'H-02', file: 'src/a.rs', line: 7, fix: 'return the error instead of unwrapping' },
  ],
}

/** The engine's ends the pane needs: command registration, the pane, the rules file, the prompt box. */
function paneWorld(on: On, store: Record<string, unknown> = {}) {
  mock.store(on, store)
  const seen = { commands: [] as string[], opened: [] as string[], reads: [] as string[], fills: [] as unknown[], submits: [] as unknown[] }
  on('command.register', async (_$, e: any) => { seen.commands.push(e.name); return { value: undefined } as any })
  on('ui.open', async (_$, e: any) => { seen.opened.push(e.id); return { value: { isPlaced: true } } as any })
  on('fs.read', async (_$, e: any) => { seen.reads.push(String(e.path)); return { value: RULES } as any })
  on('prompt.fill', async (_$, e: any) => { seen.fills.push(e); return { isFilled: true } as any })
  on('prompt.submit', async (_$, e: any) => { seen.submits.push(e); return { value: {} } as any })
  on('ui.render', { component: 'Pane' }, async () => h('Box', {}) as any)
  return seen
}

for (const surface of SURFACES) {
  test(`pane on ${surface}: /temper-lens opens it with the verdict, scores and findings by severity`, async ($, on) => {
    world(on, { surfaces: [surface], stdout: doc({ gate: GREEN, review: VERDICT }) })
    const seen = paneWorld(on)
    await start($, surface)
    expect(seen.commands).toEqual(['temper-lens'])
    expect(seen.opened).toEqual([])
    await $.command.run({ command: 'temper-lens', args: '' } as any)
    expect(seen.opened).toEqual(['temper-lens'])
    const ui = await $.ui.mount({ ...PANE, surface })
    const texts = (await ui.findAll({ type: 'Text' })).map((t: { text: string }) => t.text)
    expect(texts).toContain('changes-requested')
    expect(texts.join('|')).toContain('escape_hatches 2')
    expect(texts).toContain('blocker (1)')
    expect(texts).toContain('nit (1)')
    expect(texts.indexOf('blocker (1)')).toBeLessThan(texts.indexOf('nit (1)'))
    expect((await ui.find({ key: 'finding:H-02:src/a.rs:7' }))?.text).toContain('H-02 src/a.rs:7')
    expect(await ui.find({ key: 'finding:H-02:src/a.rs:7-rule' })).toBeUndefined()

    await ui.press({ key: 'finding:H-02:src/a.rs:7' })
    expect((await ui.find({ key: 'finding:H-02:src/a.rs:7-rule' }))?.text).toContain('No `unwrap` in production code.')
    expect(seen.reads).toHaveLength(1)
    expect(seen.reads[0]).toMatch(/skills\/rust-craft\/references\/rules\.md$/)

    await ui.press({ key: 'finding:H-02:src/a.rs:7-draft' })
    expect(seen.fills).toHaveLength(1)
    expect(seen.fills[0]).toMatchObject({ text: "Address the reviewer's blocker finding H-02 in src/a.rs:7: return the error instead of unwrapping", mode: 'insert' })
    expect(seen.submits).toEqual([])

    await ui.press({ key: 'finding:H-02:src/a.rs:7' })
    expect(await ui.find({ key: 'finding:H-02:src/a.rs:7-rule' })).toBeUndefined()
    await ui.press({ key: 'finding:ERR-02:src/b.rs:-' })
    expect((await ui.find({ key: 'finding:ERR-02:src/b.rs:--rule' }))?.text).toContain('Scope error types')
    expect(seen.reads).toHaveLength(1)
    await ui.unmount()
  })

  test(`pane on ${surface}: without a verdict it says why`, async ($, on) => {
    world(on, { surfaces: [surface], stdout: doc({ gate: RED }) })
    paneWorld(on)
    await start($, surface)
    const ui = await $.ui.mount({ ...PANE, surface })
    expect(await ui.find({ type: 'Text', text: /the gate is red/ })).toBeDefined()
    await ui.unmount()
  })

  test(`pane on ${surface}: a stale verdict says a new review is needed`, async ($, on) => {
    world(on, { surfaces: [surface], stdout: doc({ gate: GREEN, review: { ...VERDICT, stale: true } }) })
    paneWorld(on)
    await start($, surface)
    const ui = await $.ui.mount({ ...PANE, surface })
    expect(await ui.find({ type: 'Text', text: /a new review is needed/ })).toBeDefined()
    await ui.unmount()
  })
}

test('the command is not registered outside a Rust project', async ($, on) => {
  world(on, { stdout: JSON.stringify({ temper_status: 1, version: '1.3.0', project: null }) })
  const seen = paneWorld(on)
  await start($)
  expect(seen.commands).toEqual([])
})

// The Activity and Stats tabs (#113), from `temper stats --json`. The branch fetch runs only on a press.
const STATS = (over: Record<string, unknown> = {}) => JSON.stringify({
  temper_stats: 1, sessions: 2, from: 't0', to: 't1',
  events: { 'deny-scan': 2, ask: 1 }, rules: { 'H-02': 2, 'ERR-02': 1 },
  gate: { runs: 3, cached: 1, cutShort: 1, p50Ms: 3000, p90Ms: 9000, maxMs: 9000, recentMs: [3000, 9000, 1000] },
  review: { sessions: 1, roundsP50: 2, roundsMax: 2, firstApproved: 0 },
  session: { id: 'test-session', rows: [
    { t: '2026-10-11T10:00:01.000Z', event: 'session-start', agent: 'main', file: null, rules: [], via: null },
    { t: '2026-10-11T10:00:02.000Z', event: 'deny-scan', agent: 'main', file: 'src/a.rs', rules: ['H-02'], via: null },
    { t: '2026-10-11T10:00:03.000Z', event: 'ask', agent: 'temper:rust-implementer', file: 'Cargo.toml', rules: [], via: null },
  ] },
  ...over,
})

/** Like `world`, with `stats --json` answered from `stats` and every CLI run recorded. */
function statsWorld(on: On, surface: RenderSurface, branch = STATS({ sessions: 9, session: undefined })) {
  const runs: string[][] = []
  on('process.run', async (_$, e) => {
    const argv = [...e.argv]
    runs.push(argv)
    const out = argv[2] === 'stats' ? (argv.includes('--branch') ? branch : STATS()) : doc({ gate: GREEN })
    return { value: { exitCode: 0, stdout: out, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.surfaces', async () => ({ value: [surface] }) as any)
  bottoms(on)
  mock.env(on, {})
  return runs
}

for (const surface of SURFACES) {
  test(`tabs on ${surface}: Activity lists this session's denials and asks, newest first, with the rule on a press`, async ($, on) => {
    const runs = statsWorld(on, surface)
    paneWorld(on)
    await start($, surface)
    await $.command.run({ command: 'temper-lens', args: '' } as any)
    const ui = await $.ui.mount({ ...PANE, surface })
    expect(runs.some(r => r[2] === 'stats')).toBe(false)
    await ui.press({ key: 'tab-activity' })
    const stat = runs.find(r => r[2] === 'stats')
    expect(stat?.slice(2)).toEqual(['stats', '--json', '--session', 'test-session'])
    const askRow = await ui.find({ type: 'Text', text: '10:00:03Z  ask  Cargo.toml  temper:rust-implementer' })
    expect(askRow).toBeDefined()
    expect((await ui.find({ key: 'activity:2026-10-11T10:00:02.000Z:deny-scan' }))?.text).toBe('10:00:02Z  deny-scan  H-02  src/a.rs')
    expect(await ui.find({ type: 'Text', text: /session-start/ })).toBeUndefined()
    expect(await ui.find({ type: 'Text', text: /deny-scan: Edits/ })).toBeDefined()
    await ui.press({ key: 'activity:2026-10-11T10:00:02.000Z:deny-scan' })
    expect((await ui.find({ key: 'activity:2026-10-11T10:00:02.000Z:deny-scan-rule' }))?.text).toContain('No `unwrap`')
    await ui.unmount()
  })

  test(`tabs on ${surface}: Stats shows the counts; the branch is fetched only on a press`, async ($, on) => {
    const runs = statsWorld(on, surface)
    paneWorld(on)
    await start($, surface)
    const ui = await $.ui.mount({ ...PANE, surface })
    await ui.press({ key: 'tab-stats' })
    const texts = (await ui.findAll({ type: 'Text' })).map((t: { text: string }) => t.text).join('\n')
    expect(texts).toContain('gate at Stop: p50 3.0 s, p90 9.0 s, max 9.0 s over 3 run(s); 1 cut short')
    expect(texts).toContain('approved on the first verdict in 0 of 1')
    expect(texts).toContain('top rules blocked: H-02 2, ERR-02 1')
    expect(runs.some(r => r.includes('--branch'))).toBe(false)
    if (surface === 'terminal') expect(await ui.find({ key: 'gate-times' })).toBeDefined()
    await ui.press({ key: 'branch' })
    expect(runs.filter(r => r.includes('--branch'))).toHaveLength(1)
    expect(await ui.find({ type: 'Text', text: 'sessions: 9' })).toBeDefined()
    await ui.unmount()
  })
}

test('a turn end refreshes the tab on show only while the pane is open', async ($, on) => {
  const clock = mock.clock(on)
  const runs = statsWorld(on, 'terminal')
  paneWorld(on, { tab: 'stats' })
  let open = false
  on('ui.panes', async () => ({ value: open ? [{ id: 'temper-lens', title: 'temper', isPlaced: true }] : [] }) as any)
  await start($)
  await $.command.run({ command: 'temper-lens', args: '' } as any)
  const stats = () => runs.filter(r => r[2] === 'stats').length
  const before = stats()
  await turnEnd($, clock)
  expect(stats()).toBe(before)
  open = true
  await turnEnd($, clock)
  expect(stats()).toBe(before + 1)
})

test('the pane opens on the tab it was left on', async ($, on) => {
  const runs = statsWorld(on, 'terminal')
  paneWorld(on, { tab: 'stats' })
  await start($)
  await $.command.run({ command: 'temper-lens', args: '' } as any)
  expect(runs.some(r => r[2] === 'stats')).toBe(true)
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect(await ui.find({ type: 'Text', text: /gate at Stop/ })).toBeDefined()
  await ui.unmount()
})

// Live gate progress at Stop (#111): the lens polls while the settings hooks' Stop runs, shows the
// step in the spinner and the band, toasts once after, and passes the Stop's result on unchanged.
const RUNNING = (current: string, done: { name: string; ms: number }[]) => doc({
  gate: { ok: null, unavailable: false, at: null, steps: [], running: {
    startedAt: new Date(0).toISOString(),
    steps: done.map(d => ({ ...d, ok: true })),
    current: { name: current, startedAt: new Date(0).toISOString() },
  } },
})

const SPINNER = { plugin: 'temper', component: 'Spinner', props: { word: 'Working', message: null, suffix: '…', mode: 'tool-use' } } as const

/** Telemetry rows for `stats --json --session`: temper's own record of a block, at `ms`. */
const temperBlock = (event: string, ms: number) => ({ t: new Date(ms).toISOString(), event, agent: 'main', file: null, rules: [], via: null })

/**
 * A Stop world: `status --json` answers `answer()` at each run, `stats --json` lists `rows()` as
 * this session's telemetry; toasts and surfaces are recorded.
 */
function stopWorld(on: On, answer: () => string, surface: RenderSurface = 'terminal', rows: () => unknown[] = () => []) {
  const toasts: string[] = []
  let surfacesFail = false
  on('process.run', async (_$, e) => {
    const stdout = e.argv[2] === 'stats' ? STATS({ session: { id: 'test-session', rows: rows() } }) : answer()
    return { value: { exitCode: 0, stdout, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.surfaces', async () => {
    if (surfacesFail) throw new Error('surfaces broke')
    return { value: [surface] } as any
  })
  on('ui.toast', async (_$, e: any) => { toasts.push(typeof e === 'string' ? e : e.text); return { value: undefined } as any })
  on('ui.render', { component: 'Spinner' }, async (_$, e: any) => h('Text', {}, `${e.props.word}${e.props.suffix}`) as any)
  on('session.end', async () => ({}) as any)
  on('command.register', async () => ({ value: undefined }) as any)
  bottoms(on)
  mock.env(on, {})
  return { toasts, breakSurfaces: () => { surfacesFail = true } }
}

const spinnerText = async ($: any, surface: RenderSurface) => {
  const ui = await $.ui.mount({ ...SPINNER, surface })
  const t = (await ui.find({ type: 'Text' }))?.text
  await ui.unmount()
  return t
}

for (const surface of SURFACES) {
  test(`Stop on ${surface}: the spinner names the gate's step while it runs, then one toast`, async ($, on) => {
    const clock = mock.clock(on, { now: 72000 })
    let answer = doc()
    const w = stopWorld(on, () => answer, surface)
    let during = ''
    let band = ''
    on('classic.Stop', async () => {
      answer = RUNNING('clippy', [{ name: 'fmt', ms: 1000 }, { name: 'scan', ms: 200 }])
      await clock.advance(1000)
      during = (await spinnerText($, surface)) ?? ''
      band = await bandText($, surface)
      answer = doc({ gate: GREEN, review: { verdict: 'approved', stale: false, fingerprint: 'f', recorded_at: 't', scores: {}, findings: [] } })
      return {}
    })
    await start($, surface)
    expect(await spinnerText($, surface)).toBe('Working…')
    const result = await $.classic.Stop({ stop_hook_active: false } as any)
    expect(result).toEqual({})
    await clock.advance(0)
    expect(during).toBe('Working · temper: gate 3 clippy · 1m 13s')
    expect(band).toContain('gate running: 3 clippy · 1m 13s')
    expect(band).toContain('done: fmt 1.0s, scan 0.2s')
    expect(await spinnerText($, surface)).toBe('Working…')
    expect(w.toasts).toEqual(['temper: gate green, review approved'])
    await clock.advance(5000)
    expect(w.toasts).toHaveLength(1)
  })
}

const APPROVED = { verdict: 'approved', stale: false, fingerprint: 'f', recorded_at: 't', scores: {}, findings: [] }

// What the engine adds to a Stop's input on its way down; the lens adds nothing.
const ENGINE_KEYS = ['session_id', 'transcript_path', 'cwd', 'hook_event_name']

test('Stop: a block passes down and back unchanged, and its first line is the toast after', async ($, on) => {
  const clock = mock.clock(on, { now: 72000 })
  const w = stopWorld(on, () => doc({ gate: RED }), 'terminal', () => [temperBlock('stop-blocked', 72000)])
  const block = 'temper: the Rust gate is red (clippy).\nFix it before finishing.'
  const input = { stop_hook_active: false }
  const seen: unknown[] = []
  on('classic.Stop', async (_$, e) => {
    seen.push(e)
    return { block }
  })
  await start($)
  expect(await $.classic.Stop(input as any)).toEqual({ block })
  expect(seen).toHaveLength(1)
  expect(seen[0]).toMatchObject(input)
  expect(Object.keys(seen[0] as object).sort()).toEqual([...Object.keys(input), ...ENGINE_KEYS].sort())
  expect(w.toasts).toEqual([])
  await clock.advance(0)
  expect(w.toasts).toEqual(['temper: the Rust gate is red (clippy).'])
})

test('SubagentStop is watched the same way', async ($, on) => {
  const clock = mock.clock(on, { now: 72000 })
  const w = stopWorld(on, () => doc(), 'terminal', () => [temperBlock('verdict-invalid', 72500)])
  const input = { stop_hook_active: false, agent_id: 'a', agent_type: 'temper:rust-implementer' }
  const seen: unknown[] = []
  on('classic.SubagentStop', async (_$, e) => {
    seen.push(e)
    return { block: 'review the change first' }
  })
  await start($)
  expect(await $.classic.SubagentStop(input as any)).toEqual({ block: 'review the change first' })
  expect(seen).toHaveLength(1)
  expect(seen[0]).toMatchObject(input)
  expect(Object.keys(seen[0] as object).sort()).toEqual([...Object.keys(input), ...ENGINE_KEYS].sort())
  await clock.advance(0)
  expect(w.toasts).toEqual(['temper: review the change first'])
})

for (const [what, tree] of [['an approved tree', doc({ gate: GREEN, review: APPROVED })], ['an unreviewed tree', doc({ gate: GREEN })]] as const) {
  test(`Stop and SubagentStop: another hook's block on ${what} is not toasted as temper's`, async ($, on) => {
    const clock = mock.clock(on, { now: 72000 })
    // temper blocked an earlier Stop, before this one began: that row is not this block.
    const w = stopWorld(on, () => tree, 'terminal', () => [temperBlock('stop-blocked', 60000)])
    on('classic.Stop', async () => ({ block: 'lint-bot: run the formatter first' }))
    on('classic.SubagentStop', async () => ({ block: 'lint-bot: run the formatter first' }))
    await start($)
    await $.classic.Stop({ stop_hook_active: false } as any)
    await $.classic.SubagentStop({ stop_hook_active: false, agent_id: 'a', agent_type: 'temper:rust-implementer' } as any)
    await clock.advance(0)
    expect(w.toasts).toEqual([])
  })
}

test("Stop: a block after the gate ran passes back unchanged, and the toast is the block's", async ($, on) => {
  const clock = mock.clock(on, { now: 72000 })
  let answer = doc()
  const w = stopWorld(on, () => answer, 'terminal', () => [temperBlock('stop-blocked', 74000)])
  const block = 'temper gate: fmt ok | clippy FAILED\nThe work is not done while the gate is red.'
  on('classic.Stop', async () => {
    answer = RUNNING('clippy', [{ name: 'fmt', ms: 1000 }])
    await clock.advance(2000)
    answer = doc({ gate: RED })
    return { block }
  })
  await start($)
  expect(await $.classic.Stop({ stop_hook_active: false } as any)).toEqual({ block })
  await clock.advance(0)
  expect(w.toasts).toEqual(['temper gate: fmt ok | clippy FAILED'])
})

test('Stop: a quiet Stop with no gate raises no toast', async ($, on) => {
  const clock = mock.clock(on)
  const w = stopWorld(on, () => doc({ gate: GREEN }))
  on('classic.Stop', async () => ({}))
  await start($)
  await $.classic.Stop({ stop_hook_active: false } as any)
  await clock.advance(0)
  expect(w.toasts).toEqual([])
})

test('SubagentStops that overlap share one poll, and each one keeps polling until it ends', async ($, on) => {
  const clock = mock.clock(on)
  let answer = RUNNING('clippy', [])
  let polls = 0
  const w = stopWorld(on, () => {
    polls += 1
    return answer
  })
  let releaseB = () => {}
  const bHeld = new Promise<void>(r => { releaseB = r })
  on('classic.SubagentStop', async (_$, e: any) => {
    if (e.agent_id === 'b') await bHeld
    return {}
  })
  await start($)
  const b = $.classic.SubagentStop({ stop_hook_active: false, agent_id: 'b', agent_type: 'x' } as any)
  await clock.advance(1000)
  await $.classic.SubagentStop({ stop_hook_active: false, agent_id: 'a', agent_type: 'x' } as any)
  const before = polls
  await clock.advance(3000)
  expect(polls - before).toBeGreaterThanOrEqual(3)
  answer = doc({ gate: GREEN, review: APPROVED })
  releaseB()
  await b
  await clock.advance(0)
  const after = polls
  await clock.advance(3000)
  expect(polls).toBe(after)
  expect(w.toasts).toContain('temper: gate green, review approved')
})

test('SubagentStops that start together share one poll, and it stops when both end', async ($, on) => {
  const clock = mock.clock(on)
  let polls = 0
  stopWorld(on, () => {
    polls += 1
    return RUNNING('clippy', [])
  })
  let release = () => {}
  const held = new Promise<void>(r => { release = r })
  on('classic.SubagentStop', async () => {
    await held
    return {}
  })
  await start($)
  const a = $.classic.SubagentStop({ stop_hook_active: false, agent_id: 'a', agent_type: 'x' } as any)
  const b = $.classic.SubagentStop({ stop_hook_active: false, agent_id: 'b', agent_type: 'x' } as any)
  await clock.advance(3000)
  const during = polls
  expect(during).toBeGreaterThanOrEqual(3)
  expect(during).toBeLessThanOrEqual(5)
  release()
  await Promise.all([a, b])
  await clock.advance(0)
  const after = polls
  await clock.advance(5000)
  expect(polls).toBe(after)
})

test('a terminal attaching during a Stop gets its progress from then on', async ($, on) => {
  const clock = mock.clock(on)
  let surfaces: RenderSurface[] = []
  let polls = 0
  const runs: string[][] = []
  on('process.run', async (_$, e) => {
    runs.push([...e.argv])
    polls += 1
    return { value: { exitCode: 0, stdout: RUNNING('clippy', []), stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.surfaces', async () => ({ value: surfaces }) as any)
  on('ui.toast', async () => ({ value: undefined }) as any)
  bottoms(on)
  mock.env(on, {})
  on('classic.Stop', async () => {
    await clock.advance(1000)
    surfaces = ['terminal']
    await $.session.attach({ surface: 'terminal', clientId: 'terminal:default' })
    await clock.advance(4000)
    return {}
  })
  await start($, null)
  await $.classic.Stop({ stop_hook_active: false } as any)
  expect(polls).toBeGreaterThanOrEqual(4)
})

test('Stop where nothing draws: the lens runs nothing, and the block passes back unchanged', async ($, on) => {
  const clock = mock.clock(on, { now: 72000 })
  const runs = world(on, { surfaces: [] })
  on('ui.toast', async () => ({ value: undefined }) as any)
  on('classic.Stop', async () => {
    await clock.advance(3000)
    return { block: 'temper: the gate is red' }
  })
  await start($, null)
  expect(await $.classic.Stop({ stop_hook_active: false } as any)).toEqual({ block: 'temper: the gate is red' })
  await clock.advance(3000)
  expect(runs).toEqual([])
})

test('Stop: a failing surfaces call during the Stop leaves the result as the settings hooks decided', async ($, on) => {
  mock.clock(on)
  const w = stopWorld(on, () => doc())
  on('classic.Stop', async () => {
    w.breakSurfaces()
    return { block: 'the settings hook said no' }
  })
  await start($)
  expect(await $.classic.Stop({ stop_hook_active: false } as any)).toEqual({ block: 'the settings hook said no' })
})

test('Stop under TEMPER_DISABLE=1: nothing polls', async ($, on) => {
  const clock = mock.clock(on)
  const runs = world(on, { env: { TEMPER_DISABLE: '1' } })
  on('classic.Stop', async () => {
    await clock.advance(3000)
    return {}
  })
  await start($)
  await $.classic.Stop({ stop_hook_active: false } as any)
  expect(runs).toHaveLength(0)
})

for (const env of OFF_ENVS) {
  test(`the command is not registered under ${envName(env)}`, async ($, on) => {
    world(on, { stdout: doc({ review: VERDICT }), env })
    const seen = paneWorld(on)
    await start($)
    expect(seen.commands).toEqual([])
  })
}

test('an open finding stays with its finding when a refresh adds another before it', async ($, on) => {
  const clock = mock.clock(on)
  let stdout = doc({ gate: GREEN, review: VERDICT })
  on('process.run', async () => ({ value: { exitCode: 0, stdout, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }))
  on('session.surfaces', async () => ({ value: ['terminal'] }) as any)
  bottoms(on)
  mock.env(on, {})
  paneWorld(on)
  await start($)
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  await ui.press({ key: 'finding:H-02:src/a.rs:7' })
  const added = { severity: 'blocker', rule: 'ERR-02', file: 'src/c.rs', line: 1, fix: 'scope it' }
  stdout = doc({ gate: GREEN, review: { ...VERDICT, findings: [added, ...VERDICT.findings] } })
  await turnEnd($, clock)
  expect((await ui.find({ key: 'finding:H-02:src/a.rs:7-rule' }))?.text).toContain('No `unwrap` in production code.')
  expect(await ui.find({ key: 'finding:ERR-02:src/c.rs:1-rule' })).toBeUndefined()
  await ui.unmount()
})
