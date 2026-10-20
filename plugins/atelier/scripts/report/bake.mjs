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
