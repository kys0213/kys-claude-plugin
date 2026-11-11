import { describe, expect, mock, test, tier } from 'claude-code/testing'
import type { On } from 'claude-code'

tier('user')

const SELF = 'self-session'
const START = 1_800_000_000_000
const KEY = `kanban:session:${SELF}`

const ROW = { direction: 'row', column_width: 24, column_gap: 1 }

const BOARD = {
  columns: [
    {
      id: 'needs_input',
      heading: '입력 대기 1',
      cards: [
        {
          session_id: 'other',
          headline: 'project-b · 권한 요청: Bash · 3m',
          subtitle: 'Fix login bug',
          is_self: false,
        },
      ],
    },
    { id: 'working', heading: '작업 중 0', cards: [] },
    {
      id: 'idle',
      heading: '쉬는 중 1',
      cards: [{ session_id: SELF, headline: '▶ project-a · 5s', subtitle: null, is_self: true }],
    },
    { id: 'stale', heading: '응답 없음 0', cards: [] },
  ],
  expired: ['gone'],
  layout: ROW,
}

const COLOR = {
  question: 'Which color?',
  header: 'Color',
  multi_select: false,
  options: [
    { label: 'Red', description: 'warm' },
    { label: 'Blue', description: 'cool' },
  ],
}

const PETS = {
  question: 'Which pets?',
  header: 'Pets',
  multi_select: true,
  options: [
    { label: 'Cat', description: null },
    { label: 'Dog', description: null },
    { label: 'Fish', description: null },
  ],
}

const ASKING_BOARD = {
  ...BOARD,
  columns: [
    {
      id: 'needs_input',
      heading: '입력 대기 1',
      cards: [
        {
          session_id: 'other',
          headline: 'project-b · 질문 · 1m',
          subtitle: 'Fix login bug',
          progress: '할 일 1/3 · Running tests',
          is_self: false,
          asks: [{ tool_use_id: 'tu-1', questions: [COLOR, PETS] }],
        },
      ],
    },
    ...BOARD.columns.slice(1),
  ],
}

/** What the asking session's AskUserQuestion call sends, in the tool's own spelling. */
const TOOL_QUESTIONS = [
  {
    question: 'Which color?',
    header: 'Color',
    multiSelect: false,
    options: [
      { label: 'Red', description: 'warm' },
      { label: 'Blue', description: 'cool' },
    ],
  },
]

type Run = { argv: readonly string[]; stdin?: string }

type World = {
  store: Record<string, unknown>
  clock: ReturnType<typeof mock.clock>
  runs: Run[]
  shown: { value: boolean }
  /** Answers the AskUserQuestion dialog the engine has open, as the person would. */
  dialog: { answer: ((answers: Record<string, string>) => void) | null }
  /** The file names in this session's inbox directory. */
  inbox: { files: string[] }
  messages: { value: { role: 'user' | 'assistant'; text: string; toolUses: never[] }[] }
  submitted: string[]
  aborted: string[]
  /** What `$.agent.list()` answers. */
  agents: { value: { id: string; description: string; type: string; status: string; teammateId?: string }[] }
}

type CliOut = { exitCode: number; stdout: string } | 'missing'

/** A CLI that answers later (a promise) stands for one still running. */
type FakeCli = (argv: readonly string[]) => CliOut | Promise<CliOut>

function boardAndAnswers(board: unknown): FakeCli {
  return argv =>
    argv[2] === 'answer'
      ? { exitCode: 0, stdout: JSON.stringify({ answers: ['Blue', 'Cat, Fish'] }) }
      : { exitCode: 0, stdout: JSON.stringify(board) }
}

