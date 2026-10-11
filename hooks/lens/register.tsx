// temper lens: a read-only view of temper's state for the current tree (#107).
//
// The hooks in hooks.json enforce; the lens only shows what they decided. It reads one document,
// `temper status --json`, from the CLI of this same plugin, and never parses the state dir. It
// hooks only events it observes and returns every `next(e)` result unchanged, so a lens bug can
// never allow or block anything. Where no terminal or desktop draws (cloud sessions, `claude -p`,
// the VS Code chat panel), under TEMPER_DISABLE=1 or TEMPER_LENS=0, and outside a Rust project it
// runs nothing.
import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

import type { LensExpanded, LensFinding, LensStatus, LensView } from '../../types'

const view = atom({ plugin: 'temper', key: 'view' } as const, null)
const expanded = atom({ plugin: 'temper', key: 'expanded' } as const, null)

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

// Off for this load: TEMPER_DISABLE=1, TEMPER_LENS=0 or not a Rust project. Decided at session.start
// (or at the first refresh, when no surface drew at start); a reload runs register and session.start
// again, so module variables are enough here.
let off = true
let pending: { cancel: () => void } | null = null
let commandRegistered = false
// The rule registry, parsed once per load from the plugin's own rules.md: ID to its Markdown.
let rules: Map<string, string> | null = null

async function refresh($: EngineInterface): Promise<void> {
  const surfaces = await $.session.surfaces()
  if (!surfaces.some(s => DRAWS.has(s))) return
  const next = await statusView($)
  if (next === 'no-project') {
    off = true
    return
  }
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

/**
 * Each finding with a key from what it says, so an open finding stays open under the same finding
 * when a refresh adds or removes others. The nth repeat of the same rule, file and line gets `#n`.
 */
function findingKeys(findings: LensFinding[]): { f: LensFinding; key: string }[] {
  const seen = new Map<string, number>()
  return findings.map(f => {
    const base = `finding:${f.rule}:${f.file}:${f.line ?? '-'}`
    const n = seen.get(base) ?? 0
    seen.set(base, n + 1)
    return { f, key: n ? `${base}#${n}` : base }
  })
}

/** Fills the prompt box with a request to address the finding; never submits it. */
async function draftFix($: EngineInterface, f: LensFinding): Promise<void> {
  const at = f.line === null ? f.file : `${f.file}:${f.line}`
  await $.prompt.fill({ text: `Address the reviewer's ${f.severity} finding ${f.rule} in ${at}: ${f.fix}`, mode: 'insert' })
}

/** Refreshes once the current hook has returned. */
function refreshNow($: EngineInterface): void {
  $.clock.after(0, () => refresh($))
}

/** Refreshes once, DEBOUNCE_MS after the first change: later changes in that spell ride along. */
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
    off = (await $.env.get('TEMPER_DISABLE')) === '1' || (await $.env.get('TEMPER_LENS')) === '0'
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
    await $.ui.open({ id: PANE, title: 'temper' })
    return { text: 'temper lens: the review pane is open.' }
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const { Box, Button, Markdown, Text } = $.ui.resolve(e)
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
          <Text bold>Review</Text>
          <Text>{noReviewReason(s)}</Text>
        </Box>
      )
    }
    const findings = findingKeys(r.findings)
    return (
      <Box flexDirection="column">
        <Text bold>Review</Text>
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
  })

  // A client that draws attached after the session started (the desktop app connects as an SDK
  // host, so session.start saw no surface): draw now rather than at the first turn end.
  on('session.attach', async ($, e, next) => {
    const result = await next(e)
    if (!off && DRAWS.has(e.surface)) refreshNow($)
    return result
  })

  // The refresh runs after the hook returns, so a slow CLI never holds the end of the turn.
  on('turn.complete', async ($, e, next) => {
    const result = await next(e)
    if (!off) refreshNow($)
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
    if (!m) continue
    const [, id = '', level, rule, by, detail] = m
    out.set(id, `**${id}** (${level}): ${rule}\n\nEnforced by: ${by}. Detail: ${detail}.`)
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
 * The band's segments for `status`, fitted to `columns`: when narrow the baseline goes first,
 * then the review's counts, then the gate's step name, and last whole segments from the end, so
 * the band never wraps to a second row. `drop` orders the shortening (higher first; 0 is never shortened).
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
  if (width(fitted) > columns) fitted = fitted.map(p => (p.drop === 1 ? { ...p, text: p.text.replace(/ \(.*\)$/, '') } : p))
  while (width(fitted) > columns && fitted.length > 1) fitted = fitted.slice(0, -1)
  return fitted.map(({ text, color }) => (color ? { text, color } : { text }))
}

function gatePart(status: LensStatus): Part {
  const g = status.gate
  if (g?.running) {
    const step = g.running.current?.name
    return { text: step ? `gate running (${step})` : 'gate running', color: 'warning', drop: 1 }
  }
  if (!g) return { text: 'gate not run', color: 'warning', drop: 0 }
  if (g.unavailable) return { text: 'gate unavailable', color: 'warning', drop: 0 }
  if (g.ok) return { text: 'gate green', color: 'success', drop: 0 }
  const failed = g.steps.find(s => s.ok === false)?.name
  return { text: failed ? `gate red (${failed})` : 'gate red', color: 'error', drop: 1 }
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
