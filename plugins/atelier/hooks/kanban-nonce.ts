import type { EngineInterface } from 'claude-code'

import { queueRequest } from './kanban-cli'

const NONCE_PREFIX = 'kanban:nonce:'
/** Bounds the nonces of answers no session took, in the store every session shares. */
const MAX_NONCES = 32

export type AnswerKind = 'answer' | 'permit' | 'decide'

/** `pane` only for an answer carrying the nonce the pane left for this session and kind. */
export type Via = 'pane' | 'terminal'

function nonceKey(sessionId: string, kind: AnswerKind, nonce: string): string {
  return `${NONCE_PREFIX}${sessionId}:${kind}:${nonce}`
}

export function nonceKeysOf(keys: readonly string[], sessionId: string): string[] {
  return keys.filter(k => k.startsWith(`${NONCE_PREFIX}${sessionId}:`))
}

async function dropOldNonces($: EngineInterface): Promise<void> {
  const keys = (await $.store.keys()).filter(k => k.startsWith(NONCE_PREFIX))

  for (const key of keys.slice(0, Math.max(0, keys.length - MAX_NONCES))) {
    await $.store.delete(key)
  }
}

async function leaveNonce($: EngineInterface, sessionId: string, kind: AnswerKind): Promise<string | null> {
  try {
    const nonce = crypto.randomUUID()

    await $.store.set(nonceKey(sessionId, kind, nonce), { session: sessionId, kind, at: await $.clock.now() })
    await dropOldNonces($)

    return nonce
  } catch {
    return null
  }
}

/**
 * Queues a pane answer with a nonce of its own for the answering session; null once queued, else why not.
 * Without a nonce the answer still goes, and comes in through the terminal.
 */
export async function queuePaneAnswer(
  $: EngineInterface,
  sessionId: string,
  kind: AnswerKind,
  args: string[],
): Promise<string | null> {
  const nonce = await leaveNonce($, sessionId, kind)
  const refused = await queueRequest($, nonce === null ? args : [...args, '--nonce', nonce])

  if (refused !== null && nonce !== null) {
    await $.store.delete(nonceKey(sessionId, kind, nonce))
  }

  return refused
}

/** Uses up the nonce an answer this session took carries. */
export async function viaOf($: EngineInterface, kind: AnswerKind, nonce: unknown): Promise<Via> {
  if (typeof nonce !== 'string') {
    return 'terminal'
  }

  try {
    const key = nonceKey(await $.session.id(), kind, nonce)

    if ((await $.store.get(key)) === undefined) {
      return 'terminal'
    }

    await $.store.delete(key)

    return 'pane'
  } catch {
    return 'terminal'
  }
}
