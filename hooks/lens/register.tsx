// temper lens: a read-only view of temper's state for the current tree (#107).
//
// The hooks in hooks.json enforce; the lens only shows what they decided. It reads one document,
// `temper status --json`, from the CLI of this same plugin, and never parses the state dir. It
// hooks only events it observes and returns every `next(e)` result unchanged, so a lens bug can
// never allow or block anything. Where no terminal or desktop draws (cloud sessions, `claude -p`,
// the VS Code chat panel), under TEMPER_DISABLE=1, and outside a Rust project it runs nothing.
import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

import type { LensStatus, LensView } from '../../types'

const view = atom({ plugin: 'temper', key: 'view' } as const, null)

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

async function refresh($: EngineInterface): Promise<void> {
  const surfaces = await $.session.surfaces()
  if (!surfaces.some(s => DRAWS.has(s))) return
  const next = await statusView($)
  if (next === 'no-project') return
  await update($, view, () => next)
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
    return result
  })

  on('turn.complete', async ($, e, next) => {
    const result = await next(e)
    if (!off) await refresh($)
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
