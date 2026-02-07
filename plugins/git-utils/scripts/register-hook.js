#!/usr/bin/env node
/**
 * Claude Code Hook Registration Helper
 *
 * .claude/settings.json에 hook을 안전하게 등록/삭제합니다.
 * 기존 hook을 덮어쓰지 않고 병합하며, 중복을 방지합니다.
 *
 * Usage:
 *   node register-hook.js register <hookType> <matcher> <command> [options]
 *   node register-hook.js unregister <hookType> <command>
 *
 * Examples:
 *   node register-hook.js register Stop "*" "bash ./.claude/hooks/auto-commit-hook.sh" --timeout=10
 *   node register-hook.js unregister Stop "bash ./.claude/hooks/auto-commit-hook.sh"
 */

const fs = require('fs');
const path = require('path');

const SETTINGS_FILENAME = 'settings.json';
const CLAUDE_DIR = '.claude';

/**
 * settings.json 파일 경로 반환
 * @param {string} projectDir - 프로젝트 디렉토리 (기본: cwd)
 * @returns {string}
 */
function getSettingsPath(projectDir = process.cwd()) {
  return path.join(projectDir, CLAUDE_DIR, SETTINGS_FILENAME);
}

/**
 * settings.json 읽기
 * @param {string} projectDir
 * @returns {object}
 */
function readSettings(projectDir = process.cwd()) {
  const settingsPath = getSettingsPath(projectDir);

  if (!fs.existsSync(settingsPath)) {
    return { hooks: {} };
  }

  try {
    const content = fs.readFileSync(settingsPath, 'utf8');
    const settings = JSON.parse(content);
    settings.hooks = settings.hooks || {};
    return settings;
  } catch (err) {
    console.error(`Failed to read settings: ${err.message}`);
    return { hooks: {} };
  }
}

/**
 * settings.json 저장
 * @param {object} settings
 * @param {string} projectDir
 */
function writeSettings(settings, projectDir = process.cwd()) {
  const settingsPath = getSettingsPath(projectDir);
  const claudeDir = path.join(projectDir, CLAUDE_DIR);

  // .claude 디렉토리 생성
  if (!fs.existsSync(claudeDir)) {
    fs.mkdirSync(claudeDir, { recursive: true });
  }

  fs.writeFileSync(settingsPath, JSON.stringify(settings, null, 2) + '\n');
}

/**
 * Hook 등록
 * @param {object} options
 * @param {string} options.hookType - Hook 타입 (PostToolUse, Stop, etc.)
 * @param {string} options.matcher - 매처 패턴 ("Write|Edit", "*", etc.)
 * @param {string} options.command - 실행할 명령어 경로
 * @param {number} [options.timeout] - 타임아웃 (seconds)
 * @param {string} [options.projectDir] - 프로젝트 디렉토리
 * @returns {object} { success: boolean, message: string }
 */
function registerHook({ hookType, matcher, command, timeout, projectDir = process.cwd() }) {
  if (!hookType || !matcher || !command) {
    return { success: false, message: 'hookType, matcher, command are required' };
  }

  const settings = readSettings(projectDir);

  // hook 타입 배열 초기화
  settings.hooks[hookType] = settings.hooks[hookType] || [];

  // 새 hook 생성
  const hookEntry = {
    type: 'command',
    command,
  };
  if (timeout) {
    hookEntry.timeout = timeout;
  }

  const newHook = {
    matcher,
    hooks: [hookEntry],
  };

  // 동일한 command가 이미 있는지 확인
  const existingIndex = settings.hooks[hookType].findIndex(
    h => h.hooks?.some(hook => hook.command === command)
  );

  if (existingIndex >= 0) {
    // 기존 hook 업데이트
    settings.hooks[hookType][existingIndex] = newHook;
  } else {
    // 새 hook 추가
    settings.hooks[hookType].push(newHook);
  }

  writeSettings(settings, projectDir);

  return {
    success: true,
    message: existingIndex >= 0
      ? `Updated existing hook: ${command}`
      : `Registered new hook: ${command}`
  };
}

