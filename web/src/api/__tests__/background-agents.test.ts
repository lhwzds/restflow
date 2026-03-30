import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  convertSessionToBackgroundAgent,
  getBackgroundAgent,
  listMemoryChunksByTag,
  listMemoryChunksForSession,
  listMemorySessions,
} from '../background-agents'
import { BackendError, fetchJson, requestOptional, requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  BackendError: class BackendError extends Error {
    code: number
    kind: string
    details: unknown

    constructor(payload: { code: number; kind: string; message: string; details?: unknown }) {
      super(payload.message)
      this.name = 'BackendError'
      this.code = payload.code
      this.kind = payload.kind
      this.details = payload.details ?? null
    }
  },
  fetchJson: vi.fn(),
  requestOptional: vi.fn(),
  requestTyped: vi.fn(),
}))

describe('background-agents memory API', () => {
  beforeEach(() => {
    vi.mocked(fetchJson).mockReset()
    vi.mocked(requestTyped).mockReset()
    vi.mocked(requestOptional).mockReset()
  })

  it('calls get background agent through optional daemon request', async () => {
    vi.mocked(requestOptional).mockResolvedValueOnce(null)

    const result = await getBackgroundAgent('task-1')

    expect(requestOptional).toHaveBeenCalledWith({
      type: 'GetBackgroundAgent',
      data: { id: 'task-1' },
    })
    expect(result).toBeNull()
  })

  it('calls list memory sessions with agent_id', async () => {
    vi.mocked(requestTyped).mockResolvedValueOnce([])

    await listMemorySessions('agent-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListMemorySessions',
      data: { agent_id: 'agent-1' },
    })
  })

  it('calls list memory chunks for session', async () => {
    vi.mocked(requestTyped).mockResolvedValueOnce([])

    await listMemoryChunksForSession('session-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListMemoryBySession',
      data: { session_id: 'session-1' },
    })
  })

  it('returns sliced chunks from list memory by tag', async () => {
    vi.mocked(requestTyped).mockResolvedValueOnce([{ id: 'chunk-1' }, { id: 'chunk-2' }])

    const chunks = await listMemoryChunksByTag('task:task-1', 1)

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListMemory',
      data: { agent_id: null, tag: 'task:task-1' },
    })
    expect(chunks).toEqual([{ id: 'chunk-1' }])
  })

  it('unwraps executed convert-session outcomes to the created task', async () => {
    vi.mocked(fetchJson).mockResolvedValueOnce({
      status: 'executed',
      result: {
        task: { id: 'bg-1' },
        source_session_id: 'session-1',
        source_session_agent_id: 'default',
        run_now: false,
      },
    })

    const result = await convertSessionToBackgroundAgent({
      session_id: 'session-1',
      name: 'Background Session',
      run_now: false,
    })

    expect(fetchJson).toHaveBeenCalledWith('/api/background-agents/convert-session', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        session_id: 'session-1',
        name: 'Background Session',
        run_now: false,
      }),
    })
    expect(result).toEqual({ id: 'bg-1' })
  })

  it('maps confirmation-required convert-session outcomes into BackendError', async () => {
    vi.mocked(fetchJson).mockResolvedValueOnce({
      status: 'confirmation_required',
      assessment: {
        operation: 'convert_session',
        intent: 'save',
        status: 'warning',
        warnings: [{ code: 'confirm', message: 'Credential missing.' }],
        blockers: [],
        requires_confirmation: true,
        confirmation_token: 'token-1',
      },
    })

    const request = convertSessionToBackgroundAgent({
      session_id: 'session-1',
    })

    await expect(request).rejects.toMatchObject({
      code: 428,
      kind: 'conflict',
      details: {
        assessment: expect.objectContaining({
          confirmation_token: 'token-1',
        }),
      },
    })
    await expect(request).rejects.toBeInstanceOf(BackendError)
  })
})
