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

// herdr-app keeps a terminal's interactive shell after the agent in it exits, and a person may
// start another program there, so only a terminal running Claude (`program.key`, both for an
// interactive session and an attached one) holds the session. A terminal without `program`
// counts by its state alone.
const holdsClaude = (t) => isLive(t) && (t.program ? t.program.key === 'claude' : true)

/**
 * Where a card stands in herdr-app: `open` in a live terminal running Claude (the session's own
 * first, then the one a board opened), `opening` for `holdMs` after a board claimed a tab until
 * Claude runs in it, else `closed`.
 */
export function cardState(s, terminals, now, holdMs) {
  const holds = (address) => Boolean(address) && terminals.some((t) => t.address === address && holdsClaude(t))
  if (holds(s.hc_terminal)) return { kind: 'open', address: s.hc_terminal }
  const opened = s.opened ?? null
  if (holds(opened?.terminal)) return { kind: 'open', address: opened.terminal }
  if (opened && now - opened.at < holdMs) return { kind: 'opening' }
  return { kind: 'closed' }
}

/** When the first `opening` card turns `closed`; null when no card is opening. */
export function openingEnds(sessions, terminals, now, holdMs) {
  const ends = sessions
    .filter((s) => cardState(s, terminals, now, holdMs).kind === 'opening')
    .map((s) => s.opened.at + holdMs)
  return ends.length > 0 ? Math.min(...ends) : null
}

/** Claims a new tab over the opened entry this card was read with. */
export function claimArgs(s) {
  return ['kanban', 'opened', '--session', s.session_id, '--opening', '--seen', s.opened ? String(s.opened.at) : 'none']
}

export function releaseArgs(s, at) {
  return ['kanban', 'opened', '--session', s.session_id, '--clear', '--seen', String(at)]
}

/** The claim's time, or why there is none. */
export function claimed(r) {
  const out = json(r.stdout)
  if (typeof out?.error === 'string') return { error: out.error }
  const at = out?.opened?.at
  if (r.code !== 0 || typeof at !== 'number') return { error: said(r) || `exit ${r.code}` }
  return { at }
}

/** The running Claude sessions from `claude agents --json`; null when the run gave none. */
export function parseAgents(r) {
  const out = r.code === 0 ? json(r.stdout) : null
  return Array.isArray(out) ? out : null
}

// The tab's shell has `HC_TERMINAL_ID` of the tab itself; a background session's own hook never
// does, so the tab reports its terminal before it starts Claude.
const REPORT = 'atelier kanban opened --session "$1" --terminal "$HC_TERMINAL_ID" >/dev/null 2>&1'

/**
 * How to open a closed card, from the running sessions: a background session is attached,
 * a session that is not running is resumed in its folder, and a session running somewhere else
 * is left alone so the talk doesn't get a second process.
 */
export function reopenTarget(s, running) {
  const found = running.find((a) => a.sessionId === s.session_id)
  if (found && typeof found.id !== 'string') return { kind: 'elsewhere' }
  const argv = found
    ? ['sh', '-c', `${REPORT}; exec claude attach "$2"`, 'sh', s.session_id, found.id]
    : ['sh', '-c', `${REPORT}; exec claude --resume "$1"`, 'sh', s.session_id]
  return {
    kind: found ? 'attach' : 'resume',
    command: 'tab.new',
    args: { name: s.label ?? s.session_id, folder: s.cwd, argv },
  }
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

/** What a redraw depends on: the sessions without their ages, which move on every read. */
export function boardKey(sessions) {
  return JSON.stringify((sessions ?? []).map(({ age: _age, ...rest }) => rest))
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
