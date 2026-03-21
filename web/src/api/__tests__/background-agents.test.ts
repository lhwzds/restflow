import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  getBackgroundAgent,
  listMemoryChunksByTag,
  listMemoryChunksForSession,
  listMemorySessions,
} from '../background-agents'
import { requestOptional, requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  fetchJson: vi.fn(),
  requestOptional: vi.fn(),
  requestTyped: vi.fn(),
}))

describe('background-agents memory API', () => {
  beforeEach(() => {
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
})
