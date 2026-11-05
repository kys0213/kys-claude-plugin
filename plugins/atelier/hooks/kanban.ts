import type { EngineInterface, On } from 'claude-code'

const PANE = 'kanban'
const KEY_PREFIX = 'kanban:session:'
const HEARTBEAT_MS = 30_000
const REFRESH_MS = 2_000
const STALE_AFTER = '2m'
const EXPIRE_AFTER = '24h'
const CLI_TIMEOUT_MS = 5_000
/** Bounds what one session keeps in the store every session shares (4 MiB in all). */
const MAX_EVENTS = 8
const MAX_PROMPT_CHARS = 500
const COLUMN_MIN_WIDTH = 22

type KanbanEvent = { name: string; detail: string | null; at: number }

type SessionRecord = {
  v: 1
  session_id: string
  cwd: string
  repo: string | null
  title: string | null
  first_prompt: string | null
  started_at: number
  heartbeat_at: number
  ended_reason: string | null
  events: KanbanEvent[]
}

type Fields = Partial<Pick<SessionRecord, 'title' | 'first_prompt' | 'ended_reason'>>

type Entry = {
  /** The event to append; `null` only refreshes the heartbeat. */
  name: string | null
  detail?: string | null
  set?: Fields
  /** Fills these fields only while they are still `null`. */
  fill?: Fields
  /** Appends `name` only when the session has no record yet. */
  onlyIfNew?: boolean
  /** The session the entry belongs to; this session when left out. */
  sessionId?: string
}

type Card = {
  session_id: string
  label: string
  subtitle: string | null
  reason: string | null
  age: string
  is_self: boolean
}

type Column = { id: string; title: string; cards: Card[] }

type BoardView =
  | { kind: 'loading' }
  | { kind: 'board'; columns: Column[] }
  | { kind: 'error'; message: string }

type KanbanState = {
  view: BoardView
  /** Serializes this process's read-modify-write of its own record. */
  writes: Promise<void>
  timersStarted: boolean
  /** Whether the next spinner draw is recorded: once after each other event. */
  spinnerArmed: boolean
}

function isRecord(value: unknown): value is SessionRecord {
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
    heartbeat_at: now,
    events,
  })
}

/** Never rejects: a lost write only leaves the board a step behind. */
async function record($: EngineInterface, s: KanbanState, entry: Entry): Promise<void> {
  if (entry.name !== null) {
    s.spinnerArmed = entry.name !== 'ui.spinner'
  }

  s.writes = s.writes.then(() => writeRecord($, entry)).catch(() => undefined)
  await s.writes
}

function parseObject(stdout: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(stdout)

    return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : null
  } catch {
    return null
  }
}

function decodeBoard(parsed: Record<string, unknown> | null): BoardView {
  if (typeof parsed?.error === 'string') {
    return { kind: 'error', message: parsed.error }
  }

  if (Array.isArray(parsed?.columns)) {
    return { kind: 'board', columns: parsed.columns as Column[] }
  }

  return { kind: 'error', message: 'atelier kanban board 의 출력을 읽지 못했어요' }
}

function expiredIds(parsed: Record<string, unknown> | null): string[] {
  const expired = parsed?.expired

  return Array.isArray(expired) ? expired.filter((x): x is string => typeof x === 'string') : []
}

async function snapshot($: EngineInterface): Promise<unknown[]> {
  const keys = (await $.store.keys()).filter(k => k.startsWith(KEY_PREFIX))

  return Promise.all(keys.map(k => $.store.get(k)))
}

async function refresh($: EngineInterface, s: KanbanState): Promise<void> {
  try {
    const sessions = await snapshot($)
    const argv = [
      'atelier',
      'kanban',
      'board',
      '--now',
      String(await $.clock.now()),
      '--stale-after',
      STALE_AFTER,
      '--expire-after',
      EXPIRE_AFTER,
      '--self',
      await $.session.id(),
    ]
    const r = await $.process.run(argv, { stdin: JSON.stringify({ sessions }), timeoutMs: CLI_TIMEOUT_MS })

    if (r.exitCode !== 0) {
      s.view = { kind: 'error', message: `atelier kanban board 가 exit ${r.exitCode} 로 끝났어요` }
    } else {
      const parsed = parseObject(r.stdout)

      s.view = decodeBoard(parsed)

      for (const id of expiredIds(parsed)) {
        await $.store.delete(KEY_PREFIX + id)
      }
    }
  } catch {
    s.view = { kind: 'error', message: 'atelier CLI 를 실행하지 못했어요 — /atelier:update 로 설치·갱신하세요' }
  }

  $.ui.invalidate('ui.render')
}

async function isPaneShown($: EngineInterface): Promise<boolean> {
  return (await $.ui.panes()).some(p => p.id === PANE && p.isShown)
}

async function tick($: EngineInterface, s: KanbanState): Promise<void> {
  try {
    if (await isPaneShown($)) {
      await refresh($, s)
    }
  } catch {
    /** a failed tick waits for the next one */
  }
}

