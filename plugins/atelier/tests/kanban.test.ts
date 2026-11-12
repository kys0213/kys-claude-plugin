import { describe, expect, mock, test, tier } from 'claude-code/testing'
import type { On } from 'claude-code'

tier('user')

const SELF = 'self-session'
const START = 1_800_000_000_000
const KEY = `kanban:session:${SELF}`
/** Where the test session runs, as `inbox take` and `inbox wait-permit` are told. */
const SESSION_PLACE = { repo_root: null, cwd: '/work/project-a', permission_mode: null }

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
  /**
   * Answers the AskUserQuestion dialog the engine has open, as the person would; `questions` are the ones it drew.
   * `refuses` stands for the engine refusing a call whose questions it says yes to.
   */
  dialog: {
    answer: ((answers: Record<string, string>) => void) | null
    questions?: unknown
    refuses?: (questions: unknown) => boolean
  }
  /** The file names in this session's inbox directory. */
  inbox: { files: string[] }
  /** What the plugin wrote to disk, by path; `fails` makes the next writes fail. */
  disk: { files: Record<string, string>; fails: boolean }
  messages: { value: { role: 'user' | 'assistant'; text: string; toolUses: never[] }[] }
  submitted: string[]
  aborted: string[]
  /** Makes `$.turn.abort` reject, as the engine does for a turn that is no longer running. */
  abort: { refuses: boolean }
  /** What `$.agent.list()` answers. */
  agents: { value: { id: string; description: string; type: string; status: string; teammateId?: string }[] }
  invalidated: string[]
  toasts: string[]
  /** What the engine draws in the band above the prompt, beneath the plugin. */
  band: { value: unknown }
  /** Runs a tool call other than AskUserQuestion, as the engine would with its dialogs; unset answers `{ result: {} }`. */
  tools: { run: ((e: { tool: string; tool_use_id?: string }) => Promise<unknown>) | null }
  /** The prompt box's draft, a refusal the engine gives every prompt, and the context each prompt entered with. */
  prompt: { draft: string; drop: string | null; contexts: (readonly string[] | undefined)[] }
}

type CliOut = { exitCode: number; stdout: string } | 'missing'

/** A CLI that answers later (a promise) stands for one still running. */
type FakeCli = (argv: readonly string[]) => CliOut | Promise<CliOut>

function boardAndAnswers(board: unknown): FakeCli {
  return argv => {
    if (argv[2] === 'answer') {
      return { exitCode: 0, stdout: JSON.stringify({ answers: ['Blue', 'Cat, Fish'] }) }
    }

    if (argv[2] === 'reply') {
      return { exitCode: 0, stdout: JSON.stringify({ queued: { session_id: 'other', id: 'r1', kind: 'answer' } }) }
    }

    return { exitCode: 0, stdout: JSON.stringify(board) }
  }
}

const PANE_TOGGLES: CliOut = { exitCode: 0, stdout: JSON.stringify({ toggle: true }) }

/** A bare `/kanban` as the CLI answers it, unless the fake CLI fails or names `toggle` itself. */
async function withPaneToggle(cli: FakeCli, argv: readonly string[]): Promise<CliOut> {
  const out = await cli(argv)
  const bare = argv[2] === 'command' && argv[argv.indexOf('--args') + 1]?.trim() === ''
  const answered = out !== 'missing' && out.exitCode === 0 && out.stdout.trim().startsWith('{')

  return bare && answered && !out.stdout.includes('"toggle"') ? PANE_TOGGLES : out
}

/** A CLI that answers `/kanban` with a toggle and every other call with `out`. */
function failingBoard(out: CliOut): FakeCli {
  return argv => (argv[2] === 'command' ? PANE_TOGGLES : out)
}

