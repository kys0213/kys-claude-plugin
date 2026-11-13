import type { EngineInterface } from 'claude-code'

const WAITS = 'waits for the user to answer a question or make a decision before going on'
const DONE = 'needs no reply from the user'
/** The question sits at the end of an answer; what comes before it is cut first. */
const MAX_CHECKED_CHARS = 4_000

/** Whether a turn's last answer waits for the user, as the small fast model judges it; false when the model fails. */
export async function waitsForReply($: EngineInterface, answer: string): Promise<boolean> {
  try {
    return (await $.model.classify(answer.slice(-MAX_CHECKED_CHARS), [WAITS, DONE])) === WAITS
  } catch {
    return false
  }
}