async function toggle($: EngineInterface, s: KanbanState): Promise<string> {
  if (await isPaneShown($)) {
    await $.ui.close({ id: PANE })

    return '세션 칸반을 닫았어요.'
  }

  const opened = await $.ui.open({ id: PANE, title: '세션 칸반' })

  await refresh($, s)

  return opened.isPlaced ? '세션 칸반을 열었어요.' : '세션 칸반을 열었지만 이 화면에는 놓을 자리가 없어요.'
}

async function startSession($: EngineInterface, s: KanbanState): Promise<void> {
  try {
    await $.command.register({ name: 'kanban', description: '로컬 Claude 세션 칸반 pane 을 열고 닫아요' })
  } catch {
    /** the pane is still reachable once a later reload registers it */
  }

  await record($, s, { name: 'session.start', onlyIfNew: true })

  if (!s.timersStarted) {
    s.timersStarted = true
    $.clock.every(HEARTBEAT_MS, () => void record($, s, { name: null }))
    $.clock.every(REFRESH_MS, () => void tick($, s))
  }
}

function cardLine(card: Card): string {
  const marker = card.is_self ? '▶ ' : ''
  const tail = card.reason ? `${card.reason} · ${card.age}` : card.age

  return `${marker}${card.label} · ${tail}`
}

export function registerKanban(on: On): void {
  const s: KanbanState = {
    view: { kind: 'loading' },
    writes: Promise.resolve(),
    timersStarted: false,
    spinnerArmed: false,
  }

  on('session.start', async ($, e, next) => {
    await startSession($, s)

    return next(e)
  })

  on('command.run', { command: 'kanban' }, async $ => ({ text: await toggle($, s) }))

  on('prompt.submit', async ($, e, next) => {
    if (e.command === undefined && e.isMeta !== true) {
      await record($, s, { name: 'prompt.submit', fill: { first_prompt: e.text.slice(0, MAX_PROMPT_CHARS) } })
    }

    return next(e)
  })

  on('classic.UserPromptSubmit', async ($, e, next) => {
    if (typeof e.session_title === 'string') {
      await record($, s, { name: null, set: { title: e.session_title } })
    }

    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    if (e.agentId === undefined) {
      await record($, s, { name: 'turn.start' })
    }

    return next(e)
  })

  on('turn.complete', async ($, e, next) => {
    if (e.agentId === undefined) {
      await record($, s, { name: 'turn.complete' })
    }

    return next(e)
  })

  on('classic.PermissionRequest', async ($, e, next) => {
    await record($, s, { name: 'classic.PermissionRequest', detail: e.tool_name })

    return next(e)
  })

  on('classic.Notification', async ($, e, next) => {
    await record($, s, { name: 'classic.Notification', detail: e.notification_type })

    return next(e)
  })

  on('classic.PostToolUse', async ($, e, next) => {
    await record($, s, { name: 'classic.PostToolUse', detail: e.tool_name })

    return next(e)
  })

  on('classic.PostToolUseFailure', async ($, e, next) => {
    await record($, s, { name: 'classic.PostToolUseFailure', detail: e.tool_name })

    return next(e)
  })

  on('session.end', async ($, e, next) => {
    await record($, s, {
      name: 'session.end',
      detail: e.reason,
      set: { ended_reason: e.reason },
      sessionId: e.sessionId,
    })

    return next(e)
  })

  on('ui.render', { component: 'Spinner' }, async ($, e, next) => {
    if (s.spinnerArmed) {
      void record($, s, { name: 'ui.spinner' })
    }

    return next(e)
  })

  on('ui.render', { component: 'Pane', requestId: 'kanban' }, async ($, e) => {
    const { Box, Text } = $.ui.resolve(e)
    const view = s.view

    if (view.kind === 'loading') {
      return Text({ dimColor: true, children: ['세션을 불러오는 중…'] })
    }

    if (view.kind === 'error') {
      return Text({ color: 'red', children: [view.message] })
    }

    const count = view.columns.length
    const isWide = e.props.bodyColumns >= count * COLUMN_MIN_WIDTH
    const width = isWide ? Math.floor((e.props.bodyColumns - (count - 1)) / count) : e.props.bodyColumns
    /** Side by side each column is a bordered box; stacked, a heading and indented cards, to save rows. */
    const frame = isWide
      ? { width, borderStyle: 'round' as const, paddingX: 1 }
      : { width, marginBottom: 1 }
    const columns = view.columns.map(col =>
      Box({
        key: col.id,
        flexDirection: 'column',
        ...frame,
        children: [
          Text({ bold: true, dimColor: col.cards.length === 0, children: [`${col.title} ${col.cards.length}`] }),
          ...col.cards.map(card =>
            Box({
              key: card.session_id,
              flexDirection: 'column',
              paddingLeft: isWide ? 0 : 2,
              children: [
                Text({ bold: card.is_self, wrap: 'truncate', children: [cardLine(card)] }),
                ...(card.subtitle ? [Text({ dimColor: true, wrap: 'truncate', children: [card.subtitle] })] : []),
              ],
            }),
          ),
        ],
      }),
    )

    return Box({ flexDirection: isWide ? 'row' : 'column', columnGap: 1, children: columns })
  })
}
