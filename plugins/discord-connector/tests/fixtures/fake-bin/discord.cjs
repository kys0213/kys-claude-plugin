const { appendFileSync, readFileSync, existsSync } = require('node:fs')
const { join } = require('node:path')

const dir = process.env.FAKE_DIR
const args = process.argv.slice(2)
const stdin = process.stdin.isTTY ? '' : readFileSync(0, 'utf8')

appendFileSync(join(dir, 'calls.jsonl'), `${JSON.stringify({ tool: 'discord', args, stdin })}\n`)

const behaviorPath = join(dir, 'behavior.json')
const behavior = existsSync(behaviorPath) ? JSON.parse(readFileSync(behaviorPath, 'utf8')) : {}
const sub = args.filter(a => a !== '--json')[0]

if (behavior.discordFail === sub) {
  process.stdout.write(JSON.stringify({ ok: false, command: sub, error: { kind: 'api', message: 'forced failure' } }))
  process.exit(1)
}

const data = sub === 'thread' ? { thread_id: 'T-NEW', name: 'n' } : { messages: [{ message_id: 'm1' }] }
process.stdout.write(JSON.stringify({ ok: true, command: sub, data }))
