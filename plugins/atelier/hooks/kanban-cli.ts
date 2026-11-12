import type { EngineInterface } from 'claude-code'

const TIMEOUT_MS = 5_000

export const CLI_MISSING = 'atelier CLI 를 실행하지 못했어요 — /atelier:update 로 설치·갱신하세요'

export type CliOutcome =
  | { kind: 'json'; value: Record<string, unknown> }
  | { kind: 'unreadable' }
  | { kind: 'exit'; code: number }
  | { kind: 'missing' }

/** Runs `atelier kanban <args>` and reads its one line of JSON. */
export async function runKanban($: EngineInterface, args: string[], stdin?: string): Promise<CliOutcome> {
  let r: Awaited<ReturnType<EngineInterface['process']['run']>>

  try {
    r = await $.process.run(['atelier', 'kanban', ...args], {
      timeoutMs: TIMEOUT_MS,
      ...(stdin === undefined ? {} : { stdin }),
    })
  } catch {
    return { kind: 'missing' }
  }

  if (r.exitCode !== 0) {
    return { kind: 'exit', code: r.exitCode }
  }

  try {
    const parsed: unknown = JSON.parse(r.stdout)

    return typeof parsed === 'object' && parsed !== null
      ? { kind: 'json', value: parsed as Record<string, unknown> }
      : { kind: 'unreadable' }
  } catch {
    return { kind: 'unreadable' }
  }
}

/** The JSON object the command printed; null for any failure. */
export async function cli($: EngineInterface, args: string[], stdin?: string): Promise<Record<string, unknown> | null> {
  const r = await runKanban($, args, stdin)

  return r.kind === 'json' ? r.value : null
}

/** Runs a command that leaves a request in an inbox; null once it is queued, else why not. */
export async function queueRequest($: EngineInterface, args: string[]): Promise<string | null> {
  const out = await cli($, args)

  if (out !== null && typeof out.queued === 'object') {
    return null
  }

  return typeof out?.error === 'string' ? out.error : CLI_MISSING
}
