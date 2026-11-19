import assert from 'node:assert/strict'
import { describe, test } from 'node:test'

import {
  boardKey,
  decideArgs,
  layout,
  moveTarget,
  outcome,
  parseAgents,
  parseStatus,
  permitArgs,
  recordsDir,
  reopenTarget,
  replyArgs,
} from '../../extensions/herdr/mods/atelier-board/board.js'

const session = (id, column, extra = {}) => ({ session_id: id, column, hc_terminal: null, ...extra })
const live = (address, parent = null) => ({ address, parent, state: 'live' })

const ids = (groups) => groups.map((g) => [g.session.session_id, ids(g.children)])

describe('layout', () => {
  test('keeps the four columns in board order, empty ones too', () => {
    const cols = layout([session('w', 'working'), session('a', 'needs_input')], [])

    assert.deepEqual(
      cols.map((c) => [c.id, c.heading, c.count]),
      [
        ['needs_input', '입력 대기', 1],
        ['working', '작업 중', 1],
        ['idle', '쉬는 중', 0],
        ['stale', '응답 없음', 0],
      ],
    )
  })

  test('puts a session started from another session’s hc terminal under that session', () => {
    const cols = layout(
      [session('p', 'idle', { hc_terminal: 't1' }), session('c', 'idle', { hc_terminal: 't2' })],
      [live('t1'), live('t2', 't1')],
    )

    assert.deepEqual(ids(cols[2].groups), [['p', [['c', []]]]])
    assert.equal(cols[2].count, 2)
  })

  test('places a group in its most urgent member’s column', () => {
    const cols = layout(
      [session('p', 'idle', { hc_terminal: 't1' }), session('c', 'needs_input', { hc_terminal: 't2' })],
      [live('t1'), live('t2', 't1')],
    )

    assert.deepEqual(ids(cols[0].groups), [['p', [['c', []]]]])
    assert.deepEqual(cols[2].groups, [])
  })

  test('shows a child whose parent is not on the board as a card of its own', () => {
    const cols = layout([session('c', 'working', { hc_terminal: 't2' })], [live('t2', 't9')])

    assert.deepEqual(ids(cols[1].groups), [['c', []]])
  })

  test('ignores parents of terminals that are no longer live and survives a parent loop', () => {
    const cols = layout(
      [
        session('a', 'idle', { hc_terminal: 't1' }),
        session('b', 'idle', { hc_terminal: 't2' }),
        session('d', 'idle', { hc_terminal: 't3' }),
      ],
      [live('t1', 't2'), live('t2', 't1'), { address: 't3', parent: 't1', state: 'exited' }],
    )

    assert.deepEqual(ids(cols[2].groups), [
      ['a', []],
      ['b', []],
      ['d', []],
    ])
  })
})

describe('moveTarget', () => {
  test('focuses a terminal that has a pane and peeks at one that does not', () => {
    const terminals = [live('t1'), live('t2', 't1')]

    assert.deepEqual(moveTarget('t1', [{ address: 't1' }], terminals), {
      command: 'pane.focus',
      args: { terminal: 't1' },
    })
    assert.deepEqual(moveTarget('t2', [{ address: 't1' }], terminals), {
      command: 'terminal.peek',
      args: { terminal: 't2' },
    })
  })

  test('has nowhere to go without an address or a live terminal', () => {
    assert.equal(moveTarget(null, [], []), null)
    assert.equal(moveTarget('t1', [], [{ address: 't1', parent: null, state: 'exited' }]), null)
  })
})

describe('reopenTarget', () => {
  const outside = session('s1', 'stale', { label: 'fix login', cwd: '/w/app' })

  test('resumes a session that is not running, in its own folder', () => {
    assert.deepEqual(reopenTarget(outside, [{ pid: 9, sessionId: 'other', status: 'idle' }]), {
      kind: 'resume',
      command: 'tab.new',
      args: { name: 'fix login', folder: '/w/app', argv: ['claude', '--resume', 's1'] },
    })
  })

  test('attaches to a background session by its agent id', () => {
    assert.deepEqual(reopenTarget(outside, [{ id: 'bg-7', sessionId: 's1', state: 'blocked' }]), {
      kind: 'attach',
      command: 'tab.new',
      args: { name: 'fix login', folder: '/w/app', argv: ['claude', 'attach', 'bg-7'] },
    })
  })

  test('leaves a session running somewhere else alone', () => {
    assert.deepEqual(reopenTarget(outside, [{ pid: 9, sessionId: 's1', status: 'busy' }]), { kind: 'elsewhere' })
  })

  test('names the tab by the session id when the session has no label', () => {
    assert.equal(reopenTarget(session('s2', 'stale', { cwd: '/w' }), []).args.name, 's2')
  })
})

