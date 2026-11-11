import type { EngineInterface } from 'claude-code'

import { CLI_MISSING, runKanban } from './kanban-cli'
import { type AskQuestion, MAX_FIELD_CHARS } from './kanban-record'

const ANSWER_PREFIX = 'kanban:answer:'
const MAX_QUESTION_CHARS = 1_000

type ToolQuestion = {
  question: string
  header: string
  multiSelect: boolean
  options: { label: string; description?: string }[]
}

type Pick = { options: string[]; text: string | null }

export type CardAsk = { tool_use_id: string; questions: AskQuestion[] }

export type AskCard = { session_id: string; headline: string; asks?: CardAsk[] }

export type AskState = {
  /** Open AskUserQuestion calls of this session, by tool_use_id, resolved by a kanban answer. */
  waiters: Map<string, (answers: string[]) => void>
  /** The picks made on this pane, by `draftKey`. */
  drafts: Map<string, Pick[]>
  sent: Set<string>
  errors: Map<string, string>
}

type Elements = ReturnType<EngineInterface['ui']['resolve']>

type Child = ReturnType<Elements['Text']>

export function newAskState(): AskState {
  return { waiters: new Map(), drafts: new Map(), sent: new Set(), errors: new Map() }
}

export function askOf(questions: readonly ToolQuestion[], at: number) {
  return {
    at,
    questions: questions.map(q => ({
      question: q.question.slice(0, MAX_QUESTION_CHARS),
      header: q.header.slice(0, MAX_FIELD_CHARS),
      multi_select: q.multiSelect === true,
      options: q.options.map(o => ({
        label: o.label.slice(0, MAX_FIELD_CHARS),
        description: o.description?.slice(0, MAX_FIELD_CHARS) ?? null,
      })),
    })),
    orig_len: { question: questions.map(q => q.question.length), labels: questions.map(q => q.options.map(o => o.label.length)) },
  }
}

export function answerKey(sessionId: string, toolUseId: string): string {
  return `${ANSWER_PREFIX}${sessionId}:${toolUseId}`
}

export function answerKeysOf(keys: readonly string[], sessionId: string): string[] {
  return keys.filter(k => k.startsWith(`${ANSWER_PREFIX}${sessionId}:`))
}

export function awaitAnswer(s: AskState, toolUseId: string): Promise<string[]> {
  return new Promise(resolve => s.waiters.set(toolUseId, resolve))
}

/** Keys each answer by the asking call's own question, whatever the record cut. */
export function answersByQuestion(questions: readonly ToolQuestion[], answers: readonly string[]) {
  return Object.fromEntries(questions.map((q, i) => [q.question, answers[i] ?? '']))
}

function decodeAnswers(value: unknown): string[] | null {
  if (typeof value !== 'object' || value === null) {
    return null
  }

  const { v, answers } = value as Record<string, unknown>

  if (v !== 1 || !Array.isArray(answers) || !answers.every(a => typeof a === 'string')) {
    return null
  }

  return answers as string[]
}

export async function deliverAnswers($: EngineInterface, s: AskState): Promise<void> {
  if (s.waiters.size === 0) {
    return
  }

  const self = await $.session.id()

  for (const [id, resolve] of [...s.waiters]) {
    const key = answerKey(self, id)
    const answers = decodeAnswers(await $.store.get(key))

    if (answers !== null) {
      await $.store.delete(key)
      resolve(answers)
    }
  }
}

function draftKey(card: AskCard, ask: CardAsk): string {
  return `${card.session_id}:${ask.tool_use_id}`
}

function picksOf(s: AskState, key: string, count: number): Pick[] {
  return s.drafts.get(key) ?? Array.from({ length: count }, () => ({ options: [], text: null }))
}

function setPick($: EngineInterface, s: AskState, key: string, picks: Pick[], index: number, pick: Pick): void {
  s.drafts.set(key, picks.map((p, i) => (i === index ? pick : p)))
  s.errors.delete(key)
  $.ui.invalidate('ui.render')
}

function pressOption($: EngineInterface, s: AskState, key: string, ask: CardAsk, index: number, label: string): void {
  const picks = picksOf(s, key, ask.questions.length)
  const chosen = picks[index]?.options ?? []
  const options = ask.questions[index]?.multi_select
    ? chosen.includes(label)
      ? chosen.filter(o => o !== label)
      : [...chosen, label]
    : [label]

  setPick($, s, key, picks, index, { options, text: null })
}

function typeAnswer($: EngineInterface, s: AskState, key: string, ask: CardAsk, index: number, text: string): void {
  setPick($, s, key, picksOf(s, key, ask.questions.length), index, { options: [], text })
}

