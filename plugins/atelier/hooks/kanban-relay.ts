import type { EngineInterface } from 'claude-code'

import type { ToolQuestion } from './kanban-ask'
import { cli } from './kanban-cli'

/** The wait a framed question asks about, as `relay-frame` named it; passed on untouched. */
export type RelayTarget = { session: string; wait: string; kind: string }

export type Framed = { questions: ToolQuestion[]; target: RelayTarget }

function isTarget(value: unknown): value is RelayTarget {
  const t = value as Record<string, unknown> | null

  return typeof t?.session === 'string' && typeof t.wait === 'string' && typeof t.kind === 'string'
}

/**
 * The questions as `relay-frame` rewrote them over the wait their first line names, and that wait.
 * Null leaves them as asked: no wait named, or a CLI that failed.
 */
export async function frameQuestions($: EngineInterface, questions: readonly ToolQuestion[]): Promise<Framed | null> {
  const out = await cli($, ['relay-frame', '--now', String(await $.clock.now())], JSON.stringify({ questions }))

  if (!isTarget(out?.target) || !Array.isArray(out.questions) || out.questions.length !== questions.length) {
    return null
  }

  return { questions: out.questions as ToolQuestion[], target: out.target }
}
