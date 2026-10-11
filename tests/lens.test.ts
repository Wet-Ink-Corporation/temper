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
  on('session.id', async () => ({ value: 'test-session' }) as any)
  // The engine draws nothing in the band: an empty Box stands for that here.
  on('ui.render', { component: 'AbovePrompt' }, async () => h('Box', {}))
}

const turnEnd = ($: any) => $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't1', reason: 'answer' })

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
    expect(runs[0].slice(0, 1)).toEqual(['node'])
    expect(runs[0][1]).toMatch(/\/scripts\/temper\.mjs$/)
    expect(runs[0].slice(2, 5)).toEqual(['status', '--json', '--session'])
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

  test(`band on ${surface}: narrow, it drops the baseline and then the review detail`, async ($, on) => {
    world(on, {
      surfaces: [surface],
      stdout: doc({ gate: RED, review: { verdict: 'changes-requested', stale: false, fingerprint: 'f', recorded_at: 't', scores: {}, findings: [finding('blocker')] }, baseline: { path: 'b', total: 41, toolchain: '1' } }),
    })
    await start($, surface)
    expect(await bandText($, surface, 90)).toBe('temper  3 .rs changed · gate red (clippy) · review: changes-requested, 1 blocker')
    expect(await bandText($, surface, 60)).toBe('temper  3 .rs changed · gate red (clippy) · review: changes-requested')
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

test('TEMPER_DISABLE=1: the lens runs nothing and draws nothing', async ($, on) => {
  const runs = world(on, { env: { TEMPER_DISABLE: '1' } })
  await start($)
  await turnEnd($)
  expect(runs).toHaveLength(0)
  expect(await bandText($, 'terminal')).toBe('')
})

test('outside a Rust project: one CLI run at start, then nothing', async ($, on) => {
  const runs = world(on, { stdout: JSON.stringify({ temper_status: 1, version: '1.3.0', project: null }) })
  await start($)
  await turnEnd($)
  expect(runs).toHaveLength(1)
  expect(await bandText($, 'terminal')).toBe('')
})

test('where no terminal or desktop draws, the lens runs nothing', async ($, on) => {
  const runs = world(on, { surfaces: [] })
  await start($, null)
  await turnEnd($)
  expect(runs).toHaveLength(0)
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
  await start($)
  expect(await bandText($, 'terminal')).toContain('gate not run')
  stdout = doc({ gate: GREEN })
  await turnEnd($)
  expect(runs).toHaveLength(2)
  expect(await bandText($, 'terminal')).toContain('gate green')
})

test('edits refresh the band once, after a 1.5 s quiet spell; a Read does not', async ($, on) => {
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

test('the lens passes a tool call\'s result on unchanged', async ($, on) => {
  mock.clock(on)
  world(on)
  on('tool.call', async () => ({ result: { text: 'the result', isError: true } }) as any)
  await start($)
  const result = await $.tool.call({ tool: 'Edit', tool_use_id: 'u1', file_path: '/w/src/a.rs', old_string: 'a', new_string: 'b' } as any)
  expect(result).toEqual({ result: { text: 'the result', isError: true } })
})

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
function paneWorld(on: On) {
  const seen = { commands: [] as string[], opened: [] as string[], reads: [] as string[], fills: [] as unknown[], submits: [] as unknown[] }
  on('command.register', async (_$, e: any) => { seen.commands.push(e.name); return { value: undefined } as any })
  on('ui.open', async (_$, e: any) => { seen.opened.push(e.id); return { value: { isPlaced: true } } as any })
  on('fs.read', async (_$, e: any) => { seen.reads.push(String(e.path)); return { value: RULES } as any })
  on('prompt.fill', async (_$, e: any) => { seen.fills.push(e); return { isFilled: true } as any })
  on('prompt.submit', async (_$, e: any) => { seen.submits.push(e); return { value: {} } as any })
  on('ui.render', { component: 'Pane' }, async () => h('Box', {}))
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
    expect((await ui.find({ key: 'finding-1' }))?.text).toContain('H-02 src/a.rs:7')
    expect(await ui.find({ key: 'finding-1-rule' })).toBeUndefined()

    await ui.press({ key: 'finding-1' })
    expect((await ui.find({ key: 'finding-1-rule' }))?.text).toContain('No `unwrap` in production code.')
    expect(seen.reads).toHaveLength(1)
    expect(seen.reads[0]).toMatch(/skills\/rust-craft\/references\/rules\.md$/)

    await ui.press({ key: 'finding-1-draft' })
    expect(seen.fills).toHaveLength(1)
    expect(seen.fills[0]).toMatchObject({ text: "Address the reviewer's blocker finding H-02 in src/a.rs:7: return the error instead of unwrapping", mode: 'insert' })
    expect(seen.submits).toEqual([])

    await ui.press({ key: 'finding-1' })
    expect(await ui.find({ key: 'finding-1-rule' })).toBeUndefined()
    await ui.press({ key: 'finding-0' })
    expect((await ui.find({ key: 'finding-0-rule' }))?.text).toContain('Scope error types')
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

test('the command is not registered outside a Rust project or under TEMPER_DISABLE=1', async ($, on) => {
  world(on, { stdout: JSON.stringify({ temper_status: 1, version: '1.3.0', project: null }) })
  const seen = paneWorld(on)
  await start($)
  expect(seen.commands).toEqual([])
})
