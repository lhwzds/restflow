/**
 * Marketplace Skill API
 *
 * Wrappers for marketplace-related Tauri commands.
 */

import { tauriInvoke } from './tauri-client'
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

/** Search marketplace skills. */
export async function searchMarketplace(
  request: MarketplaceSearchRequest = {},
): Promise<MarketplaceSearchItem[]> {
  return tauriInvoke('marketplace_search', {
    request: {
      query: request.query ?? null,
      category: request.category ?? null,
      tags: request.tags ?? null,
      author: request.author ?? null,
      limit: request.limit ?? null,
      offset: request.offset ?? null,
      sort: request.sort ?? null,
      include_github: request.include_github ?? null,
    },
  })
}

/** List marketplace skills, alias for search without filter. */
export async function listMarketplaceSkills(
  limit?: number,
  offset?: number,
): Promise<MarketplaceSearchItem[]> {
  return searchMarketplace({ limit, offset })
}

/** Get a marketplace skill manifest. */
export async function getMarketplaceSkill(
  id: string,
  source: MarketplaceSource = 'marketplace',
): Promise<SkillManifest> {
  return tauriInvoke('marketplace_get_skill', { id, source })
}

/** Get all published versions for a skill. */
export async function getMarketplaceVersions(
  id: string,
  source: MarketplaceSource = 'marketplace',
): Promise<SkillVersion[]> {
  return tauriInvoke('marketplace_get_versions', { id, source })
}

/** Get raw markdown/content from marketplace source. */
export async function getMarketplaceContent(
  id: string,
  version?: string,
  source: MarketplaceSource = 'marketplace',
): Promise<string> {
  return tauriInvoke('marketplace_get_content', {
    id,
    version: version ?? null,
    source,
  })
}

/** Check local environment gating requirements for a skill. */
export async function checkMarketplaceGating(
  id: string,
  source: MarketplaceSource = 'marketplace',
): Promise<GatingCheckResult> {
  return tauriInvoke('marketplace_check_gating', { id, source })
}

/** Aggregate detail for one marketplace skill. */
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

/** Install one marketplace skill. */
export async function installMarketplaceSkill(
  request: InstallSkillRequest,
): Promise<InstallSkillResult> {
  try {
    if (request.overwrite) {
      try {
        await tauriInvoke('marketplace_uninstall_skill', { id: request.id })
      } catch {
        // Best-effort cleanup before reinstall.
      }
    }

    await tauriInvoke('marketplace_install_skill', {
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

/** Uninstall one installed skill. */
export async function uninstallMarketplaceSkill(id: string): Promise<UninstallSkillResult> {
  try {
    await tauriInvoke('marketplace_uninstall_skill', { id })
    return { success: true }
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
    }
  }
}

/** List all locally installed skills. */
export async function listInstalledMarketplaceSkills(): Promise<Skill[]> {
  return tauriInvoke('marketplace_list_installed')
}

/** Check whether a skill is installed locally. */
export async function isMarketplaceSkillInstalled(id: string): Promise<boolean> {
  const installed = await listInstalledMarketplaceSkills()
  return installed.some((skill) => skill.id === id)
}

/** Compute a light stats payload from search results. */
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

/** Return aggregated category list. */
export async function listMarketplaceCategories(): Promise<MarketplaceCategory[]> {
  const stats = await getMarketplaceStats()
  return stats.categories
}

/** Return featured skills by popularity. */
export async function getFeaturedMarketplaceSkills(): Promise<MarketplaceSearchItem[]> {
  const stats = await getMarketplaceStats()
  if (stats.featured_skills.length === 0) {
    return []
  }

  const items = await searchMarketplace({ limit: 200, offset: 0, sort: 'popular' })
  return items.filter((item) => stats.featured_skills.includes(item.manifest.id))
}

/** Update one installed skill to latest available version. */
export async function updateMarketplaceSkill(
  id: string,
  source: MarketplaceSource = 'marketplace',
): Promise<InstallSkillResult> {
  return installMarketplaceSkill({ id, source, overwrite: true })
}
