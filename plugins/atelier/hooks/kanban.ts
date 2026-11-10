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
import { type DetailCard, type DetailState, detailBlock, newDetailState, pruneDetail, toggleSelected } from './kanban-detail'
import { CLI_MISSING, cli, runKanban } from './kanban-cli'
import { type InboxState, awaitPermit, deliverRequests, endPermit, findInbox } from './kanban-inbox'
import { type Activity, KEY_PREFIX, MAX_FIELD_CHARS, type SessionRecord, record, recordPrompt } from './kanban-record'

const PANE = 'kanban'
const BOARD_TOOL = 'kanban_board'
const HEARTBEAT_MS = 30_000
const REFRESH_MS = 2_000
const STALE_AFTER = '2m'
const EXPIRE_AFTER = '24h'
const MAX_REPLY_CHARS = 1_000
const MAX_TODOS = 50
const MAX_RECENT = 5
/** Keys a `tool.call` input carries beside the tool's own arguments. */
const RESERVED_INPUT_KEYS = ['tool', 'tool_use_id', 'agentId', 'requestMeta', 'consent']
/** Where a prompt a person typed comes from; the rest (task notifications, triggers, plugins) are not theirs. */
const PERSON_ORIGINS = ['composer', 'bridge', 'sdk']

type Card = AskCard & DetailCard & { subtitle: string | null; progress?: string | null; last_line?: string | null; is_self: boolean }

type Column = { id: string; heading: string; cards: Card[] }

type Layout = { direction: 'row' | 'column'; column_width: number; column_gap: number }

type BoardView =
  | { kind: 'loading' }
  | { kind: 'board'; columns: Column[]; layout: Layout; width: number }
  | { kind: 'error'; message: string }

type KanbanState = InboxState &
  AskState & {
    detail: DetailState
    view: BoardView
    timersStarted: boolean
    /** The pane width the last drawing had; the board is asked for that width. */
    width: number
    refreshing: boolean
  }

