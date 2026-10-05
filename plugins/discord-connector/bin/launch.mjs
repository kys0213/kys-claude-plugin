#!/usr/bin/env node
import { spawn, spawnSync } from 'node:child_process'
import {
  appendFileSync,
  closeSync,
  existsSync,
  linkSync,
  mkdirSync,
  openSync,
  readFileSync,
  realpathSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
  writeSync,
} from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const SELF = fileURLToPath(import.meta.url)
const PLUGIN_ROOT = resolve(dirname(SELF), '..')
const BUSY_NOTICE = '이전 작업이 끝난 뒤 다시 멘션해 주세요'
const STALE_UNPARSEABLE_LOCK_MS = 60_000
const ERROR_TAIL_LINES = 20

const REQUIRED_STRING_FIELDS = ['trigger', 'guild_id', 'channel_id', 'message_id', 'content', 'bot_user_id', 'cwd']

export function neutralizeBotMention(text, botId) {
  return text.split(`<@${botId}>`).join(`@${botId}`).split(`<@!${botId}>`).join(`@${botId}`)
}

export function stripBotMention(text, botId) {
  return text.split(`<@${botId}>`).join('').split(`<@!${botId}>`).join('').trim()
}

export function validateEvent(raw) {
  let event
  try {
    event = JSON.parse(raw)
  } catch (err) {
    throw new Error(`stdin 이 JSON 이 아니에요: ${err.message}`)
  }
  if (typeof event !== 'object' || event === null) {
    throw new Error('stdin JSON 이 객체가 아니에요')
  }
  const missing = REQUIRED_STRING_FIELDS.filter(k => typeof event[k] !== 'string' || (k !== 'content' && event[k] === ''))
  if (typeof event.is_thread !== 'boolean') missing.push('is_thread')
  if (typeof event.author !== 'object' || event.author === null || typeof event.author.id !== 'string' || event.author.id === '') {
    missing.push('author.id')
  }
  if (missing.length > 0) {
    throw new Error(`stdin 필드가 없거나 형식이 달라요: ${missing.join(', ')}`)
  }
  return event
}

export function buildPrompt(event, threadId) {
  const link = `https://discord.com/channels/${event.guild_id}/${threadId}`
  return `스레드: ${link}\n당신의 최종 답은 이 Discord 스레드에 그대로 게시돼요. 스레드에 직접 글을 올릴 필요는 없어요.\n\n${stripBotMention(event.content, event.bot_user_id)}`
}

export function buildClaudeArgs(prompt, sessionId, pluginRoot = PLUGIN_ROOT) {
  const args = [
    '-p',
    prompt,
    '--permission-mode',
    'auto',
    '--permission-prompt-tool',
    'stdio',
    '--output-format',
    'json',
    '--model',
    'sonnet',
  ]
  if (sessionId) args.push('--resume', sessionId)
  args.push('--plugin-dir', pluginRoot)
  return args
}

export function parseClaudeResult(stdout) {
  let parsed
  try {
    parsed = JSON.parse(stdout)
  } catch {
    return { error: 'Claude 결과를 해석할 수 없어요 (JSON 아님)', detail: stdout }
  }
  if (typeof parsed !== 'object' || parsed === null) {
    return { error: 'Claude 결과를 해석할 수 없어요 (객체 아님)', detail: stdout }
  }
  if (parsed.is_error === true) {
    return { error: 'Claude 가 오류 결과를 돌려줬어요', detail: typeof parsed.result === 'string' ? parsed.result : stdout }
  }
  if (typeof parsed.result !== 'string' || parsed.result.trim() === '') {
    return { error: 'Claude 결과에 result 문자열이 없어요', detail: stdout }
  }
  if (typeof parsed.session_id !== 'string' || parsed.session_id === '') {
    return { error: 'Claude 결과에 session_id 가 없어요', detail: stdout }
  }
  return { result: parsed.result, sessionId: parsed.session_id }
}

export function formatErrorSummary(kind, detail) {
  const tail = detail.split('\n').slice(-ERROR_TAIL_LINES).join('\n').split('```').join("'''")
  return `**${kind}**\n\`\`\`\n${tail}\n\`\`\``
}

function stateRoot() {
  return join(homedir(), '.areum', 'discord-connector')
}

function ensureDir(dir) {
  mkdirSync(dir, { recursive: true })
  return dir
}

function log(message) {
  try {
    appendFileSync(join(ensureDir(join(stateRoot(), 'logs')), 'launcher.log'), `${new Date().toISOString()} [${process.pid}] ${message}\n`)
  } catch {
    // 로그 실패가 실행을 막으면 안 된다
  }
}

