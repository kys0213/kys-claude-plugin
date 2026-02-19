#!/usr/bin/env node
/**
 * develop-phase-gate.cjs
 *
 * PreToolUse hook: state.json의 gate 조건이 미충족이면 Write/Edit/Bash를 차단합니다.
 *
 * Matcher별 동작:
 *   Write|Edit → phase 진입 gate 검증
 *   Bash       → 종료 명령(git push, gh pr create, git commit) gate 검증
 *
 * state.json이 없으면 워크플로우 비활성 상태로 간주하여 통과(exit 0).
 */

const fs = require('fs');
const path = require('path');

const STATE_PATH = path.join(process.cwd(), '.develop-workflow', 'state.json');

// ── 1. 워크플로우 비활성이면 통과 ──────────────────────────────
if (!fs.existsSync(STATE_PATH)) {
  process.exit(0);
}

let state;
try {
  state = JSON.parse(fs.readFileSync(STATE_PATH, 'utf8'));
} catch {
  // 파싱 실패 시 차단하지 않음 (state.json이 깨진 경우 워크플로우 외부에서 수정 가능)
  process.exit(0);
}

const { phase, gates } = state;
if (!phase || !gates) {
  process.exit(0);
}

// ── 2. stdin에서 tool_input 읽기 ──────────────────────────────
let input = '';
try {
  input = fs.readFileSync('/dev/stdin', 'utf8');
} catch {
  // stdin 읽기 실패 시 통과
  process.exit(0);
}

let toolInput;
try {
  toolInput = JSON.parse(input);
} catch {
  process.exit(0);
}

const toolName = toolInput.tool_name || '';
const command = (toolInput.tool_input && toolInput.tool_input.command) || '';

// ── 3. Phase 진입 gate 규칙 ──────────────────────────────────
const ENTRY_RULES = {
  IMPLEMENT: ['review_clean_pass'],
  MERGE: ['review_clean_pass', 'architect_verified'],
};

// ── 4. 종료 명령 gate 규칙 ──────────────────────────────────
const EXIT_COMMANDS = /\b(git\s+push|gh\s+pr\s+create|git\s+commit)\b/;

const EXIT_RULES = {
  MERGE: ['re_review_clean'],
};

// ── 5. Write|Edit 차단 로직 ──────────────────────────────────
if (toolName === 'Write' || toolName === 'Edit') {
  const required = ENTRY_RULES[phase] || [];
  const blocked = required.filter(g => !gates[g]);

  if (blocked.length > 0) {
    process.stderr.write(
      `\n[PHASE GATE] ${phase} phase에서 파일 수정이 차단되었습니다.\n` +
      `미충족 gate: ${blocked.join(', ')}\n` +
      `state.json의 해당 gate를 true로 업데이트한 후 다시 시도하세요.\n\n`
    );
    process.exit(1);
  }
}

// ── 6. Bash 종료 명령 차단 로직 ──────────────────────────────
if (toolName === 'Bash' && EXIT_COMMANDS.test(command)) {
  const required = EXIT_RULES[phase] || [];
  const entryRequired = ENTRY_RULES[phase] || [];
  const allRequired = [...new Set([...entryRequired, ...required])];
  const blocked = allRequired.filter(g => !gates[g]);

  if (blocked.length > 0) {
    process.stderr.write(
      `\n[PHASE GATE] ${phase} phase에서 종료 명령이 차단되었습니다.\n` +
      `차단된 명령: ${command.substring(0, 80)}\n` +
      `미충족 gate: ${blocked.join(', ')}\n` +
      `모든 gate를 통과한 후 다시 시도하세요.\n\n`
    );
    process.exit(1);
  }
}

// ── 7. 통과 ──────────────────────────────────────────────────
process.exit(0);
