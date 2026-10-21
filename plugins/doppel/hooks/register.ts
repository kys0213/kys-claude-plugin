import type { EngineInterface, On } from 'claude-code'

import {
  ALLOW,
  DENY,
  HEARTBEAT_MS,
  REMOTE_DENY,
  WAIT_PROCESS_TIMEOUT_MS,
  WAIT_SECONDS,
  answersOf,
  askBody,
  closeBody,
  deferAfterMs,
  deferredAnswerPrompt,
  deferredAnswers,
  deniedOf,
  eventBody,
  interceptsQuestions,
  nextBackoff,
  parseAsk,
  parseAttachLine,
  parseWait,
  permissionChoice,
  permissionQuestion,
  socketPath,
  splitLines,
  summarize,
  toolArgs,
  waitArgv,
  type Answers,
  type Ask,
  type AskKind,
  type AttachMessage,
  type Question,
  type SessionEvent,
} from './logic'
import { Subject, filter, map, merge, scan, type Unsubscribe } from './reactive'

type Context = { sessionId: string; socket: string; intercepts: boolean }

type Remote = { kind: 'answered'; answers: Answers; answeredBy: string } | { kind: 'deferred' }

const NEVER: Promise<never> = new Promise(() => {})

const lifecycle = new Subject<SessionEvent>()
const turns = new Subject<SessionEvent>()
const heartbeats = new Subject<SessionEvent>()

let stopOutbound: Unsubscribe | undefined

const allowedByRemote = new Set<string>()

let headless = false
let ended = false
let heartbeat: { cancel: () => void } | undefined

async function context($: EngineInterface): Promise<Context | undefined> {
  const [home, override, thread, requester, bot] = await Promise.all([
    $.env.get('HOME'),
    $.env.get('DOPPEL_SOCKET'),
    $.env.get('DISCORD_CONNECTOR_THREAD_ID'),
    $.env.get('DISCORD_CONNECTOR_REQUESTER_ID'),
    $.env.get('DISCORD_CONNECTOR_BOT_ID'),
  ])
  const socket = socketPath(home, override)

  if (socket === undefined) {
    return undefined
  }

  return { sessionId: await $.session.id(), socket, intercepts: interceptsQuestions([thread, requester, bot]) }
}

async function request(
  $: EngineInterface,
  ctx: Context,
  method: 'GET' | 'POST',
  path: string,
  body?: string,
): Promise<string | undefined> {
  try {
    const res = await $.http.fetch(`http://doppel${path}`, {
      method,
      headers: { 'content-type': 'application/json', 'x-doppel-client': 'plugin' },
      body,
      socketPath: ctx.socket,
    })

    return res.ok ? res.text : undefined
  } catch {
    return undefined
  }
}

async function emit($: EngineInterface, event: SessionEvent): Promise<void> {
  try {
    const ctx = await context($)

    if (ctx !== undefined) {
      await request($, ctx, 'POST', '/v1/events', eventBody(ctx.sessionId, await $.clock.now(), event))
    }
  } catch {
    return
  }
}

async function openAsk(
  $: EngineInterface,
  ctx: Context,
  toolUseId: string,
  kind: AskKind,
  questions: readonly Question[],
): Promise<Ask | undefined> {
  const text = await request($, ctx, 'POST', '/v1/asks', askBody(ctx.sessionId, toolUseId, kind, questions))

  return text === undefined ? undefined : parseAsk(text)
}

async function closeAsk($: EngineInterface, ctx: Context, askId: string, answers: Answers | undefined, by: string) {
  await request($, ctx, 'POST', `/v1/asks/${askId}/close`, closeBody(answers, by))
}

/**
 * Resolves with doppel's answer, or `deferred` once `deferAt` passes. A wait that
 * fails, or an ask someone else closed, never resolves: the session's own answer
 * is then the only one left in the race.
 */
