import type { EngineInterface } from 'claude-code'

import { MAX_FIELD_CHARS, type RecordState, record } from './kanban-record'

const CLI_TIMEOUT_MS = 5_000
const REQUEST_TTL = '60s'
const MAX_PROMPT_CHARS = 2_000

type Taken = { kind: 'prompt'; text: string; body: string; from: string } | { kind: 'stop'; turn_id: string }

type Result = { id: string; kind: string; status: string }

export type InboxState = RecordState & {
  /** This session's inbox directory; null until the CLI named it, or when it could not. */
  inboxDir: string | null
  /** The main loop's running turn. */
  turnId: string | null
  taking: boolean
}

export async function cli($: EngineInterface, args: string[]): Promise<Record<string, unknown> | null> {
  try {
    const r = await $.process.run(['atelier', 'kanban', ...args], { timeoutMs: CLI_TIMEOUT_MS })
    const parsed: unknown = r.exitCode === 0 ? JSON.parse(r.stdout) : null

    return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : null
  } catch {
    return null
  }
}

export async function findInbox($: EngineInterface, s: InboxState): Promise<void> {
  const out = await cli($, ['inbox', 'dir', '--session', await $.session.id()])

  s.inboxDir = typeof out?.dir === 'string' ? out.dir : null
}

async function hasRequests($: EngineInterface, dir: string): Promise<boolean> {
  try {
    return (await $.fs.list(dir)).some(e => e.kind === 'file' && !e.name.startsWith('.') && e.name.endsWith('.json'))
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
  await record($, s, {
    name: 'prompt.submit',
    fill: { first_prompt: taken.body.slice(0, 500) },
    update: () => ({
      last_prompt: taken.body.slice(0, MAX_PROMPT_CHARS),
      decision: null,
      pending_context: null,
      permission: null,
    }),
  })
  void $.prompt.submit({ text: taken.text }).catch(() => undefined)
}

/** Takes what other sessions left in this session's inbox, at most once per request. */
export async function deliverRequests($: EngineInterface, s: InboxState): Promise<void> {
  const dir = s.inboxDir

  if (dir === null || s.taking || !(await hasRequests($, dir))) {
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