/** Stands for the engine beneath the plugin: one session, its store, its clock, the CLI. */
function world(on: On, cli: FakeCli = boardAndAnswers(BOARD)): World {
  const store: Record<string, unknown> = {}
  const runs: Run[] = []
  const shown = { value: false }
  const dialog: World['dialog'] = { answer: null }
  const inbox: World['inbox'] = { files: [] }
  const messages: World['messages'] = { value: [] }
  const submitted: string[] = []
  const aborted: string[] = []
  const agents: World['agents'] = { value: [] }

  on('store.get', ($, e) => ({ value: store[e.key] }))
  on('store.set', ($, e) => {
    store[e.key] = JSON.parse(JSON.stringify(e.value))

    return { value: undefined }
  })
  on('store.delete', ($, e) => {
    delete store[e.key]

    return { value: undefined }
  })
  on('store.keys', () => ({ value: Object.keys(store) }))
  const clock = mock.clock(on, { now: START })
  on('session.id', () => ({ value: SELF }))
  on('session.cwd', () => ({ value: '/work/project-a' }))
  on('session.repo', () => ({ value: null }))
  on('command.register', () => ({ value: { command: 'kanban' } as never }))
  on('ui.open', () => {
    shown.value = true

    return { value: { isPlaced: true } }
  })
  on('ui.close', () => {
    shown.value = false

    return { value: undefined }
  })
  on('ui.panes', () => ({
    value: shown.value ? [{ id: 'kanban', title: '세션 칸반', isShown: true, isFocused: false, isPlaced: true }] : [],
  }))
  on('ui.invalidate', () => ({ value: undefined }))
  on('process.run', async ($, e) => {
    runs.push({ argv: e.argv, stdin: e.init?.stdin })
    const out = await cli(e.argv)

    if (out === 'missing') {
      return { deny: 'ENOENT: Executable not found in $PATH' }
    }

    return { value: { ...out, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('fs.list', () => ({
    value: inbox.files.map(name => ({ name, kind: 'file' as const, size: 1, mtimeMs: START, isLink: false })),
  }))
  on('session.messages', () => ({ value: messages.value as never }))
  on('agent.list', () => ({ value: agents.value as never }))
  on('turn.abort', ($, e) => {
    aborted.push(e.turnId)

    return { value: undefined }
  })
  on('tool.register', () => ({ value: { tool: 'mcp__atelier__kanban_board' } as never }))
  on('session.start', ($, e) => ({ cwd: e.cwd }))
  on('prompt.submit', ($, e) => {
    submitted.push(e.text)

    return { text: e.text }
  })
  on('turn.start', ($, e) => ({ turnId: e.turnId }))
  on('turn.complete', () => ({ text: '' }))
  on('session.end', ($, e) => ({ sessionId: e.sessionId }))
  on('classic.UserPromptSubmit', () => ({}))
  on('classic.PermissionRequest', () => ({}))
  on('classic.Notification', () => ({}))
  on('classic.PostToolUse', () => ({}))
  on('classic.PostToolUseFailure', () => ({}))
  on('classic.Stop', () => ({}))
  on('tool.call', ($, e) => {
    if (e.tool === 'AskUserQuestion') {
      return new Promise(resolve => {
        dialog.answer = answers => resolve({ result: { questions: e.questions, answers } })
      })
    }

    if (e.tool === 'TaskCreate') {
      return { result: { task: { id: '1', subject: e.subject } } }
    }

    return { result: {} }
  })

  return { store, clock, runs, shown, dialog, inbox, messages, submitted, aborted, agents }
}

type StoredRecord = {
  session_id: string
  cwd: string
  first_prompt: string | null
  title: string | null
  started_at: number
  heartbeat_at: number
  ended_reason: string | null
  events: { name: string; detail: string | null; at: number }[]
  activity?: unknown
  tasks?: unknown
  todos?: unknown
  asks?: Record<string, unknown>
  agents?: unknown
}

function stored(w: World): StoredRecord {
  return w.store[KEY] as StoredRecord
}

function boardRuns(w: World): Run[] {
  return w.runs.filter(r => r.argv[2] === 'board')
}

function eventNames(w: World): string[] {
  return stored(w).events.map(e => e.name)
}

const PANE = {
  plugin: 'atelier',
  component: 'Pane',
  requestId: 'kanban',
  props: {
    title: '세션 칸반',
    isFocused: false,
    bodyColumns: 100,
    placement: 'dock',
    scroll: { offset: 0, bodyRows: 30 },
    view: {},
  },
} as const

describe('kanban recording', () => {
  test('session.start creates the record once, and a reload does not append again', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    expect(stored(w)).toEqual({
      v: 1,
      session_id: SELF,
      cwd: '/work/project-a',
      repo: null,
      repo_root: null,
      remote: null,
      title: null,
      first_prompt: null,
      started_at: START,
      heartbeat_at: START,
      ended_reason: null,
      events: [{ name: 'session.start', detail: null, at: START }],
    })

    await $.turn.start({ text: 'hi', turnId: 't1' })
    await w.clock.set(START + 1_000)
    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })

    expect(eventNames(w)).toEqual(['session.start', 'turn.start'])
    expect(stored(w).heartbeat_at).toBe(START + 1_000)
  })

  test('the first typed prompt is kept, later ones and slash commands are not', async ($, on) => {
    const w = world(on)

    await $.prompt.submit({ text: 'fix the login bug' } as never)
    await $.prompt.submit({ text: 'and add a test' } as never)
    await $.prompt.submit({ text: '/kanban', command: { name: 'kanban', args: '' } } as never)

    expect(stored(w).first_prompt).toBe('fix the login bug')
    expect(eventNames(w)).toEqual(['prompt.submit', 'prompt.submit'])
  })

  test('a long first prompt is cut to bound the shared store', async ($, on) => {
    const w = world(on)

    await $.prompt.submit({ text: 'x'.repeat(2_000) } as never)

    expect(stored(w).first_prompt).toHaveLength(500)
  })

  test('the session title from UserPromptSubmit is kept without adding an event', async ($, on) => {
    const w = world(on)

    await $.classic.UserPromptSubmit({ prompt: 'hi', session_title: 'Fix login bug' } as never)

    expect(stored(w).title).toBe('Fix login bug')
    expect(eventNames(w)).toEqual([])
  })

  test('only the main loop turns are recorded', async ($, on) => {
    const w = world(on)

    await $.turn.start({ text: 'hi', turnId: 't1' })
    await $.turn.start({ text: 'sub', turnId: 't2', agentId: 'a1' })
    await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't2', agentId: 'a1', reason: 'answer' })
    await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't1', reason: 'answer' })

    expect(eventNames(w)).toEqual(['turn.start', 'turn.complete'])
  })

  test('permission requests and notifications keep their tool and type as detail', async ($, on) => {
    const w = world(on)

    await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: {} } as never)
    await $.classic.Notification({ message: 'm', notification_type: 'idle_prompt' } as never)
    await $.classic.PostToolUse({ tool_name: 'Bash', tool_input: {}, tool_response: {}, tool_use_id: 'u1' } as never)

    expect(stored(w).events.map(e => [e.name, e.detail])).toEqual([
      ['classic.PermissionRequest', 'Bash'],
      ['classic.Notification', 'idle_prompt'],
      ['classic.PostToolUse', 'Bash'],
    ])
    expect(stored(w).events.some(e => 'agent' in e)).toBe(false)
  })

  test('tool events a subagent raises are marked as the agent’s', async ($, on) => {
    const w = world(on)

    await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: {}, agent_id: 'a1' } as never)
    await $.classic.PostToolUse({ tool_name: 'Bash', tool_input: {}, tool_response: {}, tool_use_id: 'u1', agent_id: 'a1' } as never)
    await $.classic.PostToolUseFailure({ tool_name: 'Read', tool_input: {}, tool_use_id: 'u2', error: 'x', agent_id: 'a1' } as never)
    await $.classic.PostToolUse({ tool_name: 'Edit', tool_input: {}, tool_response: {}, tool_use_id: 'u3' } as never)

    expect(stored(w).events.map(e => [e.name, e.detail, e.agent ?? false])).toEqual([
      ['classic.PermissionRequest', 'Bash', true],
      ['classic.PostToolUse', 'Bash', true],
      ['classic.PostToolUseFailure', 'Read', true],
      ['classic.PostToolUse', 'Edit', false],
    ])
  })

  test('only the newest events are kept', async ($, on) => {
    const w = world(on)

    for (let i = 0; i < 12; i++) {
      await $.turn.start({ text: 'hi', turnId: `t${i}` })
    }

    expect(stored(w).events).toHaveLength(8)
  })

  test('session.end marks the record ended under the ending session id', async ($, on) => {
    const w = world(on)

    await $.turn.start({ text: 'hi', turnId: 't1' })
    await $.session.end({ reason: 'prompt_input_exit', sessionId: SELF } as never)

    expect(stored(w).ended_reason).toBe('prompt_input_exit')
    expect(eventNames(w)).toEqual(['turn.start', 'session.end'])
  })

  test('a later event revives a record /clear ended under the same id', async ($, on) => {
    const w = world(on)

    await $.session.end({ reason: 'clear', sessionId: SELF } as never)
    await $.turn.start({ text: 'hi', turnId: 't1' })

    expect(stored(w).ended_reason).toBeNull()
  })

  test('a session resumed after its end starts idle, keeping the conversation but no wait from the last run', async ($, on) => {
    const w = world(on)
    const prompts = [{ text: 'fix the login bug', origin: 'typed', at: START - 9_000 }]
    const tasks = { '1': { subject: 'Write tests', active: null, status: 'pending' } }

    w.store[KEY] = {
      v: 1,
      session_id: SELF,
      cwd: '/work/project-a',
      repo: 'project-a',
      title: 'Login fix',
      first_prompt: 'fix the login bug',
      prompts,
      last_answer: 'Shall I merge?',
      pending_context: 'Running the suite',
      turn_id: 't9',
      started_at: START - 10_000,
      heartbeat_at: START - 5_000,
      ended_reason: 'prompt_input_exit',
      events: [
        { name: 'turn.start', detail: null, at: START - 8_000 },
        { name: 'turn.complete', detail: null, at: START - 6_000 },
        { name: 'session.end', detail: 'prompt_input_exit', at: START - 5_000 },
      ],
      activity: { tool: 'Bash', input: { command: 'ls' }, at: START - 7_000 },
      recent: [{ tool: 'Bash', input: { command: 'ls' }, at: START - 7_000 }],
      permission: { tool: 'Bash', input: { command: 'rm' }, at: START - 7_000, id: 'p1' },
      decision: { ask: 'Merge?', recap: null, options: ['Yes', 'No'], at: START - 6_000 },
      asks: { 'tu-1': { questions: [COLOR], at: START - 7_000 } },
      agents: [{ status: 'running', teammate: false }],
      tasks,
    }

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })

    expect(w.store[KEY]).toMatchObject({
      title: 'Login fix',
      first_prompt: 'fix the login bug',
      prompts,
      last_answer: 'Shall I merge?',
      tasks,
      ended_reason: null,
      started_at: START,
      heartbeat_at: START,
      events: [{ name: 'session.start', detail: null, at: START }],
      pending_context: null,
      turn_id: null,
      activity: null,
      recent: [],
      permission: null,
      decision: null,
      asks: {},
      agents: [],
    })
  })

  test('the spinner is recorded once after each other event, never on its own', async ($, on) => {
    const w = world(on)

    on('ui.render', ($, e) => {
      const { Text } = $.ui.resolve(e)

      return Text({ children: ['engine spinner'] })
    })
    const spinner = {
      plugin: 'atelier',
      component: 'Spinner',
      requestId: SELF,
      surface: 'terminal',
      props: { word: 'Baking', message: null, suffix: '…', mode: 'tool-use' },
    } as const

    const before = await $.ui.mount(spinner)
    await before.unmount()
    expect(w.store[KEY]).toBeUndefined()

    await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: {} } as never)
    for (let i = 0; i < 3; i++) {
      const ui = await $.ui.mount(spinner)

      expect((await ui.find({ type: 'Text', text: /engine spinner/ }))?.text).toBe('engine spinner')
      await ui.unmount()
    }
    await w.clock.settle()

    expect(eventNames(w)).toEqual(['classic.PermissionRequest', 'ui.spinner'])
  })

  test('the heartbeat refreshes the record without adding events', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await w.clock.advance(30_000)

    expect(stored(w).heartbeat_at).toBe(START + 30_000)
    expect(eventNames(w)).toEqual(['session.start'])
  })
})

