import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { after, describe, test } from 'node:test'
import { fileURLToPath } from 'node:url'

import { bakeDiagrams } from '../../scripts/report/bake.mjs'

const SCRIPT = fileURLToPath(new URL('../../scripts/report/render-report.mjs', import.meta.url))
const LOADER = '<!-- mermaid:start -->\n<script type="module">/* loader */</script>\n<!-- mermaid:end -->'
const page = (body, { loader = true } = {}) =>
  `<!doctype html><html><head><title>t</title></head><body>${body}${loader ? LOADER : ''}</body></html>`

const dir = mkdtempSync(path.join(tmpdir(), 'report-check-'))
after(() => rmSync(dir, { recursive: true, force: true }))

let n = 0
function check(html, ...args) {
  const file = path.join(dir, `r${n++}.html`)
  writeFileSync(file, html)
  const run = spawnSync(process.execPath, [SCRIPT, '--check', file, ...args], { encoding: 'utf8' })
  const lines = `${run.stdout}${run.stderr}`.trim().split('\n')
  return { code: run.status, last: lines.at(-1), all: lines.join('\n'), file, html }
}

describe('render-report --check (file only, no browser)', () => {
  test('passes a baked file and names the diagram count on the last line', () => {
    const html = bakeDiagrams(page('<pre class="mermaid">graph TD; A</pre><pre class="mermaid">graph TD; B</pre>'), [
      '<svg></svg>',
      '<svg></svg>',
    ])
    const r = check(html)

    assert.equal(r.code, 0)
    assert.match(r.last, /^OK: .*다이어그램 2개/)
  })

  test('leaves the file untouched', () => {
    const r = check(page('<pre class="mermaid">graph TD; A</pre>'))

    assert.equal(readFileSync(r.file, 'utf8'), r.html)
  })

  test('exits 2 when the loader is still in the file', () => {
    const r = check(page('<p>only text</p>'))

    assert.equal(r.code, 2)
    assert.match(r.last, /^UNVERIFIED: /)
  })

  test('exits 1 on a mermaid block left without the loader', () => {
    const r = check(page('<pre class="mermaid">graph TD; A</pre>', { loader: false }))

    assert.equal(r.code, 1)
    assert.match(r.all, /^BROKEN: /m)
  })

  test('exits 1 on unfilled placeholders', () => {
    const r = check(page('<p data-fill>결론</p>', { loader: false }))

    assert.equal(r.code, 1)
  })

  test('puts the verdict on the last line when it fails', () => {
    const r = check(page('<figure class="diagram"></figure><p data-fill>결론</p>', { loader: false }))

    assert.equal(r.code, 1)
    assert.match(r.last, /^BROKEN: /)
  })
})
