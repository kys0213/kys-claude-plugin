import type { EngineInterface } from 'claude-code'

import { CLI_MISSING, cli } from './kanban-cli'

type Section = { title: string; lines: string[] }

type DecisionView = { ask: string; recap: string | null; options: string[] }

export type DetailCard = {
  session_id: string
  headline: string
  actions?: string[]
  turn_id?: string | null
  permission_id?: string | null
  detail?: { sections: Section[]; decision: DecisionView | null; delivery: string | null }
}

export type DetailState = {
  /** The card opened on this pane. */
  selected: string | null
  /** What was typed to send, by session. */
  drafts: Map<string, string>
  /** Sessions whose stop waits for a second press. */
  confirming: Set<string>
  /** The last outcome of a request from this pane, by session. */
  notes: Map<string, { text: string; isError: boolean }>
}

type Elements = ReturnType<EngineInterface['ui']['resolve']>

type Child = ReturnType<Elements['Text']>

export function newDetailState(): DetailState {
  return { selected: null, drafts: new Map(), confirming: new Set(), notes: new Map() }
}

export function toggleSelected($: EngineInterface, s: DetailState, sessionId: string): void {
  s.selected = s.selected === sessionId ? null : sessionId
  s.confirming.clear()
  $.ui.invalidate('ui.render')
}

/** Forgets what belonged to cards the board no longer shows. */
export function pruneDetail(s: DetailState, cards: readonly DetailCard[]): void {
  const live = new Set(cards.map(c => c.session_id))

  if (s.selected !== null && !live.has(s.selected)) {
    s.selected = null
  }

  for (const map of [s.drafts, s.notes]) {
    for (const id of [...map.keys()]) {
      if (!live.has(id)) {
        map.delete(id)
      }
    }
  }
}

async function request($: EngineInterface, s: DetailState, card: DetailCard, args: string[], sent: string) {
  const out = await cli($, args)

  if (out !== null && typeof out.queued === 'object') {
    s.notes.set(card.session_id, { text: sent, isError: false })
  } else {
    const reason = typeof out?.error === 'string' ? out.error : CLI_MISSING

    s.notes.set(card.session_id, { text: reason, isError: true })
  }

  $.ui.invalidate('ui.render')
}

async function send($: EngineInterface, s: DetailState, card: DetailCard, text: string): Promise<void> {
  const self = await $.session.id()

  await request(
    $,
    s,
    card,
    ['send', '--session', card.session_id, '--from', 'pane', '--from-label', self, '--text', text],
    '보냈어요 — 세션이 받으면 시작해요',
  )

  if (!s.notes.get(card.session_id)?.isError) {
    s.drafts.delete(card.session_id)
  }
}

async function stop($: EngineInterface, s: DetailState, card: DetailCard): Promise<void> {
  s.confirming.delete(card.session_id)

  if (typeof card.turn_id === 'string') {
    await request($, s, card, ['stop', '--session', card.session_id, '--turn', card.turn_id], '중지를 보냈어요')
  }
}

function sendBlock($: EngineInterface, s: DetailState, els: Elements, card: DetailCard): Child {
  const { Input } = els

  return Input({
    key: `detail:${card.session_id}:send`,
    placeholder: '이 세션에 맡길 일을 입력하고 Enter',
    value: s.drafts.get(card.session_id) ?? '',
    submitLabel: '보내기',
    onInput: (value: string) => {
      s.drafts.set(card.session_id, value)
    },
    onSubmit: (value: string) => void send($, s, card, value),
  })
}

function stopBlock($: EngineInterface, s: DetailState, els: Elements, card: DetailCard): Child {
  const { Box, Button } = els
  const id = card.session_id

  if (!s.confirming.has(id)) {
    return Button({
      key: `detail:${id}:stop`,
      label: '중지',
      onPress: () => {
        s.confirming.add(id)
        $.ui.invalidate('ui.render')
      },
    })
  }

  return Box({
    key: `detail:${id}:confirm`,
    flexDirection: 'row',
    columnGap: 1,
    children: [
      Button({ key: `detail:${id}:stop-yes`, label: '정말 중지', variant: 'primary', onPress: () => void stop($, s, card) }),
      Button({
        key: `detail:${id}:stop-no`,
        label: '취소',
        onPress: () => {
          s.confirming.delete(id)
          $.ui.invalidate('ui.render')
        },
      }),
    ],
  })
}

