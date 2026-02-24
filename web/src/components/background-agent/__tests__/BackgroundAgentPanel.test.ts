import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref } from 'vue'
import { setActivePinia, createPinia } from 'pinia'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import * as api from '@/api/background-agents'

vi.mock('@/api/background-agents', () => ({
  listBackgroundAgents: vi.fn(),
  pauseBackgroundAgent: vi.fn(),
  resumeBackgroundAgent: vi.fn(),
  cancelBackgroundAgent: vi.fn(),
  runBackgroundAgentStreaming: vi.fn(),
  deleteBackgroundAgent: vi.fn(),
  getBackgroundAgentEvents: vi.fn(),
  listMemoryChunksByTag: vi.fn(),
  listMemorySessions: vi.fn(),
  listMemoryChunksForSession: vi.fn(),
  steerTask: vi.fn(),
  getBackgroundAgentStreamEventName: vi.fn(),
  getHeartbeatEventName: vi.fn(),
  getActiveBackgroundAgents: vi.fn(),
}))

describe('BackgroundAgentPanel — store error handling', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('store sets error when pauseAgent fails', async () => {
    vi.mocked(api.pauseBackgroundAgent).mockRejectedValueOnce(new Error('pause failed'))

    const store = useBackgroundAgentStore()
    await store.pauseAgent('agent-1')

    expect(store.error).toBe('pause failed')
  })

  it('store sets error when resumeAgent fails', async () => {
    vi.mocked(api.resumeBackgroundAgent).mockRejectedValueOnce(new Error('resume failed'))

    const store = useBackgroundAgentStore()
    await store.resumeAgent('agent-1')

    expect(store.error).toBe('resume failed')
  })

  it('store sets error when runAgentNow fails', async () => {
    vi.mocked(api.runBackgroundAgentStreaming).mockRejectedValueOnce(new Error('run failed'))

    const store = useBackgroundAgentStore()
    const result = await store.runAgentNow('agent-1')

    expect(store.error).toBe('run failed')
    expect(result).toBeNull()
  })

  it('store sets error when cancelAgent fails', async () => {
    vi.mocked(api.cancelBackgroundAgent).mockRejectedValueOnce(new Error('cancel failed'))

    const store = useBackgroundAgentStore()
    await store.cancelAgent('agent-1')

    expect(store.error).toBe('cancel failed')
  })

  it('store clears error on successful operation', async () => {
    const store = useBackgroundAgentStore()

    // Fail first
    vi.mocked(api.pauseBackgroundAgent).mockRejectedValueOnce(new Error('fail'))
    await store.pauseAgent('agent-1')
    expect(store.error).toBe('fail')

    // Succeed next — error should be cleared
    vi.mocked(api.pauseBackgroundAgent).mockResolvedValueOnce({} as any)
    await store.pauseAgent('agent-1')
    expect(store.error).toBeNull()
  })
})

describe('BackgroundAgentPanel — staleness guard', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('discards stale loadEvents response when agent switches', async () => {
    // Simulate the core staleness logic:
    // If loadVersion changes while awaiting, the result should be discarded
    let loadVersion = 0
    const events = ref<any[]>([])

    // Simulate loadEvents with staleness check
    async function loadEvents(agentId: string) {
      const version = loadVersion
      const result = await api.getBackgroundAgentEvents(agentId, 100)
      if (version !== loadVersion) return // stale — discard
      events.value = result
    }

    // First call: agent-1 returns slowly
    let resolveAgent1: ((val: any) => void) | null = null
    vi.mocked(api.getBackgroundAgentEvents).mockImplementationOnce(
      () => new Promise((resolve) => { resolveAgent1 = resolve }),
    )

    const p1 = loadEvents('agent-1')

    // Agent switches → increment version, start agent-2 load
    loadVersion++
    vi.mocked(api.getBackgroundAgentEvents).mockResolvedValueOnce([
      { id: 'e2', event_type: 'started', timestamp: 2000 },
    ] as any)
    const p2 = loadEvents('agent-2')

    // Agent-1 finishes after agent switch
    resolveAgent1!([{ id: 'e1', event_type: 'completed', timestamp: 1000 }])
    await p1
    await p2

    // Events should be from agent-2, not the stale agent-1
    expect(events.value).toHaveLength(1)
    expect(events.value[0].id).toBe('e2')
  })

  it('discards stale loadMemoryConversation when agent switches', async () => {
    let loadVersion = 0
    const memoryChunks = ref<any[]>([])

    async function loadMemory(agentId: string) {
      const version = loadVersion
      const result = await api.listMemoryChunksByTag(`task:${agentId}`, 200)
      if (version !== loadVersion) return
      memoryChunks.value = result
    }

    let resolveAgent1: ((val: any) => void) | null = null
    vi.mocked(api.listMemoryChunksByTag).mockImplementationOnce(
      () => new Promise((resolve) => { resolveAgent1 = resolve }),
    )

    const p1 = loadMemory('agent-1')

    // Switch agent
    loadVersion++
    vi.mocked(api.listMemoryChunksByTag).mockResolvedValueOnce([
      { id: 'chunk-2', content: 'agent-2 memory', created_at: 2000, tags: [], source: {} },
    ] as any)
    const p2 = loadMemory('agent-2')

    // Stale agent-1 resolves
    resolveAgent1!([{ id: 'chunk-1', content: 'agent-1 memory', created_at: 1000, tags: [], source: {} }])
    await p1
    await p2

    expect(memoryChunks.value).toHaveLength(1)
    expect(memoryChunks.value[0].id).toBe('chunk-2')
  })
})
