import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, symlinkSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { afterEach, beforeEach, describe, test } from 'node:test'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const LAUNCH = resolve(HERE, '../../bin/launch.mjs')
const PLUGIN_ROOT = resolve(HERE, '../..')
const FAKE_BIN = resolve(HERE, '../fixtures/fake-bin')
const BOT = '999'

let root
let home
let fakeDir
let workdir

beforeEach(() => {
  root = realpathSync(mkdtempSync(join(tmpdir(), 'dc-launch-')))
  home = join(root, 'home')
  fakeDir = join(root, 'fake')
  workdir = join(root, 'work')
  for (const d of [home, fakeDir, workdir]) mkdirSync(d)
})

afterEach(() => rmSync(root, { recursive: true, force: true }))

function event(overrides = {}) {
  return {
    trigger: 'mention',
    guild_id: 'G1',
    channel_id: 'C1',
    parent_channel_id: null,
    is_thread: false,
    message_id: 'M1',
    content: `<@${BOT}> 이거 고쳐줘`,
    author: { id: '222', username: 'u', bot: false },
    timestamp: '2026-10-05T00:00:00Z',
    bot_user_id: BOT,
    message_reference: null,
    cwd: workdir,
    ...overrides,
  }
}

function launch(input, extraEnv = {}) {
  const payload = typeof input === 'string' ? input : JSON.stringify(input)
  return spawnSync(process.execPath, [LAUNCH], {
    input: payload,
    encoding: 'utf8',
    env: {
      PATH: `${FAKE_BIN}:${dirname(process.execPath)}:/usr/bin:/bin`,
      HOME: home,
      FAKE_DIR: fakeDir,
      DISCORD_CONNECTOR_NO_DETACH: '1',
      ...extraEnv,
    },
  })
}

function setBehavior(b) {
  writeFileSync(join(fakeDir, 'behavior.json'), JSON.stringify(b))
}

function calls() {
  const p = join(fakeDir, 'calls.jsonl')
  if (!existsSync(p)) return []
  return readFileSync(p, 'utf8').trim().split('\n').filter(Boolean).map(l => JSON.parse(l))
}

const claudeCalls = () => calls().filter(c => c.tool === 'claude')
const discordCalls = () => calls().filter(c => c.tool === 'discord')
const sends = () => discordCalls().filter(c => c.args.includes('send'))
const subcommands = () => discordCalls().map(c => c.args.filter(a => a !== '--json')[0])

function lockPath(thread) {
  return join(home, '.areum', 'discord-connector', 'locks', `${thread}.lock`)
}

function writeLock(thread, content) {
  mkdirSync(dirname(lockPath(thread)), { recursive: true })
  writeFileSync(lockPath(thread), content)
}

function writeThreadState(thread, state) {
  const dir = join(home, '.areum', 'discord-connector', 'threads')
  mkdirSync(dir, { recursive: true })
  writeFileSync(join(dir, `${thread}.json`), JSON.stringify(state))
}

function threadStatePath(thread) {
  const dir = join(home, '.areum', 'discord-connector', 'threads')
  mkdirSync(dir, { recursive: true })
  return join(dir, `${thread}.json`)
}

function pathWithoutClaude() {
  const bin = join(root, 'bin-without-claude')
  mkdirSync(bin)
  for (const f of ['discord', 'discord.cjs']) symlinkSync(join(FAKE_BIN, f), join(bin, f))
  symlinkSync(process.execPath, join(bin, 'node'))
  return `${bin}:/usr/bin:/bin`
}

function initRepo(dir) {
  const git = (...args) => spawnSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@example.com', ...args], { cwd: dir, encoding: 'utf8' })
  assert.equal(git('init', '-q').status, 0)
  assert.equal(git('commit', '-q', '--allow-empty', '-m', 'init').status, 0)
}

function readThreadState(thread) {
  return JSON.parse(readFileSync(join(home, '.areum', 'discord-connector', 'threads', `${thread}.json`), 'utf8'))
}

