import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  convertSessionToBackgroundAgent,
  deleteBackgroundAgent,
  getBackgroundAgent,
  listMemoryChunksByTag,
  listMemoryChunksForSession,
  listMemorySessions,
  runBackgroundAgentStreaming,
} from '../background-agents'
import { fetchJson, requestOptional, requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
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

  it('returns the canonical convert-session result payload', async () => {
    const payload = {
      task: { id: 'bg-1' },
      source_session_id: 'session-1',
      source_session_agent_id: 'default',
      run_now: false,
    }
    vi.mocked(fetchJson).mockResolvedValueOnce(payload)

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
    expect(result).toEqual(payload)
  })

  it('returns the canonical run-now agent payload', async () => {
    const payload = { id: 'bg-1', status: 'running' }
    vi.mocked(requestTyped).mockResolvedValueOnce(payload)

    const result = await runBackgroundAgentStreaming('bg-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ControlBackgroundAgent',
      data: { id: 'bg-1', action: 'run_now' },
    })
    expect(result).toEqual(payload)
  })

  it('returns the canonical delete result payload', async () => {
    const payload = {
      id: 'bg-1',
      deleted: true,
    }
    vi.mocked(requestTyped).mockResolvedValueOnce(payload)

    const result = await deleteBackgroundAgent('bg-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'DeleteBackgroundAgent',
      data: { id: 'bg-1' },
    })
    expect(result).toEqual(payload)
  })
})
