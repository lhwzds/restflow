/**
 * Marketplace API functions for skill discovery and installation
 */

import { invoke } from '@tauri-apps/api/core'
import type { SkillManifest, SkillVersion, GatingCheckResult } from '@/types/generated'

/**
 * Search request parameters
 */
export interface MarketplaceSearchRequest {
  query?: string
  category?: string
  tags?: string[]
  author?: string
  limit?: number
  offset?: number
  sort?: 'relevance' | 'updated' | 'popular' | 'name'
  includeGithub?: boolean
}

/**
 * Search result from marketplace
 */
export interface MarketplaceSearchResult {
  manifest: SkillManifest
  score: number
  downloads?: number
  rating?: number
  source: 'marketplace' | 'github' | 'local' | 'builtin' | 'git'
}

/**
 * Search the marketplace for skills
 */
export async function searchMarketplace(
  request: MarketplaceSearchRequest
): Promise<MarketplaceSearchResult[]> {
  return invoke('marketplace_search', { request })
}

/**
 * Get skill details from marketplace
 */
export async function getMarketplaceSkill(
  id: string,
  source?: string
): Promise<SkillManifest> {
  return invoke('marketplace_get_skill', { id, source })
}

/**
 * Get available versions for a skill
 */
export async function getSkillVersions(
  id: string,
  source?: string
): Promise<SkillVersion[]> {
  return invoke('marketplace_get_versions', { id, source })
}

/**
 * Get skill content/documentation
 */
export async function getSkillContent(
  id: string,
  version?: string,
  source?: string
): Promise<string> {
  return invoke('marketplace_get_content', { id, version, source })
}

/**
 * Check gating requirements for a skill
 */
export async function checkSkillGating(
  id: string,
  source?: string
): Promise<GatingCheckResult> {
  return invoke('marketplace_check_gating', { id, source })
}

/**
 * Install a skill from marketplace
 */
export async function installSkill(
  id: string,
  version?: string,
  source?: string
): Promise<void> {
  return invoke('marketplace_install_skill', { id, version, source })
}

/**
 * Uninstall a skill
 */
export async function uninstallSkill(id: string): Promise<void> {
  return invoke('marketplace_uninstall_skill', { id })
}

/**
 * List installed skills
 */
export async function listInstalledSkills(): Promise<unknown[]> {
  return invoke('marketplace_list_installed')
}
