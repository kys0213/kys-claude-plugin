import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { describe, test } from 'node:test'
import vm from 'node:vm'

import { bakeDiagrams, inspectReport } from '../../scripts/report/bake.mjs'

const read = (name) => readFileSync(new URL(`../../skills/communicate/assets/${name}`, import.meta.url), 'utf8')
const TEMPLATES = {
  'report-template.html': read('report-template.html'),
  'report-template-change.html': read('report-template-change.html'),
}
const headings = (html) => [...html.matchAll(/<section>\s*<h2>([^<]+)<\/h2>/g)].map((m) => m[1])
const block = (html, re) => html.match(re)?.[0]

for (const [name, template] of Object.entries(TEMPLATES)) {
  const count = (re) => [...template.matchAll(re)].length

  describe(`report template ${name}`, () => {
    test('is a responsive single page with one stylesheet and the mermaid loader', () => {
      assert.match(template, /<meta name="viewport" content="width=device-width, initial-scale=1">/)
      assert.equal(count(/<style>/g), 1)
      assert.equal(inspectReport(template).hasLoader, true)
    })

    test('every mermaid block is one the baker can find', () => {
      assert.equal(inspectReport(template).diagrams, count(/class="[^"]*\bmermaid\b[^"]*"/g))
    })

    test('every table sits in a horizontal scroller so the page never scrolls sideways', () => {
      assert.ok(count(/<table/g) > 0)
      assert.equal(count(/<div class="table-wrap">\s*<table/g), count(/<table/g))
    })

    test('ships only placeholders, so an unfilled copy fails validation', () => {
      assert.ok(inspectReport(template).placeholders.length > 0)
    })

    test('every content section has an h2 the tab script can label it with', () => {
      assert.ok(count(/<section>/g) >= 3)
      assert.equal(count(/<section>\s*<h2>/g), count(/<section>/g))
    })

    test('keeps a parseable tab script after diagrams are baked', () => {
      const svgs = Array.from({ length: inspectReport(template).diagrams }, () => '<svg></svg>')
      const baked = bakeDiagrams(template, svgs)
      const script = baked.match(/<!-- tabs:start[^>]*-->\s*<script>([\s\S]*?)<\/script>\s*<!-- tabs:end -->/)

      assert.ok(script, 'tab script block is missing after bake')
      assert.doesNotThrow(() => new vm.Script(script[1]))
    })

    test('requests nothing external once diagrams are baked', () => {
      const svgs = Array.from({ length: inspectReport(template).diagrams }, () => '<svg></svg>')

      assert.doesNotMatch(bakeDiagrams(template, svgs), /https?:\/\//)
    })

    test('keeps mermaid blocks and placeholders out of html comments, where the baker would still see them', () => {
      for (const [comment] of template.matchAll(/<!--[\s\S]*?-->/g)) {
        assert.doesNotMatch(comment, /class="mermaid"|data-fill/, comment)
      }
    })
  })
}

describe('change report template', () => {
  const base = TEMPLATES['report-template.html']
  const change = TEMPLATES['report-template-change.html']

  test('shares the stylesheet, tab script and mermaid loader with the base template byte for byte', () => {
    for (const re of [/<style>[\s\S]*?<\/style>/, /<!-- tabs:start[\s\S]*?<!-- tabs:end -->/, /<!-- mermaid:start -->[\s\S]*?<!-- mermaid:end -->/]) {
      assert.ok(block(base, re), String(re))
      assert.equal(block(change, re), block(base, re), String(re))
    }
  })

  test('orders tabs as now, after, remaining risks, then appendix and sources', () => {
    assert.deepEqual(headings(change), ['지금', '바뀐 뒤', '남는 위험', '확인 절차', '부록', '근거'])
  })

  test('puts one before/after diagram pair in the always-visible conclusion box', () => {
    const bluf = block(change, /<section class="bluf"[\s\S]*?<\/section>/)
    assert.equal(inspectReport(bluf).diagrams, 2)
  })

  test('the base template keeps its own section names', () => {
    assert.deepEqual(headings(base), ['대상 설명', '주의할 점', '확인 절차', '결정과 버린 선택지', '상세', '근거'])
  })
})

