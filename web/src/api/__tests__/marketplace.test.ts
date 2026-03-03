import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  checkMarketplaceGating,
  getFeaturedMarketplaceSkills,
  getMarketplaceSkill,
  getMarketplaceSkillDetail,
  getMarketplaceStats,
  installMarketplaceSkill,
  isMarketplaceSkillInstalled,
  listInstalledMarketplaceSkills,
  listMarketplaceCategories,
  searchMarketplace,
  uninstallMarketplaceSkill,
  updateMarketplaceSkill,
} from '../marketplace'
import { tauriInvoke } from '../tauri-client'
import type { GatingCheckResult, SkillManifest, SkillVersion } from '@/types/generated'

vi.mock('../tauri-client', () => ({
  tauriInvoke: vi.fn(),
}))

const mockedTauriInvoke = vi.mocked(tauriInvoke)

const createManifest = (id: string, categories: string[] = ['productivity']): SkillManifest => ({
  id,
  name: `Skill ${id}`,
  version: { major: 1, minor: 0, patch: 0, prerelease: null },
  description: 'Test skill',
  author: null,
  license: null,
  homepage: null,
  repository: null,
  keywords: ['test'],
  categories,
  dependencies: [],
  permissions: { required: [], optional: [] },
  gating: {
    binaries: [],
    env_vars: [],
    supported_os: [],
    min_restflow_version: null,
  },
  source: { type: 'marketplace', url: 'https://example.com/skills' },
  icon: null,
  readme: null,
  changelog: null,
  metadata: {},
})

const passingGating: GatingCheckResult = {
  passed: true,
  missing_binaries: [],
  missing_env_vars: [],
  os_supported: true,
  restflow_version_ok: true,
  summary: 'ok',
}

const versions: SkillVersion[] = [
  { major: 1, minor: 0, patch: 0, prerelease: null },
  { major: 1, minor: 1, patch: 0, prerelease: null },
]

