import { createBundledHighlighter } from 'shiki/core'
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript'

const HIGHLIGHT_THEMES = ['github-light', 'github-dark'] as const

const LANGUAGE_LOADERS = {
  javascript: () => import('@shikijs/langs/javascript'),
  typescript: () => import('@shikijs/langs/typescript'),
  python: () => import('@shikijs/langs/python'),
  rust: () => import('@shikijs/langs/rust'),
  bash: () => import('@shikijs/langs/bash'),
  json: () => import('@shikijs/langs/json'),
  yaml: () => import('@shikijs/langs/yaml'),
  toml: () => import('@shikijs/langs/toml'),
  html: () => import('@shikijs/langs/html'),
  css: () => import('@shikijs/langs/css'),
  vue: () => import('@shikijs/langs/vue'),
  sql: () => import('@shikijs/langs/sql'),
  markdown: () => import('@shikijs/langs/markdown'),
  diff: () => import('@shikijs/langs/diff'),
  go: () => import('@shikijs/langs/go'),
  java: () => import('@shikijs/langs/java'),
  c: () => import('@shikijs/langs/c'),
  cpp: () => import('@shikijs/langs/cpp'),
} as const

type SupportedLanguage = keyof typeof LANGUAGE_LOADERS

const createHighlighterFactory = createBundledHighlighter({
  langs: LANGUAGE_LOADERS,
  themes: {
    'github-light': () => import('@shikijs/themes/github-light'),
    'github-dark': () => import('@shikijs/themes/github-dark'),
  },
  engine: createJavaScriptRegexEngine,
})

type AppHighlighter = Awaited<ReturnType<typeof createHighlighterFactory>>

const LANGUAGE_ALIASES: Record<string, string> = {
  js: 'javascript',
  ts: 'typescript',
  py: 'python',
  sh: 'bash',
  shell: 'bash',
  zsh: 'bash',
  yml: 'yaml',
}

let highlighterPromise: Promise<AppHighlighter> | null = null
const highlightCache = new Map<string, string>()
const loadedLanguages = new Set<string>(['text'])

export function normalizeLanguage(language: string): string {
  const normalized = language.trim().toLowerCase()
  if (!normalized) return 'text'
  return LANGUAGE_ALIASES[normalized] || normalized
}

function getCacheKey(code: string, language: string): string {
  return `${language}:${code}`
}

export function getHighlighter(): Promise<AppHighlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighterFactory({
      themes: [...HIGHLIGHT_THEMES],
      langs: [],
    })
  }
  return highlighterPromise
}

function hasLanguageLoader(language: string): language is SupportedLanguage {
  return Object.prototype.hasOwnProperty.call(LANGUAGE_LOADERS, language)
}

async function ensureLanguageLoaded(highlighter: AppHighlighter, language: string): Promise<boolean> {
  if (language === 'text' || loadedLanguages.has(language)) {
    return true
  }

  if (!hasLanguageLoader(language)) {
    return false
  }

  try {
    await highlighter.loadLanguage(language)
    loadedLanguages.add(language)
    return true
  } catch {
    return false
  }
}

export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

function createFallbackHtml(code: string): string {
  return `<pre class="rf-code-fallback"><code>${escapeHtml(code)}</code></pre>`
}

export async function highlightCode(code: string, language: string): Promise<string> {
  const normalized = normalizeLanguage(language)
  const cacheKey = getCacheKey(code, normalized)
  const cached = highlightCache.get(cacheKey)
  if (cached !== undefined) {
    return cached
  }

  try {
    const highlighter = await getHighlighter()
    const languageAvailable = await ensureLanguageLoaded(highlighter, normalized)
    const targetLanguage = languageAvailable ? normalized : 'text'

    if (targetLanguage === 'text' && normalized !== 'text') {
      const textCached = highlightCache.get(getCacheKey(code, 'text'))
      if (textCached !== undefined) {
        highlightCache.set(cacheKey, textCached)
        return textCached
      }
    }

    const result = highlighter.codeToHtml(code, {
      lang: targetLanguage as SupportedLanguage | 'text',
      themes: {
        light: 'github-light',
        dark: 'github-dark',
      },
      defaultColor: false,
    })

    highlightCache.set(cacheKey, result)
    if (targetLanguage === 'text' && normalized !== 'text') {
      highlightCache.set(getCacheKey(code, 'text'), result)
    }
    return result
  } catch {
    const fallback = createFallbackHtml(code)
    highlightCache.set(cacheKey, fallback)
    return fallback
  }
}

export function highlightCodeSync(code: string, language: string): string | null {
  const normalized = normalizeLanguage(language)
  const cacheKey = getCacheKey(code, normalized)
  return highlightCache.get(cacheKey) || null
}

export function clearHighlightCache(): void {
  highlightCache.clear()
}

export function getCacheStats(): { size: number; languages: Set<string> } {
  return {
    size: highlightCache.size,
    languages: new Set(loadedLanguages),
  }
}
