import type { EngineInterface, On } from 'claude-code'

/**
 * The `agent.spawn` facts a dispatch needs for `spawn-check`, held from
 * `agent.spawn` until the matching `tool.call { tool: 'Agent' }` consumes
 * them, keyed by the shared `tool_use_id`.
 */
type SpawnFact = {
  model: string | null
  resolved_model: string | null
  parent_model: string
  fork: boolean
  subagent_type: string
}

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

function decodeWarnings(parsed: unknown): readonly string[] | undefined {
  if (typeof parsed !== 'object' || parsed === null) {
    return undefined
  }

  const warnings = (parsed as Record<string, unknown>).warnings

  return Array.isArray(warnings) && warnings.every(w => typeof w === 'string')
    ? (warnings as readonly string[])
    : undefined
}

function decodeInstructions(parsed: unknown): string | undefined {
  if (typeof parsed !== 'object' || parsed === null) {
    return undefined
  }

  const instructions = (parsed as Record<string, unknown>).instructions

  return typeof instructions === 'string' ? instructions : undefined
}

export function register(on: On): void {
  const facts = new Map<string, SpawnFact>()

  on('agent.spawn', async ($, e, next) => {
    const r = await next(e)

    if (r.deny === undefined) {
      facts.set(e.tool_use_id, {
        model: e.model ?? null,
        resolved_model: r.model ?? null,
        parent_model: e.parentModel,
        fork: e.fork,
        subagent_type: e.subagentType,
      })
    }

    return r
  })

  on('tool.call', { tool: 'Agent' }, async ($, e, next) => {
    const key = e.tool_use_id
    const fact = key === undefined ? undefined : facts.get(key)

    if (key !== undefined) {
      facts.delete(key)
    }

    const r = await next(e)

    if (fact === undefined || r.deny !== undefined || r.isError) {
      return r
    }

    const warnings = await askCli($, 'spawn-check', decodeWarnings, [], JSON.stringify(fact))

    if (warnings.length === 0) {
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