/** Stands for the engine beneath the plugin: one session, its store, its clock, the CLI. */
function world(on: On, cli: FakeCli = boardAndAnswers(BOARD)): World {
  const store: Record<string, unknown> = {}
  const runs: Run[] = []
  const shown = { value: false }
  const dialog: World['dialog'] = { answer: null }
  const inbox: World['inbox'] = { files: [] }
  const disk: World['disk'] = { files: {}, fails: false }
  const messages: World['messages'] = { value: [] }
  const submitted: string[] = []
  const aborted: string[] = []
  const abort: World['abort'] = { refuses: false }
  const agents: World['agents'] = { value: [] }
  const invalidated: string[] = []
  const toasts: string[] = []
  const band: World['band'] = { value: { type: 'Box', props: {}, children: [] } }
  const tools: World['tools'] = { run: null }
  const prompt: World['prompt'] = { draft: '', drop: null, contexts: [] }

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
  on('ui.render', { component: 'AbovePrompt' }, () => band.value as never)
  on('ui.toast', ($, e) => {
    toasts.push(e.text)

    return { value: undefined }
  })
  on('ui.invalidate', ($, e) => {
    invalidated.push(e.event)

    return { value: undefined }
  })
  on('process.run', async ($, e) => {
    runs.push({ argv: e.argv, stdin: e.init?.stdin })
    const out = await withPaneToggle(cli, e.argv)

    if (out === 'missing') {
      return { deny: 'ENOENT: Executable not found in $PATH' }
    }

    return { value: { ...out, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('fs.list', () => ({
    value: inbox.files.map(name => ({ name, kind: 'file' as const, size: 1, mtimeMs: START, isLink: false })),
  }))
  on('fs.write', ($, e) => {
    if (disk.fails) {
      return { deny: 'EACCES: permission denied' }
    }

    disk.files[e.path] = e.text

    return { value: undefined }
  })
  on('session.messages', () => ({ value: messages.value as never }))
  on('agent.list', () => ({ value: agents.value as never }))
  on('turn.abort', ($, e) => {
    if (abort.refuses) {
      return { deny: 'not the running turn' }
    }

    aborted.push(e.turnId)

    return { value: undefined }
  })
  on('tool.register', () => ({ value: { tool: 'mcp__atelier__kanban_board' } as never }))
  on('session.start', ($, e) => ({ cwd: e.cwd }))
  on('prompt.submit', ($, e) => {
    if (prompt.drop !== null) {
      return { drop: prompt.drop }
    }

    submitted.push(e.text)
    prompt.contexts.push(e.context)

    return { text: e.text }
  })
  on('prompt.read', () => ({ value: { text: prompt.draft, cursor: prompt.draft.length } }))
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
      if (dialog.refuses?.(e.questions) === true) {
        throw new Error('the questions do not fit the tool schema')
      }

      dialog.questions = e.questions

      return new Promise(resolve => {
        dialog.answer = answers => resolve({ result: { questions: e.questions, answers } })
      })
    }

    if (e.tool === 'TaskCreate') {
      return { result: { task: { id: '1', subject: e.subject } } }
    }

    if (tools.run !== null) {
      return tools.run(e) as never
    }

    return { result: {} }
  })

  return { store, clock, runs, shown, dialog, inbox, disk, messages, submitted, aborted, abort, agents, invalidated, toasts, band, tools, prompt }
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

/** The argv a pane answer ran with, split into what it says and the nonce it ends on. */
function paneAnswer(run: Run | undefined): { argv: readonly string[]; nonce: string | undefined } {
  const argv = run?.argv ?? []

  return argv.at(-2) === '--nonce' ? { argv: argv.slice(0, -2), nonce: argv.at(-1) } : { argv, nonce: undefined }
}

function nonceKeys(w: World): string[] {
  return Object.keys(w.store).filter(k => k.startsWith('kanban:nonce:'))
}

/** How the last kanban answer this session took came in, as its event says. */
function answerVia(w: World): string | null | undefined {
  return stored(w).events.filter(e => e.name === 'kanban.answer').at(-1)?.detail
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

const BAND = {
  plugin: 'atelier',
  component: 'AbovePrompt',
  props: {
    hasSurvey: false,
    isCollapsed: false,
    isWorking: false,
    maxRows: 10,
    bodyColumns: 100,
    scroll: { offset: 0, bodyRows: 9 },
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

  test('an answer taken from the inbox closes the open question it names as the person would', async ($, on) => {
    const answering = { exitCode: 0, stdout: '' }
    const w = world(on, argv => {
      if (argv[3] === 'dir') {
        return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
      }

      if (argv[3] === 'take') {
        return answering
      }

      return { exitCode: 0, stdout: JSON.stringify(BOARD) }
    })

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS, tool_use_id: 'tu-9' } as never)
    await w.clock.settle()
    answering.stdout = JSON.stringify({
      actions: [{ kind: 'answer', tool_use_id: 'tu-9', answers: ['Blue'], from: 'pane' }],
      results: [{ id: 'r1', kind: 'answer', status: 'delivered' }],
    })
    w.inbox.files = ['r1.json']
    await w.clock.advance(2_000)
    const r = await call

    const take = w.runs.find(x => x.argv[3] === 'take')
    expect(JSON.parse(take?.stdin ?? '{}')).toEqual({
      asks: [{ id: 'tu-9', questions: TOOL_QUESTIONS }],
      decision: null,
      place: SESSION_PLACE,
      nonces: [],
    })
    expect(r.result).toEqual({ questions: TOOL_QUESTIONS, answers: { 'Which color?': 'Blue' } })
    expect(stored(w).asks).toEqual({})
    expect(eventNames(w)).toContain('kanban.answer')
  })

  test('an inbox take sends the nonces the pane left for this session and drops the ones the CLI used', async ($, on) => {
    const answering = { exitCode: 0, stdout: '' }
    const w = world(on, argv => {
      if (argv[3] === 'dir') {
        return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
      }

      return argv[3] === 'take' ? answering : { exitCode: 0, stdout: JSON.stringify(BOARD) }
    })

    w.store[`kanban:nonce:${SELF}:answer:n-1`] = { session: SELF, kind: 'answer', nonce: 'n-1', at: START }
    w.store[`kanban:nonce:${SELF}:decide:n-2`] = { session: SELF, kind: 'decide', nonce: 'n-2', at: START }
    w.store[`kanban:nonce:${SELF}:permit:n-3`] = { session: SELF, kind: 'permit', at: START }
    w.store['kanban:nonce:other:answer:n-4'] = { session: 'other', kind: 'answer', nonce: 'n-4', at: START }

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS, tool_use_id: 'tu-9' } as never)
    await w.clock.settle()
    answering.stdout = JSON.stringify({
      actions: [{ kind: 'answer', tool_use_id: 'tu-9', answers: ['Blue'], from: 'pane', via: 'pane' }],
      results: [{ id: 'r1', kind: 'answer', status: 'delivered', via: 'pane' }],
      used_nonces: [{ kind: 'answer', nonce: 'n-1' }],
    })
    w.inbox.files = ['r1.json']
    await w.clock.advance(2_000)
    await call

    const take = w.runs.find(x => x.argv[3] === 'take')
    expect(JSON.parse(take?.stdin ?? '{}').nonces).toEqual([
      { kind: 'answer', nonce: 'n-1' },
      { kind: 'decide', nonce: 'n-2' },
    ])
    expect(nonceKeys(w)).toEqual([
      `kanban:nonce:${SELF}:decide:n-2`,
      `kanban:nonce:${SELF}:permit:n-3`,
      'kanban:nonce:other:answer:n-4',
    ])
  })

  for (const [said, via] of [
    ['agent', 'agent'],
    [undefined, null],
  ] as const) {
    test(`an inbox answer the CLI says came via ${String(said)} is recorded with that via as given`, async ($, on) => {
      const answering = { exitCode: 0, stdout: '' }
      const w = world(on, argv => {
        if (argv[3] === 'dir') {
          return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
        }

        return argv[3] === 'take' ? answering : { exitCode: 0, stdout: JSON.stringify(BOARD) }
      })

      await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
      const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS, tool_use_id: 'tu-9' } as never)
      await w.clock.settle()
      answering.stdout = JSON.stringify({
        actions: [{ kind: 'answer', tool_use_id: 'tu-9', answers: ['Blue'], from: 'pane', via: said }],
        results: [{ id: 'r1', kind: 'answer', status: 'delivered' }],
      })
      w.inbox.files = ['r1.json']
      await w.clock.advance(2_000)
      const r = await call

      expect(r.result).toEqual({ questions: TOOL_QUESTIONS, answers: { 'Which color?': 'Blue' } })
      expect(answerVia(w)).toBe(via)
    })
  }

  test('an answer an older pane left in the store still closes the question', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS })
    await w.clock.settle()

    const [id] = Object.keys(stored(w).asks ?? {})
    expect(stored(w).asks?.[id as string]).toEqual({
      at: START,
      questions: [COLOR],
      orig_len: { question: [12], labels: [[3, 4]] },
    })

    w.store[`kanban:answer:${SELF}:${id}`] = { v: 1, answers: ['Blue'] }
    await w.clock.advance(2_000)
    const r = await call

    expect(r.result).toEqual({ questions: TOOL_QUESTIONS, answers: { 'Which color?': 'Blue' } })
    expect(stored(w).asks).toEqual({})
    expect(answerVia(w)).toBe('terminal')
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
    w.store['kanban:nonce:gone:answer:n-1'] = { session: 'gone', kind: 'answer', at: START }
    w.store['kanban:nonce:other:permit:n-2'] = { session: 'other', kind: 'permit', at: START }
    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)

    expect(w.store['kanban:answer:gone:tu-1']).toBeUndefined()
    expect(w.store['kanban:answer:other:tu-2']).toBeDefined()
    expect(nonceKeys(w)).toEqual(['kanban:nonce:other:permit:n-2'])
  })

  for (const [name, pruned, kept] of [
    ['the nonces prune-nonces names are deleted', { drop: ['kanban:nonce:other:answer:old-0'] }, ['kanban:nonce:other:answer:old-1']],
    ['a prune-nonces that fails deletes no nonce', 'missing', ['kanban:nonce:other:answer:old-0', 'kanban:nonce:other:answer:old-1']],
  ] as const) {
    test(`a pane answer hands every store key to prune-nonces: ${name}`, async ($, on) => {
      const answers = boardAndAnswers(ASKING_BOARD)
      let storeKeys: string[] = []
      const w = world(on, argv => {
        if (argv[2] !== 'prune-nonces') {
          return answers(argv)
        }

        storeKeys = Object.keys(w.store)

        return pruned === 'missing' ? 'missing' : { exitCode: 0, stdout: JSON.stringify(pruned) }
      })
      const old = ['kanban:nonce:other:answer:old-0', 'kanban:nonce:other:answer:old-1']

      w.store['unrelated'] = { keep: true }
      for (const key of old) {
        w.store[key] = { session: 'other', kind: 'answer', at: START }
      }

      await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
      const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
      await ui.press({ key: 'ask:other:tu-1:send' })
      await w.clock.settle()
      await ui.unmount()

      const { nonce } = paneAnswer(w.runs.find(r => r.argv[2] === 'reply'))
      const left = `kanban:nonce:other:answer:${nonce}`
      const prune = w.runs.find(r => r.argv[2] === 'prune-nonces')
      expect(prune?.argv).toEqual(['atelier', 'kanban', 'prune-nonces'])
      expect(JSON.parse(prune?.stdin ?? '{}')).toEqual({ keys: storeKeys })
      expect(storeKeys).toEqual(expect.arrayContaining(['unrelated', ...old, left]))
      expect(nonceKeys(w)).toEqual([...kept, left])
      expect(w.store['unrelated']).toEqual({ keep: true })
    })
  }

  test('/kanban hands the CLI its bare arguments and toggles the pane only when the CLI says so', async ($, on) => {
    let toggle = false
    const w = world(on, argv =>
      argv[2] === 'command'
        ? { exitCode: 0, stdout: JSON.stringify(toggle ? { toggle } : { toggle, error: '아직 몰라요' }) }
        : { exitCode: 0, stdout: JSON.stringify(BOARD) },
    )

    const refused = await $.command.run({ command: 'kanban', args: ' ', origin: { kind: 'plugin', name: 'x' } } as never)

    expect(w.runs.find(r => r.argv[2] === 'command')?.argv.slice(7)).toEqual(['--origin', 'plugin', '--args', ' '])
    expect(refused).toEqual({ text: '아직 몰라요' })
    expect(w.shown.value).toBe(false)

    toggle = true
    const ran = await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)

    expect(ran).toEqual({ text: '세션 칸반을 열었어요.' })
    expect(w.shown.value).toBe(true)
  })

  for (const [failure, out] of [
    ['a missing CLI', 'missing'],
    ['a CLI that exits non-zero', { exitCode: 1, stdout: '' }],
    ['a CLI whose output is not JSON', { exitCode: 0, stdout: 'nope' }],
  ] as const) {
    for (const args of ['', 'delegate off']) {
      for (const open of [false, true]) {
        test(`${failure} leaves the ${open ? 'open' : 'closed'} pane as it is on /kanban ${JSON.stringify(args)} and says why`, async ($, on) => {
          let failing = false
          const w = world(on, argv => (failing && argv[2] === 'command' ? out : boardAndAnswers(BOARD)(argv)))

          if (open) {
            await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
          }

          failing = true
          const ran = await $.command.run({ command: 'kanban', args, origin: { kind: 'composer' } } as never)

          expect(ran).toEqual({ text: 'atelier CLI 를 실행하지 못했어요 — /atelier:update 로 설치·갱신하세요' })
          expect(w.shown.value).toBe(open)
        })
      }
    }
  }

  test('/kanban again closes the pane', async ($, on) => {
    const w = world(on)

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ran = await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)

    expect(ran).toEqual({ text: '세션 칸반을 닫았어요.' })
    expect(w.shown.value).toBe(false)
  })

  for (const surface of ['terminal', 'desktop'] as const) {
    test(`the band button on ${surface} opens the pane and then closes it`, async ($, on) => {
      const w = world(on)

      const closed = await $.ui.mount({ ...BAND, surface })
      expect((await closed.find({ key: 'kanban:toggle' }))?.text).toBe('세션 칸반 열기')
      await closed.press({ key: 'kanban:toggle' })
      await closed.unmount()

      expect(w.shown.value).toBe(true)
      expect(boardRuns(w)).toHaveLength(1)

      const open = await $.ui.mount({ ...BAND, surface })
      expect((await open.find({ key: 'kanban:toggle' }))?.text).toBe('세션 칸반 닫기')
      await open.press({ key: 'kanban:toggle' })
      await open.unmount()

      expect(w.shown.value).toBe(false)
    })
  }

  test('closing the pane redraws the band', async ($, on) => {
    const w = world(on)

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    w.invalidated.length = 0
    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)

    expect(w.shown.value).toBe(false)
    expect(w.invalidated).toContain('ui.render')
  })

  test('the band button yields to a survey', async ($, on) => {
    world(on)

    const ui = await $.ui.mount({ ...BAND, props: { ...BAND.props, hasSurvey: true }, surface: 'desktop' })

    expect(await ui.find({ key: 'kanban:toggle' })).toBeUndefined()
    await ui.unmount()
  })

  test("the band button sits above another plugin's band", async ($, on) => {
    const w = world(on)
    w.band.value = { type: 'Text', props: {}, children: ['other band'] }

    const ui = await $.ui.mount({ ...BAND, surface: 'terminal' })

    expect(await ui.find({ key: 'kanban:toggle' })).toBeDefined()
    expect((await ui.find({ type: 'Text', text: /other band/ }))?.text).toBe('other band')
    await ui.unmount()
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
    world(on, failingBoard('missing'))

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
      const w = world(on, failingBoard(out))

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

  test('the picks go to atelier kanban answer and its answers to the asking session through reply', async ($, on) => {
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
    const reply = paneAnswer(w.runs.find(r => r.argv[2] === 'reply'))
    expect(reply.argv).toEqual([
      'atelier',
      'kanban',
      'reply',
      '--session',
      'other',
      '--ask',
      'tu-1',
      '--answer',
      'Blue',
      '--answer',
      'Cat, Fish',
      '--from',
      'pane',
    ])
    expect(reply.nonce).toMatch(/^[A-Za-z0-9-]{32,}$/)
    expect(w.store[`kanban:nonce:other:answer:${reply.nonce}`]).toEqual({
      session: 'other',
      kind: 'answer',
      nonce: reply.nonce,
      at: START,
    })
    expect(w.store['kanban:answer:other:tu-1']).toBeUndefined()
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
    expect(w.runs.find(r => r.argv[2] === 'reply')).toBeUndefined()
    expect(await after.find({ key: 'ask:other:tu-1:send' })).toBeDefined()
    await after.unmount()
  })

  test('a refused reply shows the reason and lets the answer be sent again', async ($, on) => {
    const w = world(on, argv => {
      if (argv[2] === 'reply') {
        return { exitCode: 0, stdout: JSON.stringify({ error: '답이 비어 있어요' }) }
      }

      return boardAndAnswers(ASKING_BOARD)(argv)
    })

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

    await ui.press({ key: 'ask:other:tu-1:send' })
    await w.clock.settle()
    await ui.unmount()

    const after = await $.ui.mount({ ...PANE, surface: 'terminal' })
    expect((await after.find({ type: 'Text', text: /비어 있어요/ }))?.text).toBe('답이 비어 있어요')
    expect(await after.find({ key: 'ask:other:tu-1:send' })).toBeDefined()
    expect(nonceKeys(w)).toEqual([])
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
            decision: { id: '1700', ask: '초안대로 진행할까요?', recap: '초안을 정리했어요.', options: ['초안대로 진행', '고칠 행 있음'] },
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
      orig_len: { ask: 11, options: [7, 7] },
    })

    await $.turn.start({ text: 'go', turnId: 't-2' })
    expect(rec().decision).toBeNull()
  })

  test('a WrapUp offering more options than kept counts every option in its lengths', async ($, on) => {
    const w = world(on)
    const actions = ['a', 'bb', 'c', 'd', 'eee'].map(label => ({ label }))

    await $.tool.call({ tool: 'WrapUp', recap: 'r', ask: 'Pick one', blocked: true, actions } as never)

    const decision = (stored(w) as Record<string, unknown>).decision as { options: string[]; orig_len: unknown }
    expect(decision.options).toEqual(['a', 'bb', 'c', 'd'])
    expect(decision.orig_len).toEqual({ ask: 8, options: [1, 2, 1, 1, 3] })
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
    expect(rec().permission).toEqual({
      id: null,
      tool: 'Bash',
      input: { command: 'npm publish' },
      input_len: { command: 11 },
      at: START,
    })
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
    expect(w.runs.filter(r => r.argv[2] === 'inbox').map(r => r.argv.slice(2, 4))).toEqual([['inbox', 'dir']])

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
      '--waits-stdin',
    ])
    expect(JSON.parse(take?.stdin ?? '{}')).toEqual({ asks: [], decision: null, place: SESSION_PLACE, nonces: [] })
    expect(w.submitted).toEqual(['Run lint'])
    const rec = stored(w) as Record<string, unknown>
    expect((rec.prompts as { text: string; origin: string }[]).at(-1)).toEqual({ text: 'Run lint', origin: 'kanban', at: START + 6_000 })
    expect(eventNames(w).at(-1)).toBe('prompt.submit')
    expect(rec.inbox).toEqual({ id: 'r1', kind: 'prompt', status: 'delivered', at: START + 6_000 })
  })

  test('a decide taken from the inbox runs as typed, and the take names the decision left open', async ($, on) => {
    const w = world(
      on,
      routes({
        'inbox dir': { dir: INBOX_DIR },
        'inbox take': {
          actions: [{ kind: 'decide', decision_at: String(START), text: '초안대로 진행', from: 'pane', via: 'pane' }],
          results: [{ id: 'r1', kind: 'decide', status: 'delivered', via: 'pane' }],
          used_nonces: [{ kind: 'decide', nonce: 'n-1' }],
        },
      }),
    )
    w.store[`kanban:nonce:${SELF}:decide:n-1`] = { session: SELF, kind: 'decide', nonce: 'n-1', at: START }

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.tool.call({
      tool: 'WrapUp',
      recap: '초안을 정리했어요.',
      ask: '초안대로 진행할까요?',
      blocked: true,
      actions: [{ label: '초안대로 진행' }],
    } as never)
    w.inbox.files = ['r1.json']
    await w.clock.advance(2_000)

    const take = w.runs.find(r => r.argv[3] === 'take')
    expect(JSON.parse(take?.stdin ?? '{}').decision).toMatchObject({ at: START, ask: '초안대로 진행할까요?' })
    expect(w.submitted).toEqual(['초안대로 진행'])
    const rec = stored(w) as Record<string, unknown>
    expect((rec.prompts as { text: string; origin: string }[]).at(-1)?.origin).toBe('kanban')
    expect(rec.decision).toBeNull()
    expect(eventNames(w).slice(-2)).toEqual(['kanban.answer', 'prompt.submit'])
    expect(answerVia(w)).toBe('pane')
    expect(nonceKeys(w)).toEqual([])
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

  test('a stop whose turn ended before the abort still lets the rest of its take through', async ($, on) => {
    const w = world(
      on,
      routes({
        'inbox dir': { dir: INBOX_DIR },
        'inbox take': {
          actions: [
            { kind: 'stop', turn_id: 't-7' },
            { kind: 'prompt', text: 'Run lint', body: 'Run lint', from: 'pane' },
          ],
          results: [
            { id: 'r2', kind: 'stop', status: 'delivered' },
            { id: 'r3', kind: 'prompt', status: 'delivered' },
          ],
        },
      }),
    )
    w.abort.refuses = true

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.turn.start({ text: 'go', turnId: 't-7' })
    w.inbox.files = ['1-1.json']
    await w.clock.advance(2_000)

    expect(w.submitted).toEqual(['Run lint'])
    expect((stored(w) as Record<string, unknown>).inbox).toEqual({ id: 'r3', kind: 'prompt', status: 'delivered', at: START + 2_000 })
  })

  test('an expired session is forgotten, which clears its inbox and record copy', async ($, on) => {
    const w = world(on)

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    expect(w.runs.filter(r => r.argv[2] === 'forget').map(r => r.argv)).toEqual([['atelier', 'kanban', 'forget', '--session', 'gone']])
    expect(w.store['kanban:session:gone']).toBeUndefined()
  })

  test('the orchestrator reads the board grouped by repository through the board tool', async ($, on) => {
    world(on, routes({ board: DETAIL_BOARD }))

    const r = await $.tool.call({ tool: 'mcp__atelier__kanban_board' } as never)
    const out = JSON.parse((r as { result: string }).result) as Record<string, unknown>
    expect(out).toEqual({ repos: [{ label: 'api', root: '/w/api', sessions: [] }] })
  })

  test('the board tool passes its columns to the board and its waits and delegate back as they came', async ($, on) => {
    const wait = { kind: 'permission', id: 'p-1', proxyable: true, blocked: [] }
    const repos = [{ label: 'api', root: '/w/api', sessions: [{ session_id: 'w', column: 'needs_input', wait }] }]
    const delegate = { session_id: 'm', since: START, conflicts: [] }
    const w = world(on, routes({ board: { ...DETAIL_BOARD, repos, delegate } }))

    const r = await $.tool.call({ tool: 'mcp__atelier__kanban_board', column: ['needs_input', 'idle'] } as never)

    const argv = w.runs.find(run => run.argv[2] === 'board')?.argv ?? []
    expect(argv.slice(-4)).toEqual(['--column', 'needs_input', '--column', 'idle'])
    expect(JSON.parse((r as { result: string }).result)).toEqual({ repos, delegate })
  })

  test('the board tool reports the error the board gives for an unknown column', async ($, on) => {
    world(on, routes({ board: { error: '모르는 컬럼이에요: done' } }))

    const r = await $.tool.call({ tool: 'mcp__atelier__kanban_board', column: ['done'] } as never)

    expect(JSON.parse((r as { result: string }).result)).toEqual({ error: '모르는 컬럼이에요: done' })
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

  test('a decision card answers that decision with the option pressed', async ($, on) => {
    const w = world(on, routes({ board: DETAIL_BOARD, decide: { queued: { session_id: 'wrap', id: 'r', kind: 'decide' } } }))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })

    await ui.press({ key: 'card:wrap' })
    await ui.unmount()
    const opened = await $.ui.mount({ ...PANE, surface: 'desktop' })
    expect((await opened.find({ type: 'Text', text: /진행할까요/ }))?.text).toBe('초안대로 진행할까요?')
    await opened.press({ key: 'detail:wrap:option:1' })
    await w.clock.settle()

    const decide = paneAnswer(w.runs.find(r => r.argv[2] === 'decide'))
    expect(decide.argv).toEqual([
      'atelier',
      'kanban',
      'decide',
      '--session',
      'wrap',
      '--at',
      '1700',
      '--text',
      '고칠 행 있음',
      '--from',
      'pane',
    ])
    expect(nonceKeys(w)).toEqual([`kanban:nonce:wrap:decide:${decide.nonce}`])
    expect(w.runs.find(r => r.argv[2] === 'send')).toBeUndefined()
    await opened.unmount()
  })

  test('text typed on a decision card answers that decision too', async ($, on) => {
    const w = world(on, routes({ board: DETAIL_BOARD, decide: { queued: { session_id: 'wrap', id: 'r', kind: 'decide' } } }))

    await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

    await ui.press({ key: 'card:wrap' })
    await ui.unmount()
    const opened = await $.ui.mount({ ...PANE, surface: 'terminal' })
    await opened.input({ key: 'detail:wrap:send', text: '3행만 고쳐 주세요' })
    await w.clock.settle()
    await opened.unmount()

    expect(paneAnswer(w.runs.find(r => r.argv[2] === 'decide')).argv.slice(-4)).toEqual(['--text', '3행만 고쳐 주세요', '--from', 'pane'])
    expect(w.runs.find(r => r.argv[2] === 'send')).toBeUndefined()
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
        const out = typeof decision === 'object' && decision !== null ? decision : { decision }

        return { exitCode: 0, stdout: JSON.stringify(out) }
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
      '--wait-stdin',
    ])
    expect(JSON.parse(waits[0]?.stdin ?? '{}')).toEqual({ nonces: [], place: SESSION_PLACE, permission: { tool: 'Bash', input: { command: 'npm publish' } } })
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
    expect((stored(w) as Record<string, unknown>).permission).toEqual({
      id: null,
      tool: 'Bash',
      input: { command: 'ls' },
      input_len: { command: 2 },
      at: START,
    })
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

    const permit = paneAnswer(w.runs.find(r => r.argv[2] === 'permit'))
    expect(permit.argv).toEqual([
      'atelier',
      'kanban',
      'permit',
      '--session',
      'asker',
      '--request',
      't-3-9',
      '--decision',
      'allow',
      '--from',
      'pane',
    ])
    expect(nonceKeys(w)).toEqual([`kanban:nonce:asker:permit:${permit.nonce}`])
  })

  test('a permit wait sends the nonces the pane left and drops the one the CLI used', async ($, on) => {
    const { w } = permitWorld(on, [
      { decision: 'allow', from: 'pane', via: 'pane', used_nonces: [{ kind: 'permit', nonce: 'n-1' }] },
    ])
    w.store[`kanban:nonce:${SELF}:permit:n-1`] = { session: SELF, kind: 'permit', nonce: 'n-1', at: START }
    w.store[`kanban:nonce:${SELF}:answer:n-2`] = { session: SELF, kind: 'answer', nonce: 'n-2', at: START }

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: {} } as never)

    const wait = w.runs.find(x => x.argv[3] === 'wait-permit')
    expect(JSON.parse(wait?.stdin ?? '{}')).toEqual({
      nonces: [
        { kind: 'permit', nonce: 'n-1' },
        { kind: 'answer', nonce: 'n-2' },
      ],
      place: SESSION_PLACE,
      permission: { tool: 'Bash', input: {} },
    })
    expect(answerVia(w)).toBe('pane')
    expect(nonceKeys(w)).toEqual([`kanban:nonce:${SELF}:answer:n-2`])
  })

  for (const [said, via] of [
    ['pane', 'pane'],
    [undefined, null],
  ] as const) {
    test(`a permit the CLI says came via ${String(said)} is the decision, recorded with that via as given`, async ($, on) => {
      const { w } = permitWorld(on, [{ decision: 'deny', from: 'pane', via: said }])

      await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
      const r = await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: {} } as never)

      expect(r).toEqual({ decision: { behavior: 'deny', message: '사람이 칸반에서 이 도구 실행을 거부했어요' } })
      expect(answerVia(w)).toBe(via)
    })
  }
})