function isAlive(pid) {
  try {
    process.kill(pid, 0)
    return true
  } catch (err) {
    return err.code === 'EPERM'
  }
}

function tryCreateLock(path) {
  try {
    const fd = openSync(path, 'wx')
    writeSync(fd, String(process.pid))
    closeSync(fd)
    return true
  } catch (err) {
    if (err.code === 'EEXIST') return false
    throw err
  }
}

const LOCK_ATTEMPTS = 5

function readLockHolder(path) {
  try {
    const content = readFileSync(path, 'utf8')
    return { content, mtimeMs: statSync(path).mtimeMs }
  } catch (err) {
    if (err.code === 'ENOENT') return undefined
    throw err
  }
}

function isStale({ content, mtimeMs }) {
  const pid = Number.parseInt(content, 10)
  if (Number.isInteger(pid) && pid > 0) return !isAlive(pid)
  return Date.now() - mtimeMs > STALE_UNPARSEABLE_LOCK_MS
}

// 잠금 파일이 없어지는 순간은 정상 경합이다 — 오류가 아니라 획득 재시도로 다룬다.
// 회수는 rename 으로 잠금을 먼저 떼어낸 뒤 판정한 내용과 같은지 확인해, 그 사이 다른 프로세스가 새로 만든 잠금을 지우지 않는다.
function reclaimStale(path, observed) {
  const moved = `${path}.stale.${process.pid}`
  try {
    renameSync(path, moved)
  } catch (err) {
    if (err.code === 'ENOENT') return
    throw err
  }
  const taken = readLockHolder(moved)
  if (taken && taken.content === observed.content) {
    unlinkSync(moved)
    log(`죽은 프로세스의 잠금을 회수해요: ${path}`)
    return
  }
  if (taken) {
    try {
      linkSync(moved, path)
    } catch (err) {
      if (err.code !== 'EEXIST') throw err
    }
    unlinkSync(moved)
  }
}

export function acquireLock(threadId) {
  const path = join(ensureDir(join(stateRoot(), 'locks')), `${threadId}.lock`)
  for (let attempt = 0; attempt < LOCK_ATTEMPTS; attempt++) {
    if (tryCreateLock(path)) return path
    const holder = readLockHolder(path)
    if (!holder) continue
    if (!isStale(holder)) return undefined
    reclaimStale(path, holder)
  }
  return undefined
}

function statePath(threadId) {
  return join(ensureDir(join(stateRoot(), 'threads')), `${threadId}.json`)
}

function loadThreadState(threadId) {
  const path = statePath(threadId)
  if (!existsSync(path)) return undefined
  const state = JSON.parse(readFileSync(path, 'utf8'))
  if (typeof state.session_id !== 'string' || typeof state.cwd !== 'string') {
    throw new Error(`스레드 상태 형식이 달라요: ${path}`)
  }
  return state
}

function saveThreadState(threadId, state) {
  const path = statePath(threadId)
  const tmp = `${path}.${process.pid}.tmp`
  writeFileSync(tmp, JSON.stringify(state))
  renameSync(tmp, path)
}

function discord(args, input) {
  const r = spawnSync('discord', ['--json', ...args], { input, encoding: 'utf8' })
  if (r.error) throw new Error(`discord 실행 실패: ${r.error.message}`)
  let envelope
  try {
    envelope = JSON.parse(r.stdout)
  } catch {
    throw new Error(`discord ${args[0]} 응답 형식 불일치 (exit ${r.status}): ${(r.stdout || r.stderr).slice(-300)}`)
  }
  if (r.status !== 0 || envelope.ok !== true) {
    throw new Error(`discord ${args[0]} 실패 (exit ${r.status}): ${JSON.stringify(envelope.error ?? envelope).slice(0, 300)}`)
  }
  return envelope.data
}

function post(threadId, botId, text) {
  discord(['send', threadId, '-', '--split'], neutralizeBotMention(text, botId))
}

function postBestEffort(threadId, botId, text) {
  try {
    post(threadId, botId, text)
  } catch (err) {
    log(`게시 실패: ${err.message}`)
  }
}

function threadNameFrom(event) {
  const text = neutralizeBotMention(stripBotMention(event.content, event.bot_user_id), event.bot_user_id).replace(/\s+/g, ' ').trim()
  return text.slice(0, 90) || 'claude'
}