describe('thread handling', () => {
  test('a top-level message creates a thread and runs and posts inside it', () => {
    const r = launch(event())

    assert.equal(r.status, 0, r.stderr)
    const create = discordCalls().find(c => c.args.includes('thread'))
    assert.deepEqual(create.args.slice(0, 6), ['--json', 'thread', 'create', 'C1', '--from-message', 'M1'])
    assert.equal(claudeCalls().length, 1)
    assert.equal(claudeCalls()[0].env.DISCORD_CONNECTOR_THREAD_ID, 'T-NEW')
    assert.equal(sends()[0].args[2], 'T-NEW')
  })

  test('a message inside a thread creates no thread', () => {
    const r = launch(event({ is_thread: true, channel_id: 'T9', parent_channel_id: 'C1' }))

    assert.equal(r.status, 0, r.stderr)
    assert.ok(!subcommands().includes('thread'))
    assert.equal(claudeCalls()[0].env.DISCORD_CONNECTOR_THREAD_ID, 'T9')
    assert.equal(sends()[0].args[2], 'T9')
  })

  test('a thread creation failure runs nothing and exits with an error', () => {
    setBehavior({ discordFail: 'thread' })

    const r = launch(event())

    assert.equal(r.status, 1)
    assert.equal(claudeCalls().length, 0)
    assert.match(readFileSync(join(home, '.areum', 'discord-connector', 'logs', 'launcher.log'), 'utf8'), /스레드를 만들지 못했어요/)
  })

  test('an author.bot event is handled like a human one', () => {
    const r = launch(event({ author: { id: '777', username: 'bot', bot: true } }))

    assert.equal(r.status, 0, r.stderr)
    assert.equal(claudeCalls().length, 1)
    assert.equal(claudeCalls()[0].env.DISCORD_CONNECTOR_REQUESTER_ID, '777')
  })
})

describe('session continuity and working directory', () => {
  test('the first run records cwd and session id and passes no --resume', () => {
    launch(event({ is_thread: true, channel_id: 'T9' }))

    const call = claudeCalls()[0]
    assert.equal(call.cwd, workdir)
    assert.ok(!call.args.includes('--resume'))
    assert.ok(!call.args.includes('--worktree'))
    assert.deepEqual(readThreadState('T9'), { session_id: 'sid-1', cwd: workdir, worktree: false })
  })

  test('the next run resumes the stored session in the stored cwd even if stdin cwd differs', () => {
    const other = join(root, 'other')
    mkdirSync(other)
    writeThreadState('T9', { session_id: 'stored-sid', cwd: workdir })

    launch(event({ is_thread: true, channel_id: 'T9', cwd: other }))

    const call = claudeCalls()[0]
    assert.equal(call.cwd, workdir)
    const i = call.args.indexOf('--resume')
    assert.equal(call.args[i + 1], 'stored-sid')
  })

  test('a vanished stored cwd posts an error to the thread and runs nothing', () => {
    writeThreadState('T9', { session_id: 'stored-sid', cwd: join(root, 'gone') })

    const r = launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.equal(r.status, 0)
    assert.equal(claudeCalls().length, 0)
    assert.equal(sends().length, 1)
    assert.match(sends()[0].stdin, /기록된 작업 디렉토리가 없어졌어요/)
    assert.ok(!existsSync(lockPath('T9')))
  })
})

