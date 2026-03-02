import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import MemorySection from '../MemorySection.vue'
import {
  exportMemoryMarkdown,
  getMemoryStats,
  listMemoryChunksForSession,
  listMemorySessions,
  searchMemory,
} from '@/api/memory'

vi.mock('@/api/memory', () => ({
  exportMemoryMarkdown: vi.fn(),
  getMemoryStats: vi.fn(),
  listMemoryChunksForSession: vi.fn(),
  listMemorySessions: vi.fn(),
  searchMemory: vi.fn(),
}))

const mockedGetMemoryStats = vi.mocked(getMemoryStats)
const mockedListMemorySessions = vi.mocked(listMemorySessions)
const mockedListChunksForSession = vi.mocked(listMemoryChunksForSession)
const mockedSearchMemory = vi.mocked(searchMemory)
const mockedExportMemory = vi.mocked(exportMemoryMarkdown)

function mountComponent() {
  return mount(MemorySection, {
    global: {
      stubs: {
        Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        Input: {
          template:
            '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue'],
        },
        Textarea: { template: '<textarea />' },
      },
    },
  })
}

describe('MemorySection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockedGetMemoryStats.mockResolvedValue({
      agent_id: 'default',
      session_count: 1,
      chunk_count: 2,
      total_tokens: 128,
      oldest_memory: null,
      newest_memory: null,
    })
    mockedListMemorySessions.mockResolvedValue([
      {
        id: 'session-1',
        agent_id: 'default',
        name: 'Session 1',
        description: null,
        chunk_count: 2,
        total_tokens: 128,
        created_at: 1000,
        updated_at: 1000,
        tags: [],
      },
    ])
    mockedListChunksForSession.mockResolvedValue([
      {
        id: 'chunk-1',
        agent_id: 'default',
        session_id: 'session-1',
        content: 'chunk content',
        content_hash: 'hash',
        source: { type: 'manual_note' },
        created_at: 1000,
        tags: [],
        token_count: 10,
      },
    ])
    mockedSearchMemory.mockResolvedValue({
      chunks: [
        {
          chunk: {
            id: 'chunk-1',
            agent_id: 'default',
            session_id: 'session-1',
            content: 'search hit',
            content_hash: 'hash',
            source: { type: 'manual_note' },
            created_at: 1000,
            tags: [],
            token_count: 10,
          },
          score: 0.9,
          breakdown: { text_score: 0.9, tag_score: 0, recency_score: 0 },
        },
      ],
      total_count: 1,
      has_more: false,
    })
    mockedExportMemory.mockResolvedValue({
      markdown: '# Export',
      chunk_count: 1,
      session_count: 1,
      agent_id: 'default',
      suggested_filename: 'default.md',
    })
  })

  it('loads memory overview on mount', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    expect(mockedGetMemoryStats).toHaveBeenCalledWith('default')
    expect(mockedListMemorySessions).toHaveBeenCalledWith('default')
    expect(wrapper.text()).toContain('sessions:')
  })

  it('runs search and export actions', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const searchButton = wrapper.findAll('button').find((button) => button.text() === 'Search')
    const exportButton = wrapper.findAll('button').find((button) => button.text() === 'Export Markdown')

    expect(searchButton).toBeDefined()
    expect(exportButton).toBeDefined()

    await searchButton!.trigger('click')
    await exportButton!.trigger('click')
    await flushPromises()

    expect(mockedSearchMemory).toHaveBeenCalled()
    expect(mockedExportMemory).toHaveBeenCalledWith('default')
  })
})
