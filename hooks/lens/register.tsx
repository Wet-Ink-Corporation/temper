// temper lens: a read-only view of temper's state for the current tree (#107).
//
// The hooks in hooks.json enforce; the lens only shows what they decided. It reads one document,
// `temper status --json`, from the CLI of this same plugin, and never parses the state dir. It
// hooks only events it observes and returns every `next(e)` result unchanged, so a lens bug can
// never allow or block anything. Where no terminal or desktop draws (cloud sessions, `claude -p`,
// the VS Code chat panel), under TEMPER_DISABLE=1, and outside a Rust project it runs nothing.
import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

import type { LensFinding, LensStats, LensStatus, LensView } from '../../types'

const view = atom({ plugin: 'temper', key: 'view' } as const, null)
const expanded = atom({ plugin: 'temper', key: 'expanded' } as const, null)
const tab = atom({ plugin: 'temper', key: 'tab' } as const, 'review')
const stats = atom({ plugin: 'temper', key: 'stats' } as const, null)
const branchStats = atom({ plugin: 'temper', key: 'branchStats' } as const, null)

type Tab = 'review' | 'activity' | 'stats'
const TABS: readonly Tab[] = ['review', 'activity', 'stats']
const TAB_LABEL: Record<Tab, string> = { review: 'Review', activity: 'Activity', stats: 'Stats' }

const PANE = 'temper-lens'
const RULES_MD = 'skills/rust-craft/references/rules.md'
const SEVERITIES = ['blocker', 'major', 'minor', 'nit'] as const
const DIMENSIONS = [
  'correctness', 'escape_hatches', 'ownership', 'errors', 'api', 'architecture',
  'tests', 'performance', 'concurrency', 'observability', 'craft', 'docs',
] as const

/** Tools whose completion can change the tree, or the gate and review state. */
const CHANGING_TOOLS = new Set(['Write', 'Edit', 'MultiEdit', 'Bash'])
const DEBOUNCE_MS = 1500
const STATUS_TIMEOUT_MS = 15000

/** A surface that draws the band. */
const DRAWS = new Set(['terminal', 'desktop'])

// Off for this load: TEMPER_DISABLE=1 or not a Rust project. Decided at session.start; a reload
// runs register and session.start again, so module variables are enough here.
let off = true
let pending: { cancel: () => void } | null = null
let commandRegistered = false
// The rule registry, parsed once per load from the plugin's own rules.md: ID to its Markdown.
let rules: Map<string, string> | null = null

async function refresh($: EngineInterface): Promise<void> {
  const surfaces = await $.session.surfaces()
  if (!surfaces.some(s => DRAWS.has(s))) return
  const next = await statusView($)
  if (next === 'no-project') return
  await update($, view, () => next)
  await registerCommand($)
}

/** `/temper-lens` opens the pane; registered once the lens knows it is in a Rust project. */
async function registerCommand($: EngineInterface): Promise<void> {
  if (commandRegistered) return
  commandRegistered = true
  await $.command.register({ name: 'temper-lens', description: "Show temper's review verdict for the current tree in a pane" })
}

/** The rule's text as Markdown, from the plugin's rules.md; a short note when it is not there. */
async function ruleText($: EngineInterface, id: string): Promise<string> {
  if (rules === null) {
    try {
      rules = parseRules(await $.fs.read(`${$.plugin.root}/${RULES_MD}`))
    } catch {
      return `**${id}**: the rule registry could not be read.`
    }
  }
  return rules.get(id) ?? `**${id}**: not in the rule registry.`
}

/** Toggles a finding's detail: the rule's text under it, and the Draft fix button. */
async function toggleFinding($: EngineInterface, key: string, rule: string): Promise<void> {
  const open = await read($, expanded)
  if (open?.key === key) {
    await update($, expanded, () => null)
    return
  }
  const text = await ruleText($, rule)
  await update($, expanded, () => ({ key, text }))
}

/** Fills the prompt box with a request to address the finding; never submits it. */
async function draftFix($: EngineInterface, f: LensFinding): Promise<void> {
  const at = f.line === null ? f.file : `${f.file}:${f.line}`
  await $.prompt.fill({ text: `Address the reviewer's ${f.severity} finding ${f.rule} in ${at}: ${f.fix}`, mode: 'insert' })
}

function refreshSoon($: EngineInterface): void {
  if (pending) return
  pending = $.clock.after(DEBOUNCE_MS, () => {
    pending = null
    return refresh($)
  })
}

