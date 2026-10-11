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

declare module 'claude-code' {
  interface PluginState {
    temper: { view: LensView | null }
  }
}