describe('answers', () => {
  const asking = (kind, extra = {}) => session('s1', 'needs_input', { wait: { kind, id: 'w-1', ...extra } })

  test('a permission answer names the request and comes from a person', () => {
    assert.deepEqual(permitArgs(asking('permission'), 'allow'), [
      'kanban', 'permit', '--session', 's1', '--request', 'w-1', '--decision', 'allow', '--from', 'pane',
    ])
  })

  test('a question answer gives one answer per question, several picks joined in option order', () => {
    const s = asking('question', {
      questions: [
        { question: 'Color?', multi_select: false, options: [{ label: 'Red' }, { label: 'Blue' }] },
        { question: 'Sizes?', multi_select: true, options: [{ label: 'S' }, { label: 'M' }, { label: 'L' }] },
      ],
    })

    assert.deepEqual(replyArgs(s, [['Blue'], ['L', 'S']]), [
      'kanban', 'reply', '--session', 's1', '--ask', 'w-1', '--answer', 'Blue', '--answer', 'S, L', '--from', 'pane',
    ])
  })

  test('a question answer with an unanswered question is not built', () => {
    const s = asking('question', {
      questions: [
        { question: 'A?', multi_select: false, options: [{ label: 'x' }] },
        { question: 'B?', multi_select: false, options: [{ label: 'y' }] },
      ],
    })

    assert.equal(replyArgs(s, [['x'], []]), null)
  })

  test('a decision answer sends the picked option as the reply', () => {
    assert.deepEqual(decideArgs(asking('decision'), '네'), [
      'kanban', 'decide', '--session', 's1', '--at', 'w-1', '--text', '네', '--from', 'pane',
    ])
  })
})

describe('reading the CLI', () => {
  test('a status run gives its sessions', () => {
    assert.deepEqual(parseStatus({ code: 0, stdout: '{"now":1,"sessions":[{"session_id":"a"}]}', stderr: '' }), {
      sessions: [{ session_id: 'a' }],
    })
  })

  test('a board’s key ignores the read time and the ages, and nothing else', () => {
    const at = (age, column = 'working') => [{ session_id: 'a', column, age, subtitle: 'x' }]

    assert.equal(boardKey(at('3s')), boardKey(at('4s')))
    assert.notEqual(boardKey(at('3s')), boardKey(at('3s', 'idle')))
    assert.notEqual(boardKey(at('3s')), boardKey([...at('3s'), { session_id: 'b', column: 'idle', age: '1m' }]))
  })

  test('a status run that failed or printed no board is an error', () => {
    for (const r of [
      { code: 1, stdout: '', stderr: 'boom' },
      { code: 0, stdout: 'not json', stderr: '' },
      { code: 0, stdout: '{"error":"--format 은 json 또는 text 예요"}', stderr: '' },
    ]) {
      assert.ok(typeof parseStatus(r).error === 'string', JSON.stringify(r))
    }
  })

  test('the records folder is the folder of a session’s record copy', () => {
    assert.equal(
      recordsDir({ code: 0, stdout: '{"path":"/Users/me/.claude/atelier/kanban/records/probe.json"}\n', stderr: '' }),
      '/Users/me/.claude/atelier/kanban/records',
    )
    for (const r of [
      { code: 1, stdout: '', stderr: 'boom' },
      { code: 0, stdout: '{"error":"x"}', stderr: '' },
      { code: 0, stdout: '{"path":"probe.json"}', stderr: '' },
    ]) {
      assert.equal(recordsDir(r), null, JSON.stringify(r))
    }
  })

  test('an agents run gives the running sessions, and nothing when it failed', () => {
    assert.deepEqual(parseAgents({ code: 0, stdout: '[{"pid":1,"sessionId":"a"}]', stderr: '' }), [{ pid: 1, sessionId: 'a' }])
    for (const r of [
      { code: 1, stdout: '', stderr: 'unknown command' },
      { code: 0, stdout: 'not json', stderr: '' },
      { code: 0, stdout: '{"error":"x"}', stderr: '' },
    ]) {
      assert.equal(parseAgents(r), null, JSON.stringify(r))
    }
  })

  test('an answer run is done unless it failed or printed an error', () => {
    assert.deepEqual(outcome({ code: 0, stdout: '{"queued":{"id":"r1"}}', stderr: '' }), { ok: true })
    assert.deepEqual(outcome({ code: 0, stdout: '{"error":"끝난 질문이에요"}', stderr: '' }), { error: '끝난 질문이에요' })
    assert.ok(typeof outcome({ code: 2, stdout: '', stderr: 'no such command' }).error === 'string')
  })
})
