const { appendFileSync, readFileSync, existsSync, fstatSync, statSync } = require('node:fs')
const { join } = require('node:path')

const dir = process.env.FAKE_DIR

// stdin 이 /dev/null 인지는 경로를 알 수 없어서, 문자 장치이면서 장치 번호(rdev)가 /dev/null 과 같은지로 판정한다
const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => k.startsWith('DISCORD_CONNECTOR_')))

appendFileSync(
  join(dir, 'calls.jsonl'),
  `${JSON.stringify({ tool: 'claude', args: process.argv.slice(2), cwd: process.cwd(), env, stdinIsDevNull: fstatSync(0).rdev === statSync('/dev/null').rdev && fstatSync(0).isCharacterDevice() })}\n`,
)

const behaviorPath = join(dir, 'behavior.json')
const behavior = (existsSync(behaviorPath) ? JSON.parse(readFileSync(behaviorPath, 'utf8')) : {}).claude ?? {}

process.stdout.write(
  behavior.stdout ?? JSON.stringify({ type: 'result', is_error: false, result: 'done <@999> and <@!999>', session_id: 'sid-1' }),
)
if (behavior.stderr) process.stderr.write(behavior.stderr)
process.exit(behavior.exit ?? 0)
