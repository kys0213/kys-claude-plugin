import type { EngineInterface, On } from 'claude-code'

type SpawnFact = {
  model: string | null
  resolved_model: string | null
  parent_model: string
  fork: boolean
  subagent_type: string
}

type SpawnVerdict = { deny: string | null; warnings: readonly string[] }

async function askCli<T>(
  $: EngineInterface,
  sub: string,
  decode: (parsed: unknown) => T | undefined,
  neutral: T,
  stdin?: string,
): Promise<T> {
  try {
    const r = await $.process.run(['atelier', 'orchestrator', sub], { stdin, timeoutMs: 5000 })
    return r.exitCode === 0 ? (decode(JSON.parse(r.stdout)) ?? neutral) : neutral
  } catch {
    return neutral
  }
}

/** An absent `deny` reads as allow, so an older CLI that emits only `warnings` still decodes. */
function decodeVerdict(parsed: unknown): SpawnVerdict | undefined {
  if (typeof parsed !== 'object' || parsed === null) {
    return undefined
  }

  const { deny, warnings } = parsed as Record<string, unknown>

  if (deny !== undefined && deny !== null && typeof deny !== 'string') {
    return undefined
  }

  if (!Array.isArray(warnings) || !warnings.every(w => typeof w === 'string')) {
    return undefined
  }

  return { deny: deny ?? null, warnings: warnings as readonly string[] }
}

const ALLOW: SpawnVerdict = { deny: null, warnings: [] }

function decodeInstructions(parsed: unknown): string | undefined {
  if (typeof parsed !== 'object' || parsed === null) {
    return undefined
  }

  const instructions = (parsed as Record<string, unknown>).instructions

  return typeof instructions === 'string' ? instructions : undefined
}

export function register(on: On): void {
  /**
   * Warnings from `agent.spawn`, held until the matching
   * `tool.call { tool: 'Agent' }` attaches them, keyed by the shared `tool_use_id`.
   */
  const pending = new Map<string, readonly string[]>()

  on('agent.spawn', async ($, e, next) => {
    /** The engine only honors a deny returned in place of `next`, so the resolved model is not known yet. */
    const fact: SpawnFact = {
      model: e.model ?? null,
      resolved_model: null,
      parent_model: e.parentModel,
      fork: e.fork,
      subagent_type: e.subagentType,
    }
    const verdict = await askCli($, 'spawn-check', decodeVerdict, ALLOW, JSON.stringify(fact))

    if (verdict.deny !== null) {
      return { deny: verdict.deny }
    }

    const r = await next(e)

    if (r.deny === undefined && verdict.warnings.length > 0) {
      pending.set(e.tool_use_id, verdict.warnings)
    }

    return r
  })

  on('tool.call', { tool: 'Agent' }, async ($, e, next) => {
    let r: Awaited<ReturnType<typeof next>>
    let warnings: readonly string[] | undefined

    try {
      r = await next(e)
    } finally {
      const key = e.tool_use_id

      if (key !== undefined) {
        warnings = pending.get(key)
        pending.delete(key)
      }
    }

    if (warnings === undefined || r.deny !== undefined || r.isError) {
      return r
    }

    return { ...r, context: [...(r.context ?? []), ...warnings] }
  })

  on('session.compact', async ($, e, next) => {
    if (e.agentId !== undefined) {
      return next(e)
    }

    const note = await askCli<string | null>($, 'compact-note', decodeInstructions, null)

    if (note === null) {
      return next(e)
    }

    return next({
      ...e,
      instructions: e.instructions ? `${e.instructions}\n\n${note}` : note,
    })
  })
}