describe('Marketplace API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('searches marketplace with transformed request', async () => {
    mockedTauriInvoke.mockResolvedValueOnce([
      {
        manifest: createManifest('skill-1'),
        score: 90,
        downloads: 1200,
        rating: 4.7,
        source: 'marketplace',
      },
    ])

    const result = await searchMarketplace({
      query: 'automation',
      include_github: true,
      sort: 'popular',
      limit: 10,
      offset: 0,
    })

    expect(mockedTauriInvoke).toHaveBeenCalledWith('marketplace_search', {
      request: expect.objectContaining({
        query: 'automation',
        include_github: true,
        sort: 'popular',
        limit: 10,
        offset: 0,
      }),
    })
    expect(result).toHaveLength(1)
  })

  it('gets one marketplace skill', async () => {
    const manifest = createManifest('skill-1')
    mockedTauriInvoke.mockResolvedValueOnce(manifest)

    const result = await getMarketplaceSkill('skill-1')

    expect(mockedTauriInvoke).toHaveBeenCalledWith('marketplace_get_skill', {
      id: 'skill-1',
      source: 'marketplace',
    })
    expect(result.id).toBe('skill-1')
  })

  it('checks marketplace gating', async () => {
    mockedTauriInvoke.mockResolvedValueOnce(passingGating)

    const result = await checkMarketplaceGating('skill-1')

    expect(mockedTauriInvoke).toHaveBeenCalledWith('marketplace_check_gating', {
      id: 'skill-1',
      source: 'marketplace',
    })
    expect(result.passed).toBe(true)
  })

  it('aggregates marketplace skill detail', async () => {
    mockedTauriInvoke
      .mockResolvedValueOnce(createManifest('skill-1'))
      .mockResolvedValueOnce(versions)
      .mockResolvedValueOnce(passingGating)
      .mockResolvedValueOnce('# Skill Readme')

    const result = await getMarketplaceSkillDetail('skill-1')

    expect(result.manifest.id).toBe('skill-1')
    expect(result.versions).toHaveLength(2)
    expect(result.gating.passed).toBe(true)
    expect(result.content).toContain('Readme')
  })

  it('keeps detail content null when content fetch fails', async () => {
    mockedTauriInvoke
      .mockResolvedValueOnce(createManifest('skill-1'))
      .mockResolvedValueOnce(versions)
      .mockResolvedValueOnce(passingGating)
      .mockRejectedValueOnce(new Error('content unavailable'))

    const result = await getMarketplaceSkillDetail('skill-1')

    expect(result.content).toBeNull()
  })

  it('installs a skill', async () => {
    mockedTauriInvoke.mockResolvedValueOnce(undefined)

    const result = await installMarketplaceSkill({
      id: 'skill-1',
      source: 'marketplace',
      version: '1.0.0',
    })

    expect(mockedTauriInvoke).toHaveBeenCalledWith('marketplace_install_skill', {
      id: 'skill-1',
      version: '1.0.0',
      source: 'marketplace',
    })
    expect(result.success).toBe(true)
  })

  it('installs with overwrite by uninstalling first', async () => {
    mockedTauriInvoke.mockResolvedValueOnce(undefined).mockResolvedValueOnce(undefined)

    const result = await installMarketplaceSkill({
      id: 'skill-1',
      overwrite: true,
    })

    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(1, 'marketplace_uninstall_skill', {
      id: 'skill-1',
    })
    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(2, 'marketplace_install_skill', {
      id: 'skill-1',
      version: null,
      source: 'marketplace',
    })
    expect(result.success).toBe(true)
  })

  it('returns install error payload when install fails', async () => {
    mockedTauriInvoke.mockRejectedValueOnce(new Error('install failed'))

    const result = await installMarketplaceSkill({ id: 'skill-1' })

    expect(result.success).toBe(false)
    expect(result.error).toContain('install failed')
  })

  it('uninstalls a skill', async () => {
    mockedTauriInvoke.mockResolvedValueOnce(undefined)

    const result = await uninstallMarketplaceSkill('skill-1')

    expect(mockedTauriInvoke).toHaveBeenCalledWith('marketplace_uninstall_skill', {
      id: 'skill-1',
    })
    expect(result.success).toBe(true)
  })

  it('lists installed skills', async () => {
    mockedTauriInvoke.mockResolvedValueOnce([
      { id: 'skill-1', name: 'Skill 1' },
      { id: 'skill-2', name: 'Skill 2' },
    ])

    const result = await listInstalledMarketplaceSkills()

    expect(mockedTauriInvoke).toHaveBeenCalledWith('marketplace_list_installed')
    expect(result).toHaveLength(2)
  })

  it('checks installed skill by id', async () => {
    mockedTauriInvoke.mockResolvedValueOnce([
      { id: 'skill-1', name: 'Skill 1' },
      { id: 'skill-2', name: 'Skill 2' },
    ])

    const result = await isMarketplaceSkillInstalled('skill-2')

    expect(result).toBe(true)
  })

  it('computes marketplace stats', async () => {
    mockedTauriInvoke.mockResolvedValueOnce([
      {
        manifest: createManifest('skill-1', ['productivity', 'ops']),
        score: 100,
        downloads: 500,
        rating: 4.8,
        source: 'marketplace',
      },
      {
        manifest: createManifest('skill-2', ['productivity']),
        score: 90,
        downloads: 250,
        rating: 4.5,
        source: 'marketplace',
      },
    ])

    const stats = await getMarketplaceStats()

    expect(stats.total_skills).toBe(2)
    expect(stats.total_downloads).toBe(750)
    expect(stats.categories).toEqual(
      expect.arrayContaining([
        { name: 'productivity', count: 2 },
        { name: 'ops', count: 1 },
      ]),
    )
    expect(stats.featured_skills[0]).toBe('skill-1')
  })

  it('lists categories from stats', async () => {
    mockedTauriInvoke.mockResolvedValueOnce([
      {
        manifest: createManifest('skill-1', ['ops']),
        score: 88,
        downloads: 10,
        rating: null,
        source: 'marketplace',
      },
    ])

    const categories = await listMarketplaceCategories()

    expect(categories).toEqual([{ name: 'ops', count: 1 }])
  })

  it('returns featured skills', async () => {
    const items = [
      {
        manifest: createManifest('skill-1'),
        score: 100,
        downloads: 1000,
        rating: 4.9,
        source: 'marketplace',
      },
      {
        manifest: createManifest('skill-2'),
        score: 90,
        downloads: 10,
        rating: 4.0,
        source: 'marketplace',
      },
    ]
    mockedTauriInvoke.mockResolvedValueOnce(items).mockResolvedValueOnce(items)

    const featured = await getFeaturedMarketplaceSkills()

    expect(featured[0]?.manifest.id).toBe('skill-1')
  })

  it('updates skill via overwrite install', async () => {
    mockedTauriInvoke.mockResolvedValueOnce(undefined).mockResolvedValueOnce(undefined)

    const result = await updateMarketplaceSkill('skill-1')

    expect(result.success).toBe(true)
  })
})
