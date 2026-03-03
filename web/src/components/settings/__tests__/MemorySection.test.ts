import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import MemorySection from '../MemorySection.vue'
import {
  deleteMemoryChunk,
  deleteMemoryChunksForAgent,
  deleteMemoryChunksForAgentTag,
  deleteMemorySession,
  exportMemoryAdvanced,
  exportMemoryMarkdown,
  getMemoryStats,
  supportsDeleteMemoryChunksForAgentTag,
  supportsExportMemoryAdvanced,
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
  deleteMemoryChunk: vi.fn(),
  deleteMemoryChunksForAgent: vi.fn(),
  deleteMemoryChunksForAgentTag: vi.fn(),
  deleteMemorySession: vi.fn(),
  exportMemoryAdvanced: vi.fn(),
  exportMemoryMarkdown: vi.fn(),
  getMemoryStats: vi.fn(),
  isUnsupportedMemoryOperationError: vi.fn(() => false),
  listMemoryChunksForSession: vi.fn(),
  listMemorySessions: vi.fn(),
  searchMemory: vi.fn(),
  supportsDeleteMemoryChunksForAgentTag: vi.fn(),
  supportsExportMemoryAdvanced: vi.fn(),
}))

const mockedDeleteChunk = vi.mocked(deleteMemoryChunk)
const mockedDeleteChunksForAgent = vi.mocked(deleteMemoryChunksForAgent)
const mockedDeleteChunksForAgentTag = vi.mocked(deleteMemoryChunksForAgentTag)
const mockedDeleteSession = vi.mocked(deleteMemorySession)
const mockedExportAdvanced = vi.mocked(exportMemoryAdvanced)
const mockedGetMemoryStats = vi.mocked(getMemoryStats)
const mockedListMemorySessions = vi.mocked(listMemorySessions)
const mockedListChunksForSession = vi.mocked(listMemoryChunksForSession)
const mockedSearchMemory = vi.mocked(searchMemory)
const mockedExportMemory = vi.mocked(exportMemoryMarkdown)
const mockedSupportsDeleteByTag = vi.mocked(supportsDeleteMemoryChunksForAgentTag)
const mockedSupportsAdvancedExport = vi.mocked(supportsExportMemoryAdvanced)

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
    mockedDeleteChunk.mockResolvedValue(true)
    mockedDeleteChunksForAgent.mockResolvedValue(2)
    mockedDeleteChunksForAgentTag.mockResolvedValue(1)
    mockedDeleteSession.mockResolvedValue(true)
    mockedExportAdvanced.mockResolvedValue({
      markdown: '# Advanced Export',
      chunk_count: 1,
      session_count: 1,
      agent_id: 'default',
      suggested_filename: 'default-advanced.md',
    })
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
    mockedSupportsDeleteByTag.mockResolvedValue(false)
    mockedSupportsAdvancedExport.mockResolvedValue(true)
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

  it('deletes one chunk after confirmation', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const deleteChunkButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.memory.deleteChunk')
    expect(deleteChunkButton).toBeDefined()

    await deleteChunkButton!.trigger('click')
    await flushPromises()

    expect(confirmMock).toHaveBeenCalled()
    expect(mockedDeleteChunk).toHaveBeenCalledWith('chunk-1')
  })

  it('clears agent chunks', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const clearButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.memory.clearAgentChunks')
    expect(clearButton).toBeDefined()

    await clearButton!.trigger('click')
    await flushPromises()

    expect(mockedDeleteChunksForAgent).toHaveBeenCalledWith('default')
  })

  it('clears agent chunks by tag when backend supports it', async () => {
    mockedSupportsDeleteByTag.mockResolvedValue(true)
    const wrapper = mountComponent()
    await flushPromises()

    const tagInput = wrapper.find('#memory-clear-tag')
    expect(tagInput.exists()).toBe(true)
    await tagInput.setValue('project-a')

    const clearByTagButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.memory.clearAgentChunksByTag')
    expect(clearByTagButton).toBeDefined()

    await clearByTagButton!.trigger('click')
    await flushPromises()

    expect(mockedDeleteChunksForAgentTag).toHaveBeenCalledWith('default', 'project-a')
  })

  it('hides advanced export action when backend does not support it', async () => {
    mockedSupportsAdvancedExport.mockResolvedValue(false)
    const wrapper = mountComponent()
    await flushPromises()

    const advancedExportButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.memory.exportAdvanced')
    expect(advancedExportButton).toBeUndefined()
  })
})
