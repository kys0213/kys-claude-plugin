import type { EngineInterface, On } from 'claude-code'

import {
  type AskCard,
  type AskState,
  answerKey,
  answerKeysOf,
  answersByQuestion,
  askBlocks,
  askOf,
  awaitAnswer,
  deliverAnswers,
  newAskState,
  pruneDrafts,
} from './kanban-ask'
import { KEY_PREFIX, MAX_FIELD_CHARS, type RecordState, type SessionRecord, record } from './kanban-record'

const PANE = 'kanban'
const HEARTBEAT_MS = 30_000
const REFRESH_MS = 2_000
const STALE_AFTER = '2m'
const EXPIRE_AFTER = '24h'
const CLI_TIMEOUT_MS = 5_000
const MAX_PROMPT_CHARS = 500
const MAX_TODOS = 50
/** Keys a `tool.call` input carries beside the tool's own arguments. */
const RESERVED_INPUT_KEYS = ['tool', 'tool_use_id', 'agentId', 'requestMeta', 'consent']

type Card = AskCard & { subtitle: string | null; progress?: string | null; is_self: boolean }

type Column = { id: string; heading: string; cards: Card[] }

type Layout = { direction: 'row' | 'column'; column_width: number; column_gap: number }

type BoardView =
  | { kind: 'loading' }
  | { kind: 'board'; columns: Column[]; layout: Layout; width: number }
  | { kind: 'error'; message: string }

type KanbanState = RecordState &
  AskState & {
    view: BoardView
    timersStarted: boolean
    /** The pane width the last drawing had; the board is asked for that width. */
    width: number
    refreshing: boolean
  }

function stringFields(input: object): Record<string, string> {
  return Object.fromEntries(
    Object.entries(input)
      .filter(([key, value]) => typeof value === 'string' && !RESERVED_INPUT_KEYS.includes(key))
      .map(([key, value]) => [key, (value as string).slice(0, MAX_FIELD_CHARS)]),
  )
}

function withoutAsk(base: SessionRecord, toolUseId: string): Partial<SessionRecord> {
  const { [toolUseId]: _closed, ...open } = base.asks ?? {}

  return { asks: open }
}

function parseObject(stdout: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(stdout)

    return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : null
  } catch {
    return null
  }
}

function decodeBoard(parsed: Record<string, unknown> | null, width: number): BoardView {
  if (typeof parsed?.error === 'string') {
    return { kind: 'error', message: parsed.error }
  }

  if (Array.isArray(parsed?.columns) && typeof parsed?.layout === 'object' && parsed.layout !== null) {
    return { kind: 'board', columns: parsed.columns as Column[], layout: parsed.layout as Layout, width }
  }

  return { kind: 'error', message: 'atelier kanban board 의 출력을 읽지 못했어요' }
}

function expiredIds(parsed: Record<string, unknown> | null): string[] {
  const expired = parsed?.expired

  return Array.isArray(expired) ? expired.filter((x): x is string => typeof x === 'string') : []
}

async function snapshot($: EngineInterface): Promise<{ keys: string[]; sessions: unknown[] }> {
  const keys = await $.store.keys()
  const sessions = await Promise.all(keys.filter(k => k.startsWith(KEY_PREFIX)).map(k => $.store.get(k)))

  return { keys, sessions }
}

async function refresh($: EngineInterface, s: KanbanState): Promise<void> {
  if (s.refreshing) {
    return
  }

  s.refreshing = true
  const width = s.width

  try {
    const { keys, sessions } = await snapshot($)
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
      '--width',
      String(width),
      '--self',
      await $.session.id(),
    ]
    const r = await $.process.run(argv, { stdin: JSON.stringify({ sessions }), timeoutMs: CLI_TIMEOUT_MS })

    if (r.exitCode !== 0) {
      s.view = { kind: 'error', message: `atelier kanban board 가 exit ${r.exitCode} 로 끝났어요` }
    } else {
      const parsed = parseObject(r.stdout)

      s.view = decodeBoard(parsed, width)

      if (s.view.kind === 'board') {
        pruneDrafts(s, s.view.columns.flatMap(c => c.cards))
      }

      for (const id of expiredIds(parsed)) {
        for (const key of [KEY_PREFIX + id, ...answerKeysOf(keys, id)]) {
          await $.store.delete(key)
        }
      }
    }
  } catch {
    s.view = { kind: 'error', message: 'atelier CLI 를 실행하지 못했어요 — /atelier:update 로 설치·갱신하세요' }
  } finally {
    s.refreshing = false
  }

  $.ui.invalidate('ui.render')
}

async function isPaneShown($: EngineInterface): Promise<boolean> {
  return (await $.ui.panes()).some(p => p.id === PANE && p.isShown)
}