async function sendAnswer($: EngineInterface, s: AskState, card: AskCard, ask: CardAsk): Promise<void> {
  const key = draftKey(card, ask)
  const picks = picksOf(s, key, ask.questions.length)

  const r = await runKanban($, ['answer'], JSON.stringify({ questions: ask.questions, picks }))
  const parsed = r.kind === 'json' ? r.value : null
  const answers = Array.isArray(parsed?.answers) ? parsed.answers : null

  if (r.kind === 'missing') {
    s.errors.set(key, CLI_MISSING)
  } else if (answers !== null) {
    try {
      await $.store.set(answerKey(card.session_id, ask.tool_use_id), { v: 1, answers })
      s.sent.add(key)
      s.errors.delete(key)
    } catch {
      s.errors.set(key, CLI_MISSING)
    }
  } else {
    s.errors.set(key, typeof parsed?.error === 'string' ? parsed.error : 'atelier kanban answer 의 출력을 읽지 못했어요')
  }

  $.ui.invalidate('ui.render')
}

/** Forgets the picks of questions the board no longer shows. */
export function pruneDrafts(s: AskState, cards: readonly AskCard[]): void {
  const live = new Set(cards.flatMap(card => (card.asks ?? []).map(ask => draftKey(card, ask))))

  for (const key of [...s.drafts.keys(), ...s.sent, ...s.errors.keys()]) {
    if (!live.has(key)) {
      s.drafts.delete(key)
      s.sent.delete(key)
      s.errors.delete(key)
    }
  }
}

function optionMark(isMulti: boolean, isPicked: boolean): string {
  if (isMulti) {
    return isPicked ? '☑ ' : '☐ '
  }

  return isPicked ? '● ' : '○ '
}

function questionBlock(
  $: EngineInterface,
  s: AskState,
  els: Elements,
  card: AskCard,
  ask: CardAsk,
  index: number,
): Child {
  const { Box, Text, Button, Input } = els
  const key = draftKey(card, ask)
  const question = ask.questions[index] as AskQuestion
  const pick = picksOf(s, key, ask.questions.length)[index] ?? { options: [], text: null }
  const typed = pick.text ?? ''
  const count = ask.questions.length

  return Box({
    key: `q:${index}`,
    flexDirection: 'column',
    children: [
      Text({ dimColor: true, children: [count > 1 ? `${index + 1}/${count} · ${question.header}` : question.header] }),
      Text({ bold: true, children: [question.question] }),
      Box({
        key: 'options',
        flexDirection: 'column',
        paddingLeft: 1,
        children: question.options.map((option, oi) =>
          Button({
            key: `ask:${key}:${index}:${oi}`,
            plain: true,
            onPress: () => pressOption($, s, key, ask, index, option.label),
            children: [
              Text({ children: [optionMark(question.multi_select, pick.options.includes(option.label)), option.label] }),
              ...(option.description ? [Text({ dimColor: true, children: [`  ${option.description}`] })] : []),
            ],
          }),
        ),
      }),
      Box({
        key: 'typed',
        paddingLeft: 1,
        children: [
          Input({
            key: `ask:${key}:${index}:text`,
            placeholder: '✎ 직접 입력…',
            value: typed,
            submitLabel: '적용',
            onSubmit: (value: string) => typeAnswer($, s, key, ask, index, value),
          }),
        ],
      }),
      ...(typed.trim() === '' ? [] : [Text({ dimColor: true, wrap: 'truncate', children: [`답: ${typed}`] })]),
    ],
  })
}

function askBlock($: EngineInterface, s: AskState, els: Elements, card: AskCard, ask: CardAsk): Child {
  const { Box, Text, Button } = els
  const key = draftKey(card, ask)
  const error = s.errors.get(key)

  return Box({
    key: `ask:${key}`,
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
          Text({ bold: true, children: ['질문'] }),
          Text({ dimColor: true, wrap: 'truncate', children: [card.headline] }),
        ],
      }),
      ...ask.questions.map((_, index) => questionBlock($, s, els, card, ask, index)),
      Box({
        key: 'foot',
        flexDirection: 'row',
        justifyContent: 'flex-end',
        columnGap: 1,
        children: [
          ...(error === undefined ? [] : [Text({ color: 'red', children: [error] })]),
          s.sent.has(key)
            ? Text({ dimColor: true, children: ['보냈어요 — 세션이 받으면 사라져요'] })
            : Button({
                key: `ask:${key}:send`,
                label: '답 보내기',
                variant: 'primary',
                onPress: () => void sendAnswer($, s, card, ask),
              }),
        ],
      }),
    ],
  })
}

export function askBlocks($: EngineInterface, s: AskState, els: Elements, cards: readonly AskCard[]): Child[] {
  return cards.flatMap(card => (card.asks ?? []).map(ask => askBlock($, s, els, card, ask)))
}
