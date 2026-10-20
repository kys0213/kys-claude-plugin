#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import { pathToFileURL } from 'node:url'
import { parseArgs } from 'node:util'

import { bakeDiagrams, inspectReport } from './bake.mjs'

const EXIT = { OK: 0, BROKEN: 1, UNVERIFIED: 2 }
const USAGE = 'usage: render-report.mjs <report.html> [--out <path>] [--mermaid-dist <dir>] [--timeout <ms>]'

class Stop extends Error {
  constructor(code, message) {
    super(message)
    this.code = code
  }
}

function parseCli(argv) {
  let parsed
  try {
    parsed = parseArgs({
      args: argv,
      allowPositionals: true,
      options: {
        out: { type: 'string' },
        'mermaid-dist': { type: 'string' },
        timeout: { type: 'string', default: '30000' },
      },
    })
  } catch (e) {
    throw new Stop(EXIT.BROKEN, `${e.message}\n${USAGE}`)
  }
  const { values, positionals } = parsed
  if (positionals.length !== 1) throw new Stop(EXIT.BROKEN, USAGE)
  if (!(Number(values.timeout) > 0)) throw new Stop(EXIT.BROKEN, `--timeout 은 양수(ms)여야 해요.\n${USAGE}`)
  const input = path.resolve(positionals[0])
  if (!existsSync(input)) throw new Stop(EXIT.BROKEN, `파일이 없어요: ${input}`)
  return {
    input,
    out: path.resolve(values.out ?? input),
    mermaidDist: values['mermaid-dist'] && path.resolve(values['mermaid-dist']),
    timeout: Number(values.timeout),
  }
}

async function loadPlaywright() {
  try {
    return await import('playwright')
  } catch {
    // ESM import 는 전역 node_modules 를 보지 않는다 — npm 전역 설치본을 직접 찾는다.
    try {
      const globalRoot = execFileSync('npm', ['root', '-g'], { encoding: 'utf8' }).trim()
      return createRequire(path.join(globalRoot, 'noop.js'))('playwright')
    } catch {
      throw new Stop(EXIT.UNVERIFIED, 'playwright 를 찾지 못했어요. `npm i -g playwright` 후 다시 실행해요.')
    }
  }
}

async function routeMermaidToLocal(page, dist) {
  // file:// 페이지(origin null)가 https 모듈을 import 하므로 CORS 헤더가 없으면 차단된다.
  await page.route(/\/npm\/mermaid@[^/]+\/dist\//, (route) => {
    const rel = new URL(route.request().url()).pathname.split('/dist/')[1]
    route.fulfill({
      path: path.join(dist, rel),
      contentType: 'text/javascript',
      headers: { 'access-control-allow-origin': '*' },
    })
  })
}

async function renderDiagrams(browser, file, { mermaidDist, timeout }) {
  const page = await browser.newPage()
  const pageErrors = []
  page.on('pageerror', (e) => pageErrors.push(e.message))
  if (mermaidDist) await routeMermaidToLocal(page, mermaidDist)

  await page.goto(pathToFileURL(file).href)
  try {
    await page.waitForSelector('html[data-diagrams]', { state: 'attached', timeout })
  } catch {
    throw new Stop(EXIT.UNVERIFIED, `${timeout}ms 안에 다이어그램 렌더링이 끝나지 않았어요.`)
  }

  const state = await page.evaluate(() => document.documentElement.dataset.diagrams)
  const errors = await page.evaluate(() => window.__diagramErrors ?? [])
  if (state === 'offline') {
    throw new Stop(
      EXIT.UNVERIFIED,
      `Mermaid 를 불러오지 못했어요 (${errors[0]?.message}). 네트워크를 확인하거나 --mermaid-dist 로 로컬 mermaid/dist 를 지정해요.`,
    )
  }
  if (state === 'error') {
    const lines = errors.map((e) => `  - 다이어그램 #${e.index + 1}:\n${e.message.split('\n').slice(0, 4).join('\n')}`)
    throw new Stop(EXIT.BROKEN, `다이어그램 문법 오류:\n${lines.join('\n')}`)
  }
  if (pageErrors.length) throw new Stop(EXIT.BROKEN, `페이지 스크립트 오류:\n  - ${pageErrors.join('\n  - ')}`)

  const svgs = await page.$$eval('pre.mermaid', (els) => els.map((el) => el.querySelector('svg')?.outerHTML ?? ''))
  await page.close()
  if (svgs.some((svg) => svg === '')) throw new Stop(EXIT.BROKEN, '렌더링 결과에 SVG 가 없는 다이어그램이 있어요.')
  return svgs
}

