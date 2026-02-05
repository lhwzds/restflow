import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { Skill } from '@/types/generated/Skill'

// Mock modules before importing the module under test
vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(),
  tauriInvoke: vi.fn(),
}))

vi.mock('../config', async () => {
  const actual = await vi.importActual('../config')
  return {
    ...actual,
    isTauri: vi.fn(),
    tauriInvoke: vi.fn(),
    apiClient: {
      get: vi.fn(),
      post: vi.fn(),
      put: vi.fn(),
      delete: vi.fn(),
    },
  }
})

describe('skills API', () => {
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

  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('listSkills', () => {
    it('should use Tauri invoke when in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue([mockSkill])

      const { listSkills } = await import('../skills')
      const result = await listSkills()

      expect(isTauri).toHaveBeenCalled()
      expect(tauriInvoke).toHaveBeenCalledWith('list_skills')
      expect(result).toEqual([mockSkill])
    })

    it('should use REST API when not in Tauri mode', async () => {
      const { isTauri, apiClient } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)
      vi.mocked(apiClient.get).mockResolvedValue({ data: [mockSkill] })

      const { listSkills } = await import('../skills')
      const result = await listSkills()

      expect(isTauri).toHaveBeenCalled()
      expect(apiClient.get).toHaveBeenCalled()
      expect(result).toEqual([mockSkill])
    })
  })

  describe('getSkill', () => {
    it('should use Tauri invoke when in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(mockSkill)

      const { getSkill } = await import('../skills')
      const result = await getSkill('skill-1')

      expect(tauriInvoke).toHaveBeenCalledWith('get_skill', { id: 'skill-1' })
      expect(result).toEqual(mockSkill)
    })

    it('should use REST API when not in Tauri mode', async () => {
      const { isTauri, apiClient } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)
      vi.mocked(apiClient.get).mockResolvedValue({ data: mockSkill })

      const { getSkill } = await import('../skills')
      const result = await getSkill('skill-1')

      expect(apiClient.get).toHaveBeenCalled()
      expect(result).toEqual(mockSkill)
    })
  })

  describe('createSkill', () => {
    it('should use Tauri invoke when in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(mockSkill)

      const { createSkill } = await import('../skills')
      const result = await createSkill({
        name: 'Test Skill',
        content: '# Test Content',
      })

      expect(tauriInvoke).toHaveBeenCalledWith(
        'create_skill',
        expect.objectContaining({
          skill: expect.objectContaining({
            name: 'Test Skill',
            content: '# Test Content',
          }),
        }),
      )
      expect(result).toEqual(mockSkill)
    })

    it('should use REST API when not in Tauri mode', async () => {
      const { isTauri, apiClient } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)
      vi.mocked(apiClient.post).mockResolvedValue({ data: mockSkill })

      const { createSkill } = await import('../skills')
      const result = await createSkill({
        name: 'Test Skill',
        content: '# Test Content',
      })

      expect(apiClient.post).toHaveBeenCalled()
      expect(result).toEqual(mockSkill)
    })
  })

  describe('deleteSkill', () => {
    it('should use Tauri invoke when in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(undefined)

      const { deleteSkill } = await import('../skills')
      await deleteSkill('skill-1')

      expect(tauriInvoke).toHaveBeenCalledWith('delete_skill', { id: 'skill-1' })
    })

    it('should use REST API when not in Tauri mode', async () => {
      const { isTauri, apiClient } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)
      vi.mocked(apiClient.delete).mockResolvedValue({})

      const { deleteSkill } = await import('../skills')
      await deleteSkill('skill-1')

      expect(apiClient.delete).toHaveBeenCalled()
    })
  })
})
