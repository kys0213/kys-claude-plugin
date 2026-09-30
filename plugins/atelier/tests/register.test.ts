import { describe, expect, test, tier } from 'claude-code/testing'

import Fixtures from './fixtures/agent-call'

tier('user')

/**
 * A minimal compacted transcript message, valid enough for `session.compact`
 * to accept (a compaction must leave at least one message).
 */
const MSG = { role: 'assistant' as const, text: 'summary', toolUses: [] }

describe('register', () => {
  test('a model-less Agent dispatch gets spawn-check warnings as context', async ($, on) => {
    const calls: { argv: readonly string[]; stdin?: string }[] = []

    on('process.run', ($, e) => {
      calls.push({ argv: e.argv, stdin: e.init?.stdin })

      return {
        value: {
          exitCode: 0,
          stdout: JSON.stringify({ warnings: ['no model given'] }),
          stderr: '',
        },
      }
    })

    const result = await Fixtures.dispatchAgentCall($, on, {
      spawnResult: { model: 'claude-sonnet-4-5-20250929' },
    })

    expect(result.context).toEqual(['no model given'])
    expect(calls).toHaveLength(1)
    expect(calls[0]?.argv).toEqual(['atelier', 'orchestrator', 'spawn-check'])
    expect(JSON.parse(calls[0]?.stdin ?? '{}')).toEqual({
      model: null,
      resolved_model: 'claude-sonnet-4-5-20250929',
      parent_model: Fixtures.PARENT_MODEL,
      fork: false,
      subagent_type: 'general-purpose',
    })
  })

  test('no warnings from spawn-check leaves the result untouched', async ($, on) => {
    on('process.run', () => ({
      value: { exitCode: 0, stdout: JSON.stringify({ warnings: [] }), stderr: '' },
    }))

    const result = await Fixtures.dispatchAgentCall($, on, {
      model: 'sonnet',
      spawnResult: { model: 'claude-sonnet-4-5-20250929' },
    })

    expect(result.context).toBeUndefined()
  })

  test('a missing CLI binary leaves the result untouched', async ($, on) => {
    on('process.run', () => ({ deny: 'ENOENT: Executable not found in $PATH' }))

    const result = await Fixtures.dispatchAgentCall($, on, {
      spawnResult: { model: 'claude-sonnet-4-5-20250929' },
    })

    expect(result).toEqual({ result: 'hi' })
  })

  for (const [label, run] of [
    ['a non-zero exit', { exitCode: 1, stdout: '', stderr: 'boom' }],
    ['stdout that is not JSON', { exitCode: 0, stdout: 'not json', stderr: '' }],
  ] as const) {
    test(`${label} from spawn-check leaves the result untouched`, async ($, on) => {
      on('process.run', () => ({ value: run }))

      const result = await Fixtures.dispatchAgentCall($, on, {
        spawnResult: { model: 'claude-sonnet-4-5-20250929' },
      })

      expect(result).toEqual({ result: 'hi' })
    })
  }

  test('a denied Agent call stays denied, with no context and no CLI call', async ($, on) => {
    let called = false

    on('process.run', () => {
      called = true

      return { value: { exitCode: 0, stdout: JSON.stringify({ warnings: [] }), stderr: '' } }
    })

    const result = await Fixtures.dispatchAgentCall($, on, {
      spawnResult: { model: 'claude-sonnet-4-5-20250929' },
      callResult: { deny: 'not allowed' },
    })

    expect(result).toEqual({ deny: 'not allowed' })
    expect(called).toBe(false)
  })

  test('an Agent call with no matching agent.spawn never calls the CLI', async ($, on) => {
    let called = false

    on('process.run', () => {
      called = true

      return { value: { exitCode: 0, stdout: JSON.stringify({ warnings: [] }), stderr: '' } }
    })
    on('tool.call', { tool: 'Agent' }, () => ({ result: 'hi' }))

    const result = await $.tool.call({
      tool: 'Agent',
      tool_use_id: Fixtures.TOOL_USE_ID,
      subagent_type: 'general-purpose',
      description: 'say hi',
      prompt: 'say hi',
    } as never)

    expect(result).toEqual({ result: 'hi' })
    expect(called).toBe(false)
  })

  test('an Agent call that resolves to isError gets no context and no CLI call', async ($, on) => {
    let called = false

    on('process.run', () => {
      called = true

      return {
        value: { exitCode: 0, stdout: JSON.stringify({ warnings: ['w'] }), stderr: '' },
      }
    })

    const result = await Fixtures.dispatchAgentCall($, on, {
      spawnResult: { model: 'claude-sonnet-4-5-20250929' },
      callResult: { result: 'failed', isError: true },
    })

    expect(result.context).toBeUndefined()
    expect(called).toBe(false)
  })

  test('a spawn that fires during the Agent call still gets spawn-check warnings', async ($, on) => {
    const calls: string[] = []

    on('process.run', ($, e) => {
      calls.push(e.argv.join(' '))

      return {
        value: { exitCode: 0, stdout: JSON.stringify({ warnings: ['w'] }), stderr: '' },
      }
    })

    const result = await Fixtures.dispatchSpawningDuringCall($, on, {
      spawnResult: { model: 'claude-sonnet-4-5-20250929' },
      outcome: { result: 'hi' },
    })

    expect(result.context).toEqual(['w'])
    expect(calls).toEqual(['atelier orchestrator spawn-check'])
  })

  test('a throw after a spawn during the Agent call clears the fact for the next call', async ($, on) => {
    on('process.run', () => ({
      value: { exitCode: 0, stdout: JSON.stringify({ warnings: ['w'] }), stderr: '' },
    }))

    await expect(
      Fixtures.dispatchSpawningDuringCall($, on, {
        spawnResult: { model: 'claude-sonnet-4-5-20250929' },
        outcome: new Error('boom'),
      }),
    ).rejects.toThrow()

    const call = {
      tool: 'Agent',
      tool_use_id: Fixtures.TOOL_USE_ID,
      subagent_type: 'general-purpose',
      description: 'say hi',
      prompt: 'say hi',
    } as never
    const second = await $.tool.call(call)

    expect(second.context).toBeUndefined()
  })

  test('main compaction gets the compact-note appended to instructions', async ($, on) => {
    on('process.run', () => ({
      value: {
        exitCode: 0,
        stdout: JSON.stringify({ instructions: 'preserve orchestrator run state' }),
        stderr: '',
      },
    }))
    on('session.start', ($, e) => ({ cwd: e.cwd }))

    let seen: string | undefined

    on('session.compact', ($, e) => {
      seen = e.instructions

      return { messages: [MSG] }
    })

    await $.session.start({ cwd: '/work', surface: null, isInteractive: false })
    await $.session.compact({ instructions: 'existing note', messages: [MSG] } as never)

    expect(seen).toBe('existing note\n\npreserve orchestrator run state')
  })

  test('main compaction with no prior instructions gets the note alone', async ($, on) => {
    on('process.run', () => ({
      value: { exitCode: 0, stdout: JSON.stringify({ instructions: 'note' }), stderr: '' },
    }))
    on('session.start', ($, e) => ({ cwd: e.cwd }))

    let seen: string | undefined

    on('session.compact', ($, e) => {
      seen = e.instructions

      return { messages: [MSG] }
    })

    await $.session.start({ cwd: '/work', surface: null, isInteractive: false })
    await $.session.compact({ messages: [MSG] } as never)

    expect(seen).toBe('note')
  })

  test('an empty compact-note string from the CLI is still appended', async ($, on) => {
    on('process.run', () => ({
      value: { exitCode: 0, stdout: JSON.stringify({ instructions: '' }), stderr: '' },
    }))
    on('session.start', ($, e) => ({ cwd: e.cwd }))

    let seen: string | undefined

    on('session.compact', ($, e) => {
      seen = e.instructions

      return { messages: [MSG] }
    })

    await $.session.start({ cwd: '/work', surface: null, isInteractive: false })
    await $.session.compact({ instructions: 'existing note', messages: [MSG] } as never)

    expect(seen).toBe('existing note\n\n')
  })

  test('a subagent compaction never calls the CLI and keeps instructions as they are', async ($, on) => {
    let called = false

    on('process.run', () => {
      called = true

      return {
        value: { exitCode: 0, stdout: JSON.stringify({ instructions: 'note' }), stderr: '' },
      }
    })
    on('session.start', ($, e) => ({ cwd: e.cwd }))

    let seen: string | undefined

    on('session.compact', ($, e) => {
      seen = e.instructions

      return { messages: [MSG] }
    })

    await $.session.start({ cwd: '/work', surface: null, isInteractive: false })
    await $.session.compact({
      agentId: 'sub-1',
      instructions: 'existing note',
      messages: [MSG],
    } as never)

    expect(called).toBe(false)
    expect(seen).toBe('existing note')
  })
})
