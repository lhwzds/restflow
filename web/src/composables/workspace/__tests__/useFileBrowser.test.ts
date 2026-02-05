import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { ref, nextTick } from 'vue'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'

// Mock API modules
vi.mock('@/api/skills', () => ({
  listSkills: vi.fn(),
}))

vi.mock('@/api/agents', () => ({
  listAgents: vi.fn(),
}))

describe('useFileBrowser', () => {
  const mockSkill: Skill = {
    id: 'skill-1',
    name: 'Test Skill',
    description: 'A test skill',
    tags: ['test'],
    content: '# Test Content',
    folder_path: null,
    gating: null,
    version: null,
    author: null,
    license: null,
    content_hash: null,
    storage_mode: 'DatabaseOnly',
    is_synced: false,
    created_at: 1000,
    updated_at: 2000,
  }

  const mockAgent: StoredAgent = {
    id: 'agent-1',
    name: 'Test Agent',
    agent: {
      model: 'claude-sonnet-4-5',
      prompt: 'You are a helpful assistant',
    },
    created_at: 1000,
    updated_at: 2000,
  }

  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('loadItems', () => {
    it('should load skills when activeTab is "skills"', async () => {
      const { listSkills } = await import('@/api/skills')
      vi.mocked(listSkills).mockResolvedValue([mockSkill])

      const { useFileBrowser } = await import('../useFileBrowser')
      const activeTab = ref<'skills' | 'agents'>('skills')
      const { items, isLoading, loadItems } = useFileBrowser(activeTab)

      expect(isLoading.value).toBe(false)
      await loadItems()

      expect(listSkills).toHaveBeenCalled()
      expect(items.value).toHaveLength(1)
      expect(items.value[0]).toEqual({
        id: 'skill-1',
        name: 'Test Skill',
        path: 'skills/skill-1',
        isDirectory: false,
        updatedAt: 2000,
        data: mockSkill,
      })
    })

    it('should load agents when activeTab is "agents"', async () => {
      const { listAgents } = await import('@/api/agents')
      vi.mocked(listAgents).mockResolvedValue([mockAgent])

      const { useFileBrowser } = await import('../useFileBrowser')
      const activeTab = ref<'skills' | 'agents'>('agents')
      const { items, loadItems } = useFileBrowser(activeTab)

      await loadItems()

      expect(listAgents).toHaveBeenCalled()
      expect(items.value).toHaveLength(1)
      expect(items.value[0]).toEqual({
        id: 'agent-1',
        name: 'Test Agent',
        path: 'agents/agent-1',
        isDirectory: false,
        updatedAt: 2000,
        data: mockAgent,
      })
    })

    it('should set loading state during fetch', async () => {
      const { listSkills } = await import('@/api/skills')

      let resolvePromise: (value: Skill[]) => void
      const promise = new Promise<Skill[]>((resolve) => {
        resolvePromise = resolve
      })
      vi.mocked(listSkills).mockReturnValue(promise)

      const { useFileBrowser } = await import('../useFileBrowser')
      const activeTab = ref<'skills' | 'agents'>('skills')
      const { isLoading, loadItems } = useFileBrowser(activeTab)

      const loadPromise = loadItems()
      expect(isLoading.value).toBe(true)

      resolvePromise!([mockSkill])
      await loadPromise

      expect(isLoading.value).toBe(false)
    })

    it('should handle errors gracefully', async () => {
      const { listSkills } = await import('@/api/skills')
      vi.mocked(listSkills).mockRejectedValue(new Error('Network error'))

      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {})

      const { useFileBrowser } = await import('../useFileBrowser')
      const activeTab = ref<'skills' | 'agents'>('skills')
      const { items, error, loadItems } = useFileBrowser(activeTab)

      await loadItems()

      expect(error.value).toBe('Network error')
      expect(items.value).toEqual([])
      expect(consoleError).toHaveBeenCalled()

      consoleError.mockRestore()
    })
  })

  describe('tab switching', () => {
    it('should reload items when activeTab changes', async () => {
      const { listSkills } = await import('@/api/skills')
      const { listAgents } = await import('@/api/agents')
      vi.mocked(listSkills).mockResolvedValue([mockSkill])
      vi.mocked(listAgents).mockResolvedValue([mockAgent])

      const { useFileBrowser } = await import('../useFileBrowser')
      const activeTab = ref<'skills' | 'agents'>('skills')
      useFileBrowser(activeTab)

      // Change tab
      activeTab.value = 'agents'
      await nextTick()
      // Wait for the async loadItems to complete
      await new Promise((resolve) => setTimeout(resolve, 0))

      expect(listAgents).toHaveBeenCalled()
    })
  })
})
