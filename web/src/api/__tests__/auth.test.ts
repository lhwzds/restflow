import { describe, it, expect, vi, beforeEach } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import {
  authInitialize,
  authDiscover,
  authListProfiles,
  authGetProfilesForProvider,
  authGetAvailableProfiles,
  authGetProfile,
  authAddProfile,
  authRemoveProfile,
  authUpdateProfile,
  authEnableProfile,
  authDisableProfile,
  authMarkSuccess,
  authMarkFailure,
  authGetApiKey,
  authGetSummary,
  authClear,
} from '../auth'
import type { AddProfileRequest, ProfileUpdate } from '../auth'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

const mockedInvoke = vi.mocked(invoke)

describe('Auth API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('authInitialize', () => {
    it('invokes auth_initialize with no parameters', async () => {
      const mockResult = { found: 2, added: 1, sources: ['env'] }
      mockedInvoke.mockResolvedValue(mockResult)

      const result = await authInitialize()

      expect(mockedInvoke).toHaveBeenCalledWith('auth_initialize')
      expect(result).toEqual(mockResult)
    })
  })

  describe('authDiscover', () => {
    it('invokes auth_discover with no parameters', async () => {
      const mockResult = { found: 3, added: 2, sources: ['env', 'file'] }
      mockedInvoke.mockResolvedValue(mockResult)

      const result = await authDiscover()

      expect(mockedInvoke).toHaveBeenCalledWith('auth_discover')
      expect(result).toEqual(mockResult)
    })
  })

  describe('authListProfiles', () => {
    it('invokes auth_list_profiles with no parameters', async () => {
      mockedInvoke.mockResolvedValue([])

      const result = await authListProfiles()

      expect(mockedInvoke).toHaveBeenCalledWith('auth_list_profiles')
      expect(result).toEqual([])
    })
  })

  describe('authGetProfilesForProvider', () => {
    it('invokes auth_get_profiles_for_provider with provider parameter', async () => {
      mockedInvoke.mockResolvedValue([])

      const result = await authGetProfilesForProvider('anthropic')

      expect(mockedInvoke).toHaveBeenCalledWith('auth_get_profiles_for_provider', {
        provider: 'anthropic',
      })
      expect(result).toEqual([])
    })
  })

  describe('authGetAvailableProfiles', () => {
    it('invokes auth_get_available_profiles with no parameters', async () => {
      mockedInvoke.mockResolvedValue([])

      const result = await authGetAvailableProfiles()

      expect(mockedInvoke).toHaveBeenCalledWith('auth_get_available_profiles')
      expect(result).toEqual([])
    })
  })

  describe('authGetProfile', () => {
    it('invokes auth_get_profile with snake_case profile_id', async () => {
      mockedInvoke.mockResolvedValue(null)

      const result = await authGetProfile('profile-123')

      expect(mockedInvoke).toHaveBeenCalledWith('auth_get_profile', {
        profile_id: 'profile-123',
      })
      expect(result).toBeNull()
    })
  })

  describe('authAddProfile', () => {
    it('invokes auth_add_profile with request object', async () => {
      const request: AddProfileRequest = {
        name: 'My Key',
        api_key: 'sk-test-123',
        provider: 'openai',
        email: 'test@example.com',
        priority: 10,
      }
      const mockResponse = { success: true }
      mockedInvoke.mockResolvedValue(mockResponse)

      const result = await authAddProfile(request)

      expect(mockedInvoke).toHaveBeenCalledWith('auth_add_profile', { request })
      expect(result).toEqual(mockResponse)
    })
  })

  describe('authRemoveProfile', () => {
    it('invokes auth_remove_profile with snake_case profile_id', async () => {
      mockedInvoke.mockResolvedValue({ success: true })

      const result = await authRemoveProfile('profile-456')

      expect(mockedInvoke).toHaveBeenCalledWith('auth_remove_profile', {
        profile_id: 'profile-456',
      })
      expect(result).toEqual({ success: true })
    })
  })

  describe('authUpdateProfile', () => {
    it('invokes auth_update_profile with snake_case profile_id and update object', async () => {
      const update: ProfileUpdate = { name: 'Updated Name', enabled: true, priority: 5 }
      mockedInvoke.mockResolvedValue({ success: true })

      const result = await authUpdateProfile('profile-789', update)

      expect(mockedInvoke).toHaveBeenCalledWith('auth_update_profile', {
        profile_id: 'profile-789',
        update,
      })
      expect(result).toEqual({ success: true })
    })
  })

  describe('authEnableProfile', () => {
    it('invokes auth_enable_profile with snake_case profile_id', async () => {
      mockedInvoke.mockResolvedValue({ success: true })

      const result = await authEnableProfile('profile-aaa')

      expect(mockedInvoke).toHaveBeenCalledWith('auth_enable_profile', {
        profile_id: 'profile-aaa',
      })
      expect(result).toEqual({ success: true })
    })
  })

  describe('authDisableProfile', () => {
    it('invokes auth_disable_profile with snake_case profile_id and reason', async () => {
      mockedInvoke.mockResolvedValue({ success: true })

      const result = await authDisableProfile('profile-bbb', 'rate limited')

      expect(mockedInvoke).toHaveBeenCalledWith('auth_disable_profile', {
        profile_id: 'profile-bbb',
        reason: 'rate limited',
      })
      expect(result).toEqual({ success: true })
    })
  })

  describe('authMarkSuccess', () => {
    it('invokes auth_mark_success with snake_case profile_id', async () => {
      mockedInvoke.mockResolvedValue({ success: true })

      const result = await authMarkSuccess('profile-ccc')

      expect(mockedInvoke).toHaveBeenCalledWith('auth_mark_success', {
        profile_id: 'profile-ccc',
      })
      expect(result).toEqual({ success: true })
    })
  })

  describe('authMarkFailure', () => {
    it('invokes auth_mark_failure with snake_case profile_id', async () => {
      mockedInvoke.mockResolvedValue({ success: true })

      const result = await authMarkFailure('profile-ddd')

      expect(mockedInvoke).toHaveBeenCalledWith('auth_mark_failure', {
        profile_id: 'profile-ddd',
      })
      expect(result).toEqual({ success: true })
    })
  })

  describe('authGetApiKey', () => {
    it('invokes auth_get_api_key with provider parameter and returns boolean', async () => {
      mockedInvoke.mockResolvedValue(true)

      const result = await authGetApiKey('openai')

      expect(mockedInvoke).toHaveBeenCalledWith('auth_get_api_key', { provider: 'openai' })
      expect(result).toBe(true)
    })

    it('returns null when no key is available', async () => {
      mockedInvoke.mockResolvedValue(null)

      const result = await authGetApiKey('anthropic')

      expect(result).toBeNull()
    })
  })

  describe('authGetSummary', () => {
    it('invokes auth_get_summary with no parameters', async () => {
      const mockSummary = {
        total: 5,
        enabled: 3,
        available: 2,
        in_cooldown: 1,
        disabled: 1,
        by_provider: { openai: 3, anthropic: 2 },
        by_source: { env: 2, manual: 3 },
      }
      mockedInvoke.mockResolvedValue(mockSummary)

      const result = await authGetSummary()

      expect(mockedInvoke).toHaveBeenCalledWith('auth_get_summary')
      expect(result).toEqual(mockSummary)
    })
  })

  describe('authClear', () => {
    it('invokes auth_clear with no parameters', async () => {
      mockedInvoke.mockResolvedValue(undefined)

      await authClear()

      expect(mockedInvoke).toHaveBeenCalledWith('auth_clear')
    })
  })

  describe('error propagation', () => {
    it('propagates errors from invoke', async () => {
      mockedInvoke.mockRejectedValue(new Error('Backend unavailable'))

      await expect(authListProfiles()).rejects.toThrow('Backend unavailable')
    })
  })
})
