import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  listMemoryChunksByTag,
  listMemoryChunksForSession,
  listMemorySessions,
} from '../background-agents'
import { tauriInvoke } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  tauriInvoke: vi.fn(),
}))

describe('background-agents memory API', () => {
  beforeEach(() => {
    vi.mocked(tauriInvoke).mockReset()
  })

  it('calls list_memory_sessions with agent_id', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce([])

    await listMemorySessions('agent-1')

    expect(tauriInvoke).toHaveBeenCalledWith('list_memory_sessions', { agent_id: 'agent-1' })
  })

  it('calls list_memory_chunks_for_session with session_id', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce([])

    await listMemoryChunksForSession('session-1')

    expect(tauriInvoke).toHaveBeenCalledWith('list_memory_chunks_for_session', {
      session_id: 'session-1',
    })
  })

  it('returns items from list_memory_chunks_by_tag', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce({
      items: [{ id: 'chunk-1' }],
      total: 1,
    })

    const chunks = await listMemoryChunksByTag('task:task-1', 50)

    expect(tauriInvoke).toHaveBeenCalledWith('list_memory_chunks_by_tag', {
      tag: 'task:task-1',
      limit: 50,
    })
    expect(chunks).toHaveLength(1)
  })
})