describe('per-thread worktree', () => {
  const argAfter = (args, flag) => args[args.indexOf(flag) + 1]

  test('a first run in a git repository passes --worktree, runs in the repository and records the worktree path', () => {
    initRepo(workdir)

    launch(event({ is_thread: true, channel_id: 'T9' }))

    const call = claudeCalls()[0]
    assert.equal(argAfter(call.args, '--worktree'), 'discord-T9')
    assert.ok(!call.args.includes('--resume'))
    assert.equal(call.cwd, workdir)
    assert.deepEqual(readThreadState('T9'), {
      session_id: 'sid-1',
      cwd: join(workdir, '.claude', 'worktrees', 'discord-T9'),
      worktree: true,
    })
  })

  test('a first run outside a git repository passes no --worktree and records the cwd', () => {
    launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.ok(!claudeCalls()[0].args.includes('--worktree'))
    assert.deepEqual(readThreadState('T9'), { session_id: 'sid-1', cwd: workdir, worktree: false })
  })

  test('a resume of a worktree thread passes --resume and --worktree and runs in the recorded worktree path', () => {
    const wt = join(workdir, '.claude', 'worktrees', 'discord-T9')
    mkdirSync(wt, { recursive: true })
    writeThreadState('T9', { session_id: 'stored-sid', cwd: wt, worktree: true })

    launch(event({ is_thread: true, channel_id: 'T9' }))

    const call = claudeCalls()[0]
    assert.equal(argAfter(call.args, '--resume'), 'stored-sid')
    assert.equal(argAfter(call.args, '--worktree'), 'discord-T9')
    assert.equal(call.cwd, wt)
    assert.deepEqual(readThreadState('T9'), { session_id: 'sid-1', cwd: wt, worktree: true })
  })

  test('a resume of a non-worktree thread passes no --worktree', () => {
    writeThreadState('T9', { session_id: 'stored-sid', cwd: workdir, worktree: false })

    launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.equal(argAfter(claudeCalls()[0].args, '--resume'), 'stored-sid')
    assert.ok(!claudeCalls()[0].args.includes('--worktree'))
  })

  test('a vanished recorded worktree posts an error to the thread, runs nothing and releases the lock', () => {
    writeThreadState('T9', { session_id: 'stored-sid', cwd: join(workdir, '.claude', 'worktrees', 'discord-T9'), worktree: true })

    const r = launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.equal(r.status, 0)
    assert.equal(claudeCalls().length, 0)
    assert.match(sends()[0].stdin, /기록된 작업 디렉토리가 없어졌어요/)
    assert.ok(!existsSync(lockPath('T9')))
  })

  test('claude stdin is connected to /dev/null', () => {
    launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.equal(claudeCalls()[0].stdinIsDevNull, true)
  })
})

describe('claude invocation', () => {
  test('required flags, thread link and trigger text are passed, bot mention removed', () => {
    launch(event({ is_thread: true, channel_id: 'T9' }))

    const { args } = claudeCalls()[0]
    const has = (flag, value) => assert.equal(args[args.indexOf(flag) + 1], value, flag)
    assert.equal(args[0], '-p')
    has('--permission-mode', 'auto')
    has('--permission-prompt-tool', 'stdio')
    has('--output-format', 'json')
    has('--model', 'sonnet')
    has('--plugin-dir', PLUGIN_ROOT)
    assert.ok(args[1].includes('https://discord.com/channels/G1/T9'))
    assert.ok(args[1].includes('이거 고쳐줘'))
    assert.ok(!args[1].includes('<@999>'))
    assert.ok(args[1].includes('최종 답은 이 Discord 스레드에 그대로 게시돼요'))
  })

  test('env carries thread, requester and bot ids', () => {
    launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.deepEqual(claudeCalls()[0].env, {
      DISCORD_CONNECTOR_NO_DETACH: '1',
      DISCORD_CONNECTOR_THREAD_ID: 'T9',
      DISCORD_CONNECTOR_REQUESTER_ID: '222',
      DISCORD_CONNECTOR_BOT_ID: BOT,
    })
  })
})