describe('kanban shell guard', () => {
  const KANBAN_SEND = 'atelier kanban send --session s-1 --from pane --text hi'

  test("a model's shell command naming the kanban is denied as the CLI says", async ($, on) => {
    const w = world(on, routes({ 'guard-bash': { deny: '모델은 --from agent 로 보내요' } }))

    const r = await $.tool.call({ tool: 'Bash', command: KANBAN_SEND } as never)

    expect(r).toEqual({ deny: '모델은 --from agent 로 보내요' })
    const guard = w.runs.find(x => x.argv[2] === 'guard-bash')
    expect(guard?.argv).toEqual(['atelier', 'kanban', 'guard-bash'])
    expect(JSON.parse(guard?.stdin ?? '{}')).toEqual({ command: KANBAN_SEND })
  })

  test("a subagent's shell command is checked the same way", async ($, on) => {
    world(on, routes({ 'guard-bash': { deny: 'no' } }))

    const r = await $.tool.call({ tool: 'Bash', command: KANBAN_SEND, agentId: 'a1' } as never)

    expect(r).toEqual({ deny: 'no' })
  })

  test('a command the CLI lets through runs', async ($, on) => {
    world(on, routes({ 'guard-bash': { deny: null } }))

    const r = await $.tool.call({ tool: 'Bash', command: 'atelier kanban send --from agent --session s --text x' } as never)

    expect(r.deny).toBeUndefined()
  })

  test('every shell command is handed to the CLI, whatever it names', async ($, on) => {
    const w = world(on, routes({ 'guard-bash': { deny: 'no' } }))

    const r = await $.tool.call({ tool: 'Bash', command: 'atelier kan""ban send --from pane' } as never)

    expect(r).toEqual({ deny: 'no' })
    const guard = w.runs.find(x => x.argv[2] === 'guard-bash')
    expect(JSON.parse(guard?.stdin ?? '{}')).toEqual({ command: 'atelier kan""ban send --from pane' })
  })

  for (const [name, cli] of [
    ['a missing CLI', (() => 'missing') as FakeCli],
    ['a CLI that exits non-zero', (() => ({ exitCode: 1, stdout: '' })) as FakeCli],
    ['a CLI error', routes({ 'guard-bash': { error: 'stdin is not JSON' } })],
  ] as const) {
    test(`${name} lets the command run`, async ($, on) => {
      world(on, cli)

      const r = await $.tool.call({ tool: 'Bash', command: KANBAN_SEND } as never)

      expect(r.deny).toBeUndefined()
    })
  }
})

