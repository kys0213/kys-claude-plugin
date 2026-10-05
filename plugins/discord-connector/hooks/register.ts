import type { EngineInterface, On } from 'claude-code'

import {
  ASK_TIMEOUT_SECONDS,
  NO_ANSWER,
  formatAskCreateFailure,
  formatPermissionDenied,
  interpretAskWait,
  neutralizeBotMention,
  parseAskId,
  planAskCreate,
  sendArgv,
  waitArgv,
  type Question,
  type RunContext,
} from './logic'

const CLI_TIMEOUT_MS = 30_000
const WAIT_TIMEOUT_MS = 600_000

async function readContext($: EngineInterface): Promise<RunContext | undefined> {
  const threadId = await $.env.get('DISCORD_CONNECTOR_THREAD_ID')
  const requesterId = await $.env.get('DISCORD_CONNECTOR_REQUESTER_ID')
  const botId = await $.env.get('DISCORD_CONNECTOR_BOT_ID')

  if (!threadId || !requesterId || !botId) {
    return undefined
  }

  return { threadId, requesterId, botId }
}

async function notify($: EngineInterface, ctx: RunContext, text: string): Promise<void> {
  try {
    await $.process.run(sendArgv(ctx.threadId), {
      stdin: neutralizeBotMention(text, ctx.botId),
      timeoutMs: CLI_TIMEOUT_MS,
    })
  } catch {
    // 알림 실패가 실행을 막으면 안 된다
  }
}

async function askOne($: EngineInterface, ctx: RunContext, q: Question): Promise<string> {
  const deadlineEpochSec = Math.floor((await $.clock.now()) / 1000) + ASK_TIMEOUT_SECONDS
  const plan = planAskCreate(q, ctx, deadlineEpochSec)

  if ('error' in plan) {
    await notify($, ctx, formatAskCreateFailure(plan.error))

    return NO_ANSWER
  }

  let askId: string | undefined
  let failure = ''

  try {
    const created = await $.process.run(plan.argv, { timeoutMs: CLI_TIMEOUT_MS })

    askId = created.exitCode === 0 ? parseAskId(created.stdout) : undefined

    if (askId === undefined) {
      failure = `exit ${created.exitCode}: ${(created.stdout || created.stderr).slice(-300)}`
    }
  } catch (err) {
    failure = String(err)
  }

  if (askId === undefined) {
    await notify($, ctx, formatAskCreateFailure(failure))

    return NO_ANSWER
  }

  try {
    const waited = await $.process.run(waitArgv(askId), { timeoutMs: WAIT_TIMEOUT_MS })

    return waited.exitCode === 0 ? interpretAskWait(waited.stdout) : NO_ANSWER
  } catch {
    return NO_ANSWER
  }
}

export function register(on: On): void {
  on('tool.call', { tool: 'AskUserQuestion' }, async ($, e, next) => {
    const ctx = await readContext($)

    if (ctx === undefined) {
      return next(e)
    }

    const answers: Record<string, string> = {}

    for (const q of e.questions) {
      answers[q.question] = await askOne($, ctx, q)
    }

    return { result: { questions: e.questions, answers } } as never
  })

  on('classic.PermissionDenied', async ($, e, next) => {
    const ctx = await readContext($)

    if (ctx !== undefined) {
      await notify($, ctx, formatPermissionDenied(e.tool_name, e.reason))
    }

    return next(e)
  })
}