function createThread(event) {
  const data = discord([
    'thread',
    'create',
    event.channel_id,
    '--from-message',
    event.message_id,
    '--name',
    threadNameFrom(event),
  ])
  if (typeof data?.thread_id !== 'string' || data.thread_id === '') {
    throw new Error(`thread create 응답에 thread_id 가 없어요: ${JSON.stringify(data)}`)
  }
  return data.thread_id
}

function runClaude(args, cwd, env) {
  return new Promise(resolvePromise => {
    const child = spawn('claude', args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] })
    let stdout = ''
    let stderr = ''
    child.stdout.on('data', d => (stdout += d))
    child.stderr.on('data', d => (stderr += d))
    child.on('error', err => resolvePromise({ spawnError: err, stdout, stderr }))
    child.on('close', (code, signal) => resolvePromise({ code, signal, stdout, stderr }))
  })
}

async function execute(event, threadId) {
  const botId = event.bot_user_id
  const prior = loadThreadState(threadId)

  if (prior && !(existsSync(prior.cwd) && statSync(prior.cwd).isDirectory())) {
    postError(threadId, botId, '기록된 작업 디렉토리가 없어졌어요', prior.cwd)
    return
  }

  const cwd = prior ? prior.cwd : event.cwd
  const args = buildClaudeArgs(buildPrompt(event, threadId), prior?.session_id)
  const env = {
    ...process.env,
    DISCORD_CONNECTOR_THREAD_ID: threadId,
    DISCORD_CONNECTOR_REQUESTER_ID: event.author.id,
    DISCORD_CONNECTOR_BOT_ID: botId,
  }

  log(`claude 실행: cwd=${cwd} resume=${prior?.session_id ?? '-'} thread=${threadId}`)
  const run = await runClaude(args, cwd, env)
  log(`claude 종료: code=${run.code} signal=${run.signal} stdout=${run.stdout}${run.stderr ? ` stderr=${run.stderr}` : ''}`)

  if (run.spawnError) {
    postError(threadId, botId, 'Claude 를 실행하지 못했어요', run.spawnError.message)
    return
  }
  if (run.code !== 0) {
    postError(threadId, botId, `Claude 가 비정상 종료했어요 (exit ${run.code ?? run.signal})`, run.stderr || run.stdout)
    return
  }
  const parsed = parseClaudeResult(run.stdout)
  if (parsed.error) {
    postError(threadId, botId, parsed.error, parsed.detail)
    return
  }

  saveThreadState(threadId, { session_id: parsed.sessionId, cwd })
  post(threadId, botId, parsed.result)
}

function postError(threadId, botId, kind, detail) {
  postBestEffort(threadId, botId, formatErrorSummary(kind, detail))
}

async function handle(event) {
  let threadId
  try {
    threadId = event.is_thread ? event.channel_id : createThread(event)
  } catch (err) {
    log(`스레드를 만들지 못했어요: ${err.message}`)
    return 1
  }

  let lock
  try {
    lock = acquireLock(threadId)
    if (!lock) {
      postBestEffort(threadId, event.bot_user_id, BUSY_NOTICE)
      return 0
    }
    await execute(event, threadId)
    return 0
  } catch (err) {
    log(`실행 실패: ${err.stack ?? err.message}`)
    postBestEffort(threadId, event.bot_user_id, formatErrorSummary('플러그인 실행 오류', err.message))
    return 1
  } finally {
    if (lock) {
      try {
        unlinkSync(lock)
      } catch (err) {
        log(`잠금 해제 실패: ${err.message}`)
      }
    }
  }
}

function readStdin() {
  return readFileSync(0, 'utf8')
}

function detach(raw) {
  const child = spawn(process.execPath, [SELF], {
    detached: true,
    stdio: ['pipe', 'ignore', 'ignore'],
    env: { ...process.env, DISCORD_CONNECTOR_DETACHED: '1' },
  })
  child.stdin.end(raw)
  child.unref()
}

async function main() {
  const raw = readStdin()
  let event
  try {
    event = validateEvent(raw)
  } catch (err) {
    log(`입력 검증 실패: ${err.message}`)
    process.stderr.write(`${err.message}\n`)
    return 1
  }

  // 테스트가 분리 실행을 끄고 같은 프로세스에서 흐름을 검증하기 위한 스위치
  const inline = process.env.DISCORD_CONNECTOR_DETACHED === '1' || process.env.DISCORD_CONNECTOR_NO_DETACH === '1'
  if (!inline) {
    detach(raw)
    return 0
  }
  return handle(event)
}

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(SELF)) {
  main().then(code => process.exit(code))
}
