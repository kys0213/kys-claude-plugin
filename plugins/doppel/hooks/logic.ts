export const WAIT_SECONDS = 590

export const WAIT_PROCESS_TIMEOUT_MS = 600_000

export const DEFAULT_DEFER_AFTER_MS = 120_000

export const HEARTBEAT_MS = 30_000

/**
 * `$.tool.check` queries skip the auto-mode classifier, so a call the classifier is
 * about to allow still reads as `ask`. Relaying only after this long keeps those
 * calls out of the inbox.
 */
export const PERMISSION_GRACE_MS = 1_500

export const ATTACH_BACKOFF_MS = { first: 1_000, max: 30_000 } as const

export const SUMMARY_CHARS = 300

const INPUT_SUMMARY_CHARS = 500

export const DEFERRED_ANSWER = '사용자 판단 보류 중 — doppel 이 답을 받으면 이어서 알려줍니다'

export const REMOTE_DENY = '사용자가 doppel 에서 실행을 거부했어요'

export const ALLOW = 'allow'

export const DENY = 'deny'

export type Question = {
  question: string
  header?: string
  multiSelect?: boolean
  options: readonly { label: string; description?: string }[]
}

export type Answers = Record<string, string | string[]>

export type AskKind = 'ask' | 'permission'

export type AskState = 'open' | 'answered' | 'deferred' | 'closed'

export type Ask = {
  ask_id: string
  state: AskState
  answers: Answers | null
  answered_by: string | null
}

export type AttachMessage =
  | { kind: 'answer'; ask_id: string; tool_use_id: string; answers: Answers; answered_by?: string }
  | { kind: 'prompt'; text: string }
  | { kind: 'context'; text: string }

export type SessionEventKind =
  | 'session.start'
  | 'session.end'
  | 'heartbeat'
  | 'turn.start'
  | 'turn.complete'
  | 'tool.failure'

export type SessionEvent = { kind: SessionEventKind; fields?: Record<string, unknown> }

export function socketPath(home: string | undefined, override: string | undefined): string | undefined {
  if (override) {
    return override
  }

  return home ? `${home}/.areum/doppel/doppel.sock` : undefined
}

export function interceptsQuestions(discordMarkers: readonly (string | undefined)[]): boolean {
  return discordMarkers.every(value => !value)
}

export function deferAfterMs(raw: string | undefined): number {
  const parsed = raw === undefined ? NaN : Number(raw)

  return Number.isFinite(parsed) && parsed >= 0 ? parsed : DEFAULT_DEFER_AFTER_MS
}

export function eventBody(sessionId: string, atMs: number, event: SessionEvent): string {
  return JSON.stringify({
    v: 1,
    session_id: sessionId,
    at: new Date(atMs).toISOString(),
    kind: event.kind,
    ...event.fields,
  })
}

export function summarize(text: string): string {
  return text.slice(0, SUMMARY_CHARS)
}

export function askBody(sessionId: string, toolUseId: string, kind: AskKind, questions: readonly Question[]): string {
  return JSON.stringify({ session_id: sessionId, tool_use_id: toolUseId, kind, questions })
}

export function closeBody(answers: Answers | undefined, answeredBy: string): string {
  return JSON.stringify(answers === undefined ? { answered_by: answeredBy } : { answers, answered_by: answeredBy })
}

export function waitArgv(askId: string, seconds: number): string[] {
  return ['doppel', 'ask', 'wait', askId, '--timeout', String(seconds), '--json']
}

function isAsk(value: unknown): value is Ask {
  if (typeof value !== 'object' || value === null) {
    return false
  }

  const ask = value as Record<string, unknown>

  return typeof ask.ask_id === 'string' && typeof ask.state === 'string'
}

/** `POST /v1/asks` and the other ask endpoints answer the Ask object unwrapped. */
export function parseAsk(text: string): Ask | undefined {
  try {
    const parsed: unknown = JSON.parse(text)

    return isAsk(parsed) ? parsed : undefined
  } catch {
    return undefined
  }
}

