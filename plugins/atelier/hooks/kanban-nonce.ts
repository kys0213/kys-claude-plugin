import type { EngineInterface } from 'claude-code'

import { cli, queueRequest } from './kanban-cli'
import { sessionKeysOf } from './kanban-record'

export const NONCE_PREFIX = 'kanban:nonce:'

export type PaneKind = 'answer' | 'permit' | 'decide' | 'prompt'

/** Where an answer came from, as `inbox take` and `inbox wait-permit` named it; passed on untouched. */
export type Via = string | null

/** A nonce the pane left for a session, as the CLI reads it and names it used. */
type LeftNonce = { kind: string; nonce: string }

function nonceKey(sessionId: string, kind: string, nonce: string): string {
  return `${NONCE_PREFIX}${sessionId}:${kind}:${nonce}`
}

function isLeftNonce(value: unknown): value is LeftNonce {
  const v = value as Record<string, unknown> | null

  return typeof v?.kind === 'string' && typeof v.nonce === 'string'
}

async function dropOldNonces($: EngineInterface): Promise<void> {
  const out = await cli($, ['prune-nonces'], JSON.stringify({ keys: await $.store.keys() }))
  const drop = Array.isArray(out?.drop) ? out.drop.filter((k): k is string => typeof k === 'string') : []

  for (const key of drop) {
    await $.store.delete(key)
  }
}

async function leaveNonce($: EngineInterface, sessionId: string, kind: PaneKind): Promise<string | null> {
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
 * Queues a pane request with a nonce of its own for the taking session; null once queued, else why not.
 * Without a nonce the request still goes, and comes in through the terminal.
 */
export async function queuePaneRequest(
  $: EngineInterface,
  sessionId: string,
  kind: PaneKind,
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

    for (const key of sessionKeysOf(await $.store.keys(), NONCE_PREFIX, self)) {
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

export function viaOf(value: unknown): Via {
  return typeof value === 'string' ? value : null
}
