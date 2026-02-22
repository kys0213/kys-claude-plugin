#!/usr/bin/env node
/**
 * signal-done.cjs — SubagentStop hook (barrier producer)
 *
 * When a background Task completes, writes its agent_id to the active barrier's FIFO.
 * Also saves last_assistant_message (truncated) to results/ for the consumer to read.
 *
 * If no active barrier exists, exits silently (exit 0).
 * Never blocks the main agent — all errors are swallowed with exit 0.
 */

const fs = require('fs');
const path = require('path');

const BARRIERS_ROOT = '/tmp/claude-barriers';
const MAX_RESULT_LENGTH = 500;

// ── 1. Read stdin (SubagentStop hook JSON) ──────────────────────
let input = '';
try {
  input = fs.readFileSync('/dev/stdin', 'utf8');
} catch {
  process.exit(0);
}

let hookData;
try {
  hookData = JSON.parse(input);
} catch {
  process.exit(0);
}

const agentId = hookData.agent_id;
if (!agentId) {
  process.exit(0);
}

// ── 2. Find active barrier ──────────────────────────────────────
let activeBarrier = null;

try {
  if (!fs.existsSync(BARRIERS_ROOT)) {
    process.exit(0);
  }

  const dirs = fs.readdirSync(BARRIERS_ROOT, { withFileTypes: true })
    .filter(d => d.isDirectory());

  for (const dir of dirs) {
    const metaPath = path.join(BARRIERS_ROOT, dir.name, 'meta.json');
    try {
      const meta = JSON.parse(fs.readFileSync(metaPath, 'utf8'));
      // Check if the barrier process is still alive
      try {
        process.kill(meta.pid, 0);
        activeBarrier = { dir: dir.name, meta };
        break;
      } catch {
        // Process not alive — stale barrier, skip
      }
    } catch {
      // No meta or parse error — skip
    }
  }
} catch {
  process.exit(0);
}

if (!activeBarrier) {
  process.exit(0);
}

// ── 3. Save result (truncated last_assistant_message) ───────────
try {
  const resultsDir = path.join(BARRIERS_ROOT, activeBarrier.dir, 'results');
  if (!fs.existsSync(resultsDir)) {
    fs.mkdirSync(resultsDir, { recursive: true });
  }

  const message = hookData.last_assistant_message || '(no message)';
  const truncated = message.length > MAX_RESULT_LENGTH
    ? message.substring(0, MAX_RESULT_LENGTH) + '...'
    : message;

  fs.writeFileSync(
    path.join(resultsDir, `${agentId}.txt`),
    truncated,
    'utf8'
  );
} catch {
  // Result save failure is non-fatal
}

// ── 4. Write agent_id to FIFO (unblock consumer) ───────────────
try {
  const fifoPath = activeBarrier.meta.fifo;
  // writeFileSync on a FIFO will block until the reader consumes it.
  // This is fine — this hook runs in its own process.
  fs.writeFileSync(fifoPath, agentId + '\n', 'utf8');
} catch {
  // FIFO write failure — consumer may have already exited
}

process.exit(0);