const COPY = '/home/me/.claude/atelier/kanban/records/self-session.json'

describe('kanban record copy', () => {
  test('every record leaves a copy of the stored record where the CLI named it', async ($, on) => {
    const w = world(on, routes({ 'record-path': { path: COPY } }))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    expect(JSON.parse(w.disk.files[COPY] as string)).toEqual(stored(w))

    await $.turn.start({ text: 'go', turnId: 't-1' })
    expect(JSON.parse(w.disk.files[COPY] as string)).toEqual(stored(w))
    expect(eventNames(w)).toContain('turn.start')
  })

  test('the CLI is asked for the copy path once per session', async ($, on) => {
    const w = world(on, routes({ 'record-path': { path: COPY } }))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.turn.start({ text: 'go', turnId: 't-1' })

    const asks = w.runs.filter(r => r.argv[2] === 'record-path')
    expect(asks.map(r => r.argv)).toEqual([['atelier', 'kanban', 'record-path', '--session', SELF]])
  })

  test('a CLI that cannot name the path leaves the store written and no copy', async ($, on) => {
    const w = world(on, () => ({ exitCode: 1, stdout: '' }))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })

    expect(stored(w).session_id).toBe(SELF)
    expect(w.disk.files).toEqual({})
  })

  test('a missing CLI leaves the store written and no copy', async ($, on) => {
    const w = world(on, () => 'missing')

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })

    expect(stored(w).session_id).toBe(SELF)
    expect(w.disk.files).toEqual({})
  })

  test('a failed copy write is redone by the next record', async ($, on) => {
    const w = world(on, routes({ 'record-path': { path: COPY } }))

    w.disk.fails = true
    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    expect(w.disk.files).toEqual({})

    w.disk.fails = false
    await $.turn.start({ text: 'go', turnId: 't-1' })
    expect(JSON.parse(w.disk.files[COPY] as string)).toEqual(stored(w))
  })

  test('a long permission input keeps its original lengths beside the cut text', async ($, on) => {
    const w = world(on)

    await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: { command: 'x'.repeat(300), timeout: 5 } } as never)

    const permission = (stored(w) as Record<string, unknown>).permission as { input: Record<string, string>; input_len: Record<string, number> }
    expect(permission.input.command).toHaveLength(200)
    expect(permission.input_len).toEqual({ command: 300 })
  })

  test('a long question keeps its original lengths beside the cut text', async ($, on) => {
    const w = world(on)
    const long = [{ ...TOOL_QUESTIONS[0], question: 'q'.repeat(1_500), options: [{ label: 'l'.repeat(250) }] }]

    void $.tool.call({ tool: 'AskUserQuestion', questions: long } as never)
    await w.clock.settle()

    const [ask] = Object.values(stored(w).asks ?? {}) as { questions: { question: string }[]; orig_len: unknown }[]
    expect(ask?.questions[0]?.question).toHaveLength(1_000)
    expect(ask?.orig_len).toEqual({ question: [1_500], labels: [[250]] })
  })
})