const TEAM = [
  { id: 'a1', description: 'Review', type: 'general-purpose', status: 'running' },
  { id: 'a2', description: 'Scout', type: 'teammate', status: 'idle', teammateId: 'scout@team' },
]

function stop(types: string[], agentId?: string): never {
  const background_tasks = types.map((type, i) => ({ id: `t${i}`, type, status: 'running', description: 'd' }))

  return { stop_hook_active: false, background_tasks, ...(agentId === undefined ? {} : { agent_id: agentId }) } as never
}

function turnEnds(w: World): (string | null)[] {
  return stored(w)
    .events.filter(e => e.name === 'turn.complete')
    .map(e => e.detail)
}

describe('kanban background work', () => {
  test('a turn end carries the task types its Stop left running, and the agents as listed then', async ($, on) => {
    const w = world(on)
    w.agents.value = TEAM

    await $.turn.start({ text: 'fan out', turnId: 't1' })
    await $.classic.Stop(stop(['subagent', 'shell', 'teammate']))
    await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't1', reason: 'answer' })

    expect(turnEnds(w)).toEqual(['subagent,shell,teammate'])
    expect(stored(w).agents).toEqual([
      { status: 'running', teammate: false },
      { status: 'idle', teammate: true },
    ])
  })

  test('a Stop that comes after the turn end fills that turn end', async ($, on) => {
    const w = world(on)

    await $.turn.start({ text: 'fan out', turnId: 't1' })
    await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't1', reason: 'answer' })
    await $.classic.Stop(stop(['workflow']))

    expect(turnEnds(w)).toEqual(['workflow'])
    expect(eventNames(w)).toEqual(['turn.start', 'turn.complete'])
  })

  test('a turn end with nothing left, or no Stop of its own, carries no detail', async ($, on) => {
    const w = world(on)

    await $.turn.start({ text: 'a', turnId: 't1' })
    await $.classic.Stop(stop([]))
    await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't1', reason: 'answer' })
    await $.turn.start({ text: 'b', turnId: 't2' })
    await $.classic.Stop(stop(['subagent']))
    await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't2', reason: 'answer' })
    await $.turn.start({ text: 'c', turnId: 't3' })
    await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't3', reason: 'aborted' } as never)

    expect(turnEnds(w)).toEqual([null, 'subagent', null])
  })

  test('a subagent loop Stop is not the main loop turn end', async ($, on) => {
    const w = world(on)

    await $.turn.start({ text: 'a', turnId: 't1' })
    await $.classic.Stop(stop(['subagent'], 'a1'))
    await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't1', reason: 'answer' })

    expect(turnEnds(w)).toEqual([null])
  })

  test('the heartbeat keeps the agent list current', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    w.agents.value = [{ ...TEAM[1], status: 'running' }]
    await w.clock.advance(30_000)

    expect(stored(w).agents).toEqual([{ status: 'running', teammate: true }])
  })
})

