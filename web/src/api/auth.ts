/**
 * Auth Profile Management API
 *
 * Browser-first wrappers around daemon request contracts.
 */

import type { AuthProfile } from '@/types/generated/AuthProfile'
import type { AuthProvider } from '@/types/generated/AuthProvider'
import type { DiscoverySummary } from '@/types/generated/DiscoverySummary'
import type { AddProfileRequest } from '@/types/generated/AddProfileRequest'
import type { ProfileResponse } from '@/types/generated/ProfileResponse'
import { requestOptional, requestTyped } from './http-client'

export type { AddProfileRequest, ProfileResponse }

export interface ManagerSummary {
  total: number
  enabled: number
  available: number
  in_cooldown: number
  disabled: number
  by_provider: Record<string, number>
  by_source: Record<string, number>
}

export interface ProfileUpdate {
  name?: string
  enabled?: boolean
  priority?: number
}

function buildProfileResponse(profile: AuthProfile): ProfileResponse {
  return { success: true, profile, error: null }
}

function buildProfileError(error: unknown): ProfileResponse {
  const message = error instanceof Error ? error.message : String(error)
  return { success: false, profile: null, error: message }
}

function summarizeProfiles(profiles: AuthProfile[]): ManagerSummary {
  const by_provider: Record<string, number> = {}
  const by_source: Record<string, number> = {}
  let enabled = 0
  let available = 0
  let in_cooldown = 0
  let disabled = 0

  for (const profile of profiles) {
    if (profile.enabled) {
      enabled += 1
    }
    if (profile.enabled && profile.health === 'healthy') {
      available += 1
    }
    if (profile.health === 'cooldown') {
      in_cooldown += 1
    }
    if (profile.health === 'disabled') {
      disabled += 1
    }

    by_provider[profile.provider] = (by_provider[profile.provider] ?? 0) + 1
    by_source[profile.source] = (by_source[profile.source] ?? 0) + 1
  }

  return {
    total: profiles.length,
    enabled,
    available,
    in_cooldown,
    disabled,
    by_provider,
    by_source,
  }
}

export async function authInitialize(): Promise<DiscoverySummary> {
  return requestTyped<DiscoverySummary>({ type: 'DiscoverAuth' })
}

export async function authDiscover(): Promise<DiscoverySummary> {
  return requestTyped<DiscoverySummary>({ type: 'DiscoverAuth' })
}

export async function authListProfiles(): Promise<AuthProfile[]> {
  return requestTyped<AuthProfile[]>({ type: 'ListAuthProfiles' })
}

export async function authGetProfilesForProvider(provider: AuthProvider): Promise<AuthProfile[]> {
  const profiles = await authListProfiles()
  return profiles.filter((profile) => profile.provider === provider)
}

export async function authGetAvailableProfiles(): Promise<AuthProfile[]> {
  const profiles = await authListProfiles()
  return profiles.filter((profile) => profile.enabled && profile.health === 'healthy')
}

export async function authGetProfile(profileId: string): Promise<AuthProfile | null> {
  return requestOptional<AuthProfile>({
    type: 'GetAuthProfile',
    data: { id: profileId },
  })
}

export async function authAddProfile(request: AddProfileRequest): Promise<ProfileResponse> {
  try {
    let profile = await requestTyped<AuthProfile>({
      type: 'AddAuthProfile',
      data: {
        name: request.name,
        credential: {
          type: 'api_key',
          key: request.api_key,
          email: request.email,
        },
        source: 'manual',
        provider: request.provider,
      },
    })

    if (request.priority !== 0) {
      profile = await requestTyped<AuthProfile>({
        type: 'UpdateAuthProfile',
        data: {
          id: profile.id,
          updates: {
            name: null,
            enabled: null,
            priority: request.priority,
          },
        },
      })
    }

    return buildProfileResponse(profile)
  } catch (error) {
    return buildProfileError(error)
  }
}

export async function authRemoveProfile(profileId: string): Promise<ProfileResponse> {
  const profile = await authGetProfile(profileId)
  if (!profile) {
    return buildProfileError(new Error(`Profile '${profileId}' not found`))
  }

  try {
    await requestTyped({
      type: 'RemoveAuthProfile',
      data: { id: profileId },
    })
    return buildProfileResponse(profile)
  } catch (error) {
    return buildProfileError(error)
  }
}

export async function authUpdateProfile(
  profileId: string,
  update: ProfileUpdate,
): Promise<ProfileResponse> {
  try {
    const profile = await requestTyped<AuthProfile>({
      type: 'UpdateAuthProfile',
      data: {
        id: profileId,
        updates: {
          name: update.name ?? null,
          enabled: update.enabled ?? null,
          priority: update.priority ?? null,
        },
      },
    })
    return buildProfileResponse(profile)
  } catch (error) {
    return buildProfileError(error)
  }
}

export async function authEnableProfile(profileId: string): Promise<ProfileResponse> {
  try {
    await requestTyped({
      type: 'EnableAuthProfile',
      data: { id: profileId },
    })
    const profile = await requestTyped<AuthProfile>({
      type: 'GetAuthProfile',
      data: { id: profileId },
    })
    return buildProfileResponse(profile)
  } catch (error) {
    return buildProfileError(error)
  }
}

export async function authDisableProfile(
  profileId: string,
  reason: string,
): Promise<ProfileResponse> {
  try {
    await requestTyped({
      type: 'DisableAuthProfile',
      data: { id: profileId, reason },
    })
    const profile = await requestTyped<AuthProfile>({
      type: 'GetAuthProfile',
      data: { id: profileId },
    })
    return buildProfileResponse(profile)
  } catch (error) {
    return buildProfileError(error)
  }
}

export async function authMarkSuccess(profileId: string): Promise<ProfileResponse> {
  try {
    await requestTyped({
      type: 'MarkAuthSuccess',
      data: { id: profileId },
    })
    const profile = await requestTyped<AuthProfile>({
      type: 'GetAuthProfile',
      data: { id: profileId },
    })
    return buildProfileResponse(profile)
  } catch (error) {
    return buildProfileError(error)
  }
}

export async function authMarkFailure(profileId: string): Promise<ProfileResponse> {
  try {
    await requestTyped({
      type: 'MarkAuthFailure',
      data: { id: profileId },
    })
    const profile = await requestTyped<AuthProfile>({
      type: 'GetAuthProfile',
      data: { id: profileId },
    })
    return buildProfileResponse(profile)
  } catch (error) {
    return buildProfileError(error)
  }
}

export async function authGetApiKey(provider: AuthProvider): Promise<boolean | null> {
  const response = await requestTyped<{ api_key: string | null }>({
    type: 'GetApiKey',
    data: { provider },
  })
  return response.api_key ? true : null
}

export async function authGetSummary(): Promise<ManagerSummary> {
  const profiles = await authListProfiles()
  return summarizeProfiles(profiles)
}

export async function authClear(): Promise<void> {
  await requestTyped({ type: 'ClearAuthProfiles' })
}