describe('kanban ledger', () => {
  const PLACE = { at: START, session: SELF, repo_root: null, cwd: '/work/project-a', permission_mode: null }
  const COLOR_ASK = { tool_use_id: 'tu-9', at: START, questions: [COLOR], orig_len: { question: [12], labels: [[3, 4]] } }
  const GIT_STATUS = { id: null, tool: 'Bash', input: { command: 'git status' }, input_len: { command: 10 }, at: START }
  const DECISION = { ask: '진행할까요?', recap: '정리했어요.', options: ['진행'], at: START, orig_len: { ask: 6, options: [2] } }
  const WRAP_UP = { tool: 'WrapUp', recap: '정리했어요.', ask: '진행할까요?', blocked: true, actions: [{ label: '진행' }] }

  function ledgerRuns(w: World): Run[] {
    return w.runs.filter(r => r.argv[2] === 'ledger' && r.argv[3] === 'record')
  }

  function recorded(w: World): unknown[] {
    return ledgerRuns(w).map(r => JSON.parse(r.stdin ?? '{}'))
  }

  /** A CLI whose `ledger bind` names call `tu-5`; every other command goes to `rest`. */
  function binding(rest: FakeCli = boardAndAnswers(BOARD)): FakeCli {
    return argv =>
      argv[2] === 'ledger' && argv[3] === 'bind' ? { exitCode: 0, stdout: JSON.stringify({ call: 'tu-5' }) } : rest(argv)
  }

  type Engine = { classic: { PermissionRequest: (e: never) => Promise<unknown> }; tool: { call: (e: never) => Promise<unknown> } }

  /** A Bash call that raises a permission dialog, then ends as `after` says once it was answered. */
  function askingCall(w: World, $: Engine, after: (id: string) => Promise<unknown>): Promise<unknown> {
    w.tools.run = async e => {
      await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: { command: 'git status' } } as never)

      return after(e.tool_use_id ?? '')
    }

    return $.tool.call({ tool: 'Bash', command: 'git status', tool_use_id: 'tu-5' } as never)
  }

  test('a question answered in its own dialog leaves its answers, by question, through ledger record', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS, tool_use_id: 'tu-9' } as never)
    await w.clock.settle()
    w.dialog.answer?.({ 'Which color?': 'Red' })
    await call

    expect(ledgerRuns(w).map(r => r.argv)).toEqual([['atelier', 'kanban', 'ledger', 'record']])
    expect(recorded(w)).toEqual([
      { ...PLACE, question: COLOR_ASK, answer: { path: 'session', answers: ['Red'], afk_timeout_ms: null, is_error: false, denied: false } },
    ])
  })

  test('a question the kanban answered leaves the way the CLI said it came', async ($, on) => {
    const answering = { exitCode: 0, stdout: '' }
    const w = world(on, argv => {
      if (argv[3] === 'dir') {
        return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
      }

      return argv[3] === 'take' ? answering : { exitCode: 0, stdout: JSON.stringify(BOARD) }
    })

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS, tool_use_id: 'tu-9' } as never)
    await w.clock.settle()
    answering.stdout = JSON.stringify({
      actions: [{ kind: 'answer', tool_use_id: 'tu-9', answers: ['Blue'], from: 'pane', via: 'pane' }],
      results: [{ id: 'r1', kind: 'answer', status: 'delivered', via: 'pane' }],
    })
    w.inbox.files = ['r1.json']
    await w.clock.advance(2_000)
    await call

    expect(recorded(w)).toEqual([
      { ...PLACE, at: START + 2_000, question: COLOR_ASK, answer: { path: 'inbox', via: 'pane', answers: ['Blue'] } },
    ])
  })

  test('a permission dialog whose tool then ran leaves its PostToolUse', async ($, on) => {
    const w = world(on, binding())

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await askingCall(w, $ as never, async id => {
      await $.classic.PostToolUse({ tool_name: 'Bash', tool_input: {}, tool_response: {}, tool_use_id: id } as never)

      return { result: {} }
    })

    expect(recorded(w)).toEqual([
      { ...PLACE, permission: GIT_STATUS, answer: { path: 'session', post: 'PostToolUse', is_error: false } },
    ])
  })

  test('a permission dialog whose tool failed after it ran leaves its PostToolUseFailure', async ($, on) => {
    const w = world(on, binding())

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await askingCall(w, $ as never, async id => {
      await $.classic.PostToolUseFailure({ tool_name: 'Bash', tool_input: {}, tool_use_id: id, error: 'exit 1' } as never)

      return { isError: true, result: 'exit 1' }
    })

    expect(recorded(w)).toEqual([
      { ...PLACE, permission: GIT_STATUS, answer: { path: 'session', post: 'PostToolUseFailure', is_error: true } },
    ])
  })

  test('a permission dialog that ended in an error without the tool running leaves just the error', async ($, on) => {
    const w = world(on, binding())

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await askingCall(w, $ as never, async () => ({ isError: true, result: "The user doesn't want to proceed with this tool use." }))

    expect(recorded(w)).toEqual([{ ...PLACE, permission: GIT_STATUS, answer: { path: 'session', post: null, is_error: true } }])
  })

  test('a tool call that raised no permission dialog leaves nothing', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.tool.call({ tool: 'Bash', command: 'git status', tool_use_id: 'tu-5' } as never)

    expect(ledgerRuns(w)).toEqual([])
  })

  test('a permission request asks ledger bind with its tool and the calls in flight', async ($, on) => {
    const w = world(on, binding())

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await askingCall(w, $ as never, async () => ({ result: {} }))

    const binds = w.runs.filter(r => r.argv[2] === 'ledger' && r.argv[3] === 'bind')
    expect(binds.map(r => r.argv)).toEqual([['atelier', 'kanban', 'ledger', 'bind']])
    expect(binds.map(r => JSON.parse(r.stdin ?? '{}'))).toEqual([
      { tool: 'Bash', calls: [{ tool_use_id: 'tu-5', tool: 'Bash', permission: false }] },
    ])
  })

  test('a permission request ledger bind names no call for leaves nothing, and the call goes on', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const r = await askingCall(w, $ as never, async () => ({ isError: true, result: 'no' }))

    expect(r).toEqual({ isError: true, result: 'no' })
    expect(ledgerRuns(w)).toEqual([])
  })

  test('a permission request whose ledger bind fails leaves nothing, and the call goes on', async ($, on) => {
    const w = world(on, () => 'missing')

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const r = await askingCall(w, $ as never, async () => ({ isError: true, result: 'no' }))

    expect(r).toEqual({ isError: true, result: 'no' })
    expect(ledgerRuns(w)).toHaveLength(0)
  })

  test('a permission the kanban answered leaves the way the CLI said it came, and not the run after it', async ($, on) => {
    const w = world(
      on,
      binding(argv => {
        if (argv[3] === 'dir') {
          return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
        }

        if (argv[3] === 'wait-permit') {
          return { exitCode: 0, stdout: JSON.stringify({ decision: 'allow', from: 'pane', via: 'pane' }) }
        }

        return { exitCode: 0, stdout: JSON.stringify(BOARD) }
      }),
    )

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.turn.start({ text: 'go', turnId: 't-1' })
    await askingCall(w, $ as never, async id => {
      await $.classic.PostToolUse({ tool_name: 'Bash', tool_input: {}, tool_response: {}, tool_use_id: id } as never)

      return { result: {} }
    })

    expect(recorded(w)).toEqual([
      { ...PLACE, permission: { ...GIT_STATUS, id: `t-1-${START}` }, answer: { path: 'inbox', via: 'pane', decision: 'allow' } },
    ])
  })

  const AUTH = { precedents: [{ q: 1, id: 'L1-aaaa' }], reason: '선례' }

  test('a model permit the CLI took leaves the precedents it gave with the answer', async ($, on) => {
    const w = world(
      on,
      binding(argv => {
        if (argv[3] === 'dir') {
          return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
        }

        if (argv[3] === 'wait-permit') {
          return { exitCode: 0, stdout: JSON.stringify({ decision: 'allow', from: 'agent', via: 'agent', auth: AUTH }) }
        }

        return { exitCode: 0, stdout: JSON.stringify(BOARD) }
      }),
    )

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.turn.start({ text: 'go', turnId: 't-1' })
    await askingCall(w, $ as never, async () => ({ result: {} }))

    expect(recorded(w)).toEqual([
      {
        ...PLACE,
        permission: { ...GIT_STATUS, id: `t-1-${START}` },
        answer: { path: 'inbox', via: 'agent', decision: 'allow', auth: AUTH },
      },
    ])
  })

  test('a model answer the CLI took leaves the precedents it gave with the answers', async ($, on) => {
    const answering = { exitCode: 0, stdout: '' }
    const w = world(on, argv => {
      if (argv[3] === 'dir') {
        return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
      }

      return argv[3] === 'take' ? answering : { exitCode: 0, stdout: JSON.stringify(BOARD) }
    })

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS, tool_use_id: 'tu-9' } as never)
    await w.clock.settle()
    answering.stdout = JSON.stringify({
      actions: [{ kind: 'answer', tool_use_id: 'tu-9', answers: ['Red'], from: 'agent', via: 'agent', auth: AUTH }],
      results: [{ id: 'r1', kind: 'answer', status: 'delivered', via: 'agent' }],
    })
    w.inbox.files = ['r1.json']
    await w.clock.advance(2_000)
    await call

    expect(recorded(w)).toEqual([
      { ...PLACE, at: START + 2_000, question: COLOR_ASK, answer: { path: 'inbox', via: 'agent', answers: ['Red'], auth: AUTH } },
    ])
  })

  test('a permit wait hands the CLI the request input uncut', async ($, on) => {
    const w = world(on, argv => {
      if (argv[3] === 'dir') {
        return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
      }

      if (argv[3] === 'wait-permit') {
        return { exitCode: 0, stdout: JSON.stringify({ decision: 'deny', from: 'pane', via: 'pane' }) }
      }

      return { exitCode: 0, stdout: JSON.stringify(BOARD) }
    })
    const command = `cat ${'a/'.repeat(150)}x`

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: { command, timeout: 5 } } as never)

    const wait = w.runs.find(x => x.argv[3] === 'wait-permit')
    expect(JSON.parse(wait?.stdin ?? '{}').permission).toEqual({ tool: 'Bash', input: { command } })
    expect((stored(w) as { permission: unknown }).permission).toBeNull()
  })

  test('a prompt while a decision is open leaves the prompt and where it came from', async ($, on) => {
    const w = world(on)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.tool.call(WRAP_UP as never)
    await $.prompt.submit({ text: '진행', origin: { kind: 'composer' } } as never)
    await $.prompt.submit({ text: '다음 일', origin: { kind: 'composer' } } as never)

    expect(recorded(w)).toEqual([
      { ...PLACE, decision: DECISION, answer: { path: 'session', text: '진행', origin: 'composer' } },
    ])
  })

  test('a decide the kanban delivered leaves the way the CLI said it came', async ($, on) => {
    const w = world(
      on,
      routes({
        'inbox dir': { dir: INBOX_DIR },
        'inbox take': {
          actions: [{ kind: 'decide', decision_at: String(START), text: '진행', from: 'pane', via: 'terminal' }],
          results: [{ id: 'r1', kind: 'decide', status: 'delivered', via: 'terminal' }],
        },
      }),
    )

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.tool.call(WRAP_UP as never)
    w.inbox.files = ['r1.json']
    await w.clock.advance(2_000)

    expect(recorded(w)).toEqual([
      { ...PLACE, at: START + 2_000, decision: DECISION, answer: { path: 'inbox', via: 'terminal', text: '진행' } },
    ])
  })

  test('a missing CLI loses only the entry, and the answer goes on', async ($, on) => {
    const w = world(on, () => 'missing')

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS, tool_use_id: 'tu-9' } as never)
    await w.clock.settle()
    w.dialog.answer?.({ 'Which color?': 'Red' })
    const r = await call

    expect(r.result).toEqual({ questions: TOOL_QUESTIONS, answers: { 'Which color?': 'Red' } })
    expect(ledgerRuns(w)).toHaveLength(1)
  })

  test('a decide taken on a relay leaves what the CLI said it came on', async ($, on) => {
    const w = world(
      on,
      routes({
        'inbox dir': { dir: INBOX_DIR },
        'inbox take': {
          actions: [
            { kind: 'decide', decision_at: String(START), text: '진행', from: 'agent', via: 'agent', auth: { relay: 'L1-r' } },
          ],
          results: [{ id: 'r1', kind: 'decide', status: 'delivered', via: 'agent' }],
        },
      }),
    )

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await $.tool.call(WRAP_UP as never)
    w.inbox.files = ['r1.json']
    await w.clock.advance(2_000)

    expect(recorded(w)).toEqual([
      {
        ...PLACE,
        at: START + 2_000,
        decision: DECISION,
        answer: { path: 'inbox', via: 'agent', text: '진행', auth: { relay: 'L1-r' } },
      },
    ])
  })
})