describe('locking', () => {
  test('a lock held by a live process posts the busy notice and runs nothing', () => {
    writeLock('T9', String(process.pid))

    const r = launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.equal(r.status, 0)
    assert.equal(claudeCalls().length, 0)
    assert.match(sends()[0].stdin, /이전 작업이 끝난 뒤 다시 멘션해 주세요/)
    assert.ok(existsSync(lockPath('T9')), 'the holder keeps its lock')
  })

  test('a lock of a dead pid is reclaimed and the run proceeds', () => {
    const dead = spawnSync(process.execPath, ['-e', '']).pid
    writeLock('T9', String(dead))

    launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.equal(claudeCalls().length, 1)
    assert.ok(!existsSync(lockPath('T9')))
  })

  test('launchers racing for a dead pid lock end in a run or the busy notice, never a plugin error', async () => {
    const dead = spawnSync(process.execPath, ['-e', '']).pid
    writeLock('T9', String(dead))
    const input = JSON.stringify(event({ is_thread: true, channel_id: 'T9' }))

    const exits = await Promise.all(
      Array.from({ length: 12 }, () => {
        const child = spawn(process.execPath, [LAUNCH], {
          env: { PATH: `${FAKE_BIN}:${dirname(process.execPath)}:/usr/bin:/bin`, HOME: home, FAKE_DIR: fakeDir, DISCORD_CONNECTOR_NO_DETACH: '1' },
        })
        child.stdin.end(input)
        return new Promise(res => child.on('close', code => res(code)))
      }),
    )

    assert.deepEqual(exits, Array(12).fill(0))
    for (const s of sends()) assert.ok(!/플러그인 실행 오류/.test(s.stdin), s.stdin)
    assert.ok(claudeCalls().length >= 1)
    assert.ok(!existsSync(lockPath('T9')))
  })

  test('the lock is released after a successful run', () => {
    launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.ok(!existsSync(lockPath('T9')))
  })
})

describe('failures are posted and release the lock', () => {
  const lines = n => Array.from({ length: n }, (_, i) => `line${i + 1}`).join('\n')

  test('a non-zero claude exit posts the failure type and the last 20 lines in a code block', () => {
    setBehavior({ claude: { exit: 3, stdout: '', stderr: lines(30) } })

    launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.equal(sends().length, 1)
    const body = sends()[0].stdin
    assert.match(body, /비정상 종료/)
    assert.match(body, /```\nline11\n/)
    assert.match(body, /line30\n```$/)
    assert.ok(!body.includes('line10\n'))
    assert.ok(!existsSync(lockPath('T9')))
    assert.ok(!existsSync(join(home, '.areum', 'discord-connector', 'threads', 'T9.json')))
  })

  test('unparseable claude output posts an interpretation error', () => {
    setBehavior({ claude: { exit: 0, stdout: 'not json at all' } })

    launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.match(sends()[0].stdin, /해석할 수 없어요/)
    assert.match(sends()[0].stdin, /not json at all/)
    assert.ok(!existsSync(lockPath('T9')))
  })

  test('a result without session_id is an error, not a success', () => {
    setBehavior({ claude: { stdout: JSON.stringify({ result: 'x' }) } })

    launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.match(sends()[0].stdin, /session_id/)
  })

  test('a corrupt thread state posts a plugin error to the thread and releases the lock', () => {
    writeFileSync(threadStatePath('T9'), '{ broken')

    const r = launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.equal(r.status, 1)
    assert.equal(claudeCalls().length, 0)
    assert.equal(sends().length, 1)
    assert.equal(sends()[0].args[2], 'T9')
    assert.match(sends()[0].stdin, /플러그인 실행 오류/)
    assert.ok(!existsSync(lockPath('T9')))
  })

  test('a claude executable that cannot start posts an error summary and releases the lock', () => {
    const r = launch(event({ is_thread: true, channel_id: 'T9' }), { PATH: pathWithoutClaude() })

    assert.equal(r.status, 0)
    assert.equal(sends().length, 1)
    assert.equal(sends()[0].args[2], 'T9')
    assert.match(sends()[0].stdin, /Claude 를 실행하지 못했어요/)
    assert.ok(!existsSync(lockPath('T9')))
  })

  test('a send failure still releases the lock', () => {
    setBehavior({ discordFail: 'send' })

    const r = launch(event({ is_thread: true, channel_id: 'T9' }))

    assert.equal(r.status, 1)
    assert.ok(!existsSync(lockPath('T9')))
  })
})