/**
 * Hook 삭제
 * @param {object} options
 * @param {string} options.hookType - Hook 타입
 * @param {string} options.command - 삭제할 명령어 경로
 * @param {string} [options.projectDir] - 프로젝트 디렉토리
 * @returns {object} { success: boolean, message: string }
 */
function unregisterHook({ hookType, command, projectDir = process.cwd() }) {
  if (!hookType || !command) {
    return { success: false, message: 'hookType and command are required' };
  }

  const settings = readSettings(projectDir);

  if (!settings.hooks[hookType]) {
    return { success: false, message: `No hooks found for type: ${hookType}` };
  }

  const initialLength = settings.hooks[hookType].length;
  settings.hooks[hookType] = settings.hooks[hookType].filter(
    h => !h.hooks?.some(hook => hook.command === command)
  );

  if (settings.hooks[hookType].length === initialLength) {
    return { success: false, message: `Hook not found: ${command}` };
  }

  // 빈 배열이면 삭제
  if (settings.hooks[hookType].length === 0) {
    delete settings.hooks[hookType];
  }

  // hooks가 비어있으면 삭제
  if (Object.keys(settings.hooks).length === 0) {
    delete settings.hooks;
  }

  writeSettings(settings, projectDir);

  return { success: true, message: `Unregistered hook: ${command}` };
}

/**
 * 등록된 Hook 목록 조회
 * @param {object} options
 * @param {string} [options.hookType] - 특정 타입만 조회 (선택)
 * @param {string} [options.projectDir] - 프로젝트 디렉토리
 * @returns {object}
 */
function listHooks({ hookType, projectDir = process.cwd() } = {}) {
  const settings = readSettings(projectDir);

  if (hookType) {
    return settings.hooks[hookType] || [];
  }

  return settings.hooks;
}

// CLI 실행
if (require.main === module) {
  const args = process.argv.slice(2);
  const action = args[0];

  // --project-dir, --timeout 등 옵션 파싱 헬퍼
  function parseOptions(rest) {
    const opts = {};
    for (const arg of rest) {
      if (arg.startsWith('--timeout=')) {
        opts.timeout = parseInt(arg.split('=')[1], 10);
      } else if (arg.startsWith('--project-dir=')) {
        opts.projectDir = arg.split('=').slice(1).join('=');
      }
    }
    return opts;
  }

  if (action === 'register') {
    const [, hookType, matcher, command, ...rest] = args;
    const opts = parseOptions(rest);

    const result = registerHook({ hookType, matcher, command, timeout: opts.timeout, projectDir: opts.projectDir });
    console.log(result.message);
    process.exit(result.success ? 0 : 1);

  } else if (action === 'unregister') {
    const [, hookType, command, ...rest] = args;
    const opts = parseOptions(rest);

    const result = unregisterHook({ hookType, command, projectDir: opts.projectDir });
    console.log(result.message);
    process.exit(result.success ? 0 : 1);

  } else if (action === 'list') {
    const [, hookType, ...rest] = args;
    const opts = parseOptions(rest);

    const hooks = listHooks({ hookType, projectDir: opts.projectDir });
    console.log(JSON.stringify(hooks, null, 2));

  } else {
    console.log(`
Usage:
  node register-hook.js register <hookType> <matcher> <command> [--timeout=ms] [--project-dir=path]
  node register-hook.js unregister <hookType> <command> [--project-dir=path]
  node register-hook.js list [hookType] [--project-dir=path]

Options:
  --timeout=ms        Hook timeout in seconds (default: none)
  --project-dir=path  Target directory for settings.json (default: cwd)
                      Use --project-dir=$HOME for user-level (~/.claude/settings.json)

Examples:
  node register-hook.js register Stop "*" "bash ./.claude/hooks/auto-commit-hook.sh" --timeout=10
  node register-hook.js register Stop "*" "bash ~/.claude/hooks/auto-commit-hook.sh" --timeout=10 --project-dir=$HOME
  node register-hook.js unregister Stop "bash ./.claude/hooks/auto-commit-hook.sh"
  node register-hook.js list Stop
    `.trim());
    process.exit(1);
  }
}

// 모듈로 사용할 수 있도록 export
module.exports = {
  registerHook,
  unregisterHook,
  listHooks,
  readSettings,
  writeSettings,
  getSettingsPath,
};
