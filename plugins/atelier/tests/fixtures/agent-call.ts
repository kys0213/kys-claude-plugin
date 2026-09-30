import type {
  AgentSpawnResult,
  EngineInterface,
  On,
  ToolCallResult,
} from 'claude-code'

/**
 * The test harness's static call scan refuses a `$` call made from inside a
 * hook a test registers with `on(...)` (only the test function's own body
 * may call `$`), so `$.agent.spawn` cannot be nested in a `tool.call` hook.
 * Both honor a `tool_use_id` given in their input, so top-level calls sharing
 * `TOOL_USE_ID` stand in for the pairing. The engine fires `agent.spawn`
 * during the Agent call's `next`, not before it.
 */
const TOOL_USE_ID = 'toolu_1'

const PARENT_MODEL = 'claude-haiku-4-5-20251001'

async function dispatchAgentCall(
  $: EngineInterface,
  on: On,
  opts: {
    model?: string
    fork?: boolean
    spawnResult: AgentSpawnResult
    callResult?: ToolCallResult<'Agent'>
  },
): Promise<ToolCallResult<'Agent'>> {
  on('agent.spawn', () => opts.spawnResult)
  on('tool.call', { tool: 'Agent' }, () => opts.callResult ?? { result: 'hi' })

  await $.agent.spawn({
    tool_use_id: TOOL_USE_ID,
    prompt: 'say hi',
    description: 'say hi',
    subagentType: 'general-purpose',
    parentModel: PARENT_MODEL,
    fork: opts.fork ?? false,
    model: opts.model,
  } as never)

  return $.tool.call({
    tool: 'Agent',
    tool_use_id: TOOL_USE_ID,
    subagent_type: 'general-purpose',
    description: 'say hi',
    prompt: 'say hi',
  } as never)
}

/** Fires `agent.spawn` while the Agent call's `next` is still in flight. */
async function dispatchSpawningDuringCall(
  $: EngineInterface,
  on: On,
  opts: {
    spawnResult: AgentSpawnResult
    outcome: ToolCallResult<'Agent'> | Error
  },
): Promise<ToolCallResult<'Agent'>> {
  let entered!: () => void
  const inside = new Promise<void>(resolve => {
    entered = resolve
  })
  let release!: () => void
  const gate = new Promise<void>(resolve => {
    release = resolve
  })

  on('agent.spawn', () => opts.spawnResult)
  let calls = 0

  on('tool.call', { tool: 'Agent' }, async () => {
    calls += 1

    if (calls > 1) {
      return { result: 'hi' }
    }

    entered()
    await gate

    if (opts.outcome instanceof Error) {
      throw opts.outcome
    }

    return opts.outcome
  })

  const pending = $.tool.call({
    tool: 'Agent',
    tool_use_id: TOOL_USE_ID,
    subagent_type: 'general-purpose',
    description: 'say hi',
    prompt: 'say hi',
  } as never)
  const settled = pending.then(
    value => ({ value }),
    (error: unknown) => ({ error }),
  )

  await inside
  await $.agent.spawn({
    tool_use_id: TOOL_USE_ID,
    prompt: 'say hi',
    description: 'say hi',
    subagentType: 'general-purpose',
    parentModel: PARENT_MODEL,
    fork: false,
  } as never)
  release()

  const outcome = await settled

  if ('error' in outcome) {
    throw outcome.error
  }

  return outcome.value
}

export default { TOOL_USE_ID, PARENT_MODEL, dispatchAgentCall, dispatchSpawningDuringCall }