export const register: Register = on => {
  off = true
  pending = null
  commandRegistered = false
  rules = null

  on('session.start', async ($, e, next) => {
    const result = await next(e)
    off = (await $.env.get('TEMPER_DISABLE')) === '1'
    if (off) return result
    const surfaces = await $.session.surfaces()
    // No surface draws yet: stay on, and draw once one attaches and a turn ends.
    if (!surfaces.some(s => DRAWS.has(s))) return result
    const first = await statusView($)
    if (first === 'no-project') {
      off = true
      return result
    }
    await update($, view, () => first)
    await registerCommand($)
    return result
  })

  on('command.run', { command: 'temper-lens' }, async $ => {
    await refresh($)
    // The pane opens on the tab it was left on, in this session or an earlier one.
    const last = await $.store.get('tab')
    const t: Tab = last === 'activity' || last === 'stats' ? last : 'review'
    await update($, tab, () => t)
    if (t !== 'review') await refreshStats($)
    await $.ui.open({ id: PANE, title: 'temper' })
    return { text: 'temper lens: the pane is open.' }
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const els = $.ui.resolve(e)
    const { Box, Button } = els
    const current = await read($, tab)
    const body = current === 'activity' ? await activityTab($, els) : current === 'stats' ? await statsTab($, els, e.surface) : await reviewTab($, els)
    return (
      <Box flexDirection="column">
        <Box>
          {TABS.map(t => (
            <Button key={`tab-${t}`} label={TAB_LABEL[t]} variant={t === current ? 'primary' : undefined} onPress={() => showTab($, t)} />
          ))}
        </Box>
        {body}
      </Box>
    )
  })

  on('turn.complete', async ($, e, next) => {
    const result = await next(e)
    if (off) return result
    await refresh($)
    // The tabs refresh only while the pane is open, and only the tab on show needs stats.
    if ((await read($, tab)) !== 'review' && (await $.ui.panes()).some(p => p.id === PANE)) await refreshStats($)
    return result
  })

  on('tool.call', async ($, e, next) => {
    const result = await next(e)
    if (!off && CHANGING_TOOLS.has(String(e.tool))) refreshSoon($)
    return result
  }).catch(($, e, next) => next(e)) // replays the call's own result: the lens never decides a tool call

  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    if (off || e.props.hasSurvey) return next(e)
    const v = await read($, view)
    if (v === null) return next(e)
    const { Box, Text } = $.ui.resolve(e)
    if (v.kind === 'unavailable') {
      return (
        <Box>
          <Text dimColor>temper: status unavailable</Text>
        </Box>
      )
    }
    const parts = bandParts(v.status, e.props.bodyColumns)
    return (
      <Box>
        <Text bold>temper </Text>
        {parts.map((p, i) => (
          <Text color={p.color} dimColor={p.color === undefined}>
            {i === 0 ? ` ${p.text}` : ` · ${p.text}`}
          </Text>
        ))}
      </Box>
    )
  })
}

type Els = ReturnType<EngineInterface['ui']['resolve']>

async function reviewTab($: EngineInterface, els: Els) {
  const { Box, Button, Markdown, Text } = els
  const v = await read($, view)
  const open = await read($, expanded)
  if (v === null || v.kind === 'unavailable') {
    return (
      <Box>
        <Text dimColor>temper: status unavailable</Text>
      </Box>
    )
  }
  const s = v.status
  const r = s.review
  if (r === null) {
    return (
      <Box flexDirection="column">
        <Text>{noReviewReason(s)}</Text>
      </Box>
    )
  }
  const findings = r.findings.map((f, i) => ({ f, key: `finding-${i}` }))
  return (
    <Box flexDirection="column">
      <Text>
        <Text color={r.stale ? 'warning' : r.verdict === 'approved' ? 'success' : 'error'}>{r.verdict}</Text>
        <Text dimColor> for {s.fingerprint}</Text>
        {r.stale ? <Text color="warning"> (stale: recorded for another tree; a new review is needed)</Text> : ''}
      </Text>
      <Text>
        {DIMENSIONS.map((d, i) => {
          const score = r.scores[d] ?? 'n/a'
          const low = typeof score === 'number' && score < 3
          return <Text color={low ? 'error' : undefined} dimColor={score === 'n/a'}>{`${i ? '  ' : ''}${d} ${score}`}</Text>
        })}
      </Text>
      {findings.length === 0 ? <Text dimColor>No findings.</Text> : ''}
      {SEVERITIES.flatMap(sev => {
        const group = findings.filter(x => x.f.severity === sev)
        if (group.length === 0) return []
        return [
          <Text bold color={sev === 'blocker' || sev === 'major' ? 'error' : undefined}>{`${sev} (${group.length})`}</Text>,
          ...group.flatMap(({ f, key }) => {
            const row = (
              <Button
                key={key}
                plain
                label={`${f.rule} ${f.line === null ? f.file : `${f.file}:${f.line}`}: ${f.fix}`}
                onPress={() => toggleFinding($, key, f.rule)}
              />
            )
            if (open?.key !== key) return [row]
            return [
              row,
              <Markdown key={`${key}-rule`} text={open.text} />,
              <Button key={`${key}-draft`} label="Draft fix" onPress={() => draftFix($, f)} />,
            ]
          }),
        ]
      })}
    </Box>
  )
}

