import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as memoryApi from '../memory'
import { invokeCommand, tauriInvoke } from '../tauri-client'
import type { MemorySearchQuery } from '@/types/generated'

vi.mock('../tauri-client', () => ({
  invokeCommand: vi.fn(),
  tauriInvoke: vi.fn(),
}))

const mockedInvokeCommand = vi.mocked(invokeCommand)
const mockedTauriInvoke = vi.mocked(tauriInvoke)

const defaultQuery: MemorySearchQuery = {
  agent_id: 'agent-1',
  query: 'rust async',
  search_mode: 'keyword',
  session_id: null,
  tags: [],
  source_type: null,
  from_time: null,
  to_time: null,
  limit: 20,
  offset: 0,
}

describe('Memory API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('searches memory', async () => {
    mockedTauriInvoke.mockResolvedValueOnce({ chunks: [], total_count: 0, has_more: false })

    const result = await memoryApi.searchMemory(defaultQuery)

    expect(mockedTauriInvoke).toHaveBeenCalledWith('search_memory', {
      query: defaultQuery,
    })
    expect(result.total_count).toBe(0)
  })

  it('searches memory with advanced request', async () => {
    mockedTauriInvoke.mockResolvedValueOnce({ chunks: [], total_count: 0, has_more: false })

    await memoryApi.searchMemoryAdvanced({
      query: defaultQuery,
      min_score: 12,
      scoring_preset: 'balanced',
    })

    expect(mockedTauriInvoke).toHaveBeenCalledWith('search_memory_advanced', {
      request: {
        query: defaultQuery,
        min_score: 12,
        scoring_preset: 'balanced',
      },
    })
  })

  it('gets one chunk', async () => {
    mockedTauriInvoke.mockResolvedValueOnce({ id: 'chunk-1', content: 'note' })

    const result = await memoryApi.getMemoryChunk('chunk-1')

    expect(mockedTauriInvoke).toHaveBeenCalledWith('get_memory_chunk', { chunkId: 'chunk-1' })
    expect(result).toEqual(expect.objectContaining({ id: 'chunk-1' }))
  })

  it('lists chunks by agent', async () => {
    mockedTauriInvoke.mockResolvedValueOnce({ items: [], total: 0 })

    const result = await memoryApi.listMemoryChunks('agent-1', 10, 5)

    expect(mockedTauriInvoke).toHaveBeenCalledWith('list_memory_chunks', {
      agentId: 'agent-1',
      limit: 10,
      offset: 5,
    })
    expect(result.total).toBe(0)
  })

  it('lists chunks by tag via generated binding command', async () => {
    mockedInvokeCommand.mockResolvedValueOnce({ items: [{ id: 'chunk-1' }], total: 1 })

    const result = await memoryApi.listMemoryChunksByTag('task:abc', 50)

    expect(mockedInvokeCommand).toHaveBeenCalledWith('listMemoryChunksByTag', 'task:abc', 50)
    expect(result.total).toBe(1)
  })

  it('lists session chunks via generated binding command', async () => {
    mockedInvokeCommand.mockResolvedValueOnce([{ id: 'chunk-1' }])

    const result = await memoryApi.listMemoryChunksForSession('session-1')

    expect(mockedInvokeCommand).toHaveBeenCalledWith('listMemoryChunksForSession', 'session-1')
    expect(result).toHaveLength(1)
  })

  it('creates and deletes chunk', async () => {
    mockedTauriInvoke
      .mockResolvedValueOnce({ id: 'chunk-1', content: 'manual' })
      .mockResolvedValueOnce(true)

    const created = await memoryApi.createMemoryChunk({
      agent_id: 'agent-1',
      content: 'manual note',
      session_id: 'session-1',
      tags: ['manual'],
    })
    const deleted = await memoryApi.deleteMemoryChunk('chunk-1')

    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(1, 'create_memory_chunk', {
      request: {
        agent_id: 'agent-1',
        content: 'manual note',
        session_id: 'session-1',
        tags: ['manual'],
      },
    })
    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(2, 'delete_memory_chunk', {
      chunkId: 'chunk-1',
    })
    expect(created).toEqual(expect.objectContaining({ id: 'chunk-1' }))
    expect(deleted).toBe(true)
  })

  it('lists and manages sessions', async () => {
    mockedInvokeCommand.mockResolvedValueOnce([{ id: 'session-1' }])
    mockedTauriInvoke
      .mockResolvedValueOnce({ id: 'session-1' })
      .mockResolvedValueOnce({ id: 'session-2' })
      .mockResolvedValueOnce(true)

    const sessions = await memoryApi.listMemorySessions('agent-1')
    const one = await memoryApi.getMemorySession('session-1')
    const created = await memoryApi.createMemorySession({
      agent_id: 'agent-1',
      name: 'Research',
      description: null,
      tags: ['research'],
    })
    const deleted = await memoryApi.deleteMemorySession('session-2', false)

    expect(mockedInvokeCommand).toHaveBeenCalledWith('listMemorySessions', 'agent-1')
    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(1, 'get_memory_session', {
      sessionId: 'session-1',
    })
    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(2, 'create_memory_session', {
      request: {
        agent_id: 'agent-1',
        name: 'Research',
        description: null,
        tags: ['research'],
      },
    })
    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(3, 'delete_memory_session', {
      sessionId: 'session-2',
      deleteChunks: false,
    })
    expect(sessions).toHaveLength(1)
    expect(one).toEqual(expect.objectContaining({ id: 'session-1' }))
    expect(created).toEqual(expect.objectContaining({ id: 'session-2' }))
    expect(deleted).toBe(true)
  })

  it('gets stats and exports memory', async () => {
    mockedTauriInvoke
      .mockResolvedValueOnce({ total_chunks: 10 })
      .mockResolvedValueOnce({ markdown: '# Agent Export' })
      .mockResolvedValueOnce({ markdown: '# Session Export' })
      .mockResolvedValueOnce({ markdown: '# Advanced Export' })

    const stats = await memoryApi.getMemoryStats('agent-1')
    const all = await memoryApi.exportMemoryMarkdown('agent-1')
    const one = await memoryApi.exportMemorySessionMarkdown('session-1')
    const advanced = await memoryApi.exportMemoryAdvanced({
      agent_id: 'agent-1',
      session_id: null,
      preset: 'compact',
      include_metadata: false,
      include_timestamps: true,
      include_source: false,
      include_tags: true,
    })

    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(1, 'get_memory_stats', {
      agentId: 'agent-1',
    })
    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(2, 'export_memory_markdown', {
      agentId: 'agent-1',
    })
    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(3, 'export_memory_session_markdown', {
      sessionId: 'session-1',
    })
    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(4, 'export_memory_advanced', {
      request: {
        agent_id: 'agent-1',
        session_id: null,
        preset: 'compact',
        include_metadata: false,
        include_timestamps: true,
        include_source: false,
        include_tags: true,
      },
    })
    expect(stats).toEqual(expect.objectContaining({ total_chunks: 10 }))
    expect(all).toEqual(expect.objectContaining({ markdown: '# Agent Export' }))
    expect(one).toEqual(expect.objectContaining({ markdown: '# Session Export' }))
    expect(advanced).toEqual(expect.objectContaining({ markdown: '# Advanced Export' }))
  })

  it('builds background memory tag and fetches task memory', async () => {
    mockedInvokeCommand.mockResolvedValueOnce({ items: [{ id: 'chunk-1' }], total: 1 })

    const tag = memoryApi.getBackgroundAgentMemoryTag('task-1')
    const result = await memoryApi.listBackgroundAgentMemory('task-1', 5)

    expect(tag).toBe('task:task-1')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('listMemoryChunksByTag', 'task:task-1', 5)
    expect(result.total).toBe(1)
  })
})
