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
}

type FakeCli = (argv: readonly string[]) => { exitCode: number; stdout: string } | 'missing'

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
  on('process.run', ($, e) => {
    runs.push({ argv: e.argv, stdin: e.init?.stdin })
    const out = cli(e.argv)

    if (out === 'missing') {
      return { deny: 'ENOENT: Executable not found in $PATH' }
    }

    return { value: { ...out, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('fs.list', () => ({
    value: inbox.files.map(name => ({ name, kind: 'file' as const, size: 1, mtimeMs: START, isLink: false })),
  }))
  on('session.messages', () => ({ value: messages.value as never }))
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

  return { store, clock, runs, shown, dialog, inbox, messages, submitted, aborted }
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
      expect((await ui.find({ type: 'Text', text: /^질문 · / }))?.text).toBe('질문 · project-b · 질문 · 1m')
      expect((await ui.find({ type: 'Text', text: /Which color/ }))?.text).toBe('[Color] Which color?')
      expect(await ui.find({ key: 'ask:other:tu-1:0:1' })).toBeDefined()
      expect(await ui.find({ key: 'ask:other:tu-1:1:text' })).toBeDefined()
      expect(await ui.find({ key: 'ask:other:tu-1:send' })).toBeDefined()
      await ui.unmount()
    })
  }

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
    expect((await after.find({ key: 'ask:other:tu-1:0:1' }))?.text).toBe('✓ Blue')
    expect((await after.find({ key: 'ask:other:tu-1:1:2' }))?.text).toBe('✓ Fish')
    expect((await after.find({ key: 'ask:other:tu-1:1:1' }))?.text).toBe('Dog')
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
  test('a prompt that no person typed starts work but is not kept as the request', async ($, on) => {
    const w = world(on)

    await $.prompt.submit({ text: 'fix the login bug', origin: { kind: 'composer' } } as never)
    await $.prompt.submit({ text: '<task-notification>done</task-notification>', origin: { kind: 'task-notification' } } as never)

    expect(stored(w).first_prompt).toBe('fix the login bug')
    expect((stored(w) as Record<string, unknown>).last_prompt).toBe('fix the login bug')
    expect(eventNames(w)).toEqual(['prompt.submit', 'prompt.submit'])
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
    expect(rec().permission).toEqual({ tool: 'Bash', input: { command: 'npm publish' }, at: START })
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
    expect(rec.last_prompt).toBe('Run lint')
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

  test('in a stacked pane an opened card takes the board’s place until going back', async ($, on) => {
    world(on, routes({ board: { ...DETAIL_BOARD, layout: { direction: 'column', column_width: 40, column_gap: 1 } } }))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, props: { ...PANE.props, bodyColumns: 40 }, surface: 'terminal' })

    const narrow = { ...PANE, props: { ...PANE.props, bodyColumns: 40 }, surface: 'terminal' as const }
    await ui.press({ key: 'card:busy' })
    await ui.unmount()
    const opened = await $.ui.mount(narrow)
    expect(await opened.find({ type: 'Text', text: /쉬는 중 2/ })).toBeUndefined()
    await opened.press({ key: 'detail:busy:back' })
    await opened.unmount()
    const back = await $.ui.mount(narrow)
    expect((await back.find({ type: 'Text', text: /쉬는 중 2/ }))?.text).toBe('쉬는 중 2')
    await back.unmount()
  })
})
