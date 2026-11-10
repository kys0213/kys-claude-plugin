import type { EngineInterface } from 'claude-code'

import { cli } from './kanban-cli'
import { MAX_FIELD_CHARS, type RecordState, record, recordPrompt } from './kanban-record'

const REQUEST_TTL = '60s'

type Taken = { kind: 'prompt'; text: string; body: string; from: string } | { kind: 'stop'; turn_id: string }

type Result = { id: string; kind: string; status: string }

export type InboxState = RecordState & {
  /** This session's inbox directory; null until the CLI named it, or when it could not. */
  inboxDir: string | null
  /** The main loop's running turn. */
  turnId: string | null
  taking: boolean
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
    (t.kind === 'stop' && typeof t.turn_id === 'string')
  )
}

function isResult(value: unknown): value is Result {
  const r = value as Record<string, unknown> | null

  return typeof r?.id === 'string' && typeof r.kind === 'string' && typeof r.status === 'string'
}

async function act($: EngineInterface, s: InboxState, taken: Taken): Promise<void> {
  if (taken.kind === 'stop') {
    if (s.turnId === taken.turn_id) {
      await $.turn.abort({ turnId: taken.turn_id })
    }

    return
  }

  /** The submitting plugin's own prompt.submit hooks do not see the prompt, so it is recorded here. */
  await recordPrompt($, s, taken.body)
  void $.prompt.submit({ text: taken.text }).catch(() => undefined)
}

/** Takes what other sessions left in this session's inbox, at most once per request. */
export async function deliverRequests($: EngineInterface, s: InboxState): Promise<void> {
  const dir = s.inboxDir

  if (dir === null || s.taking || !(await hasEntries($, dir))) {
    return
  }

  s.taking = true

  try {
    const now = await $.clock.now()
    const out = await cli($, [
      'inbox',
      'take',
      '--session',
      await $.session.id(),
      '--now',
      String(now),
      '--ttl',
      REQUEST_TTL,
      ...(s.turnId === null ? [] : ['--turn', s.turnId]),
    ])
    const actions = Array.isArray(out?.actions) ? out.actions.filter(isTaken) : []
    const results = Array.isArray(out?.results) ? out.results.filter(isResult) : []
    const last = results.at(-1)

    for (const taken of actions) {
      await act($, s, taken)
    }

    if (last !== undefined) {
      await record($, s, {
        name: null,
        update: () => ({
          inbox: {
            id: last.id.slice(0, MAX_FIELD_CHARS),
            kind: last.kind.slice(0, MAX_FIELD_CHARS),
            status: last.status.slice(0, MAX_FIELD_CHARS),
            at: now,
          },
        }),
      })
    }
  } finally {
    s.taking = false
  }
}
