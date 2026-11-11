import type { EngineInterface } from 'claude-code'

import type { ToolQuestion } from './kanban-ask'
import { cli } from './kanban-cli'
import type { Via } from './kanban-nonce'
import { type AskQuestion, type AskLengths, KEY_PREFIX, type SessionRecord, isRecord } from './kanban-record'

/** A permission request as the record keeps it. */
export type PermissionFacts = NonNullable<SessionRecord['permission']>

type DecisionFacts = NonNullable<SessionRecord['decision']>

/** The wait an answer was for, as this session's record keeps it; `ledger record` reads one of them. */
type LedgerWait =
  | { question: { tool_use_id: string; questions: AskQuestion[]; at: number; orig_len: AskLengths } }
  | { permission: PermissionFacts }
  | { decision: DecisionFacts }

/** How the answer came in, as `ledger record` reads it; who decided is left to the CLI. */
type LedgerAnswer =
  | { path: 'session'; answers: (string | null)[]; afk_timeout_ms: number | null }
  | { path: 'session'; post: string | null; is_error: boolean }
  | { path: 'session'; text: string; origin: string }
  | { path: 'inbox'; via: Via; answers: string[] }
  | { path: 'inbox'; via: Via; decision: 'allow' | 'deny' }
  | { path: 'inbox'; via: Via; text: string }

/** A tool call in flight, the permission request it raised and the post event that came for it. */
type Call = { tool: string; permission: PermissionFacts | null; post: string | null; settled: boolean }

export type LedgerState = {
  /** Tool calls in flight, by tool_use_id. */
  calls: Map<string, Call>
}

export function newLedgerState(): LedgerState {
  return { calls: new Map() }
}

async function storedRecord($: EngineInterface): Promise<SessionRecord | null> {
  const stored = await $.store.get(KEY_PREFIX + (await $.session.id()))

  return isRecord(stored) ? stored : null
}

/** Leaves one answer in the ledger. Never rejects: a failing CLI loses only that entry. */
async function recordAnswer($: EngineInterface, wait: LedgerWait, answer: LedgerAnswer): Promise<void> {
  try {
    const rec = await storedRecord($)

    await cli(
      $,
      ['ledger', 'record'],
      JSON.stringify({
        at: await $.clock.now(),
        session: await $.session.id(),
        repo_root: rec?.repo_root ?? null,
        cwd: rec?.cwd ?? (await $.session.cwd()),
        permission_mode: rec?.permission_mode ?? null,
        ...wait,
        answer,
      }),
    )
  } catch {
    /** the answer is used all the same */
  }
}

type AskFacts = { at: number; questions: AskQuestion[]; orig_len: AskLengths }

/** The dialog's own answers, one per question in the call's order; null when the call ended without them. */
function dialogAnswer(questions: readonly ToolQuestion[], r: unknown): LedgerAnswer | null {
  const result = (r as { result?: unknown; isError?: unknown; deny?: unknown } | null) ?? {}
  const answers = (result.result as { answers?: unknown } | undefined)?.answers

  if (result.isError === true || result.deny !== undefined || typeof answers !== 'object' || answers === null) {
    return null
  }

  const byQuestion = answers as Record<string, unknown>
  const afk = (result.result as { afkTimeoutMs?: unknown }).afkTimeoutMs

  return {
    path: 'session',
    answers: questions.map(q => {
      const answer = byQuestion[q.question]

      return typeof answer === 'string' ? answer : null
    }),
    afk_timeout_ms: typeof afk === 'number' ? afk : null,
  }
}

/** Records the answers an AskUserQuestion call took: its dialog's result, or the kanban's answers and `via`. */
export async function recordQuestion(
  $: EngineInterface,
  toolUseId: string,
  ask: AskFacts,
  questions: readonly ToolQuestion[],
  won: { local: unknown } | { answers: string[]; via: Via },
): Promise<void> {
  const answer = 'local' in won ? dialogAnswer(questions, won.local) : { path: 'inbox' as const, via: won.via, answers: won.answers }

  if (answer !== null) {
    await recordAnswer($, { question: { tool_use_id: toolUseId, ...ask } }, answer)
  }
}

/** A tool call starts: a permission request it raises is tied to it. */
export function startCall(s: LedgerState, toolUseId: string | undefined, tool: string): void {
  if (toolUseId !== undefined) {
    s.calls.set(toolUseId, { tool, permission: null, post: null, settled: false })
  }
}

/**
 * Ties a permission request to the one call in flight for its tool that has none yet.
 * With none, or more than one, it is tied to no call and its dialog answer is not recorded.
 */
export function askedPermission(s: LedgerState, tool: string, permission: PermissionFacts): Call | null {
  const open = [...s.calls.values()].filter(call => call.tool === tool && call.permission === null)

  if (open.length !== 1) {
    return null
  }

  const [call] = open as [Call]
  call.permission = permission

  return call
}

/** A PostToolUse or PostToolUseFailure came for the call. */
export function ranCall(s: LedgerState, toolUseId: string, post: string): void {
  const call = s.calls.get(toolUseId)

  if (call !== undefined && call.post === null) {
    call.post = post
  }
}

/** The kanban answered the permission request: the call's own end says nothing more about it. */
export async function recordPermit(
  $: EngineInterface,
  call: Call | null,
  permission: PermissionFacts,
  verdict: { decision: 'allow' | 'deny'; via: Via },
): Promise<void> {
  if (call !== null) {
    call.settled = true
  }

  await recordAnswer($, { permission }, { path: 'inbox', via: verdict.via, decision: verdict.decision })
}

/** The call ended: a permission dialog it raised is recorded with how the call went after it. */
export async function endCall($: EngineInterface, s: LedgerState, toolUseId: string | undefined, r: unknown): Promise<void> {
  const call = toolUseId === undefined ? undefined : s.calls.get(toolUseId)

  if (toolUseId === undefined || call === undefined) {
    return
  }

  s.calls.delete(toolUseId)

  if (call.permission !== null && !call.settled) {
    const isError = (r as { isError?: unknown } | null)?.isError === true

    await recordAnswer($, { permission: call.permission }, { path: 'session', post: call.post, is_error: isError })
  }
}

/** A prompt while a decision is open is recorded as that decision's answer, with the prompt's origin. */
export async function recordDecisionPrompt($: EngineInterface, text: string, origin: string): Promise<void> {
  const decision = (await storedRecord($))?.decision

  if (decision !== null && decision !== undefined) {
    await recordAnswer($, { decision }, { path: 'session', text, origin })
  }
}

/** A decide the kanban delivered is recorded with the `via` the CLI gave it. */
export async function recordDecide($: EngineInterface, text: string, via: Via): Promise<void> {
  const decision = (await storedRecord($))?.decision

  if (decision !== null && decision !== undefined) {
    await recordAnswer($, { decision }, { path: 'inbox', via, text })
  }
}
