import type { EngineInterface } from 'claude-code'

export const KEY_PREFIX = 'kanban:session:'
/** Bounds what one session keeps in the store every session shares (4 MiB in all). */
const MAX_EVENTS = 8
export const MAX_FIELD_CHARS = 200
const MAX_FIRST_PROMPT_CHARS = 500
const MAX_PROMPT_CHARS = 1_000
const MAX_PROMPTS = 5

type KanbanEvent = { name: string; detail: string | null; at: number }

export type AskQuestion = {
  question: string
  header: string
  multi_select: boolean
  options: { label: string; description: string | null }[]
}

type AskRecord = { questions: AskQuestion[]; at: number }

export type Activity = { tool: string; input: Record<string, string>; at: number }

type Decision = { ask: string; recap: string | null; options: string[]; at: number }

type Delivery = { id: string; kind: string; status: string; at: number }

type TaskRecord = { subject: string; active: string | null; status: string }

type TodoRecord = { status: string; active: string }

export type SessionRecord = {
  v: 1
  session_id: string
  cwd: string
  repo: string | null
  /** The main working tree of the session's repository. */
  repo_root?: string | null
  remote?: string | null
  permission_mode?: string | null
  title: string | null
  first_prompt: string | null
  /** Written by versions before `prompts`; read only. */
  last_prompt?: string | null
  /** The newest prompts, oldest first, with `prompt.submit`'s origin kind or `kanban`. */
  prompts?: { text: string; origin: string; at: number }[]
  last_answer?: string | null
  /** The reply just before the session started waiting mid-turn. */
  pending_context?: string | null
  turn_id?: string | null
  started_at: number
  heartbeat_at: number
  ended_reason: string | null
  events: KanbanEvent[]
  activity?: Activity | null
  recent?: Activity[]
  /** `id` names the request for a kanban answer; null when this session cannot take one. */
  permission?: (Activity & { id: string | null }) | null
  decision?: Decision | null
  inbox?: Delivery | null
  tasks?: Record<string, TaskRecord>
  todos?: TodoRecord[]
  asks?: Record<string, AskRecord>
}

type Fields = Partial<Pick<SessionRecord, 'title' | 'first_prompt' | 'ended_reason'>>

export type Entry = {
  /** The event to append; `null` only refreshes the heartbeat. */
  name: string | null
  detail?: string | null
  set?: Fields
  /** Fills these fields only while they are still `null`. */
  fill?: Fields
  /** Fields computed from the stored record and the write's time, applied last. */
  update?: (base: SessionRecord, now: number) => Partial<SessionRecord>
  /** Appends `name` only when the session has no record yet. */
  onlyIfNew?: boolean
  /** The session the entry belongs to; this session when left out. */
  sessionId?: string
}

export type RecordState = {
  /** Serializes this process's read-modify-write of its own record. */
  writes: Promise<void>
  /** Whether the next spinner draw is recorded: once after each other event. */
  spinnerArmed: boolean
}

export function isRecord(value: unknown): value is SessionRecord {
  if (typeof value !== 'object' || value === null) {
    return false
  }

  const r = value as Record<string, unknown>

  return r.v === 1 && typeof r.session_id === 'string' && Array.isArray(r.events)
}

async function freshRecord($: EngineInterface, id: string, now: number): Promise<SessionRecord> {
  const repo = await $.session.repo()

  return {
    v: 1,
    session_id: id,
    cwd: await $.session.cwd(),
    repo: repo?.name ?? null,
    repo_root: repo?.root ?? null,
    remote: repo?.remote ?? null,
    title: null,
    first_prompt: null,
    started_at: now,
    heartbeat_at: now,
    ended_reason: null,
    events: [],
  }
}

function filled(base: SessionRecord, fill: Fields): Fields {
  const out: Fields = {}

  for (const key of Object.keys(fill) as (keyof Fields)[]) {
    if (base[key] === null) {
      out[key] = fill[key]
    }
  }

  return out
}

async function writeRecord($: EngineInterface, entry: Entry): Promise<void> {
  const id = entry.sessionId ?? (await $.session.id())
  const key = KEY_PREFIX + id
  const now = await $.clock.now()
  const stored = await $.store.get(key)
  const prior = isRecord(stored) ? stored : null
  const base = prior ?? (await freshRecord($, id, now))
  const appends = entry.name !== null && (entry.onlyIfNew !== true || prior === null)
  const events = appends
    ? [...base.events, { name: entry.name as string, detail: entry.detail ?? null, at: now }].slice(-MAX_EVENTS)
    : base.events

  /** Only `set` keeps `ended_reason`: a session that still records after its end is live again. */
  await $.store.set(key, {
    ...base,
    ...filled(base, entry.fill ?? {}),
    ended_reason: null,
    ...(entry.set ?? {}),
    ...(entry.update?.(base, now) ?? {}),
    heartbeat_at: now,
    events,
  })
}

/** Never rejects: a lost write only leaves the board a step behind. */
export async function record($: EngineInterface, s: RecordState, entry: Entry): Promise<void> {
  if (entry.name !== null) {
    s.spinnerArmed = entry.name !== 'ui.spinner'
  }

  s.writes = s.writes.then(() => writeRecord($, entry)).catch(() => undefined)
  await s.writes
}

/** A prompt that starts work, kept with where it came from. */
export async function recordPrompt($: EngineInterface, s: RecordState, text: string, origin: string): Promise<void> {
  await record($, s, {
    name: 'prompt.submit',
    fill: { first_prompt: text.slice(0, MAX_FIRST_PROMPT_CHARS) },
    update: (base, now) => ({
      prompts: [...(base.prompts ?? []), { text: text.slice(0, MAX_PROMPT_CHARS), origin, at: now }].slice(-MAX_PROMPTS),
      decision: null,
      pending_context: null,
      permission: null,
    }),
  })
}
