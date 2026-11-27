const MERMAID_BLOCK = /<pre class="mermaid"[^>]*>([\s\S]*?)<\/pre>/g
const LOADER_BLOCK = /<!-- mermaid:start -->[\s\S]*?<!-- mermaid:end -->\n?/g
const FILL_SLOT = /<(\w+)[^>]*\sdata-fill(?:="[^"]*")?[^>]*>([\s\S]*?)<\/\1>/g

export function inspectReport(html) {
  return {
    diagrams: [...html.matchAll(MERMAID_BLOCK)].length,
    hasLoader: html.match(LOADER_BLOCK) !== null,
    placeholders: [...html.matchAll(FILL_SLOT)].map((m) => m[2].trim()),
  }
}

export function bakeDiagrams(html, svgs) {
  const { diagrams } = inspectReport(html)
  if (diagrams !== svgs.length) {
    throw new Error(`${diagrams} diagram(s) in the page but ${svgs.length} svg(s) rendered`)
  }

  let i = 0
  return html
    .replace(MERMAID_BLOCK, (_, source) => {
      const svg = svgs[i++]
      return (
        `<figure class="diagram">${svg}` +
        `<details class="diagram-source"><summary>다이어그램 원본 (Mermaid)</summary><pre>${source}</pre></details>` +
        `</figure>`
      )
    })
    .replace(LOADER_BLOCK, '')
}

// 브라우저 없이 파일 상태만 보고 판정한다. 구운 파일은 로더가 없고, 다이어그램마다 SVG 가 바로 들어 있다.
const BAKED_FIGURE = /<figure class="diagram">/g
const BAKED_FIGURE_WITH_SVG = /<figure class="diagram"><svg[\s>]/g

export function judgeFile(html) {
  const { diagrams: unbaked, hasLoader, placeholders } = inspectReport(html)
  const figures = [...html.matchAll(BAKED_FIGURE)].length
  const withSvg = [...html.matchAll(BAKED_FIGURE_WITH_SVG)].length
  const problems = []
  if (placeholders.length) problems.push(`채우지 않은 자리표시자(data-fill)가 ${placeholders.length}개 있어요`)
  if (figures > withSvg) problems.push(`SVG 가 없는 다이어그램이 ${figures - withSvg}개 있어요`)
  if (unbaked && !hasLoader) problems.push(`로더 없이 남은 mermaid 블록이 ${unbaked}개 있어요 (그려지지 않아요)`)
  if (problems.length) return { verdict: 'BROKEN', diagrams: withSvg, problems }
  if (hasLoader) {
    return { verdict: 'UNVERIFIED', diagrams: withSvg, problems: ['Mermaid 로더가 남아 있어요 — render-report.mjs 로 굽지 않은 파일이에요'] }
  }
  return { verdict: 'OK', diagrams: withSvg, problems: [] }
}