describe('kanban progress and questions', () => {
  test('a main loop tool call is kept as the activity with its string fields cut, until the turn ends', async ($, on) => {
    const w = world(on)

    await $.tool.call({ tool: 'Bash', command: 'x'.repeat(300), description: 'Run tests', timeout: 5 })

    expect(stored(w).activity).toEqual({
      tool: 'Bash',
      input: { command: 'x'.repeat(200), description: 'Run tests' },
      at: START,
    })
    expect(eventNames(w)).toEqual([])

    await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't1', reason: 'answer' })

    expect(stored(w).activity).toBeNull()
  })

  test('TodoWrite replaces the to-do list and the Task tools merge into the task list', async ($, on) => {
    const w = world(on)

    await $.tool.call({
      tool: 'TodoWrite',
      todos: [
        { content: 'a', status: 'completed', activeForm: 'Doing a' },
        { content: 'b', status: 'in_progress', activeForm: 'Doing b' },
      ],
    })
    await $.tool.call({ tool: 'TaskCreate', subject: 'Write tests', description: 'd', activeForm: 'Writing tests' })
    await $.tool.call({ tool: 'TaskUpdate', taskId: '1', status: 'in_progress' })
    await $.tool.call({ tool: 'TaskUpdate', taskId: '9', status: 'completed' })

    expect(stored(w).todos).toEqual([
      { status: 'completed', active: 'Doing a' },
      { status: 'in_progress', active: 'Doing b' },
    ])
    expect(stored(w).tasks).toEqual({
      '1': { subject: 'Write tests', active: 'Writing tests', status: 'in_progress' },
      '9': { subject: '', active: null, status: 'completed' },
    })
  })

  test('an open question is recorded and a kanban answer closes it as the person would', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS })
    await w.clock.settle()

    const [id] = Object.keys(stored(w).asks ?? {})
    expect(stored(w).asks?.[id as string]).toEqual({ at: START, questions: [COLOR] })

    w.store[`kanban:answer:${SELF}:${id}`] = { v: 1, answers: ['Blue'] }
    await w.clock.advance(2_000)
    const r = await call

    expect(r.result).toEqual({ questions: TOOL_QUESTIONS, answers: { 'Which color?': 'Blue' } })
    expect(stored(w).asks).toEqual({})
    expect(eventNames(w)).toContain('kanban.answer')
    expect(w.store[`kanban:answer:${SELF}:${id}`]).toBeUndefined()
  })

  test('an answer in the dialog wins and leaves no kanban event', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS })
    await w.clock.settle()
    w.dialog.answer?.({ 'Which color?': 'Red' })
    const r = await call

    expect(r.result).toEqual({ questions: TOOL_QUESTIONS, answers: { 'Which color?': 'Red' } })
    expect(stored(w).asks).toEqual({})
    expect(eventNames(w)).not.toContain('kanban.answer')
  })

  test('an answer that is not a list of strings is left unread', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    void $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS })
    await w.clock.settle()
    const [id] = Object.keys(stored(w).asks ?? {})
    w.store[`kanban:answer:${SELF}:${id}`] = { v: 1, answers: [1] }
    await w.clock.advance(2_000)

    expect(Object.keys(stored(w).asks ?? {})).toEqual([id])
    w.dialog.answer?.({ 'Which color?': 'Red' })
    await w.clock.settle()
  })
})

