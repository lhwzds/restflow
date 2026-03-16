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
import { fetchJson } from '../http-client'
import type { GatingCheckResult, SkillManifest, SkillVersion } from '@/types/generated'

vi.mock('../http-client', () => ({
  buildUrl: vi.fn((path: string) => `http://127.0.0.1:8787${path}`),
  fetchJson: vi.fn(),
}))

declare const global: typeof globalThis

const mockedFetchJson = vi.mocked(fetchJson)

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
    vi.stubGlobal('fetch', vi.fn())
  })

  it('searches marketplace through daemon HTTP', async () => {
    mockedFetchJson.mockResolvedValueOnce([
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

    expect(mockedFetchJson).toHaveBeenCalledWith('/api/marketplace/search', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
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
    mockedFetchJson.mockResolvedValueOnce(manifest)

    const result = await getMarketplaceSkill('skill-1')

    expect(mockedFetchJson).toHaveBeenCalledWith('/api/marketplace/skill', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: 'skill-1', source: 'marketplace' }),
    })
    expect(result.id).toBe('skill-1')
  })

  it('checks marketplace gating', async () => {
    mockedFetchJson.mockResolvedValueOnce(passingGating)

    const result = await checkMarketplaceGating('skill-1')

    expect(mockedFetchJson).toHaveBeenCalledWith('/api/marketplace/gating', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: 'skill-1', source: 'marketplace' }),
    })
    expect(result.passed).toBe(true)
  })

  it('aggregates marketplace skill detail', async () => {
    mockedFetchJson
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
    mockedFetchJson
      .mockResolvedValueOnce(createManifest('skill-1'))
      .mockResolvedValueOnce(versions)
      .mockResolvedValueOnce(passingGating)
      .mockRejectedValueOnce(new Error('content unavailable'))

    const result = await getMarketplaceSkillDetail('skill-1')

    expect(result.content).toBeNull()
  })

  it('installs a skill via HTTP endpoint', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 200 })))

    const result = await installMarketplaceSkill({
      id: 'skill-1',
      source: 'marketplace',
      version: '1.0.0',
    })

    expect(fetch).toHaveBeenCalledWith('http://127.0.0.1:8787/api/marketplace/install', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        id: 'skill-1',
        version: '1.0.0',
        source: 'marketplace',
      }),
    })
    expect(result.success).toBe(true)
  })

  it('installs with overwrite by uninstalling first', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 200 })))

    const result = await installMarketplaceSkill({
      id: 'skill-1',
      overwrite: true,
    })

    expect(fetch).toHaveBeenNthCalledWith(1, 'http://127.0.0.1:8787/api/marketplace/uninstall', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: 'skill-1' }),
    })
    expect(fetch).toHaveBeenNthCalledWith(2, 'http://127.0.0.1:8787/api/marketplace/install', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: 'skill-1', version: null, source: 'marketplace' }),
    })
    expect(result.success).toBe(true)
  })

  it('returns install error payload when install fails', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('install failed', { status: 500 })))

    const result = await installMarketplaceSkill({ id: 'skill-1' })

    expect(result.success).toBe(false)
    expect(result.error).toContain('install failed')
  })

  it('uninstalls a skill', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 200 })))

    const result = await uninstallMarketplaceSkill('skill-1')

    expect(fetch).toHaveBeenCalledWith('http://127.0.0.1:8787/api/marketplace/uninstall', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: 'skill-1' }),
    })
    expect(result.success).toBe(true)
  })

  it('lists installed skills', async () => {
    mockedFetchJson.mockResolvedValueOnce([
      { id: 'skill-1', name: 'Skill 1' },
      { id: 'skill-2', name: 'Skill 2' },
    ])

    const result = await listInstalledMarketplaceSkills()

    expect(mockedFetchJson).toHaveBeenCalledWith('/api/marketplace/installed')
    expect(result).toHaveLength(2)
  })

  it('checks installed skill by id', async () => {
    mockedFetchJson.mockResolvedValueOnce([
      { id: 'skill-1', name: 'Skill 1' },
      { id: 'skill-2', name: 'Skill 2' },
    ])

    const result = await isMarketplaceSkillInstalled('skill-2')

    expect(result).toBe(true)
  })

  it('computes marketplace stats and categories', async () => {
    mockedFetchJson.mockResolvedValue([
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
    const categories = await listMarketplaceCategories()
    const featured = await getFeaturedMarketplaceSkills()

    expect(stats.total_skills).toBe(2)
    expect(categories).toEqual(
      expect.arrayContaining([
        { name: 'productivity', count: 2 },
        { name: 'ops', count: 1 },
      ]),
    )
    expect(featured).toHaveLength(2)
  })

  it('updates a marketplace skill by reinstalling', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 200 })))

    const result = await updateMarketplaceSkill('skill-1')

    expect(result.success).toBe(true)
  })
})