function stringFields(input: unknown): Record<string, string> {
  if (typeof input !== 'object' || input === null) {
    return {}
  }

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

async function runBoard($: EngineInterface, sessions: unknown[], width: number) {
  const args = [
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

  return runKanban($, args, JSON.stringify({ sessions }))
}

async function dropExpired($: EngineInterface, keys: string[], ids: string[]): Promise<void> {
  for (const id of ids) {
    for (const key of [KEY_PREFIX + id, ...answerKeysOf(keys, id)]) {
      await $.store.delete(key)
    }

    await cli($, ['inbox', 'clear', '--session', id])
  }
}

async function refresh($: EngineInterface, s: KanbanState): Promise<void> {
  if (s.refreshing) {
    return
  }

  s.refreshing = true
  const width = s.width

  try {
    const { keys, sessions } = await snapshot($)
    const r = await runBoard($, sessions, width)

    if (r.kind === 'exit') {
      s.view = { kind: 'error', message: `atelier kanban board 가 exit ${r.code} 로 끝났어요` }
    } else if (r.kind === 'missing') {
      s.view = { kind: 'error', message: CLI_MISSING }
    } else {
      const parsed = r.kind === 'json' ? r.value : null

      s.view = decodeBoard(parsed, width)

      if (s.view.kind === 'board') {
        const cards = s.view.columns.flatMap(c => c.cards)

        pruneDrafts(s, cards)
        pruneDetail(s.detail, cards)
      }

      await dropExpired($, keys, expiredIds(parsed))
    }
  } catch {
    s.view = { kind: 'error', message: CLI_MISSING }
  } finally {
    s.refreshing = false
  }

  $.ui.invalidate('ui.render')
}

/** The board as the orchestrator reads it: repositories and their sessions, no cards or layout. */
async function boardForTool($: EngineInterface): Promise<string> {
  let r: Awaited<ReturnType<typeof runBoard>>

  try {
    r = await runBoard($, (await snapshot($)).sessions, 0)
  } catch {
    return JSON.stringify({ error: CLI_MISSING })
  }

  if (r.kind === 'missing') {
    return JSON.stringify({ error: CLI_MISSING })
  }

  const parsed = r.kind === 'json' ? r.value : null

  if (parsed === null || typeof parsed.error === 'string' || !Array.isArray(parsed.repos)) {
    return JSON.stringify({ error: typeof parsed?.error === 'string' ? parsed.error : 'atelier kanban board 를 읽지 못했어요' })
  }

  return JSON.stringify({ repos: parsed.repos })
}

async function isPaneShown($: EngineInterface): Promise<boolean> {
  return (await $.ui.panes()).some(p => p.id === PANE && p.isShown)
}

async function tick($: EngineInterface, s: KanbanState): Promise<void> {
  try {
    await deliverAnswers($, s)
    await deliverRequests($, s)

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

/** The reply the model wrote last, read when the session starts to wait mid-turn. */
async function lastReply($: EngineInterface): Promise<string | null> {
  try {
    const messages = await $.session.messages()
    const reply = [...messages].reverse().find(m => m.role === 'assistant' && m.text.trim() !== '')

    return reply === undefined ? null : reply.text.slice(-MAX_REPLY_CHARS)
  } catch {
    return null
  }
}

function modeOf(e: { permission_mode?: string }): Partial<SessionRecord> {
  return typeof e.permission_mode === 'string' ? { permission_mode: e.permission_mode } : {}
}

async function startSession($: EngineInterface, s: KanbanState): Promise<void> {
  try {
    await $.command.register({ name: 'kanban', description: '로컬 Claude 세션 칸반 pane 을 열고 닫아요' })
  } catch {
    /** the pane is still reachable once a later reload registers it */
  }

  try {
    await $.tool.register({
      name: BOARD_TOOL,
      description:
        'Reads the session kanban of this machine: every Claude Code session with atelier, grouped by repository root, with each session’s column (needs_input, working, idle, stale), cwd, permission mode and the actions the board offers (send, stop, answer, decide). Read-only. Send work to an idle session with `atelier kanban send --from agent`, or start one with `atelier kanban spawn`.',
      isDeferred: true,
    })
  } catch {
    /** the board stays readable on the pane */
  }

  const repo = await $.session.repo()

  await record($, s, {
    name: 'session.start',
    onlyIfNew: true,
    update: base => ({ repo_root: base.repo_root ?? repo?.root ?? null, remote: base.remote ?? repo?.remote ?? null }),
  })
  await findInbox($, s)

  if (!s.timersStarted) {
    s.timersStarted = true
    $.clock.every(HEARTBEAT_MS, () => void record($, s, { name: null }))
    $.clock.every(REFRESH_MS, () => void tick($, s))
  }
}

function drawCard($: EngineInterface, s: KanbanState, els: ReturnType<EngineInterface['ui']['resolve']>, card: Card, isRow: boolean) {
  const { Box, Text, Button } = els
  const isOpen = s.detail.selected === card.session_id

  return Box({
    key: card.session_id,
    flexDirection: 'column',
    paddingLeft: isRow ? 0 : 2,
    children: [
      Button({
        key: `card:${card.session_id}`,
        plain: true,
        onPress: () => toggleSelected($, s.detail, card.session_id),
        children: [Text({ bold: card.is_self || isOpen, wrap: 'truncate', children: [`${isOpen ? '▾ ' : ''}${card.headline}`] })],
      }),
      ...(card.subtitle ? [Text({ dimColor: true, wrap: 'truncate', children: [card.subtitle] })] : []),
      ...(card.progress ? [Text({ dimColor: true, wrap: 'truncate', children: [card.progress] })] : []),
      ...(card.last_line ? [Text({ dimColor: true, wrap: 'truncate', children: [card.last_line] })] : []),
    ],
  })
}

export function registerKanban(on: On): void {
  const s: KanbanState = {
    ...newAskState(),
    detail: newDetailState(),
    view: { kind: 'loading' },
    writes: Promise.resolve(),
    timersStarted: false,
    spinnerArmed: false,
    width: 0,
    refreshing: false,
    inboxDir: null,
    turnId: null,
    taking: false,
    permitting: null,
  }

  on('session.start', async ($, e, next) => {
    await startSession($, s)

    return next(e)
  })

  on('command.run', { command: 'kanban' }, async $ => ({ text: await toggle($, s) }))

  on('tool.call', { tool: 'mcp__atelier__kanban_board' }, async $ => ({ result: await boardForTool($) }))

  on('prompt.submit', async ($, e, next) => {
    if (e.command === undefined && e.isMeta !== true) {
      const isPerson = e.origin === undefined || PERSON_ORIGINS.includes(e.origin.kind)

      await recordPrompt($, s, isPerson ? e.text : null)
    }

    return next(e)
  })

  on('classic.UserPromptSubmit', async ($, e, next) => {
    const title = typeof e.session_title === 'string' ? { title: e.session_title } : {}

    await record($, s, { name: null, set: title, update: () => modeOf(e) })

    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    if (e.agentId === undefined) {
      s.turnId = e.turnId
      await record($, s, { name: 'turn.start', update: () => ({ turn_id: e.turnId, recent: [], decision: null }) })
    }

    return next(e)
  })

  on('turn.complete', async ($, e, next) => {
    if (e.agentId === undefined) {
      if (s.turnId === e.turnId) {
        s.turnId = null
      }

      endPermit(s)

      await record($, s, {
        name: 'turn.complete',
        update: base => ({
          activity: null,
          turn_id: null,
          permission: null,
          pending_context: null,
          last_answer: e.answer.trim() === '' ? (base.last_answer ?? null) : e.answer.slice(0, MAX_REPLY_CHARS),
        }),
      })
    }

    return next(e)
  })

  on('tool.call', async ($, e, next) => {
    if (e.agentId === undefined) {
      const input = stringFields(e)

      await record($, s, {
        name: null,
        update: (base, now) => {
          const activity: Activity = { tool: e.tool, input, at: now }

          return { activity, recent: [...(base.recent ?? []), activity].slice(-MAX_RECENT) }
        },
      })
    }

    return next(e)
  })

  on('tool.call', { tool: 'WrapUp' }, async ($, e, next) => {
    const ask = typeof e.ask === 'string' ? e.ask.trim() : ''

    if (e.agentId === undefined && ask !== '') {
      const options = (e.actions ?? [])
        .flatMap(a => ('label' in a && typeof a.label === 'string' ? [a.label.slice(0, MAX_FIELD_CHARS)] : []))
        .slice(0, 4)

      await record($, s, {
        name: null,
        update: (_, now) => ({
          decision: { ask: ask.slice(0, MAX_FIELD_CHARS), recap: e.recap.slice(0, MAX_REPLY_CHARS), options, at: now },
        }),
      })
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
    const context = await lastReply($)
    let answered = false

    await record($, s, {
      name: null,
      update: (base, now) => ({ asks: { ...base.asks, [id]: askOf(e.questions, now) }, pending_context: context }),
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
    const isQuestion = e.tool_name === 'AskUserQuestion'
    const context = isQuestion ? null : await lastReply($)
    const now = await $.clock.now()
    const id = isQuestion || s.inboxDir === null ? null : `${s.turnId ?? 'turn'}-${now}`

    await record($, s, {
      name: 'classic.PermissionRequest',
      detail: e.tool_name,
      update: () => ({
        ...modeOf(e),
        ...(isQuestion
          ? {}
          : { permission: { id, tool: e.tool_name, input: stringFields(e.tool_input), at: now }, pending_context: context }),
      }),
    })

    if (id === null) {
      return next(e)
    }

    const verdict = await awaitPermit($, s, id)

    if (verdict === null) {
      return next(e)
    }

    await record($, s, { name: 'kanban.answer', update: () => ({ permission: null }) })

    return verdict === 'allow'
      ? { decision: { behavior: 'allow' as const } }
      : { decision: { behavior: 'deny' as const, message: '사람이 칸반에서 이 도구 실행을 거부했어요' } }
  })

  on('classic.Notification', async ($, e, next) => {
    await record($, s, { name: 'classic.Notification', detail: e.notification_type })

    return next(e)
  })

  on('classic.PostToolUse', async ($, e, next) => {
    endPermit(s)
    await record($, s, {
      name: 'classic.PostToolUse',
      detail: e.tool_name,
      update: () => ({ ...modeOf(e), permission: null }),
    })

    return next(e)
  })

  on('classic.PostToolUseFailure', async ($, e, next) => {
    endPermit(s)
    await record($, s, { name: 'classic.PostToolUseFailure', detail: e.tool_name, update: () => ({ permission: null }) })

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
    const cards = view.columns.flatMap(c => c.cards)
    const selected = cards.find(c => c.session_id === s.detail.selected)
    const asks = askBlocks($, s, els, cards)

    if (!isRow && selected !== undefined) {
      const detail = detailBlock($, s.detail, els, selected, () => toggleSelected($, s.detail, selected.session_id))

      return Box({ flexDirection: 'column', children: [detail, ...asks] })
    }

    const frame = isRow ? { width, borderStyle: 'round' as const, paddingX: 1 } : { width, marginBottom: 1 }
    const columns = view.columns.map(col =>
      Box({
        key: col.id,
        flexDirection: 'column',
        ...frame,
        children: [
          Text({ bold: true, dimColor: col.cards.length === 0, children: [col.heading] }),
          ...col.cards.map(card => drawCard($, s, els, card, isRow)),
        ],
      }),
    )

    const board = Box({ key: 'board', flexDirection: direction, columnGap, children: columns })
    const detail = selected === undefined ? [] : [detailBlock($, s.detail, els, selected, null)]

    return detail.length === 0 && asks.length === 0
      ? board
      : Box({ flexDirection: 'column', children: [board, ...detail, ...asks] })
  })
}