describe('kanban pane', () => {
  test('/kanban opens the pane and asks the CLI for the board of every kanban record', async ($, on) => {
    const w = world(on)

    w.store['unrelated'] = { keep: true }
    w.store['kanban:session:gone'] = { v: 1, session_id: 'gone', events: [] }
    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })

    const ran = await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)

    expect(ran).toEqual({ text: '세션 칸반을 열었어요.' })
    expect(boardRuns(w)).toHaveLength(1)
    expect(boardRuns(w)[0]?.argv).toEqual([
      'atelier',
      'kanban',
      'board',
      '--now',
      String(START),
      '--stale-after',
      '2m',
      '--expire-after',
      '24h',
      '--width',
      '0',
      '--self',
      SELF,
    ])
    const sent = JSON.parse(boardRuns(w)[0]?.stdin ?? '{}') as { sessions: { session_id: string }[] }
    expect(sent.sessions.map(r => r.session_id).sort()).toEqual(['gone', SELF])
    expect(w.store['kanban:session:gone']).toBeUndefined()
    expect(w.store['unrelated']).toEqual({ keep: true })
  })

  test('an expired session takes its leftover kanban answers with it', async ($, on) => {
    const w = world(on)

    w.store['kanban:answer:gone:tu-1'] = { v: 1, answers: ['Red'] }
    w.store['kanban:answer:other:tu-2'] = { v: 1, answers: ['Red'] }
    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)

    expect(w.store['kanban:answer:gone:tu-1']).toBeUndefined()
    expect(w.store['kanban:answer:other:tu-2']).toBeDefined()
  })

  test('/kanban again closes the pane', async ($, on) => {
    const w = world(on)

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ran = await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)

    expect(ran).toEqual({ text: '세션 칸반을 닫았어요.' })
    expect(w.shown.value).toBe(false)
  })

  test('the timer refreshes only while the pane is shown', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await w.clock.advance(6_000)
    expect(boardRuns(w)).toHaveLength(0)

    w.shown.value = true
    await w.clock.advance(2_000)
    expect(boardRuns(w)).toHaveLength(1)
  })

  for (const surface of ['terminal', 'desktop'] as const) {
    test(`the board draws the CLI's headings and lines on ${surface}`, async ($, on) => {
      world(on)

      await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
      const ui = await $.ui.mount({ ...PANE, surface })

      expect((await ui.find({ type: 'Text', text: /입력 대기 1/ }))?.text).toBe('입력 대기 1')
      expect((await ui.find({ type: 'Text', text: /project-b/ }))?.text).toBe('project-b · 권한 요청: Bash · 3m')
      expect((await ui.find({ type: 'Text', text: /Fix login bug/ }))?.text).toBe('Fix login bug')
      expect((await ui.find({ type: 'Text', text: /project-a/ }))?.text).toBe('▶ project-a · 5s')
      expect((await ui.find({ type: 'Text', text: /응답 없음 0/ }))?.text).toBe('응답 없음 0')
      await ui.unmount()
    })
  }

  test('the board is asked again for the width the pane is drawn at', async ($, on) => {
    const w = world(on)

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
    await w.clock.settle()

    const widths = boardRuns(w).map(r => r.argv[r.argv.indexOf('--width') + 1])
    expect(widths).toEqual(['0', '100'])
    await ui.unmount()
  })

  test('a missing CLI draws a hint instead of the board', async ($, on) => {
    world(on, () => 'missing')

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

    expect((await ui.find({ type: 'Text', text: /atelier CLI/ }))?.text).toContain('/atelier:update')
    await ui.unmount()
  })

  for (const [label, out, shown] of [
    ['a non-zero exit', { exitCode: 2, stdout: '' }, /exit 2/],
    ['stdout that is not JSON', { exitCode: 0, stdout: 'nope' }, /출력을 읽지 못했어요/],
    ['an error from the CLI', { exitCode: 0, stdout: JSON.stringify({ error: 'stdin is not JSON' }) }, /stdin is not JSON/],
  ] as const) {
    test(`${label} draws the error and deletes nothing`, async ($, on) => {
      const w = world(on, () => out)

      w.store['kanban:session:gone'] = { v: 1, session_id: 'gone', events: [] }
      await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
      const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

      expect((await ui.find({ type: 'Text', text: shown }))?.text).toMatch(shown)
      expect(w.store['kanban:session:gone']).toBeDefined()
      await ui.unmount()
    })
  }

  test('a stacked layout from the CLI still draws every column', async ($, on) => {
    world(on, () => ({
      exitCode: 0,
      stdout: JSON.stringify({ ...BOARD, layout: { direction: 'column', column_width: 40, column_gap: 1 } }),
    }))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, props: { ...PANE.props, bodyColumns: 40 }, surface: 'terminal' })

    expect((await ui.find({ type: 'Text', text: /입력 대기 1/ }))?.text).toBe('입력 대기 1')
    expect((await ui.find({ type: 'Text', text: /응답 없음 0/ }))?.text).toBe('응답 없음 0')
    await ui.unmount()
  })

  for (const surface of ['terminal', 'desktop'] as const) {
    test(`a waiting question is drawn under the board with its options on ${surface}`, async ($, on) => {
      world(on, boardAndAnswers(ASKING_BOARD))

      await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
      const ui = await $.ui.mount({ ...PANE, surface })

      expect((await ui.find({ type: 'Text', text: /할 일 1\/3/ }))?.text).toBe('할 일 1/3 · Running tests')
      expect((await ui.find({ type: 'Text', text: /^질문$/ }))?.text).toBe('질문')
      expect((await ui.find({ type: 'Text', text: /project-b · 질문/ }))?.text).toBe('project-b · 질문 · 1m')
      expect((await ui.find({ type: 'Text', text: /^1\/2 · / }))?.text).toBe('1/2 · Color')
      expect((await ui.find({ type: 'Text', text: /Which color/ }))?.text).toBe('Which color?')
      expect((await ui.find({ key: 'ask:other:tu-1:0:0' }))?.text).toBe('○ Red  warm')
      expect((await ui.find({ key: 'ask:other:tu-1:1:0' }))?.text).toBe('☐ Cat')
      expect(await ui.find({ key: 'ask:other:tu-1:0:1' })).toBeDefined()
      expect(await ui.find({ key: 'ask:other:tu-1:1:text' })).toBeDefined()
      expect(await ui.find({ key: 'ask:other:tu-1:send' })).toBeDefined()
      await ui.unmount()
    })
  }

  test("a card's detail shows only that session's questions", async ($, on) => {
    world(on, boardAndAnswers(ASKING_BOARD))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
    await ui.press({ key: `card:${SELF}` })
    await ui.unmount()

    const other = await $.ui.mount({ ...PANE, surface: 'desktop' })
    expect(await other.find({ key: `detail:${SELF}` })).toBeDefined()
    expect(await other.find({ key: 'ask:other:tu-1:send' })).toBeUndefined()
    await other.press({ key: `detail:${SELF}:back` })
    await other.unmount()

    const board = await $.ui.mount({ ...PANE, surface: 'desktop' })
    await board.press({ key: 'card:other' })
    await board.unmount()

    const asking = await $.ui.mount({ ...PANE, surface: 'desktop' })
    expect(await asking.find({ key: 'detail:other' })).toBeDefined()
    expect(await asking.find({ key: 'ask:other:tu-1:send' })).toBeDefined()
    await asking.unmount()
  })

  test('the picks go to atelier kanban answer and its answers to the asking session', async ($, on) => {
    const w = world(on, boardAndAnswers(ASKING_BOARD))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

    await ui.press({ key: 'ask:other:tu-1:0:0' })
    await ui.press({ key: 'ask:other:tu-1:0:1' })
    for (const option of [2, 0, 1, 1]) {
      await ui.press({ key: `ask:other:tu-1:1:${option}` })
    }
    await ui.press({ key: 'ask:other:tu-1:send' })
    await w.clock.settle()

    const sent = w.runs.find(r => r.argv[2] === 'answer')
    expect(sent?.argv).toEqual(['atelier', 'kanban', 'answer'])
    expect(JSON.parse(sent?.stdin ?? '{}')).toEqual({
      questions: [COLOR, PETS],
      picks: [
        { options: ['Blue'], text: null },
        { options: ['Fish', 'Cat'], text: null },
      ],
    })
    expect(w.store['kanban:answer:other:tu-1']).toEqual({ v: 1, answers: ['Blue', 'Cat, Fish'] })
    await ui.unmount()

    const after = await $.ui.mount({ ...PANE, surface: 'terminal' })
    expect((await after.find({ key: 'ask:other:tu-1:0:1' }))?.text).toBe('● Blue  cool')
    expect((await after.find({ key: 'ask:other:tu-1:1:2' }))?.text).toBe('☑ Fish')
    expect((await after.find({ key: 'ask:other:tu-1:1:1' }))?.text).toBe('☐ Dog')
    expect((await after.find({ type: 'Text', text: /보냈어요/ }))?.text).toBe('보냈어요 — 세션이 받으면 사라져요')
    expect(await after.find({ key: 'ask:other:tu-1:send' })).toBeUndefined()
    await after.unmount()
  })

  test('typed text replaces the picked options of that question', async ($, on) => {
    const w = world(on, boardAndAnswers(ASKING_BOARD))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

    await ui.press({ key: 'ask:other:tu-1:0:0' })
    await ui.input({ key: 'ask:other:tu-1:0:text', text: 'Green' })
    await ui.press({ key: 'ask:other:tu-1:send' })
    await w.clock.settle()

    const sent = w.runs.find(r => r.argv[2] === 'answer')
    expect(JSON.parse(sent?.stdin ?? '{}').picks[0]).toEqual({ options: [], text: 'Green' })
    await ui.unmount()
  })

  test('a refused answer shows the reason and writes nothing', async ($, on) => {
    const w = world(on, argv =>
      argv[2] === 'answer'
        ? { exitCode: 0, stdout: JSON.stringify({ error: '답하지 않은 질문이 있어요: Pets' }) }
        : { exitCode: 0, stdout: JSON.stringify(ASKING_BOARD) },
    )

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

    await ui.press({ key: 'ask:other:tu-1:send' })
    await w.clock.settle()
    await ui.unmount()

    const after = await $.ui.mount({ ...PANE, surface: 'terminal' })
    expect((await after.find({ type: 'Text', text: /답하지 않은/ }))?.text).toBe('답하지 않은 질문이 있어요: Pets')
    expect(w.store['kanban:answer:other:tu-1']).toBeUndefined()
    expect(await after.find({ key: 'ask:other:tu-1:send' })).toBeDefined()
    await after.unmount()
  })
})

const INBOX_DIR = '/home/me/.claude/atelier/kanban/inbox/self-session'

/** Answers each kanban subcommand from a table; a missing entry answers `{}`. */
function routes(table: Record<string, unknown>): FakeCli {
  return argv => {
    const name = argv[2] === 'inbox' ? `inbox ${argv[3]}` : (argv[2] ?? '')

    return { exitCode: 0, stdout: JSON.stringify(table[name] ?? {}) }
  }
}

