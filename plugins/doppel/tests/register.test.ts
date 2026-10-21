import { describe, expect, mock, test, tier } from 'claude-code/testing'

tier('user')

type On = Parameters<typeof mock.env>[0]

type Sent = { url: string; method?: string; body?: string; socketPath?: string; headers?: Record<string, string> }

type Reply = { status: number; text: string }

const SOCKET = '/home/tester/.areum/doppel/doppel.sock'

const QUESTION = {
  question: '캐시를 어디에 둘까요?',
  header: '캐시',
  multiSelect: false,
  options: [
    { label: '메모리', description: '빠름' },
    { label: '디스크', description: '영속' },
  ],
}

const DEFERRED = '사용자 판단 보류 중 — doppel 이 답을 받으면 이어서 알려줍니다'

function askJson(state: string, answers: unknown = null, answeredBy: string | null = null) {
  return { ask_id: 'ask-000001', session_id: 's', tool_use_id: 'toolu_1', kind: 'ask', state, answers, answered_by: answeredBy }
}

function waitStdout(state: string, answers: unknown = null, answeredBy: string | null = null): string {
  return JSON.stringify({ ok: true, command: 'ask wait', data: askJson(state, answers, answeredBy) })
}

const SESSION_ID = 'sess-1'

function boot(on: On, env: Record<string, string> = {}) {
  mock.env(on, { HOME: '/home/tester', ...env })
  on('session.id', () => ({ value: SESSION_ID }))
  on('session.start', (_$, e) => ({ cwd: e.cwd }))
  on('session.end', (_$, e) => ({ sessionId: e.sessionId }))
  on('turn.start', (_$, e) => ({ turnId: e.turnId }))
  on('classic.PostToolUseFailure', () => ({}))

  return mock.clock(on, { now: 1_000 })
}

/** The daemon answers every request with `reply(path)`; returns what was sent. */
function daemon(on: On, reply: (path: string, body: unknown) => Reply = () => ({ status: 202, text: '{"ok":true}' })) {
  const sent: Sent[] = []

  on('http.fetch', (_$, e) => {
    sent.push({
      url: e.url,
      method: e.init?.method,
      body: e.init?.body,
      socketPath: e.init?.socketPath,
      headers: e.init?.headers as Record<string, string> | undefined,
    })

    const path = e.url.replace('http://doppel', '')
    const r = reply(path, e.init?.body === undefined ? undefined : JSON.parse(e.init.body))

    return { value: { status: r.status, ok: r.status < 300, headers: {}, text: r.text } }
  })

  return sent
}

function asksDaemon(on: On) {
  return daemon(on, path => {
    if (path === '/v1/asks') return { status: 200, text: JSON.stringify(askJson('open')) }
    if (path.endsWith('/close')) return { status: 200, text: JSON.stringify(askJson('closed')) }
    if (path.endsWith('/defer')) return { status: 200, text: JSON.stringify(askJson('deferred')) }
    return { status: 202, text: '{"ok":true}' }
  })
}

