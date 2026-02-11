import { createHighlighter, type Highlighter } from 'shiki'

const HIGHLIGHT_THEMES = ['github-light', 'github-dark'] as const
const HIGHLIGHT_LANGUAGES = [
  'text',
  'javascript',
  'typescript',
  'python',
  'rust',
  'bash',
  'json',
  'yaml',
  'toml',
  'html',
  'css',
  'vue',
  'sql',
  'markdown',
  'diff',
  'go',
  'java',
  'c',
  'cpp',
] as const

const LANGUAGE_ALIASES: Record<string, string> = {
  js: 'javascript',
  ts: 'typescript',
  py: 'python',
  sh: 'bash',
  shell: 'bash',
  zsh: 'bash',
  yml: 'yaml',
}

let highlighterPromise: Promise<Highlighter> | null = null

export function normalizeLanguage(language: string): string {
  const normalized = language.trim().toLowerCase()
  if (!normalized) return 'text'
  return LANGUAGE_ALIASES[normalized] || normalized
}

export function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [...HIGHLIGHT_THEMES],
      langs: [...HIGHLIGHT_LANGUAGES],
    })
  }
  return highlighterPromise
}

export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

export async function highlightCode(code: string, language: string): Promise<string> {
  try {
    const normalized = normalizeLanguage(language)
    const highlighter = await getHighlighter()
    const loadedLanguages = new Set(highlighter.getLoadedLanguages().map((item) => String(item)))
    const targetLanguage = loadedLanguages.has(normalized) ? normalized : 'text'

    return highlighter.codeToHtml(code, {
      lang: targetLanguage,
      themes: {
        light: 'github-light',
        dark: 'github-dark',
      },
      defaultColor: false,
    })
  } catch {
    return `<pre class="rf-code-fallback"><code>${escapeHtml(code)}</code></pre>`
  }
}
