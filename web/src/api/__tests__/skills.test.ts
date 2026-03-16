import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { Skill } from '@/types/generated/Skill'
import { requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  requestTyped: vi.fn(),
}))

const mockedRequestTyped = vi.mocked(requestTyped)

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

  it('lists skills', async () => {
    mockedRequestTyped.mockResolvedValue([mockSkill])

    const { listSkills } = await import('../skills')
    const result = await listSkills()

    expect(mockedRequestTyped).toHaveBeenCalledWith({ type: 'ListSkills' })
    expect(result).toEqual([mockSkill])
  })

  it('gets a skill by id', async () => {
    mockedRequestTyped.mockResolvedValue(mockSkill)

    const { getSkill } = await import('../skills')
    const result = await getSkill('skill-1')

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'GetSkill',
      data: { id: 'skill-1' },
    })
    expect(result).toEqual(mockSkill)
  })

  it('creates a skill through request contracts', async () => {
    mockedRequestTyped.mockResolvedValue(mockSkill)

    const { createSkill } = await import('../skills')
    const result = await createSkill({
      name: 'Test Skill',
      content: '# Test Content',
    })

    expect(mockedRequestTyped).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'CreateSkill',
        data: expect.objectContaining({
          skill: expect.objectContaining({
            name: 'Test Skill',
            content: '# Test Content',
          }),
        }),
      }),
    )
    expect(result).toEqual(mockSkill)
  })

  it('deletes a skill', async () => {
    mockedRequestTyped.mockResolvedValue(undefined)

    const { deleteSkill } = await import('../skills')
    await deleteSkill('skill-1')

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'DeleteSkill',
      data: { id: 'skill-1' },
    })
  })

  it('imports a skill from JSON through request contracts', async () => {
    mockedRequestTyped.mockResolvedValue(mockSkill)

    const { importSkillFromJson } = await import('../skills')
    const result = await importSkillFromJson('{"id":"skill-1","name":"Test Skill","content":"# Test Content"}')

    expect(mockedRequestTyped).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'CreateSkill',
        data: expect.objectContaining({
          skill: expect.objectContaining({ id: 'skill-1', name: 'Test Skill' }),
        }),
      }),
    )
    expect(result).toEqual(mockSkill)
  })
})
