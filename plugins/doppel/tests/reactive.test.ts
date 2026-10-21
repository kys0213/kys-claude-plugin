import { describe, expect, test } from 'claude-code/testing'

import { splitLines, type LineBuffer } from '../hooks/logic'
import { Subject, filter, map, merge, scan } from '../hooks/reactive'

function collect<T>(subscribe: (observer: (value: T) => void) => () => void): { seen: T[]; stop: () => void } {
  const seen: T[] = []

  return { seen, stop: subscribe(value => seen.push(value)) }
}

describe('reactive operators', () => {
  test('map and filter transform and drop values in order', async () => {
    const source = new Subject<number>()
    const { seen } = collect<number>(o => filter(map(source, n => n * 10), n => n > 10).subscribe(o))

    for (const n of [1, 2, 3]) source.next(n)

    expect(seen).toEqual([20, 30])
  })

  test('scan emits every accumulation from its seed', async () => {
    const source = new Subject<number>()
    const { seen } = collect<number>(o => scan(source, (acc, n) => acc + n, 100).subscribe(o))

    for (const n of [1, 2, 3]) source.next(n)

    expect(seen).toEqual([101, 103, 106])
  })

  test('merge interleaves sources and unsubscribing stops all of them', async () => {
    const a = new Subject<string>()
    const b = new Subject<string>()
    const { seen, stop } = collect<string>(o => merge(a, b).subscribe(o))

    a.next('a1')
    b.next('b1')
    stop()
    a.next('a2')

    expect(seen).toEqual(['a1', 'b1'])
  })

  test('scan over splitLines turns stdout pieces into whole lines', async () => {
    const chunks = new Subject<string>()
    const lines: string[] = []
    const seed: LineBuffer = { rest: '', lines: [] }

    scan(chunks, splitLines, seed).subscribe(buffer => lines.push(...buffer.lines))
    for (const chunk of ['{"a"', ':1}\n{"b":2}\n\n{"c"', ':3}\n']) chunks.next(chunk)

    expect(lines).toEqual(['{"a":1}', '{"b":2}', '{"c":3}'])
  })
})