function assertPostingRules(posts, threadId) {
  assert.ok(posts.length > 0, 'a post was expected')
  for (const p of posts) {
    assert.deepEqual(p.args.slice(0, 3), ['--json', 'send', threadId])
    assert.deepEqual(p.args.slice(3), ['-', '--split'])
    assert.ok(!p.args.includes('--reply-to'))
    assert.ok(!p.stdin.includes('<@999>') && !p.stdin.includes('<@!999>'), p.stdin)
  }
}

describe('posting rules', () => {
  test('the result post neutralizes bot mentions', () => {
    launch(event({ is_thread: true, channel_id: 'T9' }))

    assertPostingRules(sends(), 'T9')
    assert.match(sends()[0].stdin, /done @999 and @999/)
  })

  test('the claude failure post follows the rules', () => {
    setBehavior({ claude: { exit: 1, stdout: '', stderr: 'err <@999> <@!999>' } })

    launch(event({ is_thread: true, channel_id: 'T9' }))

    assertPostingRules(sends(), 'T9')
    assert.match(sends()[0].stdin, /err @999 @999/)
  })

  test('the busy notice follows the rules', () => {
    writeLock('T8', String(process.pid))

    launch(event({ is_thread: true, channel_id: 'T8' }))

    assertPostingRules(sends(), 'T8')
  })

  test('the vanished cwd post follows the rules', () => {
    writeThreadState('T9', { session_id: 's', cwd: join(root, 'gone <@999> <@!999>') })

    launch(event({ is_thread: true, channel_id: 'T9' }))

    assertPostingRules(sends(), 'T9')
    assert.match(sends()[0].stdin, /gone @999 @999/)
  })

  test('the plugin execution error post follows the rules', () => {
    writeFileSync(threadStatePath('T9'), '<@999> <@!999> {')

    launch(event({ is_thread: true, channel_id: 'T9' }))

    assertPostingRules(sends(), 'T9')
    assert.match(sends()[0].stdin, /플러그인 실행 오류/)
    assert.match(sends()[0].stdin, /@999 @999/)
  })

  test('the spawn failure post follows the rules', () => {
    launch(event({ is_thread: true, channel_id: 'T9' }), { PATH: pathWithoutClaude() })

    assertPostingRules(sends(), 'T9')
    assert.match(sends()[0].stdin, /Claude 를 실행하지 못했어요/)
  })

  test('the thread name never carries the bot mention', () => {
    launch(event())

    const create = discordCalls().find(c => c.args.includes('thread'))
    const name = create.args[create.args.indexOf('--name') + 1]
    assert.ok(!name.includes('<@'))
    assert.equal(name, '이거 고쳐줘')
  })
})

describe('input validation', () => {
  test('a missing field exits with an error and runs nothing', () => {
    const bad = event()
    delete bad.message_id

    const r = launch(bad)

    assert.equal(r.status, 1)
    assert.match(r.stderr, /message_id/)
    assert.equal(calls().length, 0)
  })

  test('non-JSON stdin exits with an error and runs nothing', () => {
    const r = launch('nope')

    assert.equal(r.status, 1)
    assert.equal(calls().length, 0)
    assert.match(readFileSync(join(home, '.areum', 'discord-connector', 'logs', 'launcher.log'), 'utf8'), /입력 검증 실패.*JSON 이 아니에요/)
  })
})

describe('detaching', () => {
  test('without the test switch the launcher returns at once and the work continues detached', async () => {
    const r = launch(event({ is_thread: true, channel_id: 'T9' }), { DISCORD_CONNECTOR_NO_DETACH: '' })

    assert.equal(r.status, 0, r.stderr)
    const deadline = Date.now() + 10_000
    while (sends().length === 0 && Date.now() < deadline) {
      await new Promise(res => setTimeout(res, 100))
    }
    assert.equal(sends().length, 1)
  })
})
