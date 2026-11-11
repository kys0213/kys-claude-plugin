import type { EngineInterface, PromptSubmitInput, PromptSubmitResult } from 'claude-code'

import { CLI_MISSING, cli } from './kanban-cli'
import { KEY_PREFIX, type RecordState, isRecord, record } from './kanban-record'

export const DEFAULT_REPORT_AT = '18:00'

type Pending = { id: string; since: number }

/** What `delegate wake` offered as context for the person's next prompt. */
type Offer = { context: string; ids: string[]; attachTo: string[]; report: boolean }

type DelegateMemory = {
  /** What the last wake handed back, for the next one. */
  pending: Pending[]
  delivered: string[]
  offer: Offer | null
  /** A wake or the prompt it submitted is in flight. */
  waking: boolean
  /** `kanban_report_at` from the plugin's options. */
  reportAt: string
  /** The report time error the person was last told, so each is shown once. */
  toldReportAt: string | null
}

export type DelegateState = RecordState & { delegate: DelegateMemory }

export function newDelegateMemory(reportAt: unknown): DelegateMemory {
  return {
    pending: [],
    delivered: [],
    offer: null,
    waking: false,
    reportAt: typeof reportAt === 'string' ? reportAt : DEFAULT_REPORT_AT,
    toldReportAt: null,
  }
}

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((x): x is string => typeof x === 'string') : []
}

function pendingOf(value: unknown): Pending[] {
  return Array.isArray(value)
    ? value.filter((p): p is Pending => typeof p?.id === 'string' && typeof p?.since === 'number')
    : []
}

/** `/kanban <args>` as a person or anything else ran it: the CLI's answer, and the record's `delegate_since` when it named one. */
export async function runDelegateCommand($: EngineInterface, s: DelegateState, args: string, origin: string): Promise<string> {
  const out = await cli($, [
    'command',
    '--session',
    await $.session.id(),
    '--now',
    String(await $.clock.now()),
    '--origin',
    origin,
    '--args',
    args,
  ])

  if (out === null) {
    return CLI_MISSING
  }

  if (typeof out.error === 'string') {
    return out.error
  }

  const since = out.delegate_since

  if (since === null || typeof since === 'number') {
    await record($, s, { name: null, update: () => ({ delegate_since: since }) })
    s.delegate.pending = []
    s.delegate.delivered = []
    s.delegate.offer = null
  }

  return typeof out.text === 'string' ? out.text : CLI_MISSING
}

async function delivered($: EngineInterface, s: DelegateState, ids: readonly string[], report: boolean): Promise<void> {
  s.delegate.delivered = [...s.delegate.delivered, ...ids.filter(id => !s.delegate.delivered.includes(id))]

  if (report) {
    const now = String(await $.clock.now())

    await cli($, ['delegate', 'report', '--now', now])
    await cli($, ['ledger', 'prune', '--now', now])
  }
}

async function isTurnedOn($: EngineInterface): Promise<boolean> {
  const stored = await $.store.get(KEY_PREFIX + (await $.session.id()))

  return isRecord(stored) && typeof stored.delegate_since === 'number'
}

/**
 * Asks `delegate wake` what to say now, while this session is turned on: submits its prompt, or keeps
 * its context for the person's next prompt. What went out is delivered only once the engine took it.
 */
export async function wakeDelegate($: EngineInterface, s: DelegateState, sessions: () => Promise<unknown[]>): Promise<void> {
  const memory = s.delegate

  if (memory.waking) {
    return
  }

  memory.waking = true

  try {
    if (!(await isTurnedOn($))) {
      memory.pending = []
      memory.delivered = []
      memory.offer = null

      return
    }

    const now = await $.clock.now()
    const draft = await $.prompt.read()
    const out = await cli(
      $,
      [
        'delegate',
        'wake',
        '--session',
        await $.session.id(),
        '--now',
        String(now),
        '--report-at',
        memory.reportAt,
        '--utc-offset',
        String(-new Date(now).getTimezoneOffset()),
      ],
      JSON.stringify({
        sessions: await sessions(),
        state: { pending: memory.pending, delivered: memory.delivered, draft_len: draft.text.length },
      }),
    )
    const state = out?.state as Record<string, unknown> | undefined

    if (out === null || typeof out.error === 'string' || typeof state !== 'object' || state === null) {
      return
    }

    const reportAtError = typeof out.report_at_error === 'string' ? out.report_at_error : null

    if (reportAtError !== null && reportAtError !== memory.toldReportAt) {
      $.ui.toast(reportAtError)
    }

    memory.toldReportAt = reportAtError
    memory.pending = pendingOf(state.pending)
    memory.delivered = strings(state.delivered)
    const ids = strings(out.deliver_ids)
    const report = out.report_due === true
    memory.offer =
      typeof out.context === 'string' ? { context: out.context, ids, attachTo: strings(out.attach_to), report } : null

    if (typeof out.submit === 'string') {
      const r = await $.prompt.submit({ text: out.submit })

      if (r.drop === undefined) {
        await delivered($, s, ids, report)
      }
    }
  } catch {
    /** what was not delivered stays pending for the next wake */
  } finally {
    memory.waking = false
  }
}

/** A person's prompt carries the context the last wake offered; it counts as delivered once the prompt entered. */
export async function submitWithOffer(
  $: EngineInterface,
  s: DelegateState,
  e: PromptSubmitInput,
  next: (e: PromptSubmitInput) => Promise<PromptSubmitResult>,
): Promise<PromptSubmitResult> {
  const offer = s.delegate.offer

  if (offer === null || e.command !== undefined || e.isMeta === true || !offer.attachTo.includes(e.origin?.kind ?? '')) {
    return next(e)
  }

  s.delegate.offer = null
  const r = await next({ ...e, context: [...(e.context ?? []), offer.context] })

  if (r.drop === undefined) {
    await delivered($, s, offer.ids, offer.report)
  }

  return r
}
