import { describe, expect, mock, test, tier } from 'claude-code/testing'

tier('user')

const ENV = {
  DISCORD_CONNECTOR_THREAD_ID: '111',
  DISCORD_CONNECTOR_REQUESTER_ID: '222',
  DISCORD_CONNECTOR_BOT_ID: '999',
}

const NO_ANSWER = 'Discord에서 시간 안에 답이 없었어요'

type Run = { argv: readonly string[]; stdin?: string; timeoutMs?: number }

type Reply = { exitCode: number; stdout: string; stderr?: string }

const ok = (data: unknown): Reply => ({ exitCode: 0, stdout: JSON.stringify({ ok: true, command: 'x', data }) })

const SINGLE = {
  question: '어떤 색이 좋아요?',
  header: '색',
  options: [{ label: '초록' }, { label: '파랑' }],
  multiSelect: false,
}

const MULTI = {
  question: '무엇을 켤까요?',
  header: '기능',
  options: [{ label: 'a' }, { label: 'b' }, { label: 'c' }],
  multiSelect: true,
}

function recordProcess(on: Parameters<typeof mock.env>[0], respond: (argv: readonly string[]) => Reply) {
  const runs: Run[] = []

  on('process.run', (_$, e) => {
    runs.push({ argv: e.argv, stdin: e.init?.stdin, timeoutMs: e.init?.timeoutMs })

    const r = respond(e.argv)

    return { value: { exitCode: r.exitCode, stdout: r.stdout, stderr: r.stderr ?? '' } }
  })

  return runs
}

function askCli(waitReply: Reply, createReply: Reply = ok({ ask_id: 'A1', status: 'pending', channel_id: '111' })) {
  return (argv: readonly string[]) => {
    if (argv[3] === 'create') return createReply
    if (argv[3] === 'wait') return waitReply
    return ok({ messages: [] })
  }
}

function boot(on: Parameters<typeof mock.env>[0], env: Record<string, string>) {
  mock.env(on, env)
  mock.clock(on, { now: 1_000_000_000 })
  on('classic.PermissionDenied', () => ({}))
}

const call = ($: any, questions: unknown[]) =>
  $.tool.call({ tool: 'AskUserQuestion', tool_use_id: 'toolu_1', questions } as never)

