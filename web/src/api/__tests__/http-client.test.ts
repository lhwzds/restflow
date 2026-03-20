import { beforeEach, describe, expect, it, vi } from 'vitest'
import { BackendError, fetchJson, requestOptional, requestTyped, streamClient } from '../http-client'

declare const global: typeof globalThis

function createJsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })
}

function createNdjsonResponse(lines: unknown[]): Response {
  const payload = lines.map((line) => JSON.stringify(line)).join('\n') + '\n'
  return new Response(payload, {
    status: 200,
    headers: { 'Content-Type': 'application/x-ndjson' },
  })
}

describe('http-client', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('unwraps successful response envelopes', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(createJsonResponse({
      response_type: 'Success',
      data: { ok: true },
    })))

    const result = await requestTyped<{ ok: boolean }>({ type: 'Ping' })

    expect(result).toEqual({ ok: true })
  })

  it('throws structured backend errors', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(createJsonResponse({
      response_type: 'Error',
      data: {
        code: 500,
        kind: 'internal',
        message: 'boom',
        details: null,
      },
    })))

    await expect(requestTyped({ type: 'Ping' })).rejects.toBeInstanceOf(BackendError)
  })

  it('returns null for typed 404 envelopes', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(createJsonResponse({
      response_type: 'Error',
      data: {
        code: 404,
        kind: 'not_found',
        message: 'missing',
        details: null,
      },
    })))

    const result = await requestOptional({ type: 'GetThing' })
    expect(result).toBeNull()
  })

  it('throws BackendError for non-OK JSON payloads', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            code: 428,
            kind: 'confirmation_required',
            message: 'confirm',
            details: { assessment: { status: 'warning' } },
          }),
          {
            status: 428,
            headers: { 'Content-Type': 'application/json' },
          },
        ),
      ),
    )

    await expect(fetchJson('/api/background-agents/convert-session')).rejects.toBeInstanceOf(
      BackendError,
    )
  })

  it('parses NDJSON stream frames', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        createNdjsonResponse([
          { stream_type: 'Start', data: { stream_id: 'stream-1' } },
          { stream_type: 'Data', data: { content: 'hello' } },
          { stream_type: 'Done', data: { total_tokens: 1 } },
        ]),
      ),
    )

    const frames = []
    for await (const frame of streamClient({ type: 'SubscribeSessionEvents' })) {
      frames.push(frame)
    }

    expect(frames).toEqual([
      { stream_type: 'start', data: { stream_id: 'stream-1' } },
      { stream_type: 'data', data: { content: 'hello' } },
      { stream_type: 'done', data: { total_tokens: 1 } },
    ])
  })
})
