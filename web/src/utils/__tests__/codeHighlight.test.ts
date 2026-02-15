import { describe, expect, it } from 'vitest'
import { escapeHtml, getHighlighter, highlightCode, normalizeLanguage } from '@/utils/codeHighlight'

describe('codeHighlight', () => {
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

  it('highlights known language code', async () => {
    const html = await highlightCode('const x = 1', 'typescript')
    expect(html).toContain('class="shiki')
    expect(html).toContain('const')
  })

  it('falls back to plain text for unknown language', async () => {
    const html = await highlightCode('<tag>', 'unknown-language')
    expect(html).toContain('class="shiki')
    expect(html).toContain('tag')
    expect(html).toContain('&#x3C;')
  })
})