describe('kanban relay frame', () => {
  const TARGET = { session: 'other', wait: 'tu-1', kind: 'question', recommended: ['Red'] }
  const FRAMED = [
    {
      question: '칸반 대기 · project-b · other · 질문\nWhich color?\n선택지: Red / Blue\n\n색은?',
      header: 'Color',
      multiSelect: false,
      options: [
        { label: 'Red', description: 'warm' },
        { label: 'Blue', description: 'cool' },
      ],
    },
  ]
  const FRAMED_ASK = {
    tool_use_id: 'tu-9',
    at: START,
    questions: [{ ...COLOR, question: FRAMED[0]?.question }],
    orig_len: { question: [FRAMED[0]?.question.length], labels: [[3, 4]] },
  }
  const ASKED = { tool_use_id: 'tu-9', at: START, questions: [COLOR], orig_len: { question: [12], labels: [[3, 4]] } }
  const PLACE = { at: START, session: SELF, repo_root: null, cwd: '/work/project-a', permission_mode: null }
  const CHOSE = { path: 'session', afk_timeout_ms: null, is_error: false, denied: false }

  /** A CLI whose `relay-frame` answers `framed`; every other command answers the board. */
  function framing(framed: unknown): FakeCli {
    return argv =>
      argv[2] === 'relay-frame'
        ? { exitCode: 0, stdout: JSON.stringify(framed) }
        : { exitCode: 0, stdout: JSON.stringify(BOARD) }
  }

  function recorded(w: World): unknown[] {
    return w.runs.filter(r => r.argv[2] === 'ledger' && r.argv[3] === 'record').map(r => JSON.parse(r.stdin ?? '{}'))
  }

  function frames(w: World): unknown {
    return (stored(w) as { relay_frames?: unknown }).relay_frames
  }

  /** Opens the dialog and lets the hooks settle; `call` ends once it is answered. */
  async function ask($: { tool: { call: (e: never) => Promise<unknown> } }, w: World, e: object = {}) {
    const call = $.tool.call({ tool: 'AskUserQuestion', questions: TOOL_QUESTIONS, tool_use_id: 'tu-9', ...e } as never)
    await w.clock.settle()

    return { call: call as Promise<{ result?: unknown }> }
  }

  test('the questions go to relay-frame and the dialog draws them as it rewrote them', async ($, on) => {
    const w = world(on, framing({ questions: FRAMED, target: TARGET }))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const { call } = await ask($, w)

    const run = w.runs.find(r => r.argv[2] === 'relay-frame')
    expect(run?.argv).toEqual(['atelier', 'kanban', 'relay-frame', '--now', String(START), '--session', SELF])
    expect(JSON.parse(run?.stdin ?? '{}')).toEqual({ questions: TOOL_QUESTIONS })
    expect(w.dialog.questions).toEqual(FRAMED)
    expect(frames(w)).toEqual({ 'tu-9': TARGET })

    w.dialog.answer?.({ [FRAMED[0]?.question ?? '']: 'Blue' })
    const r = await call

    expect(r.result).toEqual({ questions: FRAMED, answers: { [FRAMED[0]?.question ?? '']: 'Blue' } })
    expect(recorded(w)).toEqual([{ ...PLACE, question: FRAMED_ASK, relay_frame: TARGET, answer: { ...CHOSE, answers: ['Blue'] } }])
    expect(frames(w)).toEqual({})
  })

  test('a framed question the pane answered is recorded with its frame too', async ($, on) => {
    const answering = { exitCode: 0, stdout: '' }
    const w = world(on, argv => {
      if (argv[2] === 'relay-frame') {
        return { exitCode: 0, stdout: JSON.stringify({ questions: FRAMED, target: TARGET }) }
      }

      if (argv[3] === 'dir') {
        return { exitCode: 0, stdout: JSON.stringify({ dir: INBOX_DIR }) }
      }

      return argv[3] === 'take' ? answering : { exitCode: 0, stdout: JSON.stringify(BOARD) }
    })

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const { call } = await ask($, w)
    const take = w.runs.length
    answering.stdout = JSON.stringify({
      actions: [{ kind: 'answer', tool_use_id: 'tu-9', answers: ['Red'], from: 'pane', via: 'pane' }],
      results: [{ id: 'r1', kind: 'answer', status: 'delivered', via: 'pane' }],
    })
    w.inbox.files = ['r1.json']
    await w.clock.advance(2_000)
    const r = await call

    const taken = w.runs.slice(take).find(x => x.argv[3] === 'take')
    expect(JSON.parse(taken?.stdin ?? '{}').asks).toEqual([{ id: 'tu-9', questions: FRAMED }])
    expect(r.result).toEqual({ questions: FRAMED, answers: { [FRAMED[0]?.question ?? '']: 'Red' } })
    expect(recorded(w)).toEqual([
      {
        ...PLACE,
        at: START + 2_000,
        question: FRAMED_ASK,
        relay_frame: TARGET,
        answer: { path: 'inbox', via: 'pane', answers: ['Red'] },
      },
    ])
  })

  for (const [name, cli] of [
    ['a missing CLI', (() => 'missing') as FakeCli],
    ['a CLI that names no wait', framing({ questions: TOOL_QUESTIONS, target: null })],
    ['a CLI target with no recommendation', framing({ questions: FRAMED, target: { session: 'other', wait: 'tu-1', kind: 'question' } })],
    ['a CLI error', framing({ error: '표지가 가리키는 대기 `other tu-1` 를 찾지 못했어요' })],
  ] as const) {
    test(`${name} draws the questions as asked and binds them to no wait`, async ($, on) => {
      const w = world(on, cli)

      await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
      const { call } = await ask($, w)

      expect(w.dialog.questions).toEqual(TOOL_QUESTIONS)
      expect(frames(w) ?? {}).toEqual({})

      w.dialog.answer?.({ 'Which color?': 'Red' })
      const r = await call

      expect(r.result).toEqual({ questions: TOOL_QUESTIONS, answers: { 'Which color?': 'Red' } })
      expect(recorded(w)).toEqual([{ ...PLACE, question: ASKED, answer: { ...CHOSE, answers: ['Red'] } }])
    })
  }

  test('questions the engine refuses as rewritten are drawn as asked and bound to no wait', async ($, on) => {
    const w = world(on, framing({ questions: FRAMED, target: TARGET }))
    w.dialog.refuses = questions => JSON.stringify(questions) !== JSON.stringify(TOOL_QUESTIONS)

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    const { call } = await ask($, w)

    expect(w.dialog.questions).toEqual(TOOL_QUESTIONS)
    expect(frames(w)).toEqual({})
    expect((stored(w).asks?.['tu-9'] as { questions: unknown }).questions).toEqual([COLOR])

    w.dialog.answer?.({ 'Which color?': 'Red' })
    const r = await call

    expect(r.result).toEqual({ questions: TOOL_QUESTIONS, answers: { 'Which color?': 'Red' } })
    expect(recorded(w)).toEqual([{ ...PLACE, question: ASKED, answer: { ...CHOSE, answers: ['Red'] } }])
  })

  test("a subagent's question is not framed", async ($, on) => {
    const w = world(on, framing({ questions: FRAMED, target: TARGET }))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await ask($, w, { agentId: 'a1' })

    expect(w.runs.some(r => r.argv[2] === 'relay-frame')).toBe(false)
    expect(w.dialog.questions).toEqual(TOOL_QUESTIONS)
  })
})

