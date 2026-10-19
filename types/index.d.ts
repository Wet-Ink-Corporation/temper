// temper lens: the values its drawing reads from `$.state` (the plugin's contract).
// They mirror the parts of `temper status --json` (temper_status: 1) the lens shows.

/** A gate step as `status --json` reports it. */
export type LensStep = { name: string; ok: boolean | null; ms: number | null }

/** The gate running now for the tree: the steps done and the step it is on. */
export type LensRunning = {
  startedAt: string
  steps: LensStep[]
  current: { name: string; startedAt: string } | null
}

/** One reviewer finding, without its evidence text. */
export type LensFinding = {
  severity: 'blocker' | 'major' | 'minor' | 'nit'
  rule: string
  file: string
  line: number | null
  fix: string
}

/** The state of the current tree: the subset of `status --json` the lens draws. */
export type LensStatus = {
  fingerprint: string
  changed: string[]
  gate: { ok: boolean | null; unavailable: boolean; steps: LensStep[]; running: LensRunning | null } | null
  review: {
    verdict: 'approved' | 'changes-requested'
    stale: boolean
    scores: Record<string, number | 'n/a'>
    findings: LensFinding[]
  } | null
  baseline: { total: number } | { error: string } | null
}

/**
 * What the band draws: the tree's state, `unavailable` when `status --json` failed or did not
 * parse, `null` before the first refresh (the band then draws nothing).
 */
export type LensView = { kind: 'status'; status: LensStatus } | { kind: 'unavailable' }

/** The finding the pane shows open: its key, and its rule's text as Markdown. */
export type LensExpanded = { key: string; text: string }

/** The counts of `temper stats --json` the Stats tab shows. */
export type LensStats = {
  sessions: number
  rules: Record<string, number>
  gate: { runs: number; cached: number; cutShort: number; p50Ms: number | null; p90Ms: number | null; maxMs: number | null; recentMs: number[] }
  review: { sessions: number; roundsP50: number | null; roundsMax: number | null; firstApproved: number }
}

/** One telemetry row of this session, as `stats --json --session` gives it. */
export type LensActivityRow = { t: string; event: string; agent: string; file: string | null; rules: string[]; via: string | null }

/** What the Activity and Stats tabs draw: counts and this session's rows, or why they are missing. */
export type LensStatsView = { kind: 'stats'; totals: LensStats; session: LensActivityRow[] } | { kind: 'unavailable'; why: string }

declare module 'claude-code' {
  interface PluginState {
    temper: {
      view: LensView | null
      expanded: LensExpanded | null
      tab: 'review' | 'activity' | 'stats'
      stats: LensStatsView | null
      branchStats: LensStatsView | null
      polledAt: number
    }
  }
}
