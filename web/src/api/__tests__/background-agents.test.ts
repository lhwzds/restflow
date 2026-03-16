import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  listMemoryChunksByTag,
  listMemoryChunksForSession,
  listMemorySessions,
} from '../background-agents'
import { requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  fetchJson: vi.fn(),
  requestTyped: vi.fn(),
}))

describe('background-agents memory API', () => {
  beforeEach(() => {
    vi.mocked(requestTyped).mockReset()
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
