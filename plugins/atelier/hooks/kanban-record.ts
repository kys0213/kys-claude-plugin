import type { EngineInterface } from 'claude-code'

import { cli } from './kanban-cli'
import type { RelayTarget } from './kanban-relay'

export const KEY_PREFIX = 'kanban:session:'

/** The store keys under `prefix` that belong to session `sessionId`. */
export function sessionKeysOf(keys: readonly string[], prefix: string, sessionId: string): string[] {
  return keys.filter(k => k.startsWith(`${prefix}${sessionId}:`))
}

/** Bounds what one session keeps in the store every session shares (4 MiB in all). */
const MAX_EVENTS = 8
export const MAX_FIELD_CHARS = 200
const MAX_FIRST_PROMPT_CHARS = 500
const MAX_PROMPT_CHARS = 1_000
const MAX_PROMPTS = 5

/** `agent` marks an event a subagent's or teammate's loop raised. */
export type KanbanEvent = { name: string; detail: string | null; at: number; agent?: true }

export type AskQuestion = {
  question: string
  header: string
  multi_select: boolean
  options: { label: string; description: string | null }[]
}

/** Lengths before the record cut the text: per question, then per question's option labels. */
export type AskLengths = { question: number[]; labels: number[][] }

type AskRecord = { questions: AskQuestion[]; at: number; orig_len?: AskLengths }

export type Activity = { tool: string; input: Record<string, string>; at: number }

type Decision = {
  ask: string
  recap: string | null
  options: string[]
  at: number
  orig_len?: { ask: number; options: number[] }
}

/** `via` is how the request came in, as the CLI judged it. */
type Delivery = { id: string; kind: string; status: string; via?: string; at: number }

type TaskRecord = { subject: string; active: string | null; status: string }

type TodoRecord = { status: string; active: string }

export type AgentRecord = { status: string; teammate: boolean }

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
  permission?: (Activity & { id: string | null; input_len?: Record<string, number> }) | null
  decision?: Decision | null
  inbox?: Delivery | null
  tasks?: Record<string, TaskRecord>
  todos?: TodoRecord[]
  asks?: Record<string, AskRecord>
  /** The wait each open question was framed over, by tool_use_id, as `relay-frame` named it. */
  relay_frames?: Record<string, RelayTarget>
  /** The session's agent loops as `$.agent.list()` gave them last. */
  agents?: AgentRecord[]
  /** When a person's `/kanban delegate on` made this session the delegate; null once turned off. */
  delegate_since?: number | null
}

type Fields = Partial<Pick<SessionRecord, 'title' | 'first_prompt' | 'ended_reason'>>

export type Entry = {
  /** The event to append; `null` only refreshes the heartbeat. */
  name: string | null
  detail?: string | null
  /** Whether a subagent's or teammate's loop raised the event. */
  byAgent?: boolean
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
  /** Where the CLI keeps each session's record copy, by session id. */
  copyPaths: Map<string, string>
}

export function isRecord(value: unknown): value is SessionRecord {
  if (typeof value !== 'object' || value === null) {
    return false
  }

  const r = value as Record<string, unknown>

  return r.v === 1 && typeof r.session_id === 'string' && Array.isArray(r.events)
}

/** Where this session runs, as its record keeps it, for the CLI to judge keys and paths against. */
export async function placeOf(
  $: EngineInterface,
  rec: SessionRecord | null,
): Promise<{ repo_root: string | null; cwd: string; permission_mode: string | null }> {
  return {
    repo_root: rec?.repo_root ?? null,
    cwd: rec?.cwd ?? (await $.session.cwd()),
    permission_mode: rec?.permission_mode ?? null,
  }
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

async function writeCopy($: EngineInterface, paths: Map<string, string>, id: string, value: SessionRecord): Promise<void> {
  try {
    let path = paths.get(id)

    if (path === undefined) {
      const out = await cli($, ['record-path', '--session', id])

      if (typeof out?.path !== 'string') {
        return
      }

      path = out.path
      paths.set(id, path)
    }

    /** `$.fs.write` creates the file and its directories as needed. */
    await $.fs.write(path, JSON.stringify(value))
  } catch {
    /** a failed write is redone by the next record */
  }
}

async function writeRecord($: EngineInterface, paths: Map<string, string>, entry: Entry): Promise<void> {
  const id = entry.sessionId ?? (await $.session.id())
  const key = KEY_PREFIX + id
  const now = await $.clock.now()
  const stored = await $.store.get(key)
  const prior = isRecord(stored) ? stored : null
  const base = prior ?? (await freshRecord($, id, now))
  const updated = entry.update?.(base, now) ?? {}
  const appends = entry.name !== null && (entry.onlyIfNew !== true || prior === null)
  const kept = updated.events ?? base.events
  const events = appends
    ? [
        ...kept,
        { name: entry.name as string, detail: entry.detail ?? null, at: now, ...(entry.byAgent === true ? { agent: true as const } : {}) },
      ].slice(-MAX_EVENTS)
    : kept

  /** Only `set` keeps `ended_reason`: a session that still records after its end is live again. */
  const next: SessionRecord = {
    ...base,
    ...filled(base, entry.fill ?? {}),
    ended_reason: null,
    ...(entry.set ?? {}),
    ...updated,
    heartbeat_at: now,
    events,
  }

  await $.store.set(key, next)
  await writeCopy($, paths, id, next)
}

/** Never rejects: a lost write only leaves the board a step behind. */
export async function record($: EngineInterface, s: RecordState, entry: Entry): Promise<void> {
  if (entry.name !== null) {
    s.spinnerArmed = entry.name !== 'ui.spinner'
  }

  s.writes = s.writes.then(() => writeRecord($, s.copyPaths, entry)).catch(() => undefined)
  await s.writes
}

/** A new run of an ended session: what it waited on and did is dropped, its conversation is kept. */
export function rerun(base: SessionRecord, now: number): Partial<SessionRecord> {
  if (base.ended_reason === null) {
    return {}
  }

  return {
    started_at: now,
    events: [{ name: 'session.start', detail: null, at: now }],
    pending_context: null,
    turn_id: null,
    activity: null,
    recent: [],
    permission: null,
    decision: null,
    asks: {},
    agents: [],
    delegate_since: null,
  }
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
