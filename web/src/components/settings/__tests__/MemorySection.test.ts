import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import MemorySection from '../MemorySection.vue'
import {
  deleteMemorySession,
  exportMemoryMarkdown,
  getMemoryStats,
  listMemoryChunksForSession,
  listMemorySessions,
  searchMemory,
} from '@/api/memory'

const confirmMock = vi.fn()
const toastSuccessMock = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({
    confirm: confirmMock,
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: toastSuccessMock,
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
    loading: vi.fn(),
    dismiss: vi.fn(),
  }),
}))

vi.mock('@/api/memory', () => ({
  deleteMemorySession: vi.fn(),
  exportMemoryMarkdown: vi.fn(),
  getMemoryStats: vi.fn(),
  listMemoryChunksForSession: vi.fn(),
  listMemorySessions: vi.fn(),
  searchMemory: vi.fn(),
}))

const mockedDeleteSession = vi.mocked(deleteMemorySession)
const mockedGetMemoryStats = vi.mocked(getMemoryStats)
const mockedListMemorySessions = vi.mocked(listMemorySessions)
const mockedListChunksForSession = vi.mocked(listMemoryChunksForSession)
const mockedSearchMemory = vi.mocked(searchMemory)
const mockedExportMemory = vi.mocked(exportMemoryMarkdown)

function mountComponent() {
  return mount(MemorySection, {
    global: {
      stubs: {
        Loader2: { template: '<div />' },
        Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        Input: {
          template:
            '<input :id="$attrs.id" :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue'],
        },
        Label: { template: '<label><slot /></label>' },
        Select: { template: '<div><slot /></div>' },
        SelectContent: { template: '<div><slot /></div>' },
        SelectItem: { template: '<div><slot /></div>' },
        SelectTrigger: { template: '<div><slot /></div>' },
        SelectValue: { template: '<span />' },
        Textarea: { template: '<textarea />' },
      },
    },
  })
}

describe('MemorySection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    confirmMock.mockResolvedValue(true)
    mockedDeleteSession.mockResolvedValue(true)
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
          match_count: 1,
          score_breakdown: { frequency_score: 0.9, tag_score: 0, recency_score: 0 },
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
    mountComponent()
    await flushPromises()

    expect(mockedGetMemoryStats).toHaveBeenCalledWith('default')
    expect(mockedListMemorySessions).toHaveBeenCalledWith('default')
    expect(mockedListChunksForSession).toHaveBeenCalledWith('session-1')
  })

  it('runs search and export actions', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const searchButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.memory.search')
    const exportButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.memory.exportMarkdown')

    expect(searchButton).toBeDefined()
    expect(exportButton).toBeDefined()

    await searchButton!.trigger('click')
    await exportButton!.trigger('click')
    await flushPromises()

    expect(mockedSearchMemory).toHaveBeenCalled()
    expect(mockedExportMemory).toHaveBeenCalledWith('default')
  })

  it('deletes selected session after confirmation', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const deleteSessionButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.memory.deleteSession')
    expect(deleteSessionButton).toBeDefined()

    await deleteSessionButton!.trigger('click')
    await flushPromises()

    expect(confirmMock).toHaveBeenCalled()
    expect(mockedDeleteSession).toHaveBeenCalledWith('session-1', true)
    expect(toastSuccessMock).toHaveBeenCalledWith('settings.memory.deleteSessionSuccess')
  })
})
