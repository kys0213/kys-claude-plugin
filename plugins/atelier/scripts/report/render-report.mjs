#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import { pathToFileURL } from 'node:url'
import { parseArgs } from 'node:util'

import { bakeDiagrams, inspectReport, judgeFile } from './bake.mjs'

const EXIT = { OK: 0, BROKEN: 1, UNVERIFIED: 2 }
const USAGE = 'usage: render-report.mjs <report.html> [--out <path>] [--mermaid-dist <dir>] [--timeout <ms>]\n       render-report.mjs --check <report.html>'

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
        check: { type: 'boolean', default: false },
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
    check: values.check,
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
  const tabProblem = await page.evaluate(checkTabs)
  const tabCount = await page.$$eval('.tabbar [role=tab]', (els) => els.length)
  const overflows = []
  const blankDiagrams = []
  for (const width of VIEWPORT_WIDTHS) {
    await page.setViewportSize({ width, height: 800 })
    // 탭이 있으면 숨은 탭은 잴 수 없으니 탭마다 열어 보고 잰다.
    for (let i = 0; i < Math.max(tabCount, 1); i++) {
      const label = tabCount ? await page.evaluate(openTab, i) : ''
      const where = `${width}px 폭${label ? ` "${label}" 탭` : ''}`
      const offenders = await page.evaluate(findHorizontalOverflow)
      if (offenders) overflows.push(`${where}: ${offenders.length ? offenders.join(', ') : '원인 요소를 찾지 못했어요'}`)
      const blank = await page.evaluate(countBlankDiagrams)
      if (blank) blankDiagrams.push(`${where}: ${blank}개`)
    }
  }
  await page.emulateMedia({ media: 'print' })
  const hiddenInPrint = await page.evaluate(countHiddenSections)
  await page.close()

  if (external.length) throw new Stop(EXIT.BROKEN, `외부 리소스를 요청해요 (단일 파일이 아니에요):\n  - ${external.join('\n  - ')}`)
  if (pageErrors.length) throw new Stop(EXIT.BROKEN, `페이지 스크립트 오류:\n  - ${pageErrors.join('\n  - ')}`)
  if (missing) throw new Stop(EXIT.BROKEN, `SVG 가 없는 다이어그램이 ${missing}개 있어요.`)
  if (tabProblem) throw new Stop(EXIT.BROKEN, `탭이 제대로 만들어지지 않았어요: ${tabProblem}`)
  if (blankDiagrams.length) throw new Stop(EXIT.BROKEN, `크기가 0인 다이어그램이 있어요 (숨은 자리에서 그려졌어요):\n  - ${blankDiagrams.join('\n  - ')}`)
  if (hiddenInPrint) throw new Stop(EXIT.BROKEN, `인쇄할 때 보이지 않는 섹션이 ${hiddenInPrint}개 있어요.`)
  if (overflows.length) {
    throw new Stop(
      EXIT.BROKEN,
      `페이지가 가로로 넘쳐요 (표는 .table-wrap 으로 감싸고, 스타일을 따로 쓰지 않아요):\n  - ${overflows.join('\n  - ')}`,
    )
  }
}

// 휴대폰과 데스크톱 폭에서 페이지 전체가 가로로 스크롤되면 안 된다.
const VIEWPORT_WIDTHS = [375, 1280]

// 아래 함수들은 브라우저 안에서 실행된다.

// 섹션이 3개 이상이면 섹션마다 탭이 하나씩 있고, 보이는 탭 패널이 하나여야 한다. 문제가 없으면 null.
function checkTabs() {
  const sections = document.querySelectorAll('main > section:not(.bluf)').length
  const tabs = document.querySelectorAll('.tabbar [role=tab]').length
  if (sections < 3) return tabs ? `섹션이 ${sections}개인데 탭이 ${tabs}개예요` : null
  if (!document.documentElement.classList.contains('tabs')) return '탭 스크립트가 실행되지 않았어요 (<!-- tabs:start --> 블록 확인)'
  if (tabs !== sections) return `섹션 ${sections}개, 탭 ${tabs}개 — 섹션마다 <h2> 가 있어야 해요`
  const shown = [...document.querySelectorAll('main > section.panel')].filter((p) => p.offsetParent !== null).length
  return shown === 1 ? null : `보이는 탭 패널이 ${shown}개예요`
}

function openTab(index) {
  const tab = document.querySelectorAll('.tabbar [role=tab]')[index]
  tab.click()
  return tab.firstChild.textContent
}

function countBlankDiagrams() {
  return [...document.querySelectorAll('figure.diagram > svg')].filter((svg) => svg.checkVisibility() && svg.getBoundingClientRect().width < 1).length
}

function countHiddenSections() {
  return [...document.querySelectorAll('main > section')].filter((s) => getComputedStyle(s).display === 'none').length
}

// 넘치지 않으면 null, 넘치면 원인 요소 설명 목록을 돌려준다.
function findHorizontalOverflow() {
  const root = document.documentElement
  if (root.scrollWidth <= root.clientWidth + 1) return null
  const limit = root.clientWidth + 1
  const clipped = (el) => {
    for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
      if (['auto', 'scroll', 'hidden', 'clip'].includes(getComputedStyle(p).overflowX)) return true
    }
    return false
  }
  const over = new Set([...document.body.querySelectorAll('*')].filter((el) => el.getBoundingClientRect().right > limit && !clipped(el)))
  return [...over]
    .filter((el) => !over.has(el.parentElement))
    .slice(0, 5)
    .map((el) => {
      const cls = typeof el.className === 'string' && el.className ? `.${el.className.trim().split(/\s+/).join('.')}` : ''
      const text = (el.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 30)
      return `<${el.tagName.toLowerCase()}${cls}> "${text}"`
    })
}

// 파일 상태 한 줄. 일반 실행과 --check 가 같은 판정(judgeFile)으로 끝 줄을 쓴다.
function verdictLine(file, how) {
  const result = judgeFile(readFileSync(file, 'utf8'))
  if (result.verdict === 'BROKEN') throw new Stop(EXIT.BROKEN, `파일 상태가 깨졌어요:\n  - ${result.problems.join('\n  - ')}`)
  if (result.verdict === 'UNVERIFIED') throw new Stop(EXIT.UNVERIFIED, result.problems[0])
  return `OK: ${file} (다이어그램 ${result.diagrams}개 SVG 포함. 로더 없음. ${how})`
}

async function main() {
  const opts = parseCli(process.argv.slice(2))
  if (opts.check) {
    console.log(verdictLine(opts.input, '파일만 확인'))
    return
  }
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
  console.log(verdictLine(opts.out, '외부 요청 없음'))
}

main().then(
  () => process.exit(EXIT.OK),
  (e) => {
    if (!(e instanceof Stop)) throw e
    // 판정 줄이 항상 마지막 줄이 되게, 세부 목록을 먼저 쓴다.
    const [head, ...details] = e.message.split('\n')
    if (details.length) console.error(details.join('\n'))
    console.error(`${e.code === EXIT.UNVERIFIED ? 'UNVERIFIED' : 'BROKEN'}: ${head.replace(/:$/, '')}`)
    process.exit(e.code)
  },
)
