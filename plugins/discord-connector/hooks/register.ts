import type { EngineInterface, On } from 'claude-code'

import {
  ASK_TIMEOUT_SECONDS,
  NO_ANSWER,
  formatAskCreateFailure,
  formatPermissionDenied,
  formatLogLine,
  interpretAskWait,
  LOG_RELATIVE_PATH,
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

async function writeLog($: EngineInterface, message: string): Promise<void> {
  try {
    const home = await $.env.get('HOME')

    if (!home) {
      return
    }

    const path = `${home}/${LOG_RELATIVE_PATH}`
    const prior = (await $.fs.exists(path)) ? String(await $.fs.read(path)) : ''

    await $.fs.write(path, prior + formatLogLine(await $.clock.now(), message))
  } catch {
    // 로그 실패가 원래 흐름을 막으면 안 된다
  }
}

async function readContext($: EngineInterface): Promise<RunContext | undefined> {
  const markers = {
    DISCORD_CONNECTOR_THREAD_ID: await $.env.get('DISCORD_CONNECTOR_THREAD_ID'),
    DISCORD_CONNECTOR_REQUESTER_ID: await $.env.get('DISCORD_CONNECTOR_REQUESTER_ID'),
    DISCORD_CONNECTOR_BOT_ID: await $.env.get('DISCORD_CONNECTOR_BOT_ID'),
  }
  const missing = Object.entries(markers)
    .filter(([, value]) => !value)
    .map(([name]) => name)

  if (missing.length === Object.keys(markers).length) {
    return undefined
  }

  if (missing.length > 0) {
    await writeLog($, `표식 env 계약 위반 — 없는 값: ${missing.join(', ')}. 개입하지 않아요`)

    return undefined
  }

  return {
    threadId: markers.DISCORD_CONNECTOR_THREAD_ID as string,
    requesterId: markers.DISCORD_CONNECTOR_REQUESTER_ID as string,
    botId: markers.DISCORD_CONNECTOR_BOT_ID as string,
  }
}

async function notify($: EngineInterface, ctx: RunContext, text: string): Promise<void> {
  try {
    const sent = await $.process.run(sendArgv(ctx.threadId), {
      stdin: neutralizeBotMention(text, ctx.botId),
      timeoutMs: CLI_TIMEOUT_MS,
    })

    if (sent.exitCode !== 0) {
      await writeLog($, `알림 전송 실패 (exit ${sent.exitCode}): ${(sent.stderr || sent.stdout).slice(-300)}`)
    }
  } catch (err) {
    await writeLog($, `알림 전송 예외: ${String(err)}`)
  }
}

async function askOne($: EngineInterface, ctx: RunContext, q: Question): Promise<string> {
  const deadlineEpochSec = Math.floor((await $.clock.now()) / 1000) + ASK_TIMEOUT_SECONDS
  const plan = planAskCreate(q, ctx, deadlineEpochSec)

  if ('error' in plan) {
    await writeLog($, `질문을 만들 수 없어요: ${plan.error}`)
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
    await writeLog($, `ask create 실패: ${failure}`)
    await notify($, ctx, formatAskCreateFailure(failure))

    return NO_ANSWER
  }

  try {
    const waited = await $.process.run(waitArgv(askId), { timeoutMs: WAIT_TIMEOUT_MS })

    if (waited.exitCode !== 0) {
      await writeLog($, `ask wait 비정상 종료 (exit ${waited.exitCode}): ${(waited.stderr || waited.stdout).slice(-300)}`)

      return NO_ANSWER
    }

    const outcome = interpretAskWait(waited.stdout)

    if ('unanswered' in outcome) {
      await writeLog($, `ask wait 미응답: ${outcome.unanswered}`)

      return NO_ANSWER
    }

    return outcome.answer
  } catch (err) {
    await writeLog($, `ask wait 예외: ${String(err)}`)

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
