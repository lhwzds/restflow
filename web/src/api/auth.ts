/**
 * Auth Profile Management API
 *
 * TypeScript API for managing authentication profiles in RestFlow.
 */

import type { AuthProfile, AuthProvider, DiscoverySummary } from '@/types/generated'
import { invokeCommand } from './tauri-client'

// Local types for manager summary and requests
export interface ManagerSummary {
  total: number
  enabled: number
  available: number
  in_cooldown: number
  disabled: number
  by_provider: Record<string, number>
  by_source: Record<string, number>
}

export interface AddProfileRequest {
  name: string
  api_key: string
  provider: AuthProvider
  email?: string
  priority?: number
}

export interface ProfileUpdate {
  name?: string
  enabled?: boolean
  priority?: number
}

export interface ProfileResponse {
  success: boolean
  profile?: AuthProfile
  error?: string
}

/**
 * Initialize the auth manager and run discovery
 */
export async function authInitialize(): Promise<DiscoverySummary> {
  return invokeCommand('authInitialize')
}

/**
 * Run credential discovery
 */
export async function authDiscover(): Promise<DiscoverySummary> {
  return invokeCommand('authDiscover')
}

/**
 * List all profiles
 */
export async function authListProfiles(): Promise<AuthProfile[]> {
  return invokeCommand('authListProfiles')
}

/**
 * Get profiles for a specific provider
 */
export async function authGetProfilesForProvider(provider: AuthProvider): Promise<AuthProfile[]> {
  return invokeCommand('authGetProfilesForProvider', provider)
}

/**
 * Get available profiles (enabled, not expired, not in cooldown)
 */
export async function authGetAvailableProfiles(): Promise<AuthProfile[]> {
  return invokeCommand('authGetAvailableProfiles')
}

/**
 * Get a specific profile by ID
 */
export async function authGetProfile(profileId: string): Promise<AuthProfile | null> {
  return invokeCommand('authGetProfile', profileId)
}

/**
 * Add a manual profile
 */
export async function authAddProfile(request: AddProfileRequest): Promise<ProfileResponse> {
  return invokeCommand('authAddProfile', request)
}

/**
 * Remove a profile
 */
export async function authRemoveProfile(profileId: string): Promise<ProfileResponse> {
  return invokeCommand('authRemoveProfile', profileId)
}

/**
 * Update a profile
 */
export async function authUpdateProfile(
  profileId: string,
  update: ProfileUpdate,
): Promise<ProfileResponse> {
  return invokeCommand('authUpdateProfile', profileId, {
    name: update.name ?? null,
    enabled: update.enabled ?? null,
    priority: update.priority ?? null,
  })
}

/**
 * Enable a profile
 */
export async function authEnableProfile(profileId: string): Promise<ProfileResponse> {
  return invokeCommand('authEnableProfile', profileId)
}

/**
 * Disable a profile
 */
export async function authDisableProfile(
  profileId: string,
  reason: string,
): Promise<ProfileResponse> {
  return invokeCommand('authDisableProfile', profileId, reason)
}

/**
 * Mark a profile as successfully used
 */
export async function authMarkSuccess(profileId: string): Promise<ProfileResponse> {
  return invokeCommand('authMarkSuccess', profileId)
}

/**
 * Mark a profile as failed
 */
export async function authMarkFailure(profileId: string): Promise<ProfileResponse> {
  return invokeCommand('authMarkFailure', profileId)
}

/**
 * Check if an API key exists for a provider (selects best available profile)
 */
export async function authGetApiKey(provider: AuthProvider): Promise<boolean | null> {
  return invokeCommand('authGetApiKey', provider)
}

/**
 * Get manager summary
 */
export async function authGetSummary(): Promise<ManagerSummary> {
  return invokeCommand('authGetSummary')
}

/**
 * Clear all profiles
 */
export async function authClear(): Promise<void> {
  await invokeCommand('authClear')
}
