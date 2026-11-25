import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { describe, test } from 'node:test'
import vm from 'node:vm'

import { bakeDiagrams, inspectReport } from '../../scripts/report/bake.mjs'

const template = readFileSync(new URL('../../skills/communicate/assets/report-template.html', import.meta.url), 'utf8')
const count = (re) => [...template.matchAll(re)].length

describe('report template', () => {
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
})