const DETAIL_BOARD = {
  ...BOARD,
  columns: [
    { id: 'needs_input', heading: '입력 대기 0', cards: [] },
    {
      id: 'working',
      heading: '작업 중 1',
      cards: [
        {
          session_id: 'busy',
          headline: 'api · 2m',
          subtitle: 'Ship it',
          is_self: false,
          turn_id: 't-9',
          actions: ['stop'],
          detail: { sections: [{ title: '요청', lines: ['Ship it'] }], decision: null, delivery: null },
        },
      ],
    },
    {
      id: 'idle',
      heading: '쉬는 중 2',
      cards: [
        {
          session_id: 'calm',
          headline: 'web · 5m',
          subtitle: 'Fix CSS',
          last_line: '직전: Fixed the header.',
          is_self: false,
          actions: ['send'],
          detail: {
            sections: [{ title: '직전 응답', lines: ['Fixed the header.'] }],
            decision: null,
            delivery: '보낸 요청: 전달됐어요 · 4s',
          },
        },
        {
          session_id: 'wrap',
          headline: 'docs · 결정 요청 · 1m',
          subtitle: 'Draft spec',
          is_self: false,
          actions: ['decide', 'send'],
          detail: {
            sections: [],
            decision: { ask: '초안대로 진행할까요?', recap: '초안을 정리했어요.', options: ['초안대로 진행', '고칠 행 있음'] },
            delivery: null,
          },
        },
      ],
    },
    { id: 'stale', heading: '응답 없음 0', cards: [] },
  ],
  repos: [{ label: 'api', root: '/w/api', sessions: [] }],
}

