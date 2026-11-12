import type { EngineInterface } from 'claude-code'

import { type AskState, answerAsk, openAsks } from './kanban-ask'
import { cli } from './kanban-cli'
import { recordDecide } from './kanban-ledger'
import { type Via, dropUsedNonces, leftNonces, viaOf } from './kanban-nonce'
import {
  KEY_PREFIX,
  MAX_FIELD_CHARS,
  type RecordState,
  type SessionRecord,
  isRecord,
  placeOf,
  record,
  recordPrompt,
} from './kanban-record'

const REQUEST_TTL = '60s'
const PERMIT_WAIT = '2s'
/** Bounds one permission wait to about half an hour of two-second waits. */
const MAX_PERMIT_WAITS = 900

type Taken =
  | { kind: 'prompt'; text: string; body: string; from: string }
  | { kind: 'stop'; turn_id: string }
  | { kind: 'answer'; tool_use_id: string; answers: string[]; from: string; via?: unknown; auth?: unknown }
  | { kind: 'decide'; decision_at: string; text: string; from: string; via?: unknown; auth?: unknown }

type Result = { id: string; kind: string; status: string; via?: unknown }

export type InboxState = RecordState & {
  /** This session's inbox directory; null until the CLI named it, or when it could not. */
  inboxDir: string | null
  /** The main loop's running turn. */
  turnId: string | null
  taking: boolean
  /** The permission request whose kanban answer a hook waits for. */
  permitting: string | null
}

export async function findInbox($: EngineInterface, s: InboxState): Promise<void> {
  const out = await cli($, ['inbox', 'dir', '--session', await $.session.id()])

  s.inboxDir = typeof out?.dir === 'string' ? out.dir : null
}

async function hasEntries($: EngineInterface, dir: string): Promise<boolean> {
  try {
    return (await $.fs.list(dir)).length > 0
  } catch {
    return false
  }
}

function isTaken(value: unknown): value is Taken {
  if (typeof value !== 'object' || value === null) {
    return false
  }

  const t = value as Record<string, unknown>

  return (
    (t.kind === 'prompt' && typeof t.text === 'string' && typeof t.body === 'string') ||
    (t.kind === 'stop' && typeof t.turn_id === 'string') ||
    (t.kind === 'answer' &&
      typeof t.tool_use_id === 'string' &&
      Array.isArray(t.answers) &&
      t.answers.every(a => typeof a === 'string')) ||
    (t.kind === 'decide' && typeof t.text === 'string')
  )
}

function isResult(value: unknown): value is Result {
  const r = value as Record<string, unknown> | null

  return typeof r?.id === 'string' && typeof r.kind === 'string' && typeof r.status === 'string'
}

async function act($: EngineInterface, s: InboxState & AskState, taken: Taken): Promise<void> {
  if (taken.kind === 'stop') {
    if (s.turnId === taken.turn_id) {
      await $.turn.abort({ turnId: taken.turn_id })
    }

    return
  }

  if (taken.kind === 'answer') {
    answerAsk(s, taken.tool_use_id, { answers: taken.answers, via: viaOf(taken.via), auth: taken.auth })

    return
  }

  if (taken.kind === 'decide') {
    await recordDecide($, taken.text, viaOf(taken.via), taken.auth)
    await record($, s, { name: 'kanban.answer', detail: viaOf(taken.via) })
  }

  /** The submitting plugin's own prompt.submit hooks do not see the prompt, so it is recorded here. */
  await recordPrompt($, s, taken.kind === 'decide' ? taken.text : taken.body, 'kanban')
  void $.prompt.submit({ text: taken.text }).catch(() => undefined)
}

async function storedRecord($: EngineInterface, self: string): Promise<SessionRecord | null> {
  const stored = await $.store.get(KEY_PREFIX + self)

  return isRecord(stored) ? stored : null
}

async function waits($: EngineInterface, s: AskState, self: string): Promise<string> {
  const rec = await storedRecord($, self)

  return JSON.stringify({
    asks: openAsks(s),
    decision: rec?.decision ?? null,
    place: await placeOf($, rec),
    nonces: await leftNonces($, self),
  })
}

async function recordDelivery($: EngineInterface, s: InboxState, result: Result, at: number): Promise<void> {
  await record($, s, {
    name: null,
    update: () => ({
      inbox: {
        id: result.id.slice(0, MAX_FIELD_CHARS),
        kind: result.kind.slice(0, MAX_FIELD_CHARS),
        status: result.status.slice(0, MAX_FIELD_CHARS),
        ...(typeof result.via === 'string' ? { via: result.via.slice(0, MAX_FIELD_CHARS) } : {}),
        at,
      },
    }),
  })
}

/** Takes what other sessions left in this session's inbox, at most once per request. */
export async function deliverRequests($: EngineInterface, s: InboxState & AskState): Promise<void> {
  const dir = s.inboxDir

  if (dir === null || s.taking || !(await hasEntries($, dir))) {
    return
  }

  s.taking = true

  try {
    const now = await $.clock.now()
    const self = await $.session.id()
    const out = await cli(
      $,
      [
        'inbox',
        'take',
        '--session',
        self,
        '--now',
        String(now),
        '--ttl',
        REQUEST_TTL,
        '--waits-stdin',
        ...(s.turnId === null ? [] : ['--turn', s.turnId]),
      ],
      await waits($, s, self),
    )
    const actions = Array.isArray(out?.actions) ? out.actions.filter(isTaken) : []
    const results = Array.isArray(out?.results) ? out.results.filter(isResult) : []
    const last = results.at(-1)

    await dropUsedNonces($, self, out?.used_nonces)

    for (const taken of actions) {
      await act($, s, taken)
    }

    if (last !== undefined) {
      await recordDelivery($, s, last, now)
    }
  } finally {
    s.taking = false
  }
}

/**
 * Waits for the kanban answer to permission request `id` while the engine shows its own dialog.
 * Each wait is a CLI call in flight, which a hook's time budget does not count.
 * `request` is the tool and its input whole, for the CLI to judge a model's answer against.
 */
export async function awaitPermit(
  $: EngineInterface,
  s: InboxState,
  id: string,
  request: { tool: string; input: Record<string, string> },
): Promise<{ decision: 'allow' | 'deny'; via: Via; auth?: unknown } | null> {
  const self = await $.session.id()
  const place = await placeOf($, await storedRecord($, self))
  s.permitting = id

  try {
    for (let i = 0; i < MAX_PERMIT_WAITS && s.permitting === id; i++) {
      const out = await cli(
        $,
        [
          'inbox',
          'wait-permit',
          '--session',
          self,
          '--request',
          id,
          '--now',
          String(await $.clock.now()),
          '--ttl',
          REQUEST_TTL,
          '--for',
          PERMIT_WAIT,
          '--wait-stdin',
        ],
        JSON.stringify({ nonces: await leftNonces($, self), place, permission: request }),
      )

      if (out === null || typeof out.error === 'string') {
        return null
      }

      if (out.decision === 'allow' || out.decision === 'deny') {
        await dropUsedNonces($, self, out.used_nonces)

        return { decision: out.decision, via: viaOf(out.via), auth: out.auth }
      }

      if (typeof out.status === 'string') {
        await recordDelivery($, s, { id, kind: 'permit', status: out.status, via: out.via }, await $.clock.now())
      }
    }

    return null
  } finally {
    if (s.permitting === id) {
      s.permitting = null
    }
  }
}

/** The dialog was answered, or the turn moved on: a waiting hook stops at its next wait. */
export function endPermit(s: InboxState): void {
  s.permitting = null
}
