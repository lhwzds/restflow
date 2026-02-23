import type { StreamStep } from '@/composables/workspace/useChatStream'

type JsonRecord = Record<string, unknown>

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function parseMaybeJson(value: unknown): unknown {
  if (typeof value !== 'string') return value
  const trimmed = value.trim()
  if (!trimmed) return ''

  try {
    return JSON.parse(trimmed)
  } catch {
    return value
  }
}

export function statusColor(code: number): string {
  if (code >= 200 && code < 300) return 'bg-emerald-100 text-emerald-800'
  if (code >= 500) return 'bg-rose-100 text-rose-800'
  return 'bg-amber-100 text-amber-800'
}

export function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case 'GET':
      return 'bg-sky-100 text-sky-800'
    case 'POST':
      return 'bg-emerald-100 text-emerald-800'
    case 'PUT':
    case 'PATCH':
      return 'bg-orange-100 text-orange-800'
    case 'DELETE':
      return 'bg-rose-100 text-rose-800'
    default:
      return 'bg-zinc-100 text-zinc-800'
  }
}

export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return '0ms'
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)}s`
  return `${(ms / 60_000).toFixed(1)}m`
}

export function parseToolArguments(step: StreamStep): JsonRecord {
  const parsed = parseMaybeJson(step.arguments)
  return isRecord(parsed) ? parsed : {}
}

export function parseToolResult(step: StreamStep): unknown {
  return parseMaybeJson(step.result)
}

export function detectLanguage(path: string): string {
  const extension = path.split('.').pop()?.toLowerCase() ?? ''
  switch (extension) {
    case 'rs':
      return 'rust'
    case 'ts':
    case 'tsx':
      return 'typescript'
    case 'js':
    case 'jsx':
      return 'javascript'
    case 'json':
      return 'json'
    case 'md':
      return 'markdown'
    case 'py':
      return 'python'
    case 'vue':
      return 'vue'
    case 'sh':
    case 'bash':
      return 'bash'
    case 'yml':
    case 'yaml':
      return 'yaml'
    case 'toml':
      return 'toml'
    default:
      return 'text'
  }
}

export async function copyText(text: string): Promise<boolean> {
  if (!text.trim()) return false
  if (!globalThis.navigator?.clipboard?.writeText) return false

  await globalThis.navigator.clipboard.writeText(text)
  return true
}
