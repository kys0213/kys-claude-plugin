import assert from 'node:assert/strict'
import { describe, test } from 'node:test'

import { bakeDiagrams, inspectReport } from '../../scripts/report/bake.mjs'

const LOADER = '<!-- mermaid:start -->\n<script type="module">/* loader */</script>\n<!-- mermaid:end -->'

const page = (body, { loader = true } = {}) =>
  `<!doctype html><html><head><title>t</title></head><body>${body}${loader ? LOADER : ''}</body></html>`

describe('inspectReport', () => {
  test('counts mermaid blocks and detects the loader', () => {
    const html = page('<pre class="mermaid">graph TD; A--&gt;B</pre><pre class="mermaid">graph LR; C</pre>')

    assert.deepEqual(inspectReport(html), { diagrams: 2, hasLoader: true, placeholders: [] })
  })

  test('reports a page without diagrams or loader', () => {
    assert.deepEqual(inspectReport(page('<p>x</p>', { loader: false })), {
      diagrams: 0,
      hasLoader: false,
      placeholders: [],
    })
  })

  test('lists template placeholders left unfilled', () => {
    const html = page('<h1><span data-fill>제목</span></h1><p data-fill="">결론</p><code>${{ secrets.X }}</code>')

    assert.deepEqual(inspectReport(html).placeholders, ['제목', '결론'])
  })

  test('counts mermaid blocks that carry extra attributes', () => {
    assert.equal(inspectReport(page('<pre class="mermaid" data-fill>graph TD; A</pre>')).diagrams, 1)
  })

  test('ignores pre blocks that are not mermaid', () => {
    assert.equal(inspectReport(page('<pre class="code">x</pre>')).diagrams, 0)
  })
})

describe('bakeDiagrams', () => {
  test('replaces each mermaid block with its svg in order and keeps the source', () => {
    const html = page('<pre class="mermaid">graph TD; A--&gt;B</pre><p>mid</p><pre class="mermaid">graph LR; C</pre>')

    const out = bakeDiagrams(html, ['<svg id="d0"></svg>', '<svg id="d1"></svg>'])

    assert.equal(inspectReport(out).diagrams, 0)
    assert.ok(out.indexOf('<svg id="d0">') < out.indexOf('<p>mid</p>'))
    assert.ok(out.indexOf('<p>mid</p>') < out.indexOf('<svg id="d1">'))
    assert.match(out, /<details class="diagram-source">[\s\S]*graph TD; A--&gt;B[\s\S]*<\/details>/)
  })

  test('removes the loader so the baked file makes no network request', () => {
    const out = bakeDiagrams(page('<pre class="mermaid">graph TD; A</pre>'), ['<svg></svg>'])

    assert.equal(inspectReport(out).hasLoader, false)
    assert.doesNotMatch(out, /<script/)
  })

  test('removes the loader from a page without diagrams', () => {
    const out = bakeDiagrams(page('<p>only text</p>'), [])

    assert.equal(out, page('<p>only text</p>', { loader: false }))
  })

  test('rejects a svg count that does not match the diagram count', () => {
    assert.throws(() => bakeDiagrams(page('<pre class="mermaid">graph TD; A</pre>'), []), /1 diagram.*0 svg/)
  })

  test('keeps the replacement literal when the svg contains dollar patterns', () => {
    const out = bakeDiagrams(page('<pre class="mermaid">graph TD; A</pre>'), ['<svg><text>$& $1</text></svg>'])

    assert.match(out, /<text>\$& \$1<\/text>/)
  })
})
