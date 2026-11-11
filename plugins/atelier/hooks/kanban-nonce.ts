import type { EngineInterface } from 'claude-code'

import { queueRequest } from './kanban-cli'

const NONCE_PREFIX = 'kanban:nonce:'
/** Bounds the nonces of answers no session took, in the store every session shares. */
const MAX_NONCES = 32

export type AnswerKind = 'answer' | 'permit' | 'decide'

/** Where an answer came from, as `inbox take` and `inbox wait-permit` say. */
export type Via = 'pane' | 'terminal' | 'agent'

/** A nonce the pane left for a session, as the CLI reads it and names it used. */
type LeftNonce = { kind: string; nonce: string }

function nonceKey(sessionId: string, kind: string, nonce: string): string {
  return `${NONCE_PREFIX}${sessionId}:${kind}:${nonce}`
}

export function nonceKeysOf(keys: readonly string[], sessionId: string): string[] {
  return keys.filter(k => k.startsWith(`${NONCE_PREFIX}${sessionId}:`))
}

function isLeftNonce(value: unknown): value is LeftNonce {
  const v = value as Record<string, unknown> | null

  return typeof v?.kind === 'string' && typeof v.nonce === 'string'
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

    await $.store.set(nonceKey(sessionId, kind, nonce), { session: sessionId, kind, nonce, at: await $.clock.now() })
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

/** The nonces the pane left for session `self`, for the CLI to judge answers against. */
export async function leftNonces($: EngineInterface, self: string): Promise<LeftNonce[]> {
  try {
    const nonces: LeftNonce[] = []

    for (const key of nonceKeysOf(await $.store.keys(), self)) {
      const value = await $.store.get(key)

      if (isLeftNonce(value)) {
        nonces.push({ kind: value.kind, nonce: value.nonce })
      }
    }

    return nonces
  } catch {
    return []
  }
}

/** Deletes the nonces the CLI says the answers it took used up. */
export async function dropUsedNonces($: EngineInterface, self: string, used: unknown): Promise<void> {
  if (!Array.isArray(used)) {
    return
  }

  try {
    for (const left of used.filter(isLeftNonce)) {
      await $.store.delete(nonceKey(self, left.kind, left.nonce))
    }
  } catch {
    /** Fail-open: a nonce left behind goes with the cap or with the session's record. */
  }
}

/** The CLI's `via`; one it does not name proves nothing. */
export function viaOf(value: unknown): Via {
  return value === 'pane' || value === 'agent' ? value : 'terminal'
}
