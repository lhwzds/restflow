import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  listMemoryChunksByTag,
  listMemoryChunksForSession,
  listMemorySessions,
} from '../background-agents'
import { invokeCommand } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  invokeCommand: vi.fn(),
}))

describe('background-agents memory API', () => {
  beforeEach(() => {
    vi.mocked(invokeCommand).mockReset()
  })

  it('calls list_memory_sessions with agent_id', async () => {
    vi.mocked(invokeCommand).mockResolvedValueOnce([])

    await listMemorySessions('agent-1')

    expect(invokeCommand).toHaveBeenCalledWith('listMemorySessions', 'agent-1')
  })

  it('calls list_memory_chunks_for_session with session_id', async () => {
    vi.mocked(invokeCommand).mockResolvedValueOnce([])

    await listMemoryChunksForSession('session-1')

    expect(invokeCommand).toHaveBeenCalledWith('listMemoryChunksForSession', 'session-1')
  })

  it('returns items from list_memory_chunks_by_tag', async () => {
    vi.mocked(invokeCommand).mockResolvedValueOnce({
      items: [{ id: 'chunk-1' }],
      total: 1,
    })

    const chunks = await listMemoryChunksByTag('task:task-1', 50)

    expect(invokeCommand).toHaveBeenCalledWith('listMemoryChunksByTag', 'task:task-1', 50)
    expect(chunks).toHaveLength(1)
  })
})