async function tick($: EngineInterface, s: KanbanState): Promise<void> {
  try {
    await deliverAnswers($, s)

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

export function registerKanban(on: On): void {
  const s: KanbanState = {
    ...newAskState(),
    view: { kind: 'loading' },
    writes: Promise.resolve(),
    timersStarted: false,
    spinnerArmed: false,
    width: 0,
    refreshing: false,
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
      await record($, s, { name: 'turn.complete', update: () => ({ activity: null }) })
    }

    return next(e)
  })

  on('tool.call', async ($, e, next) => {
    if (e.agentId === undefined) {
      const input = stringFields(e)

      await record($, s, { name: null, update: (_, now) => ({ activity: { tool: e.tool, input, at: now } }) })
    }

    return next(e)
  })

  on('tool.call', { tool: 'TodoWrite' }, async ($, e, next) => {
    const r = await next(e)

    if (e.agentId === undefined && r.deny === undefined && !r.isError) {
      const todos = e.todos
        .slice(0, MAX_TODOS)
        .map(t => ({ status: t.status, active: t.activeForm.slice(0, MAX_FIELD_CHARS) }))

      await record($, s, { name: null, update: () => ({ todos }) })
    }

    return r
  })

  on('tool.call', { tool: 'TaskCreate' }, async ($, e, next) => {
    const r = await next(e)

    if (r.deny === undefined && !r.isError) {
      const task = {
        subject: e.subject.slice(0, MAX_FIELD_CHARS),
        active: e.activeForm?.slice(0, MAX_FIELD_CHARS) ?? null,
        status: 'pending',
      }
      const id = r.result.task.id

      await record($, s, { name: null, update: base => ({ tasks: { ...base.tasks, [id]: task } }) })
    }

    return r
  })

  on('tool.call', { tool: 'TaskUpdate' }, async ($, e, next) => {
    const r = await next(e)

    if (r.deny === undefined && !r.isError) {
      const changed = {
        ...(e.subject === undefined ? {} : { subject: e.subject.slice(0, MAX_FIELD_CHARS) }),
        ...(e.activeForm === undefined ? {} : { active: e.activeForm.slice(0, MAX_FIELD_CHARS) }),
        ...(e.status === undefined ? {} : { status: e.status }),
      }

      await record($, s, {
        name: null,
        update: base => {
          const prior = base.tasks?.[e.taskId] ?? { subject: '', active: null, status: 'pending' }

          return { tasks: { ...base.tasks, [e.taskId]: { ...prior, ...changed } } }
        },
      })
    }

    return r
  })

  on('tool.call', { tool: 'AskUserQuestion' }, async ($, e, next) => {
    const id = e.tool_use_id

    if (id === undefined) {
      return next(e)
    }

    const fromKanban = awaitAnswer(s, id)
    let answered = false

    await record($, s, {
      name: null,
      update: (base, now) => ({ asks: { ...base.asks, [id]: askOf(e.questions, now) } }),
    })

    try {
      const won = await Promise.race([
        next(e).then(r => ({ local: r })),
        fromKanban.then(answers => ({ answers })),
      ])

      if ('local' in won) {
        return won.local
      }

      answered = true

      return { result: { questions: e.questions, answers: answersByQuestion(e.questions, won.answers) } }
    } finally {
      s.waiters.delete(id)
      await $.store.delete(answerKey(await $.session.id(), id))
      await record($, s, { name: answered ? 'kanban.answer' : null, update: base => withoutAsk(base, id) })
    }
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
    const els = $.ui.resolve(e)
    const { Box, Text } = els
    const view = s.view
    s.width = e.props.bodyColumns

    if (view.kind === 'loading') {
      return Text({ dimColor: true, children: ['세션을 불러오는 중…'] })
    }

    if (view.kind === 'error') {
      return Text({ color: 'red', children: [view.message] })
    }

    if (view.width !== s.width) {
      void refresh($, s)
    }

    const { direction, column_width: width, column_gap: columnGap } = view.layout
    const isRow = direction === 'row'
    /** Stacked columns drop their borders to save rows. */
    const frame = isRow ? { width, borderStyle: 'round' as const, paddingX: 1 } : { width, marginBottom: 1 }
    const columns = view.columns.map(col =>
      Box({
        key: col.id,
        flexDirection: 'column',
        ...frame,
        children: [
          Text({ bold: true, dimColor: col.cards.length === 0, children: [col.heading] }),
          ...col.cards.map(card =>
            Box({
              key: card.session_id,
              flexDirection: 'column',
              paddingLeft: isRow ? 0 : 2,
              children: [
                Text({ bold: card.is_self, wrap: 'truncate', children: [card.headline] }),
                ...(card.subtitle ? [Text({ dimColor: true, wrap: 'truncate', children: [card.subtitle] })] : []),
                ...(card.progress ? [Text({ dimColor: true, wrap: 'truncate', children: [card.progress] })] : []),
              ],
            }),
          ),
        ],
      }),
    )

    const board = Box({ key: 'board', flexDirection: direction, columnGap, children: columns })
    const asks = askBlocks($, s, els, view.columns.flatMap(c => c.cards))

    return asks.length === 0 ? board : Box({ flexDirection: 'column', children: [board, ...asks] })
  })
}