/** Events the Activity tab lists: what temper denied, asked or blocked in this session. */
const ACTIVITY = /^(?:deny-|ask$|stop-blocked$|verdict-invalid)/

/** Where docs/troubleshooting.md explains each kind of event. */
const LEGEND: [RegExp, string][] = [
  [/^deny-(?:scan|skill-gate)$/, 'Edits (`Write`, `Edit`, `MultiEdit`)'],
  [/^deny-/, 'Shell commands (`Bash`)'],
  [/^ask$/, 'Edits, or Shell commands (`Bash`)'],
  [/^stop-blocked$/, 'Finishing a turn (the `Stop` hook)'],
  [/^verdict-invalid/, 'Review verdicts (the `rust-reviewer` subagent)'],
]

async function activityTab($: EngineInterface, els: Els) {
  const { Box, Button, Markdown, Text } = els
  const st = await read($, stats)
  const open = await read($, expanded)
  if (st === null) return <Text dimColor>Loading this session's activity.</Text>
  if (st.kind === 'unavailable') return <Text dimColor>temper: stats unavailable</Text>
  const rows = st.session.filter(r => ACTIVITY.test(r.event)).reverse()
  if (rows.length === 0) return <Text dimColor>temper has denied or asked nothing in this session.</Text>
  const seen = [...new Set(rows.map(r => r.event))]
  return (
    <Box flexDirection="column">
      {rows.flatMap((r, i) => {
        const key = `activity-${i}`
        const label = [r.t.slice(11, 19), r.event, r.rules.join(' '), r.file ?? '', r.agent === 'main' ? '' : r.agent].filter(Boolean).join('  ')
        const rule = r.rules[0]
        if (!rule) return [<Text key={key}>{label}</Text>]
        const row = <Button key={key} plain label={label} onPress={() => toggleFinding($, key, rule)} />
        return open?.key === key ? [row, <Markdown key={`${key}-rule`} text={open.text} />] : [row]
      })}
      <Text dimColor>In docs/troubleshooting.md:</Text>
      {seen.flatMap(ev => {
        const hit = LEGEND.find(([re]) => re.test(ev))
        return hit ? [<Text dimColor>{`  ${ev}: ${hit[1]}`}</Text>] : []
      })}
    </Box>
  )
}

async function statsTab($: EngineInterface, els: Els, surface: string) {
  const { Box, Button, Text } = els
  const st = await read($, stats)
  const br = await read($, branchStats)
  if (st === null) return <Text dimColor>Loading stats.</Text>
  if (st.kind === 'unavailable') return <Text dimColor>temper: stats unavailable</Text>
  return (
    <Box flexDirection="column">
      <Text bold>This clone</Text>
      {statsLines(st.totals).map(l => <Text>{l}</Text>)}
      {surface === 'terminal' && st.totals.gate.recentMs.length > 1 ? sparkline(els, st.totals.gate.recentMs) : ''}
      <Text bold>Telemetry branch</Text>
      {br === null
        ? <Button key="branch" label="Fetch every session pushed to the telemetry branch" onPress={() => fetchBranch($)} />
        : br.kind === 'unavailable'
          ? <Text dimColor>{br.why}</Text>
          : <Box flexDirection="column">{statsLines(br.totals).map(l => <Text>{l}</Text>)}</Box>}
    </Box>
  )
}

