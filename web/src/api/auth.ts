/**
 * Auth Profile Management API
 * 
 * TypeScript API for managing authentication profiles in RestFlow.
 */

import { invoke } from '@tauri-apps/api/core';
import type { AuthProfile, AuthProvider, DiscoverySummary } from '@/types/generated';

// Local types for manager summary and requests
export interface ManagerSummary {
  total: number;
  enabled: number;
  available: number;
  in_cooldown: number;
  disabled: number;
  by_provider: Record<string, number>;
  by_source: Record<string, number>;
}

export interface AddProfileRequest {
  name: string;
  api_key: string;
  provider: AuthProvider;
  email?: string;
  priority?: number;
}

export interface ProfileUpdate {
  name?: string;
  enabled?: boolean;
  priority?: number;
}

export interface ProfileResponse {
  success: boolean;
  profile?: AuthProfile;
  error?: string;
}

/**
 * Initialize the auth manager and run discovery
 */
export async function authInitialize(): Promise<DiscoverySummary> {
  return invoke<DiscoverySummary>('auth_initialize');
}

/**
 * Run credential discovery
 */
export async function authDiscover(): Promise<DiscoverySummary> {
  return invoke<DiscoverySummary>('auth_discover');
}

/**
 * List all profiles
 */
export async function authListProfiles(): Promise<AuthProfile[]> {
  return invoke<AuthProfile[]>('auth_list_profiles');
}

/**
 * Get profiles for a specific provider
 */
export async function authGetProfilesForProvider(provider: AuthProvider): Promise<AuthProfile[]> {
  return invoke<AuthProfile[]>('auth_get_profiles_for_provider', { provider });
}

/**
 * Get available profiles (enabled, not expired, not in cooldown)
 */
export async function authGetAvailableProfiles(): Promise<AuthProfile[]> {
  return invoke<AuthProfile[]>('auth_get_available_profiles');
}

/**
 * Get a specific profile by ID
 */
export async function authGetProfile(profileId: string): Promise<AuthProfile | null> {
  return invoke<AuthProfile | null>('auth_get_profile', { profile_id: profileId });
}

/**
 * Add a manual profile
 */
export async function authAddProfile(request: AddProfileRequest): Promise<ProfileResponse> {
  return invoke<ProfileResponse>('auth_add_profile', { request });
}

/**
 * Remove a profile
 */
export async function authRemoveProfile(profileId: string): Promise<ProfileResponse> {
  return invoke<ProfileResponse>('auth_remove_profile', { profile_id: profileId });
}

/**
 * Update a profile
 */
export async function authUpdateProfile(profileId: string, update: ProfileUpdate): Promise<ProfileResponse> {
  return invoke<ProfileResponse>('auth_update_profile', { profile_id: profileId, update });
}

/**
 * Enable a profile
 */
export async function authEnableProfile(profileId: string): Promise<ProfileResponse> {
  return invoke<ProfileResponse>('auth_enable_profile', { profile_id: profileId });
}

/**
 * Disable a profile
 */
export async function authDisableProfile(profileId: string, reason: string): Promise<ProfileResponse> {
  return invoke<ProfileResponse>('auth_disable_profile', { profile_id: profileId, reason });
}

/**
 * Mark a profile as successfully used
 */
export async function authMarkSuccess(profileId: string): Promise<ProfileResponse> {
  return invoke<ProfileResponse>('auth_mark_success', { profile_id: profileId });
}

/**
 * Mark a profile as failed
 */
export async function authMarkFailure(profileId: string): Promise<ProfileResponse> {
  return invoke<ProfileResponse>('auth_mark_failure', { profile_id: profileId });
}

/**
 * Get API key for a provider (selects best available profile)
 */
export async function authGetApiKey(provider: AuthProvider): Promise<string | null> {
  return invoke<string | null>('auth_get_api_key', { provider });
}

/**
 * Get manager summary
 */
export async function authGetSummary(): Promise<ManagerSummary> {
  return invoke<ManagerSummary>('auth_get_summary');
}

/**
 * Clear all profiles
 */
export async function authClear(): Promise<void> {
  return invoke<void>('auth_clear');
}
