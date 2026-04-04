import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as legacyMemoryApi from '../background-agent-memory'
import * as memoryApi from '../memory'
import { requestOptional, requestTyped } from '../http-client'
import type { MemorySearchQuery } from '@/types/generated'

vi.mock('../http-client', () => ({
  requestOptional: vi.fn(),
  requestTyped: vi.fn(),
}))

const mockedRequestTyped = vi.mocked(requestTyped)
const mockedRequestOptional = vi.mocked(requestOptional)

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
    mockedRequestTyped.mockResolvedValueOnce({ chunks: [], total_count: 0, has_more: false })

    const result = await memoryApi.searchMemory(defaultQuery)

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'SearchMemoryRanked',
      data: {
        query: defaultQuery,
        min_score: null,
        scoring_preset: null,
      },
    })
    expect(result.total_count).toBe(0)
  })

  it('searches memory with advanced request', async () => {
    mockedRequestTyped.mockResolvedValueOnce({ chunks: [], total_count: 0, has_more: false })

    await memoryApi.searchMemoryAdvanced({
      query: defaultQuery,
      min_score: 12,
      scoring_preset: 'balanced',
    })

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'SearchMemoryRanked',
      data: {
        query: defaultQuery,
        min_score: 12,
        scoring_preset: 'balanced',
      },
    })
  })

  it('gets one chunk', async () => {
    mockedRequestOptional.mockResolvedValueOnce({ id: 'chunk-1', content: 'note' })

    const result = await memoryApi.getMemoryChunk('chunk-1')

    expect(mockedRequestOptional).toHaveBeenCalledWith({
      type: 'GetMemoryChunk',
      data: { id: 'chunk-1' },
    })
    expect(result).toEqual(expect.objectContaining({ id: 'chunk-1' }))
  })

  it('lists chunks by agent', async () => {
    mockedRequestTyped.mockResolvedValueOnce([{ id: 'chunk-1' }, { id: 'chunk-2' }])

    const result = await memoryApi.listMemoryChunks('agent-1', 1, 1)

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'ListMemory',
      data: { agent_id: 'agent-1', tag: null },
    })
    expect(result).toEqual({ items: [{ id: 'chunk-2' }], total: 2 })
  })

  it('lists chunks by tag', async () => {
    mockedRequestTyped.mockResolvedValueOnce([{ id: 'chunk-1' }])

    const result = await memoryApi.listMemoryChunksByTag('task:abc', 50)

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'ListMemory',
      data: { agent_id: null, tag: 'task:abc' },
    })
    expect(result.total).toBe(1)
  })

  it('lists session chunks', async () => {
    mockedRequestTyped.mockResolvedValueOnce([{ id: 'chunk-1' }])

    const result = await memoryApi.listMemoryChunksForSession('session-1')

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'ListMemoryBySession',
      data: { session_id: 'session-1' },
    })
    expect(result).toHaveLength(1)
  })

  it('creates and deletes chunk', async () => {
    mockedRequestTyped
      .mockResolvedValueOnce({ id: 'chunk-1', content: 'manual' })
      .mockResolvedValueOnce({ deleted: true })

    const created = await memoryApi.createMemoryChunk({
      agent_id: 'agent-1',
      content: 'manual note',
      session_id: 'session-1',
      tags: ['manual'],
    })
    const deleted = await memoryApi.deleteMemoryChunk('chunk-1')

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, {
      type: 'CreateMemoryChunk',
      data: {
        chunk: {
          agent_id: 'agent-1',
          content: 'manual note',
          session_id: 'session-1',
          tags: ['manual'],
        },
      },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(2, {
      type: 'DeleteMemory',
      data: { id: 'chunk-1' },
    })
    expect(created).toEqual(expect.objectContaining({ id: 'chunk-1' }))
    expect(deleted).toBe(true)
  })

  it('lists and manages sessions', async () => {
    mockedRequestTyped.mockResolvedValueOnce([{ id: 'session-1' }])
    mockedRequestOptional.mockResolvedValueOnce({ id: 'session-1' })
    mockedRequestTyped
      .mockResolvedValueOnce({ id: 'session-2' })
      .mockResolvedValueOnce({ deleted: true })

    const sessions = await memoryApi.listMemorySessions('agent-1')
    const one = await memoryApi.getMemorySession('session-1')
    const created = await memoryApi.createMemorySession({
      agent_id: 'agent-1',
      name: 'Research',
      description: null,
      tags: ['research'],
    })
    const deleted = await memoryApi.deleteMemorySession('session-2', false)

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, {
      type: 'ListMemorySessions',
      data: { agent_id: 'agent-1' },
    })
    expect(mockedRequestOptional).toHaveBeenCalledWith({
      type: 'GetMemorySession',
      data: { session_id: 'session-1' },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(2, {
      type: 'CreateMemorySession',
      data: {
        session: {
          agent_id: 'agent-1',
          name: 'Research',
          description: null,
          tags: ['research'],
        },
      },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(3, {
      type: 'DeleteMemorySession',
      data: {
        session_id: 'session-2',
        delete_chunks: false,
      },
    })
    expect(sessions).toHaveLength(1)
    expect(one).toEqual(expect.objectContaining({ id: 'session-1' }))
    expect(created).toEqual(expect.objectContaining({ id: 'session-2' }))
    expect(deleted).toBe(true)
  })

  it('gets stats and exports memory', async () => {
    mockedRequestTyped
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

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, {
      type: 'GetMemoryStats',
      data: { agent_id: 'agent-1' },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(2, {
      type: 'ExportMemory',
      data: { agent_id: 'agent-1' },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(3, {
      type: 'ExportMemorySession',
      data: { session_id: 'session-1' },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(4, {
      type: 'ExportMemoryAdvanced',
      data: {
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

  it('builds task memory tag and fetches task memory', async () => {
    mockedRequestTyped.mockResolvedValueOnce([{ id: 'chunk-1' }])

    const tag = memoryApi.getTaskMemoryTag('task-1')
    const result = await memoryApi.listTaskMemory('task-1', 5)

    expect(tag).toBe('task:task-1')
    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'ListMemory',
      data: { agent_id: null, tag: 'task:task-1' },
    })
    expect(result.total).toBe(1)
  })

  it('keeps background memory helpers as compatibility aliases', async () => {
    mockedRequestTyped.mockResolvedValueOnce([{ id: 'chunk-1' }])

    const tag = legacyMemoryApi.getBackgroundAgentMemoryTag('task-2')
    const result = await legacyMemoryApi.listBackgroundAgentMemory('task-2', 3)

    expect(tag).toBe('task:task-2')
    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'ListMemory',
      data: { agent_id: null, tag: 'task:task-2' },
    })
    expect(result).toEqual({ items: [{ id: 'chunk-1' }], total: 1 })
  })
})