/** The Stats tab's lines for one set of counts. */
export function statsLines(t: LensStats): string[] {
  const secs = (ms: number | null) => (ms === null ? 'n/a' : `${(ms / 1000).toFixed(1)} s`)
  const g = t.gate
  const rv = t.review
  const top = Object.entries(t.rules).slice(0, 5).map(([id, n]) => `${id} ${n}`).join(', ')
  return [
    `sessions: ${t.sessions}`,
    g.runs ? `gate at Stop: p50 ${secs(g.p50Ms)}, p90 ${secs(g.p90Ms)}, max ${secs(g.maxMs)} over ${g.runs} run(s); ${g.cutShort} cut short` : `gate at Stop: no timed runs; ${g.cutShort} cut short`,
    rv.sessions ? `review: rounds per session p50 ${rv.roundsP50}, max ${rv.roundsMax}; approved on the first verdict in ${rv.firstApproved} of ${rv.sessions}` : 'review: no verdicts yet',
    `top rules blocked: ${top || 'none'}`,
  ]
}

/** Gate times as one row of block glyphs, oldest first, on the terminal's Raster. */
function sparkline(els: Els, ms: number[]) {
  const { Raster } = els as any
  const bars = '\u2581\u2582\u2583\u2584\u2585\u2586\u2587\u2588'
  const max = Math.max(...ms, 1)
  const words = new Uint32Array(ms.length * 3)
  ms.forEach((v, i) => {
    words[i * 3] = bars.charCodeAt(Math.min(7, Math.floor((v / max) * 7.999)))
    words[i * 3 + 1] = 0x01000000
    words[i * 3 + 2] = 0x01000000
  })
  return <Raster key="gate-times" columns={ms.length} rows={1} cells={(new Uint8Array(words.buffer) as any).toBase64()} />
}

async function showTab($: EngineInterface, t: Tab): Promise<void> {
  await update($, tab, () => t)
  await $.store.set('tab', t)
  if (t !== 'review') await refreshStats($)
}

/** Runs `temper stats --json` (and with `--branch`, the branch's), for the pane's tabs. */
async function runStats($: EngineInterface, args: string[]): Promise<{ ok: true; doc: any } | { ok: false; why: string }> {
  try {
    const run = await $.process.run(['node', `${$.plugin.root}/scripts/temper.mjs`, 'stats', '--json', ...args], { timeoutMs: STATUS_TIMEOUT_MS * 4 })
    if (run.exitCode !== 0) return { ok: false, why: run.stderr.trim().split('\n')[0] || 'temper: stats unavailable' }
    const doc = JSON.parse(run.stdout)
    if (!doc || doc.temper_stats !== 1 || doc.project === null) return { ok: false, why: 'temper: stats unavailable' }
    return { ok: true, doc }
  } catch {
    return { ok: false, why: 'temper: stats unavailable' }
  }
}

function totals(doc: any): LensStats {
  return { sessions: doc.sessions, rules: doc.rules ?? {}, gate: { ...doc.gate, recentMs: doc.gate?.recentMs ?? [] }, review: doc.review }
}

async function refreshStats($: EngineInterface): Promise<void> {
  const sid = await $.session.id()
  const r = await runStats($, ['--session', sid])
  await update($, stats, () => (r.ok
    ? { kind: 'stats', totals: totals(r.doc), session: r.doc.session?.rows ?? [] }
    : { kind: 'unavailable', why: r.why }))
}

/** The telemetry branch is fetched only when the person asks: it is a network call. */
async function fetchBranch($: EngineInterface): Promise<void> {
  const r = await runStats($, ['--branch'])
  await update($, branchStats, () => (r.ok ? { kind: 'stats', totals: totals(r.doc), session: [] } : { kind: 'unavailable', why: r.why }))
}

/** Why the tree has no review, and what clears it. */
function noReviewReason(s: LensStatus): string {
  if (s.changed.length === 0) return 'No Rust changes since the session base, so there is nothing to review.'
  if (s.gate?.ok === false) return 'No review for this tree yet: the gate is red. Fix it, then the temper:rust-reviewer subagent can review the change.'
  return 'No review for this tree yet. The temper:rust-reviewer subagent reviews the change once the gate is green.'
}

/** rules.md's registry rows, `| ID | Level | Rule | Enforced by | Detail |`, by ID, as Markdown. */
export function parseRules(text: string): Map<string, string> {
  const out = new Map<string, string>()
  for (const line of text.split('\n')) {
    const m = /^\| ([A-Z]{1,4}-\d{2}) \| (MUST|SHOULD|CONSIDER) \| (.*) \| (.*) \| (.*) \|\s*$/.exec(line)
    if (m) out.set(m[1], `**${m[1]}** (${m[2]}): ${m[3]}\n\nEnforced by: ${m[4]}. Detail: ${m[5]}.`)
  }
  return out
}

