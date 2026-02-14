import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { Skill } from '@/types/generated/Skill'
import { tauriInvoke } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(() => true),
  tauriInvoke: vi.fn(),
}))

const mockedTauriInvoke = vi.mocked(tauriInvoke)

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
    status: 'active',
    auto_complete: false,
    storage_mode: 'DatabaseOnly',
    is_synced: false,
    created_at: 1000,
    updated_at: 2000,
  }

  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('listSkills', () => {
    it('should invoke list_skills', async () => {
      mockedTauriInvoke.mockResolvedValue([mockSkill])

      const { listSkills } = await import('../skills')
      const result = await listSkills()

      expect(mockedTauriInvoke).toHaveBeenCalledWith('list_skills')
      expect(result).toEqual([mockSkill])
    })
  })

  describe('getSkill', () => {
    it('should invoke get_skill with id', async () => {
      mockedTauriInvoke.mockResolvedValue(mockSkill)

      const { getSkill } = await import('../skills')
      const result = await getSkill('skill-1')

      expect(mockedTauriInvoke).toHaveBeenCalledWith('get_skill', { id: 'skill-1' })
      expect(result).toEqual(mockSkill)
    })
  })

  describe('createSkill', () => {
    it('should invoke create_skill with skill data', async () => {
      mockedTauriInvoke.mockResolvedValue(mockSkill)

      const { createSkill } = await import('../skills')
      const result = await createSkill({
        name: 'Test Skill',
        content: '# Test Content',
      })

      expect(mockedTauriInvoke).toHaveBeenCalledWith(
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
  })

  describe('deleteSkill', () => {
    it('should invoke delete_skill with id', async () => {
      mockedTauriInvoke.mockResolvedValue(undefined)

      const { deleteSkill } = await import('../skills')
      await deleteSkill('skill-1')

      expect(mockedTauriInvoke).toHaveBeenCalledWith('delete_skill', { id: 'skill-1' })
    })
  })
})
