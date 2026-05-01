import { beforeEach, describe, expect, it } from 'vitest'
import {
  clearHighlightCache,
  escapeHtml,
  getCacheStats,
  getHighlighter,
  highlightCode,
  highlightCodeSync,
  normalizeLanguage,
} from '@/utils/codeHighlight'

describe('codeHighlight', () => {
  beforeEach(() => {
    clearHighlightCache()
  })

  it('normalizes language aliases', () => {
    expect(normalizeLanguage('ts')).toBe('typescript')
    expect(normalizeLanguage('YML')).toBe('yaml')
    expect(normalizeLanguage('')).toBe('text')
  })

  it('escapes unsafe HTML characters', () => {
    expect(escapeHtml('<script>"x"&\'y\'</script>')).toBe(
      '&lt;script&gt;&quot;x&quot;&amp;&#39;y&#39;&lt;/script&gt;',
    )
  })

  it('returns singleton highlighter promise', () => {
    const first = getHighlighter()
    const second = getHighlighter()
    expect(first).toBe(second)
  })

  it('highlights known language code and caches sync result', async () => {
    const html = await highlightCode('const x = 1', 'typescript')
    expect(html).toContain('class="shiki')
    expect(html).toContain('const')

    const cached = highlightCodeSync('const x = 1', 'typescript')
    expect(cached).toBe(html)
  })

  it('falls back to text for unknown language', async () => {
    const unknownLanguageHtml = await highlightCode('<tag>', 'unknown-language')
    const textHtml = await highlightCode('<tag>', 'text')

    expect(unknownLanguageHtml).toContain('class="shiki')
    expect(unknownLanguageHtml).toContain('&#x3C;')
    expect(unknownLanguageHtml).toBe(textHtml)
  })

  it('returns cache miss after clearing cache', async () => {
    await highlightCode('let x = 1', 'javascript')
    expect(highlightCodeSync('let x = 1', 'javascript')).toBeTruthy()

    clearHighlightCache()
    expect(highlightCodeSync('let x = 1', 'javascript')).toBeNull()
  })

  it('tracks loaded languages in cache stats after highlight', async () => {
    await highlightCode('const x = 1', 'typescript')
    const stats = getCacheStats()

    expect(stats.size).toBeGreaterThan(0)
    expect(stats.languages.has('typescript')).toBe(true)
  })
})