/** `doppel ask wait --json` wraps the Ask in the CLI envelope. */
export function parseWait(stdout: string): Ask | undefined {
  try {
    const parsed = JSON.parse(stdout) as { ok?: unknown; data?: unknown }

    return parsed.ok === true && isAsk(parsed.data) ? parsed.data : undefined
  } catch {
    return undefined
  }
}

export function isAnswers(value: unknown): value is Answers {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return false
  }

  return Object.values(value).every(
    v => typeof v === 'string' || (Array.isArray(v) && v.every(item => typeof item === 'string')),
  )
}

/** Core answers `AskUserQuestion` as `{ result: { questions, answers } }`. */
export function answersOf(result: unknown): Answers | undefined {
  const answers = (result as { result?: { answers?: unknown } } | undefined)?.result?.answers

  return isAnswers(answers) ? answers : undefined
}

export function deniedOf(result: unknown): boolean {
  return typeof (result as { deny?: unknown } | undefined)?.deny === 'string'
}

export function deferredAnswers(questions: readonly Question[]): Answers {
  return Object.fromEntries(questions.map(q => [q.question, DEFERRED_ANSWER]))
}

const RESERVED_CALL_KEYS = new Set(['tool', 'tool_use_id', 'consent', 'agentId', 'requestMeta'])

/** A `tool.call` event carries the tool's own arguments beside the reserved keys. */
export function toolArgs(e: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(e).filter(([key]) => !RESERVED_CALL_KEYS.has(key)))
}

function inputSummary(tool: string, input: Record<string, unknown>): string {
  const field =
    tool === 'Bash'
      ? input.command
      : tool === 'NotebookEdit'
        ? input.notebook_path
        : ['Edit', 'Write', 'Read', 'MultiEdit'].includes(tool)
          ? input.file_path
          : undefined
  const summary = typeof field === 'string' ? field : JSON.stringify(input)

  return summary.slice(0, INPUT_SUMMARY_CHARS)
}

export function permissionQuestion(tool: string, input: Record<string, unknown>): Question {
  return {
    question: `${tool} 실행 허용: ${inputSummary(tool, input)}`,
    header: '권한',
    multiSelect: false,
    options: [
      { label: ALLOW, description: '이번 호출을 실행해요' },
      { label: DENY, description: '이번 호출을 막아요' },
    ],
  }
}

export function permissionChoice(answers: Answers | null, question: Question): string | undefined {
  const choice = answers?.[question.question]
  const first = Array.isArray(choice) ? choice[0] : choice

  return first === ALLOW || first === DENY ? first : undefined
}

function isAttachMessage(value: unknown): value is AttachMessage {
  if (typeof value !== 'object' || value === null) {
    return false
  }

  const m = value as Record<string, unknown>

  switch (m.kind) {
    case 'answer':
      return typeof m.ask_id === 'string' && typeof m.tool_use_id === 'string' && isAnswers(m.answers)
    case 'prompt':
    case 'context':
      return typeof m.text === 'string'
    default:
      return false
  }
}

export function parseAttachLine(line: string): AttachMessage | undefined {
  try {
    const parsed: unknown = JSON.parse(line)

    return isAttachMessage(parsed) ? parsed : undefined
  } catch {
    return undefined
  }
}

export type LineBuffer = { rest: string; lines: string[] }

/** `$.process.spawn` hands stdout over in pieces as they came, not in lines. */
export function splitLines(buffer: LineBuffer, chunk: string): LineBuffer {
  const parts = (buffer.rest + chunk).split('\n')
  const rest = parts.pop() ?? ''

  return { rest, lines: parts.filter(line => line.trim() !== '') }
}

export function deferredAnswerPrompt(answers: Answers): string {
  const lines = Object.entries(answers).map(
    ([question, answer]) => `- ${question}: ${Array.isArray(answer) ? answer.join(', ') : answer}`,
  )

  return ['앞서 보류했던 질문에 doppel 에서 답이 왔어요. 이 답으로 이어서 진행해 주세요.', ...lines].join('\n')
}

export function nextBackoff(previousMs: number | undefined): number {
  return previousMs === undefined
    ? ATTACH_BACKOFF_MS.first
    : Math.min(previousMs * 2, ATTACH_BACKOFF_MS.max)
}
