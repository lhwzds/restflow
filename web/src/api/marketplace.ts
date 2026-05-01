/**
 * Marketplace Skill API
 *
 * Browser-first wrappers around daemon marketplace HTTP endpoints.
 */

import { buildUrl, fetchJson } from './http-client'
import type { GatingCheckResult, Skill, SkillManifest, SkillVersion } from '@/types/generated'

export type MarketplaceSource = 'marketplace' | 'github'

export interface MarketplaceSearchRequest {
  query?: string
  category?: string
  tags?: string[]
  author?: string
  limit?: number
  offset?: number
  sort?: 'relevance' | 'recently_updated' | 'popular' | 'name'
  include_github?: boolean
}

export interface MarketplaceSearchItem {
  manifest: SkillManifest
  score: number
  downloads: number | null
  rating: number | null
  source: MarketplaceSource | string
}

export interface MarketplaceSkillDetail {
  manifest: SkillManifest
  versions: SkillVersion[]
  content: string | null
  gating: GatingCheckResult
}

export interface InstallSkillRequest {
  id: string
  source?: MarketplaceSource
  version?: string
  overwrite?: boolean
}

export interface InstallSkillResult {
  success: boolean
  error?: string
}

export interface UninstallSkillResult {
  success: boolean
  error?: string
}

export interface MarketplaceCategory {
  name: string
  count: number
}

export interface MarketplaceStats {
  total_skills: number
  total_downloads: number
  categories: MarketplaceCategory[]
  featured_skills: string[]
}

async function postNoContent(path: string, body: unknown): Promise<void> {
  const response = await fetch(buildUrl(path), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })

  if (!response.ok) {
    throw new Error((await response.text()) || `HTTP ${response.status}`)
  }
}

export async function searchMarketplace(
  request: MarketplaceSearchRequest = {},
): Promise<MarketplaceSearchItem[]> {
  return fetchJson<MarketplaceSearchItem[]>('/api/marketplace/search', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
}

export async function listMarketplaceSkills(
  limit?: number,
  offset?: number,
): Promise<MarketplaceSearchItem[]> {
  return searchMarketplace({ limit, offset })
}

export async function getMarketplaceSkill(
  id: string,
  source: MarketplaceSource = 'marketplace',
): Promise<SkillManifest> {
  return fetchJson<SkillManifest>('/api/marketplace/skill', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, source }),
  })
}

export async function getMarketplaceVersions(
  id: string,
  source: MarketplaceSource = 'marketplace',
): Promise<SkillVersion[]> {
  return fetchJson<SkillVersion[]>('/api/marketplace/versions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, source }),
  })
}

export async function getMarketplaceContent(
  id: string,
  version?: string,
  source: MarketplaceSource = 'marketplace',
): Promise<string> {
  return fetchJson<string>('/api/marketplace/content', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, version: version ?? null, source }),
  })
}

export async function checkMarketplaceGating(
  id: string,
  source: MarketplaceSource = 'marketplace',
): Promise<GatingCheckResult> {
  return fetchJson<GatingCheckResult>('/api/marketplace/gating', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, source }),
  })
}

export async function getMarketplaceSkillDetail(
  id: string,
  source: MarketplaceSource = 'marketplace',
): Promise<MarketplaceSkillDetail> {
  const [manifest, versions, gating] = await Promise.all([
    getMarketplaceSkill(id, source),
    getMarketplaceVersions(id, source),
    checkMarketplaceGating(id, source),
  ])

  let content: string | null = null
  try {
    content = await getMarketplaceContent(id, undefined, source)
  } catch {
    content = null
  }

  return { manifest, versions, content, gating }
}

export async function installMarketplaceSkill(
  request: InstallSkillRequest,
): Promise<InstallSkillResult> {
  try {
    if (request.overwrite) {
      try {
        await postNoContent('/api/marketplace/uninstall', { id: request.id })
      } catch {
        // Best-effort cleanup before reinstall.
      }
    }

    await postNoContent('/api/marketplace/install', {
      id: request.id,
      version: request.version ?? null,
      source: request.source ?? 'marketplace',
    })
    return { success: true }
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
    }
  }
}

export async function uninstallMarketplaceSkill(id: string): Promise<UninstallSkillResult> {
  try {
    await postNoContent('/api/marketplace/uninstall', { id })
    return { success: true }
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
    }
  }
}

export async function listInstalledMarketplaceSkills(): Promise<Skill[]> {
  return fetchJson<Skill[]>('/api/marketplace/installed')
}

export async function isMarketplaceSkillInstalled(id: string): Promise<boolean> {
  const installed = await listInstalledMarketplaceSkills()
  return installed.some((skill) => skill.id === id)
}

export async function getMarketplaceStats(): Promise<MarketplaceStats> {
  const items = await searchMarketplace({ limit: 200, offset: 0, sort: 'popular' })
  const categories = new Map<string, number>()
  let totalDownloads = 0

  for (const item of items) {
    totalDownloads += item.downloads ?? 0
    for (const category of item.manifest.categories) {
      categories.set(category, (categories.get(category) ?? 0) + 1)
    }
  }

  const sortedFeatured = [...items]
    .sort((a, b) => (b.downloads ?? 0) - (a.downloads ?? 0))
    .slice(0, 5)
    .map((item) => item.manifest.id)

  return {
    total_skills: items.length,
    total_downloads: totalDownloads,
    categories: [...categories.entries()].map(([name, count]) => ({ name, count })),
    featured_skills: sortedFeatured,
  }
}

export async function listMarketplaceCategories(): Promise<MarketplaceCategory[]> {
  const stats = await getMarketplaceStats()
  return stats.categories
}

export async function getFeaturedMarketplaceSkills(): Promise<MarketplaceSearchItem[]> {
  const stats = await getMarketplaceStats()
  if (stats.featured_skills.length === 0) {
    return []
  }

  const items = await searchMarketplace({ limit: 200, offset: 0, sort: 'popular' })
  return items.filter((item) => stats.featured_skills.includes(item.manifest.id))
}

export async function updateMarketplaceSkill(
  id: string,
  source: MarketplaceSource = 'marketplace',
): Promise<InstallSkillResult> {
  return installMarketplaceSkill({ id, source, overwrite: true })
}
