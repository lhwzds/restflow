import type { ErrorPayload } from '@/types/generated/ErrorPayload'
import type { IpcRequest } from '@/types/generated/IpcRequest'
import type { IpcResponse } from '@/types/generated/IpcResponse'
import type { IpcStreamEvent } from '@/types/generated/IpcStreamEvent'
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

function normalizeEnvelopeTag(tag: string): string {
  return tag.toLowerCase()
}

function normalizeStreamTag(tag: string): string {
  return tag.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase()
}

type RawEnvelope = {
  response_type: string
  data?: unknown
}

type RawStreamFrame = {
  stream_type: string
  data: unknown
}

function normalizeIpcResponse(payload: RawEnvelope): IpcResponse {
  const responseType = normalizeEnvelopeTag(payload.response_type)

  switch (responseType) {
    case 'success':
      return {
        response_type: 'success',
        data: 'data' in payload ? payload.data : null,
      }
    case 'error':
      return {
        response_type: 'error',
        data:
          payload.data !== undefined
            ? (payload.data as ErrorPayload)
            : ({
            code: 500,
            kind: 'internal',
            message: 'Unknown daemon error',
            details: null,
          } as ErrorPayload),
      }
    case 'pong':
      return { response_type: 'pong' }
    default:
      throw new Error(`Unknown response envelope: ${payload.response_type}`)
  }
}

function normalizeStreamFrame(frame: RawStreamFrame): StreamFrame {
  const streamType = normalizeStreamTag(frame.stream_type)

  switch (streamType) {
    case 'start':
      return { stream_type: 'start', data: frame.data as { stream_id: string } }
    case 'ack':
      return { stream_type: 'ack', data: frame.data as { content: string } }
    case 'data':
      return { stream_type: 'data', data: frame.data as { content: string } }
    case 'tool_call':
      return {
        stream_type: 'tool_call',
        data: frame.data as { id: string; name: string; arguments: unknown },
      }
    case 'tool_result':
      return {
        stream_type: 'tool_result',
        data: frame.data as { id: string; result: string; success: boolean },
      }
    case 'event':
      return { stream_type: 'event', data: frame.data as { event: IpcStreamEvent } }
    case 'done':
      return {
        stream_type: 'done',
        data: frame.data as { total_tokens?: number | null },
      }
    case 'error':
      return { stream_type: 'error', data: frame.data as ErrorPayload }
    default:
      throw new Error(`Unknown stream envelope: ${frame.stream_type}`)
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
    const contentType = response.headers.get('content-type') ?? ''
    if (contentType.includes('application/json') && text.trim()) {
      try {
        const payload = JSON.parse(text) as Partial<ErrorPayload>
        if (
          typeof payload.code === 'number' &&
          typeof payload.kind === 'string' &&
          typeof payload.message === 'string'
        ) {
          throw new BackendError(payload as ErrorPayload)
        }
      } catch (error) {
        if (error instanceof BackendError) {
          throw error
        }
      }
    }
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

  const payload = await readJson<RawEnvelope>(response)
  return normalizeIpcResponse(payload)
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
        const frame = JSON.parse(trimmed) as RawStreamFrame
        yield normalizeStreamFrame(frame)
      }
    }

    buffer += decoder.decode()
    const trimmed = buffer.trim()
    if (trimmed) {
      const frame = JSON.parse(trimmed) as RawStreamFrame
      yield normalizeStreamFrame(frame)
    }
  } finally {
    reader.releaseLock()
  }
}
