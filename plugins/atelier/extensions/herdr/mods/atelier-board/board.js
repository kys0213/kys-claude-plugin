// What the panel shows and runs, from `atelier kanban status` and herdr's terminals.
// No herdr or gpui imports, so it runs under node's test runner as well.

export const COLUMNS = [
  { id: 'needs_input', heading: '입력 대기' },
  { id: 'working', heading: '작업 중' },
  { id: 'idle', heading: '쉬는 중' },
  { id: 'stale', heading: '응답 없음' },
]

const RANK = { needs_input: 4, working: 3, idle: 2, stale: 1 }

const isLive = (t) => t?.state === 'live'

// A session's parent on the board: the session whose hc terminal started its own.
function parents(sessions, terminals) {
  const byTerminal = new Map(sessions.filter((s) => s.hc_terminal).map((s) => [s.hc_terminal, s]))
  const terminal = new Map(terminals.filter(isLive).map((t) => [t.address, t]))
  const parentOf = new Map()

  for (const s of sessions) {
    const up = byTerminal.get(terminal.get(s.hc_terminal)?.parent)
    if (up && up !== s) parentOf.set(s, up)
  }

  // A loop (only possible from stale addresses) leaves its members unparented.
  for (const s of sessions) {
    const seen = new Set([s])
    for (let p = parentOf.get(s); p; p = parentOf.get(p)) {
      if (seen.has(p)) {
        for (const member of seen) parentOf.delete(member)
        break
      }
      seen.add(p)
    }
  }

  return parentOf
}

const members = (g) => [g.session, ...g.children.flatMap(members)]

/**
 * The four columns, each with its groups: a session and the sessions its hc terminal started.
 * A group stands in its most urgent member's column, in the order its head was listed.
 */
export function layout(sessions, terminals) {
  const parentOf = parents(sessions, terminals)
  const group = (s) => ({
    session: s,
    children: sessions.filter((c) => parentOf.get(c) === s).map(group),
  })
  const groups = sessions.filter((s) => !parentOf.has(s)).map(group)
  const columnOf = (g) =>
    members(g).reduce((best, m) => ((RANK[m.column] ?? 0) > (RANK[best] ?? 0) ? m.column : best), g.session.column)

  return COLUMNS.map((c) => {
    const placed = groups.filter((g) => columnOf(g) === c.id)
    return { ...c, groups: placed, count: placed.reduce((n, g) => n + members(g).length, 0) }
  })
}

/** The command that shows a terminal: its pane, or a peek for a sub-agent without one. */
export function moveTarget(address, agents, terminals) {
  if (!address || !terminals.some((t) => t.address === address && isLive(t))) return null
  const hasPane = agents.some((a) => a.address === address)
  return { command: hasPane ? 'pane.focus' : 'terminal.peek', args: { terminal: address } }
}

export function permitArgs(s, decision) {
  return ['kanban', 'permit', '--session', s.session_id, '--request', s.wait.id, '--decision', decision, '--from', 'pane']
}

/** `picks[i]` are the labels chosen for question i; null until every question has one. */
export function replyArgs(s, picks) {
  const questions = s.wait.questions ?? []
  const answers = questions.map((q, i) => {
    const chosen = new Set(picks[i] ?? [])
    return q.options.map((o) => o.label).filter((label) => chosen.has(label)).join(', ')
  })
  if (answers.length === 0 || answers.some((a) => a === '')) return null
  return ['kanban', 'reply', '--session', s.session_id, '--ask', s.wait.id, ...answers.flatMap((a) => ['--answer', a]), '--from', 'pane']
}

export function decideArgs(s, text) {
  return ['kanban', 'decide', '--session', s.session_id, '--at', s.wait.id, '--text', text, '--from', 'pane']
}

const said = (r) => [r.stdout, r.stderr].map((x) => (x ?? '').trim()).filter(Boolean).join(' / ')

function json(text) {
  try {
    return JSON.parse(text)
  } catch {
    return null
  }
}

export function parseStatus(r) {
  const out = r.code === 0 ? json(r.stdout) : null
  if (Array.isArray(out?.sessions)) return { sessions: out.sessions }
  return { error: typeof out?.error === 'string' ? out.error : `atelier kanban status 가 실패했어요: ${said(r) || `exit ${r.code}`}` }
}

/** The folder `atelier kanban record-path` puts a session's record copy in; null when unknown. */
export function recordsDir(r) {
  const path = r.code === 0 ? json(r.stdout)?.path : null
  if (typeof path !== 'string') return null
  const cut = path.lastIndexOf('/')
  return cut > 0 ? path.slice(0, cut) : null
}

export function outcome(r) {
  const out = json(r.stdout)
  if (typeof out?.error === 'string') return { error: out.error }
  if (r.code !== 0 || out === null) return { error: said(r) || `exit ${r.code}` }
  return { ok: true }
}