describe('kanban details and requests', () => {
  test('every prompt is kept with where it came from, the newest five', async ($, on) => {
    const w = world(on)

    await $.prompt.submit({ text: 'fix the login bug', origin: { kind: 'composer' } } as never)
    await $.prompt.submit({ text: '<task-notification>done</task-notification>', origin: { kind: 'task-notification' } } as never)
    for (const n of [1, 2, 3, 4]) {
      await $.prompt.submit({ text: `step ${n}`, origin: { kind: 'sdk' } } as never)
    }

    const prompts = (stored(w) as Record<string, unknown>).prompts as { text: string; origin: string }[]
    expect(stored(w).first_prompt).toBe('fix the login bug')
    expect(prompts.map(p => [p.origin, p.text])).toEqual([
      ['task-notification', '<task-notification>done</task-notification>'],
      ['sdk', 'step 1'],
      ['sdk', 'step 2'],
      ['sdk', 'step 3'],
      ['sdk', 'step 4'],
    ])
    expect(eventNames(w).filter(n => n === 'prompt.submit')).toHaveLength(6)
  })

  test('a long prompt is cut to bound the shared store', async ($, on) => {
    const w = world(on)

    await $.prompt.submit({ text: 'x'.repeat(5_000), origin: { kind: 'composer' } } as never)

    const prompts = (stored(w) as Record<string, unknown>).prompts as { text: string }[]
    expect(prompts[0]?.text).toHaveLength(1_000)
  })

  test('a turn keeps its id, its recent tools and its reply', async ($, on) => {
    const w = world(on)
    const rec = () => stored(w) as Record<string, unknown>

    await $.turn.start({ text: 'go', turnId: 't-1' })
    for (const n of [1, 2, 3, 4, 5, 6]) {
      await $.tool.call({ tool: 'Read', file_path: `/w/f${n}.ts` } as never)
    }
    expect(rec().turn_id).toBe('t-1')
    expect((rec().recent as { input: { file_path: string } }[]).map(a => a.input.file_path)).toEqual([
      '/w/f2.ts',
      '/w/f3.ts',
      '/w/f4.ts',
      '/w/f5.ts',
      '/w/f6.ts',
    ])

    await $.turn.complete({ turnId: 't-1', answer: 'All done.', durationMs: 1, isAborted: false } as never)
    expect(rec().turn_id).toBeNull()
    expect(rec().last_answer).toBe('All done.')
  })

  test('a WrapUp that asks leaves a decision until the next turn', async ($, on) => {
    const w = world(on)
    const rec = () => stored(w) as Record<string, unknown>

    await $.tool.call({
      tool: 'WrapUp',
      recap: '초안을 정리했어요.',
      ask: '초안대로 진행할까요?',
      blocked: true,
      actions: [{ label: '초안대로 진행' }, { label: '고칠 행 있음' }, { action: 'verify' }],
    } as never)
    expect(rec().decision).toEqual({
      ask: '초안대로 진행할까요?',
      recap: '초안을 정리했어요.',
      options: ['초안대로 진행', '고칠 행 있음'],
      at: START,
    })

    await $.turn.start({ text: 'go', turnId: 't-2' })
    expect(rec().decision).toBeNull()
  })

  test('a WrapUp without a question leaves no decision', async ($, on) => {
    const w = world(on)

    await $.tool.call({ tool: 'WrapUp', recap: 'Done.' } as never)
    expect((stored(w) as Record<string, unknown>).decision).toBeUndefined()
  })

  test('a permission request keeps what it asks for and the reply before it, until the tool ends', async ($, on) => {
    const w = world(on)
    const rec = () => stored(w) as Record<string, unknown>

    w.messages.value = [
      { role: 'user', text: 'release', toolUses: [] },
      { role: 'assistant', text: 'Tests pass. Publishing.', toolUses: [] },
    ]
    await $.classic.PermissionRequest({
      tool_name: 'Bash',
      tool_input: { command: 'npm publish', timeout: 5 },
      permission_mode: 'default',
    } as never)
    expect(rec().permission).toEqual({ id: null, tool: 'Bash', input: { command: 'npm publish' }, at: START })
    expect(rec().pending_context).toBe('Tests pass. Publishing.')
    expect(rec().permission_mode).toBe('default')

    await $.classic.PostToolUse({ tool_name: 'Bash', tool_input: {}, tool_response: {} } as never)
    expect(rec().permission).toBeNull()
  })

  test('the inbox is found at start and taken only when a request is waiting', async ($, on) => {
    const w = world(
      on,
      routes({
        'inbox dir': { dir: INBOX_DIR },
        'inbox take': {
          actions: [{ kind: 'prompt', text: 'Run lint', body: 'Run lint', from: 'pane' }],
          results: [{ id: 'r1', kind: 'prompt', status: 'delivered' }],
        },
      }),
    )

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    expect(w.runs.map(r => r.argv.slice(2, 4))).toEqual([['inbox', 'dir']])

    await w.clock.advance(4_000)
    expect(w.runs.filter(r => r.argv[3] === 'take')).toHaveLength(0)

    w.inbox.files = ['.123.tmp', '123-1.json']
    await w.clock.advance(2_000)
    const take = w.runs.find(r => r.argv[3] === 'take')
    expect(take?.argv).toEqual([
      'atelier',
      'kanban',
      'inbox',
      'take',
      '--session',
      SELF,
      '--now',
      String(START + 6_000),
      '--ttl',
      '60s',
    ])
    expect(w.submitted).toEqual(['Run lint'])
    const rec = stored(w) as Record<string, unknown>
    expect((rec.prompts as { text: string; origin: string }[]).at(-1)).toEqual({ text: 'Run lint', origin: 'kanban', at: START + 6_000 })
    expect(eventNames(w).at(-1)).toBe('prompt.submit')
    expect(rec.inbox).toEqual({ id: 'r1', kind: 'prompt', status: 'delivered', at: START + 6_000 })
  })

  test('a stop for the running turn aborts it', async ($, on) => {
    const w = world(
      on,
      routes({
        'inbox dir': { dir: INBOX_DIR },
        'inbox take': {
          actions: [{ kind: 'stop', turn_id: 't-7' }],
          results: [{ id: 'r2', kind: 'stop', status: 'delivered' }],
        },
      }),
    )

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.turn.start({ text: 'go', turnId: 't-7' })
    w.inbox.files = ['1-1.json']
    await w.clock.advance(2_000)

    const take = w.runs.find(r => r.argv[3] === 'take')
    expect(take?.argv.slice(-2)).toEqual(['--turn', 't-7'])
    expect(w.aborted).toEqual(['t-7'])
  })

  test('an expired session also loses its inbox', async ($, on) => {
    const w = world(on)

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    expect(w.runs.find(r => r.argv[3] === 'clear')?.argv).toEqual(['atelier', 'kanban', 'inbox', 'clear', '--session', 'gone'])
  })

  test('the orchestrator reads the board grouped by repository through the board tool', async ($, on) => {
    world(on, routes({ board: DETAIL_BOARD }))

    const r = await $.tool.call({ tool: 'mcp__atelier__kanban_board' } as never)
    const out = JSON.parse((r as { result: string }).result) as Record<string, unknown>
    expect(out).toEqual({ repos: [{ label: 'api', root: '/w/api', sessions: [] }] })
  })

  for (const surface of ['terminal', 'desktop'] as const) {
    test(`pressing an idle card opens its detail and sends a prompt on ${surface}`, async ($, on) => {
      const w = world(on, routes({ board: DETAIL_BOARD, send: { queued: { session_id: 'calm', id: 'r', kind: 'prompt' } } }))

      await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
      const ui = await $.ui.mount({ ...PANE, surface })

      expect((await ui.find({ type: 'Text', text: /직전: / }))?.text).toBe('직전: Fixed the header.')
      expect(await ui.find({ key: 'detail:calm:send' })).toBeUndefined()

      await ui.press({ key: 'card:calm' })
      await ui.unmount()
      const opened = await $.ui.mount({ ...PANE, surface })
      expect((await opened.find({ type: 'Text', text: /보낸 요청/ }))?.text).toBe('보낸 요청: 전달됐어요 · 4s')
      await opened.input({ key: 'detail:calm:send', text: 'Add dark mode' })
      await w.clock.settle()
      await opened.unmount()
      const after = await $.ui.mount({ ...PANE, surface })

      expect(w.runs.find(r => r.argv[2] === 'send')?.argv).toEqual([
        'atelier',
        'kanban',
        'send',
        '--session',
        'calm',
        '--from',
        'pane',
        '--from-label',
        SELF,
        '--text',
        'Add dark mode',
      ])
      expect((await after.find({ type: 'Text', text: /^보냈어요/ }))?.text).toBe('보냈어요 — 세션이 받으면 시작해요')
      await after.unmount()
    })
  }

  test('stopping a working card asks once more before it sends the stop', async ($, on) => {
    const w = world(on, routes({ board: DETAIL_BOARD, stop: { queued: { session_id: 'busy', id: 'r', kind: 'stop' } } }))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

    await ui.press({ key: 'card:busy' })
    await ui.unmount()
    const opened = await $.ui.mount({ ...PANE, surface: 'terminal' })
    await opened.press({ key: 'detail:busy:stop' })
    expect(w.runs.find(r => r.argv[2] === 'stop')).toBeUndefined()
    await opened.unmount()

    const confirming = await $.ui.mount({ ...PANE, surface: 'terminal' })
    await confirming.press({ key: 'detail:busy:stop-yes' })
    await w.clock.settle()
    await confirming.unmount()
    expect(w.runs.find(r => r.argv[2] === 'stop')?.argv).toEqual([
      'atelier',
      'kanban',
      'stop',
      '--session',
      'busy',
      '--turn',
      't-9',
    ])
  })

  test('a decision card sends the option pressed as the reply', async ($, on) => {
    const w = world(on, routes({ board: DETAIL_BOARD, send: { queued: { session_id: 'wrap', id: 'r', kind: 'prompt' } } }))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })

    await ui.press({ key: 'card:wrap' })
    await ui.unmount()
    const opened = await $.ui.mount({ ...PANE, surface: 'desktop' })
    expect((await opened.find({ type: 'Text', text: /진행할까요/ }))?.text).toBe('초안대로 진행할까요?')
    await opened.press({ key: 'detail:wrap:option:1' })
    await w.clock.settle()

    expect(w.runs.find(r => r.argv[2] === 'send')?.argv.at(-1)).toBe('고칠 행 있음')
    await opened.unmount()
  })

  test('a refused send shows why', async ($, on) => {
    world(on, routes({ board: DETAIL_BOARD, send: { error: '보낼 글이 비어 있어요' } }))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

    await ui.press({ key: 'card:calm' })
    await ui.unmount()
    const opened = await $.ui.mount({ ...PANE, surface: 'terminal' })
    await opened.input({ key: 'detail:calm:send', text: ' ' })
    await opened.unmount()
    const after = await $.ui.mount({ ...PANE, surface: 'terminal' })
    expect((await after.find({ type: 'Text', text: /비어 있어요/ }))?.text).toBe('보낼 글이 비어 있어요')
    await after.unmount()
  })

  for (const [label, layout, columns] of [
    ['a side-by-side', { direction: 'row', column_width: 24, column_gap: 1 }, 100],
    ['a stacked', { direction: 'column', column_width: 40, column_gap: 1 }, 40],
  ] as const) {
    test(`in ${label} pane an opened card takes the board’s place until going back to it`, async ($, on) => {
      world(on, routes({ board: { ...DETAIL_BOARD, layout } }))

      await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
      const pane = { ...PANE, props: { ...PANE.props, bodyColumns: columns }, surface: 'terminal' as const }
      const ui = await $.ui.mount(pane)

      await ui.press({ key: 'card:busy' })
      await ui.unmount()
      const opened = await $.ui.mount(pane)
      expect(await opened.find({ type: 'Text', text: /쉬는 중 2/ })).toBeUndefined()
      expect((await opened.find({ type: 'Text', text: /Ship it/ }))?.text).toBe('Ship it')
      expect((await opened.find({ key: 'detail:busy:back' }))?.text).toBe('← 보드')
      await opened.press({ key: 'detail:busy:back' })
      await opened.unmount()
      const back = await $.ui.mount(pane)
      expect((await back.find({ type: 'Text', text: /쉬는 중 2/ }))?.text).toBe('쉬는 중 2')
      expect(await back.find({ key: 'detail:busy:back' })).toBeUndefined()
      await back.unmount()
    })
  }

  for (const surface of ['terminal', 'desktop'] as const) {
    test(`each card is drawn as its own bordered box on ${surface}`, async ($, on) => {
      world(on, routes({ board: DETAIL_BOARD }))

      await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
      const ui = await $.ui.mount({ ...PANE, surface })

      for (const id of ['busy', 'calm', 'wrap']) {
        const box = await ui.find({ type: 'Box', key: id })
        expect(box?.props.borderStyle).toBe('round')
        expect(box?.props.borderDimColor).toBe(true)
      }
      await ui.unmount()
    })
  }

  test('an opened card that leaves the board takes the pane back to the board', async ($, on) => {
    const board = { value: DETAIL_BOARD as typeof DETAIL_BOARD }
    const w = world(on, argv => routes({ board: board.value })(argv))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
    await ui.press({ key: 'card:busy' })
    await ui.unmount()

    board.value = {
      ...DETAIL_BOARD,
      columns: DETAIL_BOARD.columns.map(c => (c.id === 'working' ? { ...c, heading: '작업 중 0', cards: [] } : c)),
    }
    w.shown.value = true
    await w.clock.advance(2_000)

    const after = await $.ui.mount({ ...PANE, surface: 'terminal' })
    expect((await after.find({ type: 'Text', text: /쉬는 중 2/ }))?.text).toBe('쉬는 중 2')
    expect(await after.find({ key: 'detail:busy:back' })).toBeUndefined()
    await after.unmount()
  })
})

