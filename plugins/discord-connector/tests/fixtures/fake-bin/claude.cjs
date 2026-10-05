const { appendFileSync, readFileSync, existsSync } = require('node:fs')
const { join } = require('node:path')

const dir = process.env.FAKE_DIR

const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => k.startsWith('DISCORD_CONNECTOR_')))

appendFileSync(
  join(dir, 'calls.jsonl'),
  `${JSON.stringify({ tool: 'claude', args: process.argv.slice(2), cwd: process.cwd(), env })}\n`,
)

const behaviorPath = join(dir, 'behavior.json')
const behavior = (existsSync(behaviorPath) ? JSON.parse(readFileSync(behaviorPath, 'utf8')) : {}).claude ?? {}

process.stdout.write(
  behavior.stdout ?? JSON.stringify({ type: 'result', is_error: false, result: 'done <@999> and <@!999>', session_id: 'sid-1' }),
)
if (behavior.stderr) process.stderr.write(behavior.stderr)
process.exit(behavior.exit ?? 0)