describe('AskUserQuestion relay', () => {
  test('without the Discord run markers the call passes through untouched', async ($, on) => {
    boot(on, {})
    const runs = recordProcess(on, () => ok({}))
    on('tool.call', { tool: 'AskUserQuestion' }, () => ({ result: { questions: [], answers: { passthrough: 'yes' } } }) as never)

    const result = await call($, [SINGLE])

    expect(runs).toHaveLength(0)
    expect(result).toEqual({ result: { questions: [], answers: { passthrough: 'yes' } } })
  })

  test('a single choice is created with the requester, timeout and deadline, then waited', async ($, on) => {
    boot(on, ENV)
    const runs = recordProcess(on, askCli(ok({ status: 'answered', kind: 'choice', value: '초록' })))

    const result = await call($, [SINGLE])

    expect(runs[0]?.argv).toEqual([
      'discord', '--json', 'ask', 'create', '111',
      '어떤 색이 좋아요?\n마감 <t:1000540:t>',
      '--option', '초록', '--option', '파랑',
      '--timeout', '540', '--allowed-user', '222',
    ])
    expect(runs[1]?.argv).toEqual(['discord', '--json', 'ask', 'wait', 'A1', '--timeout', '540'])
    expect(runs[1]?.timeoutMs).toBe(600000)
    expect(result).toEqual({ result: { questions: [SINGLE], answers: { '어떤 색이 좋아요?': '초록' } } })
  })

  test('a multi select question passes --multi-select and joins the label array', async ($, on) => {
    boot(on, ENV)
    const runs = recordProcess(on, askCli(ok({ status: 'answered', kind: 'multi_choice', value: ['a', 'c'] })))

    const result = await call($, [MULTI])

    expect(runs[0]?.argv).toContain('--multi-select')
    expect(result).toEqual({ result: { questions: [MULTI], answers: { '무엇을 켤까요?': 'a, c' } } })
  })

  test('several questions are asked one after another and answered by question text', async ($, on) => {
    boot(on, ENV)
    let n = 0
    const runs = recordProcess(on, argv => {
      if (argv[3] === 'create') return ok({ ask_id: `A${++n}`, status: 'pending', channel_id: '111' })
      return ok({ status: 'answered', kind: 'choice', value: `v${argv[4]}` })
    })

    const result = await call($, [SINGLE, MULTI])

    expect(runs.map(r => r.argv[3])).toEqual(['create', 'wait', 'create', 'wait'])
    expect(result).toEqual({
      result: { questions: [SINGLE, MULTI], answers: { [SINGLE.question]: 'vA1', [MULTI.question]: 'vA2' } },
    })
  })

  test('a timed out question answers with the no-answer text', async ($, on) => {
    boot(on, ENV)
    recordProcess(on, askCli(ok({ status: 'timed_out' })))

    const result = await call($, [SINGLE])

    expect(result).toEqual({ result: { questions: [SINGLE], answers: { [SINGLE.question]: NO_ANSWER } } })
  })

  test('a failing ask wait answers with the no-answer text', async ($, on) => {
    boot(on, ENV)
    recordProcess(on, askCli({ exitCode: 1, stdout: '', stderr: 'boom' }))

    const result = await call($, [SINGLE])

    expect(result).toEqual({ result: { questions: [SINGLE], answers: { [SINGLE.question]: NO_ANSWER } } })
  })

  test('a failing ask create notifies the thread and answers with the no-answer text', async ($, on) => {
    boot(on, ENV)
    const runs = recordProcess(
      on,
      askCli(ok({}), { exitCode: 1, stdout: JSON.stringify({ ok: false, command: 'ask', error: 'daemon not running' }) }),
    )

    const result = await call($, [SINGLE])

    expect(runs.map(r => r.argv[2])).toEqual(['ask', 'send'])
    expect(runs[1]?.argv).toEqual(['discord', '--json', 'send', '111', '-', '--split'])
    expect(runs[1]?.stdin).toContain('daemon not running')
    expect(result).toEqual({ result: { questions: [SINGLE], answers: { [SINGLE.question]: NO_ANSWER } } })
  })

  test('an option count or label length outside the CLI limits is reported instead of created', async ($, on) => {
    boot(on, ENV)
    const runs = recordProcess(on, askCli(ok({})))
    const tooMany = { ...SINGLE, options: ['1', '2', '3', '4', '5'].map(label => ({ label })) }

    const result = await call($, [tooMany])

    expect(runs.map(r => r.argv[2])).toEqual(['send'])
    expect(result).toEqual({ result: { questions: [tooMany], answers: { [tooMany.question]: NO_ANSWER } } })
  })

  test('bot mentions in the question and options never reach the CLI', async ($, on) => {
    boot(on, ENV)
    const runs = recordProcess(on, askCli(ok({ status: 'timed_out' })))
    const q = { ...SINGLE, question: '<@999> 괜찮아요? <@!999>', options: [{ label: '<@999>네' }, { label: '아니오' }] }

    await call($, [q])

    const created = runs[0]?.argv.join('\n') ?? ''

    expect(created).not.toContain('<@999>')
    expect(created).not.toContain('<@!999>')
    expect(created).toContain('@999 괜찮아요? @999')
  })
})

describe('PermissionDenied notice', () => {
  test('posts the denied tool and reason to the thread and lets the flow continue', async ($, on) => {
    boot(on, ENV)
    const runs = recordProcess(on, () => ok({ messages: [] }))

    const result = await $.classic.PermissionDenied({
      tool_name: 'Bash',
      tool_input: {},
      tool_use_id: 't1',
      reason: '<@999> rm -rf 는 위험해요',
    } as never)

    expect(runs).toHaveLength(1)
    expect(runs[0]?.argv).toEqual(['discord', '--json', 'send', '111', '-', '--split'])
    expect(runs[0]?.stdin).toContain('Bash')
    expect(runs[0]?.stdin).toContain('rm -rf')
    expect(runs[0]?.stdin).not.toContain('<@999>')
    expect(result).toEqual({})
  })

  test('without the Discord run markers nothing is posted', async ($, on) => {
    boot(on, {})
    const runs = recordProcess(on, () => ok({}))

    await $.classic.PermissionDenied({ tool_name: 'Bash', tool_input: {}, tool_use_id: 't1', reason: 'x' } as never)

    expect(runs).toHaveLength(0)
  })
})
