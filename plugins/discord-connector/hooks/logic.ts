export const NO_ANSWER = 'Discord에서 시간 안에 답이 없었어요'

export const ASK_TIMEOUT_SECONDS = 540

export type RunContext = {
  threadId: string
  requesterId: string
  botId: string
}

export type Question = {
  question: string
  options: readonly { label: string }[]
  multiSelect: boolean
}

const LIMITS = {
  single: { maxOptions: 4, maxLabel: 80 },
  multi: { maxOptions: 25, maxLabel: 100 },
} as const

export function neutralizeBotMention(text: string, botId: string): string {
  return text.split(`<@${botId}>`).join(`@${botId}`).split(`<@!${botId}>`).join(`@${botId}`)
}

export function sendArgv(threadId: string): string[] {
  return ['discord', '--json', 'send', threadId, '-', '--split']
}

export function waitArgv(askId: string): string[] {
  return ['discord', '--json', 'ask', 'wait', askId, '--timeout', String(ASK_TIMEOUT_SECONDS)]
}

export type AskCreatePlan = { argv: string[] } | { error: string }

export function planAskCreate(q: Question, ctx: RunContext, deadlineEpochSec: number): AskCreatePlan {
  const limit = q.multiSelect ? LIMITS.multi : LIMITS.single
  const kind = q.multiSelect ? '다중 선택' : '단일 선택'

  if (q.options.length < 1 || q.options.length > limit.maxOptions) {
    return { error: `${kind} 질문의 선택지는 1~${limit.maxOptions}개여야 해요 (받은 개수: ${q.options.length})` }
  }

  const tooLong = q.options.find(o => o.label.length > limit.maxLabel)

  if (tooLong !== undefined) {
    return { error: `${kind} 선택지 라벨은 ${limit.maxLabel}자 이하여야 해요: ${tooLong.label.slice(0, 30)}…` }
  }

  const body = `${neutralizeBotMention(q.question, ctx.botId)}\n마감 <t:${deadlineEpochSec}:t>`
  const argv = ['discord', '--json', 'ask', 'create', ctx.threadId, body]

  for (const o of q.options) {
    argv.push('--option', neutralizeBotMention(o.label, ctx.botId))
  }

  argv.push('--timeout', String(ASK_TIMEOUT_SECONDS), '--allowed-user', ctx.requesterId)

  if (q.multiSelect) {
    argv.push('--multi-select')
  }

  return { argv }
}

function envelopeData(stdout: string): Record<string, unknown> | undefined {
  let parsed: unknown

  try {
    parsed = JSON.parse(stdout)
  } catch {
    return undefined
  }

  if (typeof parsed !== 'object' || parsed === null || (parsed as { ok?: unknown }).ok !== true) {
    return undefined
  }

  const data = (parsed as { data?: unknown }).data

  return typeof data === 'object' && data !== null ? (data as Record<string, unknown>) : undefined
}

export function parseAskId(stdout: string): string | undefined {
  const id = envelopeData(stdout)?.ask_id

  return typeof id === 'string' && id !== '' ? id : undefined
}

export type AskWaitOutcome = { answer: string } | { unanswered: string }

export function interpretAskWait(stdout: string): AskWaitOutcome {
  const data = envelopeData(stdout)

  if (data === undefined) {
    return { unanswered: `응답이 성공 엔벨로프가 아니에요: ${stdout.slice(-300)}` }
  }

  if (data.status !== 'answered') {
    return { unanswered: `상태가 answered 가 아니에요: ${String(data.status)}` }
  }

  const value = data.value

  if (typeof value === 'string') {
    return { answer: value }
  }

  if (Array.isArray(value) && value.every(v => typeof v === 'string')) {
    return { answer: value.join(', ') }
  }

  return { unanswered: `answered 인데 value 형식이 달라요: ${JSON.stringify(value)}` }
}

export const LOG_RELATIVE_PATH = '.areum/discord-connector/logs/hook.log'

export function formatLogLine(epochMs: number, message: string): string {
  return `${new Date(epochMs).toISOString()} ${message}\n`
}

export function formatAskCreateFailure(reason: string): string {
  return `질문을 Discord 에 올리지 못했어요: ${reason}\n질문은 답 없이 넘어가요.`
}

export function formatPermissionDenied(toolName: string, reason: string): string {
  return [
    '자동 모드가 도구 실행을 거부했어요.',
    `- 도구: ${toolName}`,
    `- 사유: ${reason}`,
    '다시 진행하려면 이 스레드에서 봇을 멘션해 재지시해 주세요.',
  ].join('\n')
}
