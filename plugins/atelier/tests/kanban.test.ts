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

type Run = { argv: readonly string[]; stdin?: string }

type World = {
  store: Record<string, unknown>
  clock: ReturnType<typeof mock.clock>
  runs: Run[]
  shown: { value: boolean }
}

/** Stands for the engine beneath the plugin: one session, its store, its clock, the CLI. */
type FakeCli = (argv: readonly string[]) => { exitCode: number; stdout: string } | 'missing'

function world(on: On, cli: FakeCli = () => ({ exitCode: 0, stdout: JSON.stringify(BOARD) })): World {
  const store: Record<string, unknown> = {}
  const runs: Run[] = []
  const shown = { value: false }

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
  on('session.start', ($, e) => ({ cwd: e.cwd }))
  on('prompt.submit', ($, e) => ({ text: e.text }))
  on('turn.start', ($, e) => ({ turnId: e.turnId }))
  on('turn.complete', () => ({ text: '' }))
  on('session.end', ($, e) => ({ sessionId: e.sessionId }))
  on('classic.UserPromptSubmit', () => ({}))
  on('classic.PermissionRequest', () => ({}))
  on('classic.Notification', () => ({}))
  on('classic.PostToolUse', () => ({}))
  on('classic.PostToolUseFailure', () => ({}))

  return { store, clock, runs, shown }
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
}

function stored(w: World): StoredRecord {
  return w.store[KEY] as StoredRecord
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

describe('kanban pane', () => {
  test('/kanban opens the pane and asks the CLI for the board of every kanban record', async ($, on) => {
    const w = world(on)

    w.store['unrelated'] = { keep: true }
    w.store['kanban:session:gone'] = { v: 1, session_id: 'gone', events: [] }
    await $.session.start({ cwd: '/work/project-a', surface: null, isInteractive: true })

    const ran = await $.command.run({ command: 'kanban', args: '', origin: { kind: 'composer' } } as never)

    expect(ran).toEqual({ text: '세션 칸반을 열었어요.' })
    expect(w.runs).toHaveLength(1)
    expect(w.runs[0]?.argv).toEqual([
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
    const sent = JSON.parse(w.runs[0]?.stdin ?? '{}') as { sessions: { session_id: string }[] }
    expect(sent.sessions.map(r => r.session_id).sort()).toEqual(['gone', SELF])
    expect(w.store['kanban:session:gone']).toBeUndefined()
    expect(w.store['unrelated']).toEqual({ keep: true })
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
    expect(w.runs).toHaveLength(0)

    w.shown.value = true
    await w.clock.advance(2_000)
    expect(w.runs).toHaveLength(1)
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

    const widths = w.runs.map(r => r.argv[r.argv.indexOf('--width') + 1])
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
})