describe('kanban permission answers', () => {
  function permitWorld(on: On, decisions: unknown[]) {
    let calls = 0
    const w = world(on, argv => {
      if (argv[2] === 'inbox' && argv[3] === 'dir') {
        return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
      }

      if (argv[2] === 'inbox' && argv[3] === 'wait-permit') {
        const decision = decisions[Math.min(calls, decisions.length - 1)]
        calls += 1

        return { exitCode: 0, stdout: JSON.stringify({ decision }) }
      }

      return { exitCode: 0, stdout: JSON.stringify(BOARD) }
    })

    return { w, calls: () => calls }
  }

  test('a permission request waits for a kanban answer and gives it as the decision', async ($, on) => {
    const { w } = permitWorld(on, [null, 'allow'])

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.turn.start({ text: 'go', turnId: 't-1' })
    const r = await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: { command: 'npm publish' } } as never)

    expect(r).toEqual({ decision: { behavior: 'allow' } })
    const waits = w.runs.filter(x => x.argv[3] === 'wait-permit')
    expect(waits).toHaveLength(2)
    expect(waits[0]?.argv).toEqual([
      'atelier',
      'kanban',
      'inbox',
      'wait-permit',
      '--session',
      SELF,
      '--request',
      `t-1-${START}`,
      '--now',
      String(START),
      '--ttl',
      '60s',
      '--for',
      '2s',
    ])
    expect(eventNames(w).at(-1)).toBe('kanban.answer')
    expect((stored(w) as Record<string, unknown>).permission).toBeNull()
  })

  test('a deny from the kanban refuses the tool with a reason', async ($, on) => {
    permitWorld(on, ['deny'])

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const r = await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: {} } as never)

    expect(r).toEqual({ decision: { behavior: 'deny', message: '사람이 칸반에서 이 도구 실행을 거부했어요' } })
  })

  test('an answer in the dialog ends the wait without a decision', async ($, on) => {
    let release: () => void = () => undefined
    let started: () => void = () => undefined
    const waiting = new Promise<void>(resolve => {
      started = resolve
    })
    let waits = 0
    world(on, argv => {
      if (argv[3] === 'dir') {
        return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
      }

      if (argv[3] === 'wait-permit') {
        waits += 1
        started()

        return new Promise(resolve => {
          release = () => resolve({ exitCode: 0, stdout: JSON.stringify({ decision: null }) })
        })
      }

      return { exitCode: 0, stdout: JSON.stringify(BOARD) }
    })

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const pending = $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: {} } as never)
    await waiting
    await $.classic.PostToolUse({ tool_name: 'Bash', tool_input: {}, tool_response: {} } as never)
    release()

    expect(await pending).toEqual({})
    expect(waits).toBe(1)
  })

  test('without an inbox the request is recorded without an id and nothing waits', async ($, on) => {
    const w = world(on)

    const r = await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: { command: 'ls' } } as never)

    expect(r).toEqual({})
    expect((stored(w) as Record<string, unknown>).permission).toEqual({ id: null, tool: 'Bash', input: { command: 'ls' }, at: START })
    expect(w.runs.filter(x => x.argv[3] === 'wait-permit')).toHaveLength(0)
  })

  test('a permission card answers with the pressed verdict', async ($, on) => {
    const board = {
      ...BOARD,
      columns: [
        {
          id: 'needs_input',
          heading: '입력 대기 1',
          cards: [
            {
              session_id: 'asker',
              headline: 'api · 권한 요청: Bash · 1m',
              subtitle: null,
              is_self: false,
              turn_id: 't-3',
              permission_id: 't-3-9',
              actions: ['permit', 'stop'],
              detail: { sections: [{ title: '기다리는 것', lines: ['Bash: npm publish'] }], decision: null, delivery: null },
            },
          ],
        },
        ...BOARD.columns.slice(1),
      ],
    }
    const w = world(on, routes({ board, permit: { queued: { session_id: 'asker', id: 't-3-9', kind: 'permit' } } }))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
    await ui.press({ key: 'card:asker' })
    await ui.unmount()

    const opened = await $.ui.mount({ ...PANE, surface: 'desktop' })
    expect(await opened.find({ key: 'detail:asker:deny' })).toBeDefined()
    await opened.press({ key: 'detail:asker:allow' })
    await w.clock.settle()
    await opened.unmount()

    expect(w.runs.find(r => r.argv[2] === 'permit')?.argv).toEqual([
      'atelier',
      'kanban',
      'permit',
      '--session',
      'asker',
      '--request',
      't-3-9',
      '--decision',
      'allow',
    ])
  })
})