describe('kanban delegate', () => {
  const WAIT_ID = 'wait:w:question:tu-1'
  const SAID = '[칸반 위임] 입력 대기 1건\n- w question tu-1'
  const ON = { text: '칸반 위임을 켰어요', delegate_since: START }

  type Wake = {
    submit?: string | null
    context?: string | null
    deliver_ids?: string[]
    report_due?: boolean
    report_at_error?: string | null
    toast?: string | null
    state?: unknown
  }

  function wakeOut(wake: Wake) {
    return {
      delegate: true,
      submit: null,
      context: null,
      deliver_ids: [WAIT_ID],
      report_due: false,
      report_at_error: null,
      toast: null,
      state: { pending: [{ id: WAIT_ID, since: START }], delivered: [] },
      ...wake,
    }
  }

  /**
   * A CLI that turns delegation on, and answers each `delegate wake` with the next of `wakes`, the last one after;
   * `delegate attach` answers the next of `attaches`, then no.
   */
  function delegating(wakes: Wake[], command: unknown = ON, attaches: boolean[] = []): FakeCli {
    let n = 0
    let attached = 0

    return argv => {
      if (argv[2] === 'command') {
        return { exitCode: 0, stdout: JSON.stringify(command) }
      }

      if (argv[2] === 'delegate' && argv[3] === 'attach') {
        attached += 1

        return { exitCode: 0, stdout: JSON.stringify({ attach: attaches[attached - 1] ?? false }) }
      }

      if (argv[2] === 'delegate' && argv[3] === 'wake') {
        const wake = wakes[Math.min(n, wakes.length - 1)] ?? {}
        n += 1

        return { exitCode: 0, stdout: JSON.stringify(wakeOut(wake)) }
      }

      return { exitCode: 0, stdout: JSON.stringify(argv[2] === 'board' ? BOARD : {}) }
    }
  }

  function wakeRuns(w: World): Run[] {
    return w.runs.filter(r => r.argv[2] === 'delegate' && r.argv[3] === 'wake')
  }

  function attachRuns(w: World): (readonly string[])[] {
    return w.runs.filter(r => r.argv[2] === 'delegate' && r.argv[3] === 'attach').map(r => r.argv.slice(2))
  }

  function sentState(run: Run | undefined): unknown {
    return (JSON.parse(run?.stdin ?? '{}') as { state?: unknown }).state
  }

  async function turnOn($: { command: { run: (e: never) => Promise<unknown> } }) {
    return $.command.run({ command: 'kanban', args: 'delegate on', origin: { kind: 'composer' } } as never)
  }

  test('/kanban delegate hands the CLI its arguments and origin, and keeps the delegate_since it names', async ($, on) => {
    const w = world(on, delegating([]))

    const ran = await turnOn($)

    expect(ran).toEqual({ text: '칸반 위임을 켰어요' })
    expect(w.runs.find(r => r.argv[2] === 'command')?.argv).toEqual([
      'atelier',
      'kanban',
      'command',
      '--session',
      SELF,
      '--now',
      String(START),
      '--origin',
      'composer',
      '--args',
      'delegate on',
    ])
    expect((stored(w) as Record<string, unknown>).delegate_since).toBe(START)
    expect(w.shown.value).toBe(false)
    expect(boardRuns(w)).toHaveLength(0)
  })

  test('a command the CLI refuses shows why and leaves the record as it was', async ($, on) => {
    const w = world(on, delegating([], { error: '사람이 입력한 `/kanban delegate` 만 받아요 (출처: plugin)' }))

    const ran = await $.command.run({ command: 'kanban', args: 'delegate on', origin: { kind: 'plugin', name: 'x' } } as never)

    expect(ran).toEqual({ text: '사람이 입력한 `/kanban delegate` 만 받아요 (출처: plugin)' })
    expect(w.runs.find(r => r.argv[2] === 'command')?.argv.slice(7, 9)).toEqual(['--origin', 'plugin'])
    expect(w.store[KEY]).toBeUndefined()
  })

  test('a missing CLI says so and turns nothing on', async ($, on) => {
    const w = world(on, () => 'missing')

    const ran = await turnOn($)

    expect(ran).toEqual({ text: 'atelier CLI 를 실행하지 못했어요 — /atelier:update 로 설치·갱신하세요' })
    expect(w.store[KEY]).toBeUndefined()
  })

  test('a session never turned on does not ask for a wake', async ($, on) => {
    const w = world(on, delegating([{ submit: SAID }]))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await w.clock.advance(4_000)

    expect(wakeRuns(w)).toHaveLength(0)
    expect(w.submitted).toEqual([])
  })

  test('a delegate submits the prompt the wake gives and hands its ids back as delivered', async ($, on) => {
    const w = world(on, delegating([{ submit: SAID }, {}]))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await turnOn($)
    w.prompt.draft = 'half typed'
    await w.clock.advance(2_000)

    const first = wakeRuns(w)[0]
    const now = START + 2_000
    expect(first?.argv).toEqual([
      'atelier',
      'kanban',
      'delegate',
      'wake',
      '--session',
      SELF,
      '--now',
      String(now),
      '--report-at',
      '18:00',
      '--utc-offset',
      String(-new Date(now).getTimezoneOffset()),
    ])
    const sent = JSON.parse(first?.stdin ?? '{}') as { sessions: { session_id: string }[]; state: unknown }
    expect(sent.sessions.map(r => r.session_id)).toEqual([SELF])
    expect(sent.state).toEqual({ pending: [], delivered: [], draft_len: 10 })
    expect(w.submitted).toEqual([SAID])

    await w.clock.advance(2_000)
    expect(sentState(wakeRuns(w)[1])).toEqual({
      pending: [{ id: WAIT_ID, since: START }],
      delivered: [WAIT_ID],
      draft_len: 10,
    })
  })

  test('a prompt the engine drops leaves its ids undelivered', async ($, on) => {
    const w = world(on, delegating([{ submit: SAID }, {}]))
    w.prompt.drop = 'a Stop stands'

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await turnOn($)
    await w.clock.advance(4_000)

    expect(sentState(wakeRuns(w)[1])).toEqual({ pending: [{ id: WAIT_ID, since: START }], delivered: [], draft_len: 0 })
  })

  test('a context rides on the next prompt delegate attach lets it, and only then counts as delivered', async ($, on) => {
    const w = world(on, delegating([{ context: SAID }, {}], ON, [false, true]))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await turnOn($)
    await w.clock.advance(2_000)
    expect(w.submitted).toEqual([])

    await $.prompt.submit({ text: 'from a plugin', origin: { kind: 'plugin', name: 'x' } } as never)
    await $.prompt.submit({ text: 'what is next?', origin: { kind: 'composer' } } as never)
    await $.prompt.submit({ text: 'and then?', origin: { kind: 'composer' } } as never)

    expect(attachRuns(w)).toEqual([
      ['delegate', 'attach', '--origin', 'plugin'],
      ['delegate', 'attach', '--origin', 'composer'],
    ])
    expect(w.prompt.contexts).toEqual([undefined, [SAID], undefined])
    await w.clock.advance(2_000)
    expect((sentState(wakeRuns(w)[1]) as { delivered: string[] }).delivered).toEqual([WAIT_ID])
  })

  test('delegate attach is told when the prompt is a slash command or a meta prompt', async ($, on) => {
    const w = world(on, delegating([{ context: SAID }, {}]))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await turnOn($)
    await w.clock.advance(2_000)
    await $.prompt.submit({ text: '/compact', command: 'compact', origin: { kind: 'composer' } } as never)
    await $.prompt.submit({ text: 'note', isMeta: true } as never)

    expect(attachRuns(w)).toEqual([
      ['delegate', 'attach', '--origin', 'composer', '--slash-command'],
      ['delegate', 'attach', '--origin', '', '--meta'],
    ])
    expect(w.prompt.contexts).toEqual([undefined, undefined])
  })

  test('an attach the CLI cannot answer leaves the prompt as typed and the context offered', async ($, on) => {
    const delegate = delegating([{ context: SAID }, {}], ON, [true])
    let failing = true
    const w = world(on, argv => (failing && argv[3] === 'attach' ? 'missing' : delegate(argv)))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await turnOn($)
    await w.clock.advance(2_000)
    await $.prompt.submit({ text: 'what is next?', origin: { kind: 'composer' } } as never)
    failing = false
    await $.prompt.submit({ text: 'and now?', origin: { kind: 'composer' } } as never)

    expect(w.prompt.contexts).toEqual([undefined, [SAID]])
  })

  test('a daily report delivered is left in the ledger and the ledger is pruned', async ($, on) => {
    const w = world(on, delegating([{ submit: '[칸반 위임] 일일 보고 — 직전 보고 이후(--since 0)', report_due: true }, {}]))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await turnOn($)
    await w.clock.advance(2_000)

    const after = w.runs.filter(r => (r.argv[2] === 'delegate' && r.argv[3] === 'report') || (r.argv[2] === 'ledger' && r.argv[3] === 'prune'))
    expect(after.map(r => r.argv.slice(2))).toEqual([
      ['delegate', 'report', '--now', String(START + 2_000)],
      ['ledger', 'prune', '--now', String(START + 2_000)],
    ])
  })

  for (const [name, cli] of [
    ['a missing CLI', (() => 'missing') as FakeCli],
    ['a CLI error', ((argv: readonly string[]) =>
      argv[2] === 'command'
        ? { exitCode: 0, stdout: JSON.stringify(ON) }
        : { exitCode: 0, stdout: JSON.stringify({ error: '--report-at 은 `HH:MM` 이에요' }) }) as FakeCli],
  ] as const) {
    test(`${name} on a wake says nothing`, async ($, on) => {
      const w = world(on, cli)

      await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
      w.store[KEY] = { ...(w.store[KEY] as object), delegate_since: START }
      await w.clock.advance(4_000)
      await $.prompt.submit({ text: 'hi', origin: { kind: 'composer' } } as never)

      expect(w.submitted).toEqual(['hi'])
      expect(w.prompt.contexts).toEqual([undefined])
    })
  }

  test('the report time comes from the plugin options', { options: { kanban_report_at: '09:30' } }, async ($, on) => {
    const w = world(on, delegating([{}]))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await turnOn($)
    await w.clock.advance(2_000)

    const argv = wakeRuns(w)[0]?.argv ?? []
    expect(argv[argv.indexOf('--report-at') + 1]).toBe('09:30')
  })

  test('the toast a wake names is shown, and its report time error goes back to the next wake', async ($, on) => {
    const error = '--report-at 은 `HH:MM`(00:00~23:59) 이에요 (받은 값: 9:00)'
    const w = world(on, delegating([{ submit: SAID, report_at_error: error, toast: error }, { report_at_error: error }]))

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    await turnOn($)
    await w.clock.advance(2_000)
    await w.clock.advance(2_000)

    const told = wakeRuns(w).map(r => (JSON.parse(r.stdin ?? '{}') as { told_report_at?: unknown }).told_report_at)
    expect(told.slice(0, 2)).toEqual([null, error])
    expect(w.toasts).toEqual([error])
    expect(w.submitted).toEqual([SAID])
  })

  test('the inbox record keeps how the last request came in', async ($, on) => {
    const w = world(
      on,
      routes({
        'inbox dir': { dir: INBOX_DIR },
        'inbox take': { actions: [], results: [{ id: 'r1', kind: 'answer', status: 'refused', via: 'agent' }] },
      }),
    )

    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })
    w.inbox.files = ['r1.json']
    await w.clock.advance(2_000)

    expect((stored(w) as Record<string, unknown>).inbox).toEqual({
      id: 'r1',
      kind: 'answer',
      status: 'refused',
      via: 'agent',
      at: START + 2_000,
    })
  })
})