async function verifyOffline(browser, file) {
  const page = await browser.newPage()
  const external = []
  const pageErrors = []
  page.on('pageerror', (e) => pageErrors.push(e.message))
  await page.route('**/*', (route) => {
    const url = route.request().url()
    if (url.startsWith('file:') || url.startsWith('data:')) return route.continue()
    external.push(url)
    return route.abort()
  })

  await page.goto(pathToFileURL(file).href, { waitUntil: 'load' })
  const missing = await page.$$eval('figure.diagram', (els) => els.filter((f) => !f.querySelector(':scope > svg')).length)
  await page.close()

  if (external.length) throw new Stop(EXIT.BROKEN, `외부 리소스를 요청해요 (단일 파일이 아니에요):\n  - ${external.join('\n  - ')}`)
  if (pageErrors.length) throw new Stop(EXIT.BROKEN, `페이지 스크립트 오류:\n  - ${pageErrors.join('\n  - ')}`)
  if (missing) throw new Stop(EXIT.BROKEN, `SVG 가 없는 다이어그램이 ${missing}개 있어요.`)
}

async function main() {
  const opts = parseCli(process.argv.slice(2))
  const html = readFileSync(opts.input, 'utf8')
  const report = inspectReport(html)

  if (report.placeholders.length) {
    const list = report.placeholders.map((p) => `  - ${p.split('\n')[0]}`).join('\n')
    throw new Stop(EXIT.BROKEN, `채우지 않은 자리표시자(data-fill)가 ${report.placeholders.length}개 있어요:\n${list}`)
  }
  if (report.diagrams > 0 && !report.hasLoader) {
    throw new Stop(EXIT.BROKEN, 'Mermaid 블록이 있는데 로더(<!-- mermaid:start --> ~ <!-- mermaid:end -->)가 없어요.')
  }

  const { chromium } = await loadPlaywright()
  let browser
  try {
    browser = await chromium.launch()
  } catch (e) {
    throw new Stop(EXIT.UNVERIFIED, `Chromium 을 실행하지 못했어요: ${e.message.split('\n')[0]}`)
  }

  try {
    const svgs = report.hasLoader ? await renderDiagrams(browser, opts.input, opts) : []
    // 상대 경로 리소스가 같게 풀리도록 결과 파일과 같은 폴더에서 검증한 뒤 교체한다.
    const staged = `${opts.out}.render-check.html`
    writeFileSync(staged, bakeDiagrams(html, svgs))
    try {
      await verifyOffline(browser, staged)
      renameSync(staged, opts.out)
    } finally {
      rmSync(staged, { force: true })
    }
  } finally {
    await browser.close()
  }
  const baked = report.diagrams ? `다이어그램 ${report.diagrams}개를 SVG 로 넣었어요` : '새로 넣을 다이어그램 없음'
  console.log(`OK: ${opts.out} (${baked}. 외부 요청 없음)`)
}

main().then(
  () => process.exit(EXIT.OK),
  (e) => {
    if (!(e instanceof Stop)) throw e
    console.error(e.code === EXIT.UNVERIFIED ? `UNVERIFIED: ${e.message}` : `BROKEN: ${e.message}`)
    process.exit(e.code)
  },
)