function waits(on: On, clock: ReturnType<typeof mock.clock>, stdouts: (string | 'hang')[]) {
  const runs: (readonly string[])[] = []

  on('process.run', async (_$, e) => {
    runs.push(e.argv)

    const next = stdouts[Math.min(runs.length - 1, stdouts.length - 1)] ?? 'hang'

    if (next === 'hang') {
      await clock.sleep(3_600_000)
    }

    return { value: { exitCode: 0, stdout: next, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })

  return runs
}

function noAttach(on: On) {
  const spawned: (readonly string[])[] = []

  on('process.spawn', async function* (_$, e) {
    spawned.push(e.argv)
    yield* []

    return { value: { code: 0, signal: null } }
  })

  return spawned
}

const posted = (sent: Sent[], path: string) => sent.filter(s => s.url === `http://doppel${path}`)

const bodyOf = (s: Sent | undefined) => (s?.body === undefined ? undefined : JSON.parse(s.body))

const ask = ($: any) => $.tool.call({ tool: 'AskUserQuestion', tool_use_id: 'toolu_1', questions: [QUESTION] } as never)

function terminalAnswers(on: On, clock: ReturnType<typeof mock.clock>, mode: 'answer' | 'hang') {
  on('tool.call', { tool: 'AskUserQuestion' }, async () => {
    if (mode === 'hang') {
      await clock.sleep(3_600_000)
    }

    return { result: { questions: [QUESTION], answers: { [QUESTION.question]: '메모리' } } } as never
  })
}

describe('session events', () => {
  test('session.start reports the session, spawns attach and heartbeats', async ($, on) => {
    const clock = boot(on, { DOPPEL_LAUNCHED: '1' })
    const sent = daemon(on)
    const spawned = noAttach(on)
    const id = SESSION_ID

    await $.session.start({ cwd: '/work/repo', surface: null, isInteractive: false })
    await clock.settle()

    const start = bodyOf(posted(sent, '/v1/events')[0])

    expect(start).toMatchObject({
      v: 1,
      session_id: id,
      kind: 'session.start',
      cwd: '/work/repo',
      surface: 'headless',
      launched_by: 'doppel',
    })
    expect(posted(sent, '/v1/events')[0]?.socketPath).toBe(SOCKET)
    expect(posted(sent, '/v1/events')[0]?.headers?.['x-doppel-client']).toBe('plugin')
    expect(spawned[0]).toEqual(['doppel', 'attach', '--session', id])

    await clock.advance(30_000)

    expect(posted(sent, '/v1/events').map(s => bodyOf(s).kind)).toContain('heartbeat')
  })

  test('DOPPEL_SOCKET overrides the socket and an interactive start is a terminal user session', async ($, on) => {
    const clock = boot(on, { DOPPEL_SOCKET: '/tmp/d.sock' })
    const sent = daemon(on)
    noAttach(on)

    await $.session.start({ cwd: '/w', surface: 'terminal', isInteractive: true })
    await clock.settle()

    expect(sent[0]?.socketPath).toBe('/tmp/d.sock')
    expect(bodyOf(sent[0])).toMatchObject({ surface: 'terminal', launched_by: 'user' })
  })

  test('main-loop turns and tool failures are reported; subagent turns are not', async ($, on) => {
    const clock = boot(on)
    const sent = daemon(on)
    noAttach(on)
    on('turn.complete', (_$, e) => ({ text: e.answer }))

    await $.session.start({ cwd: '/w', surface: null, isInteractive: true })
    await $.turn.start({ text: 'hi', turnId: 't1' })
    const turn = { answer: 'x'.repeat(400), durationMs: 5, isAborted: false, reason: 'answer' as const }

    await $.turn.complete({ ...turn, turnId: 't1' })
    await $.turn.complete({ ...turn, turnId: 't2', agentId: 'a1' })
    await $.classic.PostToolUseFailure({ tool_name: 'Bash', tool_input: {}, tool_use_id: 't', error: 'boom' } as never)
    await clock.settle()

    const kinds = posted(sent, '/v1/events').map(s => bodyOf(s))

    expect(kinds.map(k => k.kind)).toEqual(['session.start', 'turn.start', 'turn.complete', 'tool.failure'])
    expect(kinds[2].summary).toHaveLength(300)
    expect(kinds[3].tool).toBe('Bash')
  })

  test('session.end reports its reason and stops the heartbeat', async ($, on) => {
    const clock = boot(on)
    const sent = daemon(on)
    noAttach(on)

    await $.session.start({ cwd: '/w', surface: null, isInteractive: true })
    await $.session.end({ reason: 'clear', sessionId: 's', resume: {} } as never)
    await clock.advance(60_000)

    const kinds = posted(sent, '/v1/events').map(s => bodyOf(s).kind)

    expect(kinds).toEqual(['session.start', 'session.end'])
    expect(bodyOf(posted(sent, '/v1/events')[1]).reason).toBe('clear')
  })
})

describe('AskUserQuestion race', () => {
  test('an unreachable daemon leaves the question to the session', async ($, on) => {
    const clock = boot(on)
    on('http.fetch', () => ({ deny: 'ECONNREFUSED' }))
    const runs = waits(on, clock, [])
    terminalAnswers(on, clock, 'answer')

    const result = await ask($)

    expect(runs).toHaveLength(0)
    expect(result).toEqual({ result: { questions: [QUESTION], answers: { [QUESTION.question]: '메모리' } } })
  })

  test('a doppel answer wins over a pending terminal dialog and is closed with who answered', async ($, on) => {
    const clock = boot(on)
    const sent = asksDaemon(on)
    const runs = waits(on, clock, [waitStdout('answered', { [QUESTION.question]: '디스크' }, 'slack')])
    terminalAnswers(on, clock, 'hang')

    const result = await ask($)

    expect(bodyOf(posted(sent, '/v1/asks')[0])).toMatchObject({ tool_use_id: 'toolu_1', kind: 'ask', questions: [QUESTION] })
    expect(runs[0]).toEqual(['doppel', 'ask', 'wait', 'ask-000001', '--timeout', '590', '--json'])
    expect(result).toEqual({ result: { questions: [QUESTION], answers: { [QUESTION.question]: '디스크' } } })
    expect(bodyOf(posted(sent, '/v1/asks/ask-000001/close')[0])).toEqual({
      answers: { [QUESTION.question]: '디스크' },
      answered_by: 'slack',
    })
  })

  test('a terminal answer wins over a waiting doppel and is closed as answered by the session', async ($, on) => {
    const clock = boot(on)
    const sent = asksDaemon(on)
    waits(on, clock, ['hang'])
    terminalAnswers(on, clock, 'answer')

    const result = await ask($)

    expect(result).toEqual({ result: { questions: [QUESTION], answers: { [QUESTION.question]: '메모리' } } })
    expect(bodyOf(posted(sent, '/v1/asks/ask-000001/close')[0])).toEqual({
      answers: { [QUESTION.question]: '메모리' },
      answered_by: 'session',
    })
  })

  test('a wait that times out while still open is called again', async ($, on) => {
    const clock = boot(on)
    asksDaemon(on)
    const runs = waits(on, clock, [waitStdout('open'), waitStdout('answered', { [QUESTION.question]: '디스크' }, 'doppel-ui')])
    terminalAnswers(on, clock, 'hang')

    const result = await ask($)

    expect(runs).toHaveLength(2)
    expect(result).toEqual({ result: { questions: [QUESTION], answers: { [QUESTION.question]: '디스크' } } })
  })

  test('a failing wait leaves the race to the session', async ($, on) => {
    const clock = boot(on)
    asksDaemon(on)
    on('process.run', () => ({ value: { exitCode: 1, stdout: '', stderr: 'no daemon', isStdoutTruncated: false, isStderrTruncated: false } }))
    terminalAnswers(on, clock, 'answer')

    const result = await ask($)

    expect(result).toEqual({ result: { questions: [QUESTION], answers: { [QUESTION.question]: '메모리' } } })
  })

  test('a headless session defers an unanswered question', async ($, on) => {
    const clock = boot(on, { DOPPEL_ASK_DEFER_AFTER_MS: '0' })
    const sent = asksDaemon(on)
    noAttach(on)
    waits(on, clock, ['hang'])
    terminalAnswers(on, clock, 'hang')

    await $.session.start({ cwd: '/w', surface: null, isInteractive: false })
    const result = await ask($)

    expect(posted(sent, '/v1/asks/ask-000001/defer')).toHaveLength(1)
    expect(result).toEqual({ result: { questions: [QUESTION], answers: { [QUESTION.question]: DEFERRED } } })
  })

  test('a terminal session never defers', async ($, on) => {
    const clock = boot(on, { DOPPEL_ASK_DEFER_AFTER_MS: '0' })
    const sent = asksDaemon(on)
    noAttach(on)
    const runs = waits(on, clock, [waitStdout('answered', { [QUESTION.question]: '디스크' }, 'doppel-ui')])
    terminalAnswers(on, clock, 'hang')

    await $.session.start({ cwd: '/w', surface: 'terminal', isInteractive: true })
    const result = await ask($)

    expect(posted(sent, '/v1/asks/ask-000001/defer')).toHaveLength(0)
    expect(runs).toHaveLength(1)
    expect(result).toEqual({ result: { questions: [QUESTION], answers: { [QUESTION.question]: '디스크' } } })
  })

  test('a Discord connector run is observed only: no ask is opened', async ($, on) => {
    const clock = boot(on, { DISCORD_CONNECTOR_THREAD_ID: '111' })
    const sent = asksDaemon(on)
    const spawned = noAttach(on)
    terminalAnswers(on, clock, 'answer')

    await $.session.start({ cwd: '/w', surface: null, isInteractive: false })
    const result = await ask($)
    await clock.settle()

    expect(posted(sent, '/v1/asks')).toHaveLength(0)
    expect(posted(sent, '/v1/events')).toHaveLength(1)
    expect(spawned).toHaveLength(0)
    expect(result).toEqual({ result: { questions: [QUESTION], answers: { [QUESTION.question]: '메모리' } } })
  })
})

describe('permission relay', () => {
  const PERMISSION_Q = 'Bash 실행 허용: make deploy'

  function bash(on: On, $: any, clock: ReturnType<typeof mock.clock>, firstCall: 'hang' | 'deny' | 'late-deny') {
    const decisions: string[] = []

    on('tool.check', () => ({ decision: 'ask' }))
    on('tool.call', { tool: 'Bash' }, async (_inner, e) => {
      const verdict = await $.tool.check({ tool: 'Bash', input: { command: 'make deploy' }, tool_use_id: e.tool_use_id })

      decisions.push(verdict.decision)

      if (verdict.decision === 'allow') {
        return { result: { stdout: 'deployed' } } as never
      }

      if (firstCall === 'hang') {
        await clock.sleep(3_600_000)
      }

      if (firstCall === 'late-deny') {
        await clock.sleep(5_000)
      }

      return { deny: 'denied in the terminal' }
    })

    return decisions
  }

  const call = ($: any) => $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_b', command: 'make deploy' } as never)

  test('a doppel allow re-runs the call past the permission check', async ($, on) => {
    const clock = boot(on)
    const sent = asksDaemon(on)
    waits(on, clock, [waitStdout('answered', { [PERMISSION_Q]: 'allow' }, 'doppel-ui')])
    const decisions = bash(on, $, clock, 'hang')

    const pending = call($)
    await clock.advance(1_500)
    const result = await pending

    expect(bodyOf(posted(sent, '/v1/asks')[0])).toMatchObject({
      tool_use_id: 'toolu_b',
      kind: 'permission',
      questions: [{ question: PERMISSION_Q, options: [{ label: 'allow' }, { label: 'deny' }] }],
    })
    expect(decisions).toEqual(['ask', 'allow'])
    expect(result).toMatchObject({ result: { stdout: 'deployed' } })
    expect(bodyOf(posted(sent, '/v1/asks/ask-000001/close')[0])).toEqual({
      answers: { [PERMISSION_Q]: 'allow' },
      answered_by: 'doppel-ui',
    })
  })

  test('a doppel deny refuses the call', async ($, on) => {
    const clock = boot(on)
    const sent = asksDaemon(on)
    waits(on, clock, [waitStdout('answered', { [PERMISSION_Q]: 'deny' }, 'doppel-ui')])
    bash(on, $, clock, 'hang')

    const pending = call($)
    await clock.advance(1_500)
    const result = await pending

    expect(result.deny).toBe('사용자가 doppel 에서 실행을 거부했어요')
    expect(bodyOf(posted(sent, '/v1/asks/ask-000001/close')[0])).toEqual({
      answers: { [PERMISSION_Q]: 'deny' },
      answered_by: 'doppel-ui',
    })
  })

  test('a terminal deny after the grace window closes the ask as denied by the session', async ($, on) => {
    const clock = boot(on)
    const sent = asksDaemon(on)
    waits(on, clock, ['hang'])
    bash(on, $, clock, 'late-deny')

    const pending = call($)
    await clock.advance(1_500)
    expect(posted(sent, '/v1/asks')).toHaveLength(1)
    await clock.advance(5_000)
    const result = await pending

    expect(result.deny).toBe('denied in the terminal')
    expect(bodyOf(posted(sent, '/v1/asks/ask-000001/close')[0])).toEqual({
      answers: { [PERMISSION_Q]: 'deny' },
      answered_by: 'session',
    })
  })

  test('a call the session decides within the grace window opens no ask', async ($, on) => {
    const clock = boot(on)
    const sent = asksDaemon(on)
    waits(on, clock, ['hang'])
    bash(on, $, clock, 'deny')

    const result = await call($)

    expect(result.deny).toBe('denied in the terminal')
    expect(posted(sent, '/v1/asks')).toHaveLength(0)
  })

  test('a call the engine does not ask about is not relayed', async ($, on) => {
    boot(on)
    const sent = asksDaemon(on)
    on('tool.check', () => ({ decision: 'allow' }))
    on('tool.call', { tool: 'Bash' }, () => ({ result: { stdout: 'ran' } }) as never)

    const result = await call($)

    expect(posted(sent, '/v1/asks')).toHaveLength(0)
    expect(result).toMatchObject({ result: { stdout: 'ran' } })
  })
})

describe('attach messages', () => {
  function attachWith(on: On, chunks: string[]) {
    on('process.spawn', async function* () {
      for (const text of chunks) {
        yield { stream: 'stdout' as const, text }
      }

      return { value: { code: 0, signal: null } }
    })
  }

  test('a late answer split across chunks opens a turn and closes the deferred ask', async ($, on) => {
    const clock = boot(on)
    const sent = asksDaemon(on)
    const prompts: string[] = []
    on('prompt.submit', (_$, e) => {
      prompts.push(e.text)

      return { text: e.text }
    })
    const line = JSON.stringify({ kind: 'answer', ask_id: 'ask-000009', tool_use_id: 'toolu_9', answers: { '어디?': '디스크' } })
    attachWith(on, [line.slice(0, 20), `${line.slice(20)}\n`])

    await $.session.start({ cwd: '/w', surface: null, isInteractive: false })
    await clock.settle()

    expect(prompts).toHaveLength(1)
    expect(prompts[0]).toContain('- 어디?: 디스크')
    expect(bodyOf(posted(sent, '/v1/asks/ask-000009/close')[0])).toEqual({
      answers: { '어디?': '디스크' },
      answered_by: 'doppel-ui',
    })
  })

  test('prompt and context messages reach the session; malformed lines are ignored', async ($, on) => {
    const clock = boot(on)
    daemon(on)
    const appended: unknown[] = []
    on('session.append', (_$, e, next) => {
      appended.push(e.message.content)

      return next(e)
    })
    const prompts: string[] = []
    on('prompt.submit', (_$, e) => {
      prompts.push(e.text)

      return { text: e.text }
    })
    attachWith(on, [
      'not json\n',
      `${JSON.stringify({ kind: 'prompt', text: '새 작업' })}\n`,
      `${JSON.stringify({ kind: 'context', text: '참고: 배포 중' })}\n`,
      `${JSON.stringify({ kind: 'unknown' })}\n`,
    ])

    await $.session.start({ cwd: '/w', surface: null, isInteractive: false })
    await clock.settle()

    expect(prompts).toEqual(['새 작업'])
    expect(JSON.stringify(appended)).toContain('참고: 배포 중')
  })

  test('an attach that ends is spawned again after a backoff', async ($, on) => {
    const clock = boot(on)
    daemon(on)
    const spawned = noAttach(on)

    await $.session.start({ cwd: '/w', surface: null, isInteractive: false })
    await clock.settle()

    expect(spawned).toHaveLength(1)

    await clock.advance(1_000)

    expect(spawned).toHaveLength(2)
  })
})
