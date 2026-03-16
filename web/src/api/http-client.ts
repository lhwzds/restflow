import type { ErrorPayload } from '@/types/generated/ErrorPayload'
import type { IpcRequest } from '@/types/generated/IpcRequest'
import type { IpcResponse } from '@/types/generated/IpcResponse'
import type { StreamFrame } from '@/types/generated/StreamFrame'

export class BackendError extends Error {
  code: number
  kind: string
  details: unknown

  constructor(payload: ErrorPayload) {
    super(payload.message)
    this.name = 'BackendError'
    this.code = payload.code
    this.kind = payload.kind
    this.details = payload.details ?? null
  }
}

function resolveBaseUrl(): string {
  const configured = import.meta.env.VITE_DAEMON_URL?.trim()
  if (configured) {
    return configured.replace(/\/$/, '')
  }

  if (typeof window !== 'undefined' && /^https?:/i.test(window.location.origin)) {
    return window.location.origin.replace(/\/$/, '')
  }

  return 'http://127.0.0.1:8787'
}

export function buildUrl(path: string): string {
  return `${resolveBaseUrl()}${path}`
}

async function readJson<T>(response: Response): Promise<T> {
  if (!response.ok) {
    const text = await response.text()
    throw new Error(text || `HTTP ${response.status}`)
  }
  return (await response.json()) as T
}

export async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(buildUrl(path), init)
  return readJson<T>(response)
}

export async function requestClient(request: IpcRequest): Promise<IpcResponse> {
  const response = await fetch(buildUrl('/api/request'), {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json',
    },
    body: JSON.stringify(request),
  })

  return readJson<IpcResponse>(response)
}

export async function requestTyped<T>(request: IpcRequest): Promise<T> {
  const response = await requestClient(request)
  switch (response.response_type) {
    case 'success':
      return response.data as T
    case 'error':
      throw new BackendError(response.data)
    case 'pong':
      return null as T
    default:
      throw new Error('Unknown response envelope')
  }
}

export async function requestOptional<T>(request: IpcRequest): Promise<T | null> {
  try {
    return await requestTyped<T>(request)
  } catch (error) {
    if (error instanceof BackendError && error.code === 404) {
      return null
    }
    throw error
  }
}

export async function* streamClient(
  request: IpcRequest,
  init?: Omit<RequestInit, 'body' | 'method' | 'headers'>,
): AsyncGenerator<StreamFrame> {
  const response = await fetch(buildUrl('/api/stream'), {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/x-ndjson',
    },
    body: JSON.stringify(request),
    signal: init?.signal,
  })

  if (!response.ok) {
    const text = await response.text()
    throw new Error(text || `HTTP ${response.status}`)
  }

  if (!response.body) {
    throw new Error('Streaming response body is missing')
  }

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      const { value, done } = await reader.read()
      if (done) {
        break
      }

      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() ?? ''

      for (const line of lines) {
        const trimmed = line.trim()
        if (!trimmed) continue
        yield JSON.parse(trimmed) as StreamFrame
      }
    }

    buffer += decoder.decode()
    const trimmed = buffer.trim()
    if (trimmed) {
      yield JSON.parse(trimmed) as StreamFrame
    }
  } finally {
    reader.releaseLock()
  }
}