async function awaitRemote($: EngineInterface, ask: Ask, deferAt?: number): Promise<Remote> {
  for (;;) {
    let seconds = WAIT_SECONDS

    if (deferAt !== undefined) {
      const left = deferAt - (await $.clock.now())

      if (left <= 0) {
        return { kind: 'deferred' }
      }

      seconds = Math.min(seconds, Math.ceil(left / 1000))
    }

    let current: Ask | undefined

    try {
      const run = await $.process.run(waitArgv(ask.ask_id, seconds), { timeoutMs: WAIT_PROCESS_TIMEOUT_MS })

      current = run.exitCode === 0 ? parseWait(run.stdout) : undefined
    } catch {
      current = undefined
    }

    if (current === undefined || current.state === 'closed') {
      return NEVER
    }

    if (current.state === 'answered' && current.answers !== null) {
      return { kind: 'answered', answers: current.answers, answeredBy: current.answered_by ?? 'doppel-ui' }
    }
  }
}

async function handleInbound($: EngineInterface, ctx: Context, message: AttachMessage): Promise<void> {
  try {
    switch (message.kind) {
      case 'answer':
        await $.prompt.submit({ text: deferredAnswerPrompt(message.answers) })
        await closeAsk($, ctx, message.ask_id, message.answers, message.answered_by ?? 'doppel-ui')
        break
      case 'prompt':
        await $.prompt.submit({ text: message.text })
        break
      case 'context':
        await $.session.append({ message: { type: 'user', content: [{ type: 'text', text: message.text }] } })
        break
    }
  } catch {
    return
  }
}

function startAttach($: EngineInterface, ctx: Context, backoffMs?: number): void {
  void (async () => {
    const chunks = new Subject<string>()
    const lines = new Subject<string>()
    let received = false
    const stopLines = scan(chunks, splitLines, { rest: '', lines: [] }).subscribe(buffer => {
      for (const line of buffer.lines) {
        lines.next(line)
      }
    })
    const stopMessages = filter(map(lines, parseAttachLine), isMessage).subscribe(message => {
      void handleInbound($, ctx, message)
    })

    try {
      for await (const { stream, text } of $.process.spawn({ argv: ['doppel', 'attach', '--session', ctx.sessionId] })) {
        if (stream === 'stdout') {
          received = true
          chunks.next(text)
        }
      }
    } catch {
      /** `doppel` 이 없거나 연결이 끊겼다 — 아래에서 다시 띄운다 */
    } finally {
      stopLines()
      stopMessages()
    }

    if (!ended) {
      const delay = nextBackoff(received ? undefined : backoffMs)

      $.clock.after(delay, () => startAttach($, ctx, delay))
    }
  })()
}

