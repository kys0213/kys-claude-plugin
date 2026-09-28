import type { EngineInterface, On } from 'claude-code'

/**
 * Every CLI call is fail-open: a missing binary, a non-zero exit, a timeout
 * or output that does not parse as the expected shape all fall back to
 * returning the original hook result untouched.
 */

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

async function runOrchestrator(
  $: EngineInterface,
  sub: string,
  stdin?: string,
): Promise<unknown> {
  let result: { exitCode: number; stdout: string; stderr: string }

  try {
    result = await $.process.run(['atelier', 'orchestrator', sub], {
      stdin,
      timeoutMs: 5000,
    })
  } catch {
    return undefined
  }

  if (result.exitCode !== 0) {
    return undefined
  }

  try {
    return JSON.parse(result.stdout)
  } catch {
    return undefined
  }
}

async function spawnCheck(
  $: EngineInterface,
  fact: SpawnFact,
): Promise<readonly string[] | undefined> {
  const parsed = await runOrchestrator($, 'spawn-check', JSON.stringify(fact))

  if (typeof parsed !== 'object' || parsed === null) {
    return undefined
  }

  const warnings = (parsed as Record<string, unknown>).warnings

  if (!Array.isArray(warnings) || !warnings.every(w => typeof w === 'string')) {
    return undefined
  }

  return warnings as readonly string[]
}

async function compactNote($: EngineInterface): Promise<string | undefined> {
  const parsed = await runOrchestrator($, 'compact-note')

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
    const r = await next(e)
    const key = e.tool_use_id
    const fact = key === undefined ? undefined : facts.get(key)

    if (key !== undefined) {
      facts.delete(key)
    }

    if (fact === undefined || r.deny !== undefined) {
      return r
    }

    const warnings = await spawnCheck($, fact)

    if (warnings === undefined || warnings.length === 0) {
      return r
    }

    return { ...r, context: [...(r.context ?? []), ...warnings] }
  })

  on('session.compact', async ($, e, next) => {
    if (e.agentId !== undefined) {
      return next(e)
    }

    const note = await compactNote($)

    if (note === undefined) {
      return next(e)
    }

    return next({
      ...e,
      instructions: e.instructions ? `${e.instructions}\n\n${note}` : note,
    })
  })
}
