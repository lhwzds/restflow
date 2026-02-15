import { describe, it, expect } from 'vitest'
import type { SkillVersion } from '@/types/generated'
import { formatSkillVersion } from '@/utils/skillVersion'

const createVersion = (overrides: Partial<SkillVersion> = {}): SkillVersion => ({
  major: 1,
  minor: 2,
  patch: 3,
  prerelease: null,
  ...overrides,
})

describe('formatSkillVersion', () => {
  it('formats release versions without prerelease', () => {
    const version = createVersion()
    expect(formatSkillVersion(version)).toBe('1.2.3')
  })

  it('includes prerelease tag when present', () => {
    const version = createVersion({ prerelease: 'beta.1' })
    expect(formatSkillVersion(version)).toBe('1.2.3-beta.1')
  })
})