async function permit($: EngineInterface, s: DetailState, card: DetailCard, decision: 'allow' | 'deny'): Promise<void> {
  if (typeof card.permission_id === 'string') {
    await request(
      $,
      s,
      card,
      ['permit', '--session', card.session_id, '--request', card.permission_id, '--decision', decision],
      decision === 'allow' ? '허용을 보냈어요' : '거부를 보냈어요',
    )
  }
}

function permitBlock($: EngineInterface, s: DetailState, els: Elements, card: DetailCard): Child {
  const { Box, Button } = els
  const id = card.session_id

  return Box({
    key: `detail:${id}:permit`,
    flexDirection: 'row',
    columnGap: 1,
    children: [
      Button({ key: `detail:${id}:allow`, label: '허용', variant: 'primary', onPress: () => void permit($, s, card, 'allow') }),
      Button({ key: `detail:${id}:deny`, label: '거부', onPress: () => void permit($, s, card, 'deny') }),
    ],
  })
}

function decisionBlock($: EngineInterface, s: DetailState, els: Elements, card: DetailCard): Child[] {
  const { Box, Text, Button } = els
  const decision = card.detail?.decision

  if (decision === null || decision === undefined) {
    return []
  }

  return [
    ...(decision.recap ? [Text({ children: [decision.recap] })] : []),
    Text({ bold: true, children: [decision.ask] }),
    Box({
      key: `detail:${card.session_id}:options`,
      flexDirection: 'row',
      flexWrap: 'wrap',
      columnGap: 1,
      children: decision.options.map((label, i) =>
        Button({
          key: `detail:${card.session_id}:option:${i}`,
          label,
          ...(i === 0 ? { variant: 'primary' as const } : {}),
          onPress: () => void send($, s, card, label),
        }),
      ),
    }),
  ]
}

function sectionBlock(els: Elements, section: Section, index: number): Child {
  const { Box, Text } = els

  return Box({
    key: `section:${index}`,
    flexDirection: 'column',
    children: [
      Text({ bold: true, children: [section.title] }),
      Box({
        key: 'lines',
        flexDirection: 'column',
        paddingLeft: 2,
        children: section.lines.map(line => Text({ children: [line] })),
      }),
    ],
  })
}

export function detailBlock(
  $: EngineInterface,
  s: DetailState,
  els: Elements,
  card: DetailCard,
  close: { label: string; onPress: () => void },
): Child {
  const { Box, Text, Button } = els
  const actions = card.actions ?? []
  const note = s.notes.get(card.session_id)
  const delivery = card.detail?.delivery
  const offers = [
    ...(actions.includes('decide') ? decisionBlock($, s, els, card) : []),
    ...(actions.includes('permit') ? [permitBlock($, s, els, card)] : []),
    ...(actions.includes('send') ? [sendBlock($, s, els, card)] : []),
    ...(actions.includes('stop') ? [stopBlock($, s, els, card)] : []),
    ...(actions.includes('answer') ? [Text({ dimColor: true, children: ['질문은 아래 질문 카드에서 답해요'] })] : []),
  ]
  const status = [
    ...(note === undefined ? [] : [Text({ color: note.isError ? 'red' : undefined, dimColor: !note.isError, children: [note.text] })]),
    ...(delivery ? [Text({ dimColor: true, children: [delivery] })] : []),
  ]

  return Box({
    key: `detail:${card.session_id}`,
    flexDirection: 'column',
    marginTop: 1,
    rowGap: 1,
    borderStyle: 'round',
    borderDimColor: true,
    paddingX: 1,
    children: [
      Box({
        key: 'head',
        flexDirection: 'row',
        justifyContent: 'space-between',
        columnGap: 1,
        children: [
          Text({ bold: true, wrap: 'truncate', children: [card.headline] }),
          Button({ key: `detail:${card.session_id}:back`, label: close.label, plain: true, dimColor: true, onPress: close.onPress }),
        ],
      }),
      ...(card.detail?.sections ?? []).map((section, i) => sectionBlock(els, section, i)),
      ...(offers.length === 0 ? [] : [Box({ key: 'offers', flexDirection: 'column', rowGap: 1, children: offers })]),
      ...(status.length === 0 ? [] : [Box({ key: 'status', flexDirection: 'column', children: status })]),
    ],
  })
}