/** Runs `temper status --json` for this session: its view, or `no-project` outside a Rust project. */
async function statusView($: EngineInterface): Promise<LensView | 'no-project'> {
  try {
    const sid = await $.session.id()
    const run = await $.process.run(
      ['node', `${$.plugin.root}/scripts/temper.mjs`, 'status', '--json', '--session', sid],
      { timeoutMs: STATUS_TIMEOUT_MS },
    )
    if (run.exitCode !== 0) return { kind: 'unavailable' }
    const doc = JSON.parse(run.stdout)
    if (!doc || doc.temper_status !== 1) return { kind: 'unavailable' }
    if (doc.project === null) return 'no-project'
    return { kind: 'status', status: slim(doc) }
  } catch {
    return { kind: 'unavailable' }
  }
}

/** The parts of the status document the lens keeps in `$.state`. */
function slim(doc: Record<string, any>): LensStatus {
  const g = doc.gate
  const r = doc.review
  const b = doc.baseline
  return {
    fingerprint: String(doc.fingerprint ?? ''),
    changed: Array.isArray(doc.changed) ? doc.changed.map(String) : [],
    gate: g ? {
      ok: g.ok ?? null,
      unavailable: Boolean(g.unavailable),
      steps: Array.isArray(g.steps) ? g.steps.map((s: any) => ({ name: String(s.name), ok: s.ok ?? null, ms: s.ms ?? null })) : [],
      running: g.running ?? null,
    } : null,
    review: r ? {
      verdict: r.verdict,
      stale: Boolean(r.stale),
      scores: r.scores ?? {},
      findings: Array.isArray(r.findings) ? r.findings : [],
    } : null,
    baseline: b ? ('error' in b ? { error: String(b.error) } : { total: Number(b.total) }) : null,
  }
}

type Part = { text: string; color?: 'success' | 'error' | 'warning'; drop: number }

/**
 * The band's segments for `status`, fitted to `columns`: the baseline goes first when narrow,
 * then the review's detail. `drop` orders what goes (higher first; 0 never).
 */
export function bandParts(status: LensStatus, columns: number): { text: string; color?: Part['color'] }[] {
  const n = status.changed.length
  const parts: Part[] = [{ text: n ? `${n} .rs changed` : 'no .rs changes', drop: 0 }]
  parts.push(gatePart(status))
  const review = reviewPart(status)
  if (review) parts.push(review)
  const b = status.baseline
  if (b) parts.push('error' in b ? { text: 'baseline unreadable', color: 'warning', drop: 3 } : { text: `baseline ${b.total}`, drop: 3 })
  const width = (ps: Part[]) => 'temper '.length + ps.reduce((w, p, i) => w + p.text.length + (i === 0 ? 1 : 3), 0)
  let fitted = parts
  if (width(fitted) > columns) fitted = fitted.filter(p => p.drop < 3)
  if (width(fitted) > columns) fitted = fitted.map(p => (p.drop === 2 ? { ...p, text: p.text.replace(/,.*$/, '') } : p))
  return fitted.map(({ text, color }) => (color ? { text, color } : { text }))
}

function gatePart(status: LensStatus): Part {
  const g = status.gate
  if (g?.running) {
    const step = g.running.current?.name
    return { text: step ? `gate running (${step})` : 'gate running', color: 'warning', drop: 0 }
  }
  if (!g) return { text: 'gate not run', color: 'warning', drop: 0 }
  if (g.unavailable) return { text: 'gate unavailable', color: 'warning', drop: 0 }
  if (g.ok) return { text: 'gate green', color: 'success', drop: 0 }
  const failed = g.steps.find(s => s.ok === false)?.name
  return { text: failed ? `gate red (${failed})` : 'gate red', color: 'error', drop: 0 }
}

function reviewPart(status: LensStatus): Part | null {
  const r = status.review
  if (!r) return status.changed.length ? { text: 'review: none', color: 'warning', drop: 0 } : null
  if (r.stale) return { text: `review: ${r.verdict} (stale)`, color: 'warning', drop: 2 }
  if (r.verdict === 'approved') return { text: 'review: approved', color: 'success', drop: 0 }
  const counts = (['blocker', 'major'] as const)
    .map(sev => [sev, r.findings.filter(f => f.severity === sev).length] as const)
    .filter(([, c]) => c > 0)
    .map(([sev, c]) => `${c} ${sev}`)
  return { text: `review: changes-requested${counts.length ? `, ${counts.join(', ')}` : ''}`, color: 'error', drop: 2 }
}
