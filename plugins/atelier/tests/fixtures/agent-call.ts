import type {
  AgentSpawnResult,
  EngineInterface,
  On,
  ToolCallResult,
} from 'claude-code'

/**
 * The `tool_use_id` shared by a dispatch's `agent.spawn` and its
 * `tool.call { tool: 'Agent' }`.
 *
 * The test harness's static call scan refuses a `$` call made from inside a
 * hook a test registers with `on(...)` (only the test function's own body
 * may call `$`), so a real engine's "the same call raises both events, tied
 * by tool_use_id" cannot be reproduced by nesting `$.agent.spawn` inside a
 * `tool.call` hook here. Both `$.agent.spawn` and `$.tool.call` honor a
 * `tool_use_id` given in their input, so the pairing is reproduced instead by
 * two top-level calls sharing this constant.
 */
const TOOL_USE_ID = 'toolu_1'

/**
 * The parent model a dispatch's `agent.spawn` carries as `parentModel`.
 */
const PARENT_MODEL = 'claude-haiku-4-5-20251001'

/**
 * Registers the bottom `agent.spawn` and `tool.call { tool: 'Agent' }`
 * hooks for one dispatch, then raises both, sharing `TOOL_USE_ID`, as the
 * engine raises them together for one real Agent call.
 *
 * @param $ the test's `$`
 * @param on the test's `on`; every hook here must be registered before any
 *   `$` call the test has already made
 * @param opts `model` as the Agent tool call itself named it (absent when
 *   none given); `spawnResult` what the bottom `agent.spawn` hook answers;
 *   `callResult` what the bottom `tool.call` hook answers (`{ result: 'hi' }`
 *   when omitted); `fork` whether the dispatch is a fork (false when omitted)
 * @returns the `tool.call`'s result
 */
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

export default { TOOL_USE_ID, PARENT_MODEL, dispatchAgentCall }
