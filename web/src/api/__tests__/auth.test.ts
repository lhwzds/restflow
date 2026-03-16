import { describe, it, expect, vi, beforeEach } from 'vitest'
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
import { requestOptional, requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  requestOptional: vi.fn(),
  requestTyped: vi.fn(),
}))

const mockedRequestTyped = vi.mocked(requestTyped)
const mockedRequestOptional = vi.mocked(requestOptional)

const profile = {
  id: 'profile-1',
  name: 'Main',
  provider: 'openai',
  source: 'manual',
  enabled: true,
  priority: 0,
  health: 'healthy',
  usage_count: 0,
  failure_count: 0,
  cooldown_until: null,
  last_used_at: null,
  created_at: 1,
  updated_at: 2,
}

describe('Auth API', () => {
  beforeEach(() => {
    mockedRequestTyped.mockReset()
    mockedRequestOptional.mockReset()
  })

  it('loads discovery through daemon request contracts', async () => {
    mockedRequestTyped.mockResolvedValueOnce({ found: 1 }).mockResolvedValueOnce({ found: 2 })

    await authInitialize()
    await authDiscover()

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, { type: 'DiscoverAuth' })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(2, { type: 'DiscoverAuth' })
  })

  it('lists and filters profiles', async () => {
    mockedRequestTyped
      .mockResolvedValueOnce([profile])
      .mockResolvedValueOnce([profile])
      .mockResolvedValueOnce([profile])
    mockedRequestOptional.mockResolvedValueOnce(profile)

    await authListProfiles()
    await authGetProfilesForProvider('openai')
    await authGetAvailableProfiles()
    await authGetProfile('profile-1')

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, { type: 'ListAuthProfiles' })
    expect(mockedRequestOptional).toHaveBeenCalledWith({
      type: 'GetAuthProfile',
      data: { id: 'profile-1' },
    })
  })

  it('maps mutation payloads to request contracts', async () => {
    const addRequest: AddProfileRequest = {
      name: 'My Key',
      api_key: 'sk-test-123',
      provider: 'openai',
      email: 'test@example.com',
      priority: 10,
    }
    const update: ProfileUpdate = { name: 'Updated Name', enabled: true, priority: 5 }

    mockedRequestTyped
      .mockResolvedValueOnce(profile)
      .mockResolvedValueOnce({ ...profile, priority: 10 })
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce({ ...profile, name: 'Updated Name' })
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(profile)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(profile)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(profile)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(profile)
      .mockResolvedValueOnce(undefined)
    mockedRequestOptional.mockResolvedValueOnce(profile)

    await authAddProfile(addRequest)
    await authRemoveProfile('profile-1')
    await authUpdateProfile('profile-1', update)
    await authEnableProfile('profile-1')
    await authDisableProfile('profile-1', 'rate limited')
    await authMarkSuccess('profile-1')
    await authMarkFailure('profile-1')

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'AddAuthProfile',
      data: {
        name: 'My Key',
        credential: {
          type: 'api_key',
          key: 'sk-test-123',
          email: 'test@example.com',
        },
        source: 'manual',
        provider: 'openai',
      },
    })
    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'UpdateAuthProfile',
      data: {
        id: 'profile-1',
        updates: {
          name: 'Updated Name',
          enabled: true,
          priority: 5,
        },
      },
    })
    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'DisableAuthProfile',
      data: { id: 'profile-1', reason: 'rate limited' },
    })
  })

  it('returns api-key presence and summary', async () => {
    mockedRequestTyped
      .mockResolvedValueOnce({ api_key: 'secret' })
      .mockResolvedValueOnce([profile])
      .mockResolvedValueOnce(undefined)

    const hasKey = await authGetApiKey('anthropic')
    const summary = await authGetSummary()
    await authClear()

    expect(hasKey).toBe(true)
    expect(summary.total).toBe(1)
    expect(mockedRequestTyped).toHaveBeenLastCalledWith({ type: 'ClearAuthProfiles' })
  })

  it('propagates typed request errors', async () => {
    mockedRequestTyped.mockRejectedValue(new Error('Backend unavailable'))

    await expect(authListProfiles()).rejects.toThrow('Backend unavailable')
  })
})
