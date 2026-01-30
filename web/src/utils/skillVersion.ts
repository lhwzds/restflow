import type { SkillVersion } from '@/types/generated'

export function formatSkillVersion(version: SkillVersion): string {
  return `${version.major}.${version.minor}.${version.patch}${version.prerelease ? `-${version.prerelease}` : ''}`
}