export function register(on: On): void {
  on('session.start', async ($, e, next) => {
    const started = await next(e)

    try {
      headless = !e.isInteractive
      ended = false

      const ctx = await context($)

      if (ctx === undefined) {
        return started
      }

      const launchedBy = (await $.env.get('DOPPEL_LAUNCHED')) === '1' ? 'doppel' : 'user'

      stopOutbound?.()
      stopOutbound = merge(lifecycle, turns, heartbeats).subscribe(event => {
        void emit($, event)
      })
      lifecycle.next({
        kind: 'session.start',
        fields: { cwd: e.cwd, surface: headless ? 'headless' : 'terminal', launched_by: launchedBy },
      })
      heartbeat?.cancel()
      heartbeat = $.clock.every(HEARTBEAT_MS, () => heartbeats.next({ kind: 'heartbeat' }))

      if (ctx.intercepts) {
        startAttach($, ctx)
      }
    } catch {
      return started
    }

    return started
  })

  on('session.end', async ($, e, next) => {
    ended = true
    heartbeat?.cancel()
    heartbeat = undefined
    stopOutbound?.()
    stopOutbound = undefined
    await emit($, { kind: 'session.end', fields: { reason: e.reason } })

    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    const started = await next(e)

    if (e.agentId === undefined) {
      turns.next({ kind: 'turn.start' })
    }

    return started
  })

  on('turn.complete', async ($, e, next) => {
    const completed = await next(e)

    if (e.agentId === undefined) {
      turns.next({ kind: 'turn.complete', fields: { summary: summarize(e.answer) } })
    }

    return completed
  })

  on('classic.PostToolUseFailure', async ($, e, next) => {
    turns.next({ kind: 'tool.failure', fields: { tool: e.tool_name } })

    return next(e)
  })

  on('tool.call', { tool: 'AskUserQuestion' }, async ($, e, next) => {
    const ctx = await context($).catch(() => undefined)

    if (ctx === undefined || !ctx.intercepts || e.tool_use_id === undefined) {
      return next(e)
    }

    const questions = e.questions as readonly Question[]
    const ask = await openAsk($, ctx, e.tool_use_id, 'ask', questions)

    if (ask === undefined) {
      return next(e)
    }

    const deferAt = headless
      ? (await $.clock.now()) + deferAfterMs(await $.env.get('DOPPEL_ASK_DEFER_AFTER_MS'))
      : undefined
    const winner = await Promise.race([
      next(e).then(result => ({ source: 'session' as const, result })),
      awaitRemote($, ask, deferAt).then(remote => ({ source: 'remote' as const, remote })),
    ])

    if (winner.source === 'session') {
      await closeAsk($, ctx, ask.ask_id, answersOf(winner.result), 'session')

      return winner.result
    }

    if (winner.remote.kind === 'deferred') {
      await request($, ctx, 'POST', `/v1/asks/${ask.ask_id}/defer`)

      return { result: { questions, answers: deferredAnswers(questions) } } as never
    }

    await closeAsk($, ctx, ask.ask_id, winner.remote.answers, winner.remote.answeredBy)

    return { result: { questions, answers: winner.remote.answers } } as never
  })

  on('tool.check', async ($, e, next) => {
    if (e.tool_use_id !== undefined && allowedByRemote.has(e.tool_use_id)) {
      return { decision: 'allow', reason: '사용자가 doppel 에서 허용했어요' }
    }

    return next(e)
  })

  on('tool.call', async ($, e, next) => {
    const toolUseId = e.tool_use_id

    if (e.tool === 'AskUserQuestion' || toolUseId === undefined) {
      return next(e)
    }

    const ctx = await context($).catch(() => undefined)

    if (ctx === undefined || !ctx.intercepts) {
      return next(e)
    }

    const input = toolArgs(e as Record<string, unknown>)
    const verdict = await $.tool.check({ tool: e.tool, input }).catch(() => undefined)

    if (verdict?.decision !== 'ask') {
      return next(e)
    }

    const question = permissionQuestion(e.tool, input)
    const ask = await openAsk($, ctx, toolUseId, 'permission', [question])

    if (ask === undefined) {
      return next(e)
    }

    const local = next(e)
    const winner = await Promise.race([
      local.then(result => ({ source: 'session' as const, result })),
      awaitRemote($, ask).then(remote => ({ source: 'remote' as const, remote })),
    ])

    if (winner.source === 'session') {
      const choice = deniedOf(winner.result) ? DENY : ALLOW

      await closeAsk($, ctx, ask.ask_id, { [question.question]: choice }, 'session')

      return winner.result
    }

    const answers = winner.remote.kind === 'answered' ? winner.remote.answers : null
    const choice = permissionChoice(answers, question)

    if (choice === DENY) {
      await closeAsk($, ctx, ask.ask_id, { [question.question]: DENY }, by(winner.remote))

      return { deny: REMOTE_DENY }
    }

    if (choice === ALLOW) {
      allowedByRemote.add(toolUseId)

      try {
        const result = await next(e)

        await closeAsk($, ctx, ask.ask_id, { [question.question]: ALLOW }, by(winner.remote))

        return result
      } finally {
        allowedByRemote.delete(toolUseId)
      }
    }

    return local
  })
}

function isMessage(message: AttachMessage | undefined): message is AttachMessage {
  return message !== undefined
}

function by(remote: Remote): string {
  return remote.kind === 'answered' ? remote.answeredBy : 'doppel-ui'
}
