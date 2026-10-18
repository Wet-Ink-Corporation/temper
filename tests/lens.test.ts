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
    expect(await bandText($, surface)).toContain('gate running (clippy)')
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
